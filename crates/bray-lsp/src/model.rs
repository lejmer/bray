use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InitializeParams {
    pub(crate) locale: Option<String>,
    pub(crate) capabilities: ClientCapabilities,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClientCapabilities {
    pub(crate) text_document: Option<TextDocumentClientCapabilities>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TextDocumentClientCapabilities {
    pub(crate) semantic_tokens: Option<SemanticTokensClientCapabilities>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SemanticTokensClientCapabilities {
    #[serde(default)]
    pub(crate) multiline_token_support: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub(crate) struct Position {
    pub(crate) line: u32,
    pub(crate) character: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct Range {
    pub(crate) start: Position,
    pub(crate) end: Position,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TextDocumentIdentifier {
    pub(crate) uri: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VersionedTextDocumentIdentifier {
    pub(crate) uri: String,
    pub(crate) version: i64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TextDocumentItem {
    pub(crate) uri: String,
    pub(crate) version: i64,
    pub(crate) text: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TextDocumentPositionParams {
    pub(crate) text_document: TextDocumentIdentifier,
    pub(crate) position: Position,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodeActionParams {
    pub(crate) text_document: TextDocumentIdentifier,
    pub(crate) range: Range,
    pub(crate) context: CodeActionContext,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodeActionContext {
    pub(crate) diagnostics: Vec<CodeActionContextDiagnostic>,
    pub(crate) only: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct CodeActionContextDiagnostic {
    pub(crate) range: Range,
    pub(crate) code: u32,
    pub(crate) data: Option<DiagnosticData>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiagnosticData {
    pub(crate) source_version: u64,
    pub(crate) diagnostic_ordinal: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DidOpenParams {
    pub(crate) text_document: TextDocumentItem,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DidChangeParams {
    pub(crate) text_document: VersionedTextDocumentIdentifier,
    pub(crate) content_changes: Vec<ContentChange>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct DidCloseParams {
    #[serde(rename = "textDocument")]
    pub(crate) text_document: TextDocumentIdentifier,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ContentChange {
    pub(crate) range: Option<Range>,
    pub(crate) text: String,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ReferenceParams {
    #[serde(flatten)]
    pub(crate) position: TextDocumentPositionParams,
    pub(crate) context: ReferenceContext,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReferenceContext {
    pub(crate) include_declaration: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct DocumentParams {
    #[serde(rename = "textDocument")]
    pub(crate) text_document: TextDocumentIdentifier,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct CancellationParams {
    pub(crate) id: serde_json::Value,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct Location {
    pub(crate) uri: String,
    pub(crate) range: Range,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct Hover {
    pub(crate) contents: MarkupContent,
    pub(crate) range: Option<Range>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MarkupContent {
    pub(crate) kind: &'static str,
    pub(crate) value: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CompletionItem {
    pub(crate) label: String,
    pub(crate) kind: u32,
    pub(crate) detail: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SignatureHelp {
    pub(crate) signatures: Vec<SignatureInformation>,
    pub(crate) active_signature: u32,
    pub(crate) active_parameter: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct SignatureInformation {
    pub(crate) label: String,
    pub(crate) parameters: Vec<ParameterInformation>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct ParameterInformation {
    pub(crate) label: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DocumentSymbol {
    pub(crate) name: String,
    pub(crate) detail: String,
    pub(crate) kind: u32,
    pub(crate) range: Range,
    pub(crate) selection_range: Range,
    pub(crate) children: Vec<DocumentSymbol>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct SemanticTokens {
    pub(crate) data: Vec<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct Diagnostic {
    pub(crate) range: Range,
    pub(crate) severity: u32,
    pub(crate) code: u32,
    pub(crate) source: &'static str,
    pub(crate) message: String,
    pub(crate) data: DiagnosticData,
    #[serde(rename = "relatedInformation", skip_serializing_if = "Vec::is_empty")]
    pub(crate) related_information: Vec<DiagnosticRelatedInformation>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct DiagnosticRelatedInformation {
    pub(crate) location: Location,
    pub(crate) message: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodeAction {
    pub(crate) title: String,
    pub(crate) kind: &'static str,
    pub(crate) is_preferred: bool,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) edit: WorkspaceEdit,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct WorkspaceEdit {
    pub(crate) changes: std::collections::BTreeMap<String, Vec<TextEdit>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct TextEdit {
    pub(crate) range: Range,
    #[serde(rename = "newText")]
    pub(crate) new_text: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DocumentDiagnosticReport {
    pub(crate) kind: &'static str,
    pub(crate) items: Vec<Diagnostic>,
}
