use std::collections::{BTreeMap, VecDeque};
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::Arc;

use bray_compilation::{CancellationToken, WorkerBudget};
use bray_messages::{DiagnosticLocale, LanguageServerMessage, LanguageServerMessageRenderer};
use bray_project::ProjectGraph;
use bray_target::TargetIdentity;
use crossbeam_channel::Sender;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::model::{
    CancellationParams, DidChangeParams, DidCloseParams, DidOpenParams, DocumentParams,
    InitializeParams, ReferenceParams, TextDocumentPositionParams,
};
use crate::protocol::{
    CONTENT_MODIFIED, INTERNAL_ERROR, INVALID_PARAMS, IncomingMessage, METHOD_NOT_FOUND,
    REQUEST_CANCELLED, read_messages, write_error, write_notification, write_result,
};
use crate::query::{Query, QueryError, execute};
use crate::workspace::{
    CompilationRevision, DocumentSnapshot, Workspace, WorkspaceError, WorkspaceUpdate,
};

const PUBLISH_DIAGNOSTICS: &str = "textDocument/publishDiagnostics";
const SHOW_MESSAGE: &str = "window/showMessage";

/// Bray language server for one validated project graph.
pub struct LanguageServer {
    workspace_root: PathBuf,
    graph: Arc<ProjectGraph>,
    target: TargetIdentity,
    worker_budget: WorkerBudget,
}

impl LanguageServer {
    /// Creates a server for one validated project graph.
    pub fn new(
        workspace_root: impl Into<PathBuf>,
        graph: Arc<ProjectGraph>,
        target: TargetIdentity,
        worker_budget: WorkerBudget,
    ) -> Self {
        Self {
            workspace_root: workspace_root.into(),
            graph,
            target,
            worker_budget,
        }
    }

    /// Serves Language Server Protocol messages until the peer exits or closes input.
    pub fn run(self, input: Box<dyn Read + Send>, output: &mut dyn Write) -> io::Result<()> {
        let mut workspace = Workspace::new(
            self.workspace_root,
            Arc::clone(&self.graph),
            self.target,
            self.worker_budget,
        );

        run_event_loop(input, output, &mut workspace, self.worker_budget.get())
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
    diagnostic_id: Option<u64>,
    document: DocumentSnapshot,
    query: Query,
    cancellation: CancellationToken,
}

struct CompletedTask {
    id: Option<Value>,
    diagnostic_id: Option<u64>,
    uri: String,
    revision: u64,
    compilation_revision: CompilationRevision,
    cancellation: CancellationToken,
    result: Result<Value, QueryError>,
}

struct InFlightRequest {
    uri: String,
    compilation_revision: CompilationRevision,
    cancellation: CancellationToken,
}

struct InFlightDiagnostics {
    id: u64,
    compilation_revision: CompilationRevision,
    cancellation: CancellationToken,
}

#[derive(Clone, Copy)]
struct ClientConfiguration {
    renderer: LanguageServerMessageRenderer,
    multiline_semantic_tokens: bool,
}

impl Default for ClientConfiguration {
    fn default() -> Self {
        Self {
            renderer: LanguageServerMessageRenderer::english(),
            multiline_semantic_tokens: false,
        }
    }
}

fn run_event_loop(
    input: Box<dyn Read + Send>,
    output: &mut dyn Write,
    workspace: &mut Workspace,
    worker_count: usize,
) -> io::Result<()> {
    let (sender, receiver) = crossbeam_channel::unbounded();

    spawn_reader(input, sender.clone());

    let mut pending = VecDeque::new();
    let mut requests = BTreeMap::new();
    let mut diagnostics = BTreeMap::new();
    let mut active = 0_usize;
    let mut reader_closed = false;
    let mut next_diagnostic_id = 0_u64;
    let mut client = ClientConfiguration::default();

    loop {
        while active < worker_count {
            let Some(task) = pending.pop_front() else {
                break;
            };

            active += 1;

            spawn_task(task, sender.clone());
        }

        if reader_closed && active == 0 && pending.is_empty() {
            break;
        }

        let event = receiver.recv().map_err(|_| {
            io::Error::new(
                io::ErrorKind::BrokenPipe,
                "language_server_event_channel_closed",
            )
        })?;

        match event {
            Event::Incoming(message) => {
                handle_message(
                    message,
                    workspace,
                    output,
                    &mut pending,
                    &mut requests,
                    &mut diagnostics,
                    &mut reader_closed,
                    &mut next_diagnostic_id,
                    &mut client,
                )?;
            }
            Event::Completed(task) => {
                active = active.saturating_sub(1);

                if let Some(id) = task.id.as_ref() {
                    requests.remove(&request_key(id));
                }

                if let Some(id) = task.diagnostic_id
                    && diagnostics
                        .get(&task.uri)
                        .is_some_and(|diagnostic| diagnostic.id == id)
                {
                    diagnostics.remove(&task.uri);
                }

                publish_completed(task, workspace, output, client.renderer)?;
            }
            Event::ReaderClosed => reader_closed = true,
            Event::ReaderFailed(error) => return Err(error),
        }
    }

    Ok(())
}

fn spawn_reader(mut input: Box<dyn Read + Send>, sender: Sender<Event>) {
    std::thread::spawn(move || {
        let result = read_messages(input.as_mut(), |message| {
            let exits = matches!(
                &message,
                IncomingMessage::Notification { method, .. } if method == "exit"
            );

            sender.send(Event::Incoming(message)).is_ok() && !exits
        });

        let event = match result {
            Ok(()) => Event::ReaderClosed,
            Err(error) => Event::ReaderFailed(error),
        };

        let _ = sender.send(event);
    });
}

fn spawn_task(task: PendingTask, sender: Sender<Event>) {
    std::thread::spawn(move || {
        let result = execute(&task.document, task.query, &task.cancellation);

        let completed = CompletedTask {
            id: task.id,
            diagnostic_id: task.diagnostic_id,
            uri: task.document.uri,
            revision: task.document.revision,
            compilation_revision: task.document.compilation_revision,
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
    diagnostics: &mut BTreeMap<String, InFlightDiagnostics>,
    reader_closed: &mut bool,
    next_diagnostic_id: &mut u64,
    client: &mut ClientConfiguration,
) -> io::Result<()> {
    match message {
        IncomingMessage::Request { id, method, params } => {
            if method == "initialize" {
                let params = serde_json::from_value::<InitializeParams>(params).unwrap_or_default();

                *client = client_configuration(&params);

                return write_result(output, id, initialize_result());
            }

            if method == "shutdown" {
                return write_result(output, id, Value::Null);
            }

            let task = match prepare_request(
                workspace,
                id.clone(),
                &method,
                params,
                client.multiline_semantic_tokens,
            ) {
                Ok(task) => task,
                Err(RequestPreparationError::InvalidParams) => {
                    return write_error(
                        output,
                        id,
                        INVALID_PARAMS,
                        LanguageServerMessage::InvalidParams,
                        client.renderer,
                    );
                }
                Err(RequestPreparationError::MethodNotFound) => {
                    return write_error(
                        output,
                        id,
                        METHOD_NOT_FOUND,
                        LanguageServerMessage::MethodNotFound,
                        client.renderer,
                    );
                }
                Err(RequestPreparationError::Workspace(error)) => {
                    return write_error(
                        output,
                        id,
                        INTERNAL_ERROR,
                        error.message(),
                        client.renderer,
                    );
                }
            };

            requests.insert(
                request_key(&id),
                InFlightRequest {
                    uri: task.document.uri.clone(),
                    compilation_revision: task.document.compilation_revision.clone(),
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

                for diagnostic in diagnostics.values() {
                    diagnostic.cancellation.cancel();
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
                    Ok(update) => {
                        queue_workspace_update(
                            update,
                            workspace,
                            pending,
                            requests,
                            diagnostics,
                            next_diagnostic_id,
                        );
                    }
                    Err(error) => publish_workspace_error(output, error, client.renderer)?,
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
                    Ok(update) => {
                        queue_workspace_update(
                            update,
                            workspace,
                            pending,
                            requests,
                            diagnostics,
                            next_diagnostic_id,
                        );
                    }
                    Err(error) => publish_workspace_error(output, error, client.renderer)?,
                }
            }
            "textDocument/didClose" => {
                let Ok(params) = serde_json::from_value::<DidCloseParams>(params) else {
                    return Ok(());
                };

                let uri = params.text_document.uri;

                cancel_document_requests(requests, &uri);
                cancel_document_diagnostics(diagnostics, &uri);

                match workspace.close_document(&uri) {
                    Ok(affected) => {
                        cancel_stale_work(workspace, requests, diagnostics);

                        for document in affected {
                            queue_diagnostics(document, pending, diagnostics, next_diagnostic_id);
                        }
                    }
                    Err(error) => publish_workspace_error(output, error, client.renderer)?,
                }

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
    multiline_semantic_tokens: bool,
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

            (
                params.text_document.uri,
                Query::SemanticTokens {
                    multiline_support: multiline_semantic_tokens,
                },
            )
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
        diagnostic_id: None,
        document,
        query,
        cancellation: CancellationToken::new(),
    })
}

fn diagnostic_task(document: DocumentSnapshot, id: u64) -> PendingTask {
    PendingTask {
        id: None,
        diagnostic_id: Some(id),
        document,
        query: Query::Diagnostics,
        cancellation: CancellationToken::new(),
    }
}

fn publish_completed(
    task: CompletedTask,
    workspace: &Workspace,
    output: &mut dyn Write,
    renderer: LanguageServerMessageRenderer,
) -> io::Result<()> {
    if task.cancellation.is_cancelled() || matches!(task.result, Err(QueryError::Cancelled)) {
        if let Some(id) = task.id {
            return write_error(
                output,
                id,
                REQUEST_CANCELLED,
                LanguageServerMessage::RequestCancelled,
                renderer,
            );
        }

        return Ok(());
    }

    if !workspace.is_current(&task.compilation_revision) {
        if let Some(id) = task.id {
            return write_error(
                output,
                id,
                CONTENT_MODIFIED,
                LanguageServerMessage::ContentModified,
                renderer,
            );
        }

        return Ok(());
    }

    match (task.id, task.result) {
        (Some(id), Ok(result)) => write_result(output, id, result),
        (Some(id), Err(_)) => write_error(
            output,
            id,
            INTERNAL_ERROR,
            LanguageServerMessage::QueryFailed,
            renderer,
        ),
        (None, Ok(report)) => {
            let diagnostics = report.get("items").cloned().unwrap_or_else(|| json!([]));

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

fn cancel_stale_work(
    workspace: &Workspace,
    requests: &BTreeMap<String, InFlightRequest>,
    diagnostics: &BTreeMap<String, InFlightDiagnostics>,
) {
    for request in requests
        .values()
        .filter(|request| !workspace.is_current(&request.compilation_revision))
    {
        request.cancellation.cancel();
    }

    for diagnostic in diagnostics
        .values()
        .filter(|diagnostic| !workspace.is_current(&diagnostic.compilation_revision))
    {
        diagnostic.cancellation.cancel();
    }
}

fn cancel_document_requests(requests: &BTreeMap<String, InFlightRequest>, uri: &str) {
    for request in requests.values().filter(|request| request.uri == uri) {
        request.cancellation.cancel();
    }
}

fn cancel_document_diagnostics(diagnostics: &mut BTreeMap<String, InFlightDiagnostics>, uri: &str) {
    if let Some(diagnostic) = diagnostics.remove(uri) {
        diagnostic.cancellation.cancel();
    }
}

fn queue_workspace_update(
    update: WorkspaceUpdate,
    workspace: &Workspace,
    pending: &mut VecDeque<PendingTask>,
    requests: &BTreeMap<String, InFlightRequest>,
    diagnostics: &mut BTreeMap<String, InFlightDiagnostics>,
    next_diagnostic_id: &mut u64,
) {
    debug_assert!(
        update
            .affected
            .iter()
            .any(|document| document.uri == update.current.uri)
    );

    cancel_stale_work(workspace, requests, diagnostics);

    for document in update.affected {
        queue_diagnostics(document, pending, diagnostics, next_diagnostic_id);
    }
}

fn queue_diagnostics(
    document: DocumentSnapshot,
    pending: &mut VecDeque<PendingTask>,
    diagnostics: &mut BTreeMap<String, InFlightDiagnostics>,
    next_diagnostic_id: &mut u64,
) {
    if let Some(previous) = diagnostics.remove(&document.uri) {
        previous.cancellation.cancel();
    }

    *next_diagnostic_id = next_diagnostic_id.saturating_add(1);

    let id = *next_diagnostic_id;
    let cancellation = CancellationToken::new();

    diagnostics.insert(
        document.uri.clone(),
        InFlightDiagnostics {
            id,
            compilation_revision: document.compilation_revision.clone(),
            cancellation: cancellation.clone(),
        },
    );

    let mut task = diagnostic_task(document, id);
    task.cancellation = cancellation;

    pending.push_back(task);
}

fn publish_workspace_error(
    output: &mut dyn Write,
    error: WorkspaceError,
    renderer: LanguageServerMessageRenderer,
) -> io::Result<()> {
    write_notification(
        output,
        SHOW_MESSAGE,
        json!({
            "type": 1,
            "message": renderer.render(error.message()),
        }),
    )
}

fn client_configuration(params: &InitializeParams) -> ClientConfiguration {
    let locale = match params.locale.as_deref() {
        Some(_) | None => DiagnosticLocale::English,
    };

    let multiline_semantic_tokens = params
        .capabilities
        .text_document
        .as_ref()
        .and_then(|capabilities| capabilities.semantic_tokens)
        .is_some_and(|capabilities| capabilities.multiline_token_support);

    ClientConfiguration {
        renderer: LanguageServerMessageRenderer::new(locale),
        multiline_semantic_tokens,
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
    use std::collections::{BTreeMap, VecDeque};
    use std::io::{self, Cursor, Read};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::Duration;

    use bray_compilation::WorkerBudget;
    use bray_project::load_project_graph;
    use serde_json::{Value, json};

    use super::{InFlightDiagnostics, LanguageServer, PendingTask, queue_diagnostics};
    use crate::protocol::read_messages;
    use crate::test_support::ProjectFixture;
    use crate::workspace::Workspace;

    #[test]
    fn protocol_advertises_and_answers_every_editor_feature() {
        let fixture = ProjectFixture::new();

        let graph = load_project_graph(fixture.path())
            .unwrap_or_else(|error| panic!("test project should load: {error:?}"));

        let target = graph
            .targets()
            .first()
            .unwrap_or_else(|| panic!("test target should exist"))
            .identity()
            .clone();

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
            request(2, "textDocument/hover", position_params(&uri, 9, 23)),
            request(3, "textDocument/definition", position_params(&uri, 9, 23)),
            request(
                4,
                "textDocument/references",
                json!({
                    "textDocument": { "uri": uri },
                    "position": { "line": 9, "character": 23 },
                    "context": { "includeDeclaration": true },
                }),
            ),
            request(5, "textDocument/completion", position_params(&uri, 9, 4)),
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
            target,
            WorkerBudget::new(2)
                .unwrap_or_else(|error| panic!("test worker budget should form: {error:?}")),
        )
        .run(Box::new(Cursor::new(input)), &mut output)
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
    fn exit_stops_before_reading_from_a_blocked_transport_again() {
        let fixture = ProjectFixture::new();

        let graph = load_project_graph(fixture.path())
            .unwrap_or_else(|error| panic!("test project should load: {error:?}"));

        let target = graph
            .targets()
            .first()
            .unwrap_or_else(|| panic!("test target should exist"))
            .identity()
            .clone();

        let input = framed([
            request(1, "initialize", json!({})),
            request(2, "shutdown", Value::Null),
            notification("exit", Value::Null),
        ]);

        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let read_after_exit = Arc::new(AtomicBool::new(false));

        let reader = BlockingAfterInput {
            input: Cursor::new(input),
            gate: Arc::clone(&gate),
            read_after_exit: Arc::clone(&read_after_exit),
        };

        let (sender, receiver) = std::sync::mpsc::channel();

        let root = fixture.path().to_owned();

        std::thread::spawn(move || {
            let mut output = Vec::new();

            let result = LanguageServer::new(root, Arc::new(graph), target, WorkerBudget::serial())
                .run(Box::new(reader), &mut output);

            let _ = sender.send(result);
        });

        let result = receiver.recv_timeout(Duration::from_secs(2));

        if result.is_err() {
            let (released, changed) = &*gate;

            let mut released = released.lock().unwrap_or_else(|error| error.into_inner());

            *released = true;
            changed.notify_all();
        }

        assert!(
            result.is_ok(),
            "exit should not wait for another transport read"
        );

        assert!(!read_after_exit.load(Ordering::Acquire));
    }

    #[test]
    fn queuing_new_diagnostics_cancels_the_superseded_task() {
        let fixture = ProjectFixture::new();

        let graph = load_project_graph(fixture.path())
            .unwrap_or_else(|error| panic!("test project should load: {error:?}"));

        let target = graph
            .targets()
            .first()
            .unwrap_or_else(|| panic!("test target should exist"))
            .identity()
            .clone();

        let uri = bray_source::SourceOrigin::file_uri_from_path(&fixture.source)
            .unwrap_or_else(|error| panic!("test source URI should form: {error:?}"));

        let mut workspace = Workspace::new(
            fixture.path(),
            Arc::new(graph),
            target,
            WorkerBudget::serial(),
        );

        let document = workspace
            .open_document(uri, 1, fixture.source_text.to_owned())
            .unwrap_or_else(|error| panic!("test document should open: {error:?}"))
            .current;

        let mut pending = VecDeque::<PendingTask>::new();
        let mut diagnostics = BTreeMap::<String, InFlightDiagnostics>::new();
        let mut next = 0_u64;

        queue_diagnostics(document.clone(), &mut pending, &mut diagnostics, &mut next);

        let first = diagnostics
            .get(&document.uri)
            .unwrap_or_else(|| panic!("first diagnostic task should be tracked"))
            .cancellation
            .clone();

        queue_diagnostics(document, &mut pending, &mut diagnostics, &mut next);

        assert!(first.is_cancelled());
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(pending.len(), 2);
    }

    struct BlockingAfterInput {
        input: Cursor<Vec<u8>>,
        gate: Arc<(Mutex<bool>, Condvar)>,
        read_after_exit: Arc<AtomicBool>,
    }

    impl Read for BlockingAfterInput {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let read = self.input.read(buffer)?;

            if read != 0 {
                return Ok(read);
            }

            self.read_after_exit.store(true, Ordering::Release);

            let (released, changed) = &*self.gate;

            let mut released = released.lock().unwrap_or_else(|error| error.into_inner());

            while !*released {
                released = changed
                    .wait(released)
                    .unwrap_or_else(|error| error.into_inner());
            }

            Ok(0)
        }
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

            output
                .extend_from_slice(format!("Content-Length: {}\r\n\r\n", content.len()).as_bytes());

            output.extend_from_slice(&content);
        }

        output
    }
}
