use super::super::semantic::{code_actions, diagnostics, semantic_tokens};
use super::QueryError;
use super::completion::completion;
use super::navigation::{definition, document_symbols, hover, references};
use super::signature::signature_help;
use crate::model::{CodeActionContext, Position, Range};
use crate::workspace::DocumentSnapshot;
use bray_compilation::CancellationToken;

pub(crate) enum Query {
    CodeActions {
        range: Range,
        context: CodeActionContext,
    },
    Completion(Position),
    Definition(Position),
    Diagnostics,
    DocumentSymbols,
    Hover(Position),
    References {
        position: Position,
        include_declaration: bool,
    },
    SemanticTokens {
        multiline_support: bool,
    },
    SignatureHelp(Position),
}

pub(crate) fn execute(
    document: &DocumentSnapshot,
    query: Query,
    cancellation: &CancellationToken,
) -> Result<serde_json::Value, QueryError> {
    let result = match query {
        Query::CodeActions { range, context } => {
            serde_json::to_value(code_actions(document, range, &context, cancellation)?)
        }
        Query::Completion(position) => {
            serde_json::to_value(completion(document, position, cancellation)?)
        }
        Query::Definition(position) => {
            serde_json::to_value(definition(document, position, cancellation)?)
        }
        Query::Diagnostics => serde_json::to_value(diagnostics(document, cancellation)?),
        Query::DocumentSymbols => serde_json::to_value(document_symbols(document, cancellation)?),
        Query::Hover(position) => serde_json::to_value(hover(document, position, cancellation)?),
        Query::References {
            position,
            include_declaration,
        } => serde_json::to_value(references(
            document,
            position,
            include_declaration,
            cancellation,
        )?),
        Query::SemanticTokens { multiline_support } => {
            serde_json::to_value(semantic_tokens(document, cancellation, multiline_support)?)
        }
        Query::SignatureHelp(position) => {
            serde_json::to_value(signature_help(document, position, cancellation)?)
        }
    };

    result.map_err(QueryError::Serialization)
}
