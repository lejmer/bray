use std::collections::{BTreeMap, VecDeque};
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::Arc;

use bray_compilation::{CancellationToken, WorkerBudget};
use bray_project::ProjectGraph;
use crossbeam_channel::Sender;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::model::{
    CancellationParams, DidChangeParams, DidCloseParams, DidOpenParams, DocumentParams,
    ReferenceParams, TextDocumentPositionParams,
};
use crate::protocol::{
    CONTENT_MODIFIED, INTERNAL_ERROR, INVALID_PARAMS, IncomingMessage, METHOD_NOT_FOUND,
    REQUEST_CANCELLED, read_messages, write_error, write_notification, write_result,
};
use crate::query::{Query, QueryError, execute};
use crate::workspace::{DocumentSnapshot, Workspace, WorkspaceError};

const PUBLISH_DIAGNOSTICS: &str = "textDocument/publishDiagnostics";

/// Bray language server for one validated project graph.
pub struct LanguageServer {
    workspace_root: PathBuf,
    graph: Arc<ProjectGraph>,
    worker_budget: WorkerBudget,
}

impl LanguageServer {
    /// Creates a server for one validated project graph.
    pub fn new(
        workspace_root: impl Into<PathBuf>,
        graph: Arc<ProjectGraph>,
        worker_budget: WorkerBudget,
    ) -> Self {
        Self {
            workspace_root: workspace_root.into(),
            graph,
            worker_budget,
        }
    }

    /// Serves Language Server Protocol messages until the peer exits or closes input.
    pub fn run(
        self,
        input: &mut (dyn Read + Send),
        output: &mut dyn Write,
    ) -> io::Result<()> {
        let mut workspace = Workspace::new(
            self.workspace_root,
            Arc::clone(&self.graph),
            self.worker_budget,
        );

        run_event_loop(
            input,
            output,
            &mut workspace,
            self.worker_budget.get(),
        )
    }
}

enum Event {
    Incoming(IncomingMessage),
    Completed(CompletedTask),
    ReaderClosed,
    ReaderFailed(io::Error),
}

struct PendingTask {
    id: Option<Value>,
    document: DocumentSnapshot,
    query: Query,
    cancellation: CancellationToken,
}

struct CompletedTask {
    id: Option<Value>,
    uri: String,
    revision: u64,
    cancellation: CancellationToken,
    result: Result<Value, QueryError>,
}

struct InFlightRequest {
    uri: String,
    revision: u64,
    cancellation: CancellationToken,
}

fn run_event_loop(
    input: &mut (dyn Read + Send),
    output: &mut dyn Write,
    workspace: &mut Workspace,
    worker_count: usize,
) -> io::Result<()> {
    let (sender, receiver) = crossbeam_channel::unbounded();

    std::thread::scope(|scope| {
        spawn_reader(scope, input, sender.clone());

        let mut pending = VecDeque::new();
        let mut requests = BTreeMap::new();
        let mut active = 0_usize;
        let mut reader_closed = false;

        loop {
            while active < worker_count {
                let Some(task) = pending.pop_front() else {
                    break;
                };

                active += 1;

                spawn_task(scope, task, sender.clone());
            }

            if reader_closed && active == 0 && pending.is_empty() {
                break;
            }

            let event = receiver.recv().map_err(|_| {
                io::Error::new(io::ErrorKind::BrokenPipe, "language_server_event_channel_closed")
            })?;

            match event {
                Event::Incoming(message) => {
                    handle_message(
                        message,
                        workspace,
                        output,
                        &mut pending,
                        &mut requests,
                        &mut reader_closed,
                    )?;
                }
                Event::Completed(task) => {
                    active = active.saturating_sub(1);

                    if let Some(id) = task.id.as_ref() {
                        requests.remove(&request_key(id));
                    }

                    publish_completed(task, workspace, output)?;
                }
                Event::ReaderClosed => reader_closed = true,
                Event::ReaderFailed(error) => return Err(error),
            }
        }

        Ok(())
    })
}

fn spawn_reader<'scope>(
    scope: &'scope std::thread::Scope<'scope, '_>,
    input: &'scope mut (dyn Read + Send),
    sender: Sender<Event>,
) {
    scope.spawn(move || {
        let result = read_messages(input, |message| {
            sender.send(Event::Incoming(message)).is_ok()
        });

        let event = match result {
            Ok(()) => Event::ReaderClosed,
            Err(error) => Event::ReaderFailed(error),
        };

        let _ = sender.send(event);
    });
}

fn spawn_task<'scope>(
    scope: &'scope std::thread::Scope<'scope, '_>,
    task: PendingTask,
    sender: Sender<Event>,
) {
    scope.spawn(move || {
        let result = execute(&task.document, task.query, &task.cancellation);

        let completed = CompletedTask {
            id: task.id,
            uri: task.document.uri,
            revision: task.document.revision,
            cancellation: task.cancellation,
            result,
        };

        let _ = sender.send(Event::Completed(completed));
    });
}

fn handle_message(
    message: IncomingMessage,
    workspace: &mut Workspace,
    output: &mut dyn Write,
    pending: &mut VecDeque<PendingTask>,
    requests: &mut BTreeMap<String, InFlightRequest>,
    reader_closed: &mut bool,
) -> io::Result<()> {
    match message {
        IncomingMessage::Request { id, method, params } => {
            if method == "initialize" {
                return write_result(output, id, initialize_result());
            }

            if method == "shutdown" {
                return write_result(output, id, Value::Null);
            }

            let task = match prepare_request(workspace, id.clone(), &method, params) {
                Ok(task) => task,
                Err(RequestPreparationError::InvalidParams) => {
                    return write_error(
                        output,
                        id,
                        INVALID_PARAMS,
                        "language_server_invalid_params",
                    );
                }
                Err(RequestPreparationError::MethodNotFound) => {
                    return write_error(
                        output,
                        id,
                        METHOD_NOT_FOUND,
                        "language_server_method_not_found",
                    );
                }
                Err(RequestPreparationError::Workspace(error)) => {
                    return write_error(output, id, INTERNAL_ERROR, error.as_str());
                }
            };

            requests.insert(
                request_key(&id),
                InFlightRequest {
                    uri: task.document.uri.clone(),
                    revision: task.document.revision,
                    cancellation: task.cancellation.clone(),
                },
            );

            pending.push_back(task);
        }
        IncomingMessage::Notification { method, params } => match method.as_str() {
            "initialized" => {}
            "exit" => {
                *reader_closed = true;

                for request in requests.values() {
                    request.cancellation.cancel();
                }
            }
            "$/cancelRequest" => {
                if let Ok(params) = serde_json::from_value::<CancellationParams>(params) {
                    if let Some(request) = requests.get(&request_key(&params.id)) {
                        request.cancellation.cancel();
                    }
                }
            }
            "textDocument/didOpen" => {
                let Ok(params) = serde_json::from_value::<DidOpenParams>(params) else {
                    return Ok(());
                };

                match workspace.open_document(
                    params.text_document.uri,
                    params.text_document.version,
                    params.text_document.text,
                ) {
                    Ok(document) => {
                        cancel_stale_requests(requests, &document.uri, document.revision);
                        pending.push_back(diagnostic_task(document));
                    }
                    Err(_) => {}
                }
            }
            "textDocument/didChange" => {
                let Ok(params) = serde_json::from_value::<DidChangeParams>(params) else {
                    return Ok(());
                };

                match workspace.change_document(
                    &params.text_document.uri,
                    params.text_document.version,
                    &params.content_changes,
                ) {
                    Ok(document) => {
                        cancel_stale_requests(requests, &document.uri, document.revision);
                        pending.push_back(diagnostic_task(document));
                    }
                    Err(_) => {}
                }
            }
            "textDocument/didClose" => {
                let Ok(params) = serde_json::from_value::<DidCloseParams>(params) else {
                    return Ok(());
                };

                let uri = params.text_document.uri;

                cancel_document_requests(requests, &uri);

                let _ = workspace.close_document(&uri);

                write_notification(
                    output,
                    PUBLISH_DIAGNOSTICS,
                    json!({
                        "uri": uri,
                        "diagnostics": [],
                    }),
                )?;
            }
            _ => {}
        },
        IncomingMessage::Response => {}
    }

    Ok(())
}

fn prepare_request(
    workspace: &Workspace,
    id: Value,
    method: &str,
    params: Value,
) -> Result<PendingTask, RequestPreparationError> {
    let (uri, query) = match method {
        "textDocument/hover" => {
            let params = parameters::<TextDocumentPositionParams>(params)?;

            (params.text_document.uri, Query::Hover(params.position))
        }
        "textDocument/definition" => {
            let params = parameters::<TextDocumentPositionParams>(params)?;

            (params.text_document.uri, Query::Definition(params.position))
        }
        "textDocument/references" => {
            let params = parameters::<ReferenceParams>(params)?;

            (
                params.position.text_document.uri,
                Query::References {
                    position: params.position.position,
                    include_declaration: params.context.include_declaration,
                },
            )
        }
        "textDocument/completion" => {
            let params = parameters::<TextDocumentPositionParams>(params)?;

            (params.text_document.uri, Query::Completion(params.position))
        }
        "textDocument/signatureHelp" => {
            let params = parameters::<TextDocumentPositionParams>(params)?;

            (
                params.text_document.uri,
                Query::SignatureHelp(params.position),
            )
        }
        "textDocument/documentSymbol" => {
            let params = parameters::<DocumentParams>(params)?;

            (params.text_document.uri, Query::DocumentSymbols)
        }
        "textDocument/semanticTokens/full" => {
            let params = parameters::<DocumentParams>(params)?;

            (params.text_document.uri, Query::SemanticTokens)
        }
        "textDocument/diagnostic" => {
            let params = parameters::<DocumentParams>(params)?;

            (params.text_document.uri, Query::Diagnostics)
        }
        _ => return Err(RequestPreparationError::MethodNotFound),
    };

    let document = workspace
        .document(&uri)
        .ok_or(RequestPreparationError::Workspace(
            WorkspaceError::DocumentNotFound,
        ))?;

    Ok(PendingTask {
        id: Some(id),
        document,
        query,
        cancellation: CancellationToken::new(),
    })
}

fn diagnostic_task(document: DocumentSnapshot) -> PendingTask {
    PendingTask {
        id: None,
        document,
        query: Query::Diagnostics,
        cancellation: CancellationToken::new(),
    }
}

fn publish_completed(
    task: CompletedTask,
    workspace: &Workspace,
    output: &mut dyn Write,
) -> io::Result<()> {
    if task.cancellation.is_cancelled() || matches!(task.result, Err(QueryError::Cancelled)) {
        if let Some(id) = task.id {
            return write_error(
                output,
                id,
                REQUEST_CANCELLED,
                "language_server_request_cancelled",
            );
        }

        return Ok(());
    }

    if !workspace.is_current(&task.uri, task.revision) {
        if let Some(id) = task.id {
            return write_error(
                output,
                id,
                CONTENT_MODIFIED,
                "language_server_content_modified",
            );
        }

        return Ok(());
    }

    match (task.id, task.result) {
        (Some(id), Ok(result)) => write_result(output, id, result),
        (Some(id), Err(_)) => {
            write_error(output, id, INTERNAL_ERROR, "language_server_query_failed")
        }
        (None, Ok(report)) => {
            let diagnostics = report
                .get("items")
                .cloned()
                .unwrap_or_else(|| json!([]));

            write_notification(
                output,
                PUBLISH_DIAGNOSTICS,
                json!({
                    "uri": task.uri,
                    "version": task.revision,
                    "diagnostics": diagnostics,
                }),
            )
        }
        (None, Err(_)) => Ok(()),
    }
}

fn cancel_stale_requests(
    requests: &BTreeMap<String, InFlightRequest>,
    uri: &str,
    revision: u64,
) {
    for request in requests
        .values()
        .filter(|request| request.uri == uri && request.revision < revision)
    {
        request.cancellation.cancel();
    }
}

fn cancel_document_requests(
    requests: &BTreeMap<String, InFlightRequest>,
    uri: &str,
) {
    for request in requests.values().filter(|request| request.uri == uri) {
        request.cancellation.cancel();
    }
}

fn request_key(id: &Value) -> String {
    id.to_string()
}

fn parameters<T: DeserializeOwned>(params: Value) -> Result<T, RequestPreparationError> {
    serde_json::from_value(params).map_err(|_| RequestPreparationError::InvalidParams)
}

enum RequestPreparationError {
    InvalidParams,
    MethodNotFound,
    Workspace(WorkspaceError),
}

fn initialize_result() -> Value {
    json!({
        "capabilities": {
            "textDocumentSync": {
                "openClose": true,
                "change": 2,
            },
            "diagnosticProvider": {
                "interFileDependencies": true,
                "workspaceDiagnostics": false,
            },
            "hoverProvider": true,
            "definitionProvider": true,
            "referencesProvider": true,
            "completionProvider": {
                "resolveProvider": false,
                "triggerCharacters": [".", ":"],
            },
            "signatureHelpProvider": {
                "triggerCharacters": ["(", ","],
            },
            "documentSymbolProvider": true,
            "semanticTokensProvider": {
                "legend": {
                    "tokenTypes": [
                        "namespace",
                        "type",
                        "class",
                        "enum",
                        "interface",
                        "struct",
                        "typeParameter",
                        "parameter",
                        "variable",
                        "property",
                        "enumMember",
                        "function",
                        "method",
                        "keyword",
                        "modifier",
                        "comment",
                        "string",
                        "number",
                        "operator"
                    ],
                    "tokenModifiers": [],
                },
                "full": true,
            },
        },
        "serverInfo": {
            "name": "Bray language server",
            "version": env!("CARGO_PKG_VERSION"),
        },
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::io::Cursor;
    use std::sync::Arc;

    use bray_compilation::WorkerBudget;
    use bray_project::load_project_graph;
    use serde_json::{Value, json};

    use super::{InFlightRequest, LanguageServer, cancel_stale_requests};
    use crate::protocol::read_messages;
    use crate::test_support::ProjectFixture;

    #[test]
    fn protocol_advertises_and_answers_every_editor_feature() {
        let fixture = ProjectFixture::new();

        let graph = load_project_graph(fixture.path())
            .unwrap_or_else(|error| panic!("test project should load: {error:?}"));

        let uri = bray_source::SourceOrigin::file_uri_from_path(&fixture.source)
            .unwrap_or_else(|error| panic!("test source URI should form: {error:?}"));

        let requests = [
            request(1, "initialize", json!({})),
            notification("initialized", json!({})),
            notification(
                "textDocument/didOpen",
                json!({
                    "textDocument": {
                        "uri": uri,
                        "languageId": "bray",
                        "version": 1,
                        "text": fixture.source_text,
                    }
                }),
            ),
            request(
                2,
                "textDocument/hover",
                position_params(&uri, 9, 23),
            ),
            request(
                3,
                "textDocument/definition",
                position_params(&uri, 9, 23),
            ),
            request(
                4,
                "textDocument/references",
                json!({
                    "textDocument": { "uri": uri },
                    "position": { "line": 9, "character": 23 },
                    "context": { "includeDeclaration": true },
                }),
            ),
            request(
                5,
                "textDocument/completion",
                position_params(&uri, 9, 4),
            ),
            request(
                6,
                "textDocument/signatureHelp",
                position_params(&uri, 9, 29),
            ),
            request(
                7,
                "textDocument/documentSymbol",
                json!({ "textDocument": { "uri": uri } }),
            ),
            request(
                8,
                "textDocument/semanticTokens/full",
                json!({ "textDocument": { "uri": uri } }),
            ),
            request(
                9,
                "textDocument/diagnostic",
                json!({ "textDocument": { "uri": uri } }),
            ),
            request(10, "shutdown", Value::Null),
            notification("exit", Value::Null),
        ];

        let input = framed(requests);
        let mut output = Vec::new();

        LanguageServer::new(
            fixture.path(),
            Arc::new(graph),
            WorkerBudget::new(2).unwrap_or_else(|error| {
                panic!("test worker budget should form: {error:?}")
            }),
        )
        .run(&mut Cursor::new(input), &mut output)
        .unwrap_or_else(|error| panic!("test server should run: {error:?}"));

        let mut responses = Vec::new();

        read_messages(&mut Cursor::new(output), |message| {
            if let crate::protocol::IncomingMessage::Response = message {
                responses.push(message);
            }

            true
        })
        .unwrap_or_else(|error| panic!("server output should parse: {error}"));

        assert_eq!(responses.len(), 10);
    }

    #[test]
    fn revisions_cancel_only_stale_requests_for_the_changed_document() {
        let stale = bray_compilation::CancellationToken::new();
        let current = bray_compilation::CancellationToken::new();
        let unrelated = bray_compilation::CancellationToken::new();

        let requests = BTreeMap::from([
            (
                String::from("stale"),
                InFlightRequest {
                    uri: String::from("file:///changed.bray"),
                    revision: 1,
                    cancellation: stale.clone(),
                },
            ),
            (
                String::from("current"),
                InFlightRequest {
                    uri: String::from("file:///changed.bray"),
                    revision: 2,
                    cancellation: current.clone(),
                },
            ),
            (
                String::from("unrelated"),
                InFlightRequest {
                    uri: String::from("file:///other.bray"),
                    revision: 1,
                    cancellation: unrelated.clone(),
                },
            ),
        ]);

        cancel_stale_requests(&requests, "file:///changed.bray", 2);

        assert!(stale.is_cancelled());
        assert!(!current.is_cancelled());
        assert!(!unrelated.is_cancelled());
    }

    fn request(id: u32, method: &str, params: Value) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        })
    }

    fn notification(method: &str, params: Value) -> Value {
        json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        })
    }

    fn position_params(uri: &str, line: u32, character: u32) -> Value {
        json!({
            "textDocument": { "uri": uri },
            "position": {
                "line": line,
                "character": character,
            },
        })
    }

    fn framed(messages: impl IntoIterator<Item = Value>) -> Vec<u8> {
        let mut output = Vec::new();

        for message in messages {
            let content = serde_json::to_vec(&message)
                .unwrap_or_else(|error| panic!("test message should serialize: {error}"));

            output.extend_from_slice(
                format!("Content-Length: {}\r\n\r\n", content.len()).as_bytes(),
            );

            output.extend_from_slice(&content);
        }

        output
    }
}
