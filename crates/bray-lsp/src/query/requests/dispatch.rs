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

#[cfg(test)]
mod tests {
    use bray_compilation::{Compilation, CompilationOptions, CompilationRequest, WorkerBudget};
    use bray_source::{SourceId, SourceIdentity, SourceInput, SourceVersion};
    use bray_symbols::{PackageIdentity, ProductIdentity, ProductKind};

    use super::{
        QueryError, completion, definition, document_symbols, hover, references, signature_help,
    };
    use crate::model::Position;
    use crate::query::semantic::{diagnostics, semantic_tokens};
    use crate::workspace::{CompilationRevision, DocumentSnapshot};

    #[test]
    fn hover_reports_source_and_type_information() {
        let document = feature_document();
        let cancellation = bray_compilation::CancellationToken::new();

        let hover = hover(
            &document,
            Position {
                line: 9,
                character: 23,
            },
            &cancellation,
        )
        .unwrap_or_else(|error| panic!("hover should complete: {error:?}"));

        assert!(
            hover
                .as_ref()
                .is_some_and(|hover| !hover.contents.value.is_empty()),
            "hover should contain semantic information: {hover:?}"
        );
    }

    #[test]
    fn definition_finds_the_referenced_declaration() {
        let document = feature_document();
        let cancellation = bray_compilation::CancellationToken::new();

        let definition = definition(
            &document,
            Position {
                line: 9,
                character: 23,
            },
            &cancellation,
        )
        .unwrap_or_else(|error| panic!("definition should complete: {error:?}"));

        assert!(
            definition.as_ref().is_some_and(|location| {
                location.uri == "file:///test.bray" && location.range.start != location.range.end
            }),
            "definition should identify source syntax: {definition:?}"
        );
    }

    #[test]
    fn references_find_declarations_and_uses() {
        let document = feature_document();
        let cancellation = bray_compilation::CancellationToken::new();

        let references = references(
            &document,
            Position {
                line: 9,
                character: 23,
            },
            true,
            &cancellation,
        )
        .unwrap_or_else(|error| panic!("references should complete: {error:?}"));

        assert_eq!(references.len(), 2, "unexpected references: {references:?}");

        assert!(references.iter().all(|location| {
            location.uri == "file:///test.bray" && location.range.start != location.range.end
        }));

        assert_ne!(references[0].range, references[1].range);
    }

    #[test]
    fn completion_includes_surface_and_visible_local_names() {
        let document = feature_document();
        let cancellation = bray_compilation::CancellationToken::new();

        let completion = completion(
            &document,
            Position {
                line: 10,
                character: 4,
            },
            &cancellation,
        )
        .unwrap_or_else(|error| panic!("completion should complete: {error:?}"));

        assert!(completion.iter().any(|item| item.label == "add"));
        assert!(completion.iter().any(|item| item.label == "value"));
        assert!(!completion.iter().any(|item| item.label == "left"));
        assert!(!completion.iter().any(|item| item.label == "right"));
    }

    #[test]
    fn signature_help_reports_callable_parameters() {
        let document = feature_document();
        let cancellation = bray_compilation::CancellationToken::new();

        let signature = signature_help(
            &document,
            Position {
                line: 9,
                character: 29,
            },
            &cancellation,
        )
        .unwrap_or_else(|error| panic!("signature help should complete: {error:?}"));

        assert!(
            signature.as_ref().is_some_and(|signature| {
                signature.signatures.len() == 1
                    && signature.signatures[0].parameters.len() == 2
                    && signature.active_parameter == 1
            }),
            "unexpected signature help: {signature:?}"
        );
    }

    #[test]
    fn document_symbols_preserve_declaration_hierarchy() {
        let document = feature_document();
        let cancellation = bray_compilation::CancellationToken::new();

        let symbols = document_symbols(&document, &cancellation)
            .unwrap_or_else(|error| panic!("document symbols should complete: {error:?}"));

        assert!(contains_symbol(&symbols, "add"));
    }

    #[test]
    fn semantic_tokens_cover_keywords_and_comments() {
        let document = feature_document();
        let cancellation = bray_compilation::CancellationToken::new();

        let tokens = semantic_tokens(&document, &cancellation, false)
            .unwrap_or_else(|error| panic!("semantic tokens should complete: {error:?}"));

        assert!(!tokens.data.is_empty());
        assert!(tokens.data.chunks_exact(5).any(|token| token[3] == 7));
        assert!(tokens.data.chunks_exact(5).any(|token| token[3] == 8));
        assert!(tokens.data.chunks_exact(5).any(|token| token[3] == 11));
        assert!(tokens.data.chunks_exact(5).any(|token| token[3] == 13));
        assert!(tokens.data.chunks_exact(5).any(|token| token[3] == 15));
    }

    #[test]
    fn multiline_tokens_are_split_when_the_client_does_not_support_them() {
        let document = document(concat!("module app;\n", "/* first\n", "   second */\n",));

        let cancellation = bray_compilation::CancellationToken::new();

        let tokens = semantic_tokens(&document, &cancellation, false)
            .unwrap_or_else(|error| panic!("semantic tokens should complete: {error:?}"));

        let comment_lines = absolute_tokens(&tokens.data)
            .into_iter()
            .filter(|token| token.3 == 15)
            .map(|token| token.0)
            .collect::<Vec<_>>();

        assert_eq!(comment_lines, [1, 2]);
    }

    #[test]
    fn cancelled_queries_stop_before_publishing_results() {
        let document = document("module app;\n");
        let cancellation = bray_compilation::CancellationToken::new();

        cancellation.cancel();

        assert!(matches!(
            semantic_tokens(&document, &cancellation, false),
            Err(QueryError::Cancelled)
        ));
    }

    #[test]
    fn diagnostics_render_localized_messages() {
        let document = document("module app\n");
        let cancellation = bray_compilation::CancellationToken::new();

        let diagnostics = diagnostics(&document, &cancellation)
            .unwrap_or_else(|error| panic!("diagnostics should complete: {error:?}"));

        assert!(
            diagnostics.items.iter().any(|diagnostic| {
                diagnostic.code == 3003
                    && diagnostic.message
                        == "unexpected end of file while expecting semicolon token\nsource ends here\ninsert semicolon token"
            }),
            "missing localized syntax diagnostic: {:?}",
            diagnostics.items
        );
    }

    fn feature_document() -> DocumentSnapshot {
        document(concat!(
            "module app;\n",
            "\n",
            "func add(left: i32, right: i32) -> i32\n",
            "{\n",
            "    return left + right;\n",
            "}\n",
            "\n",
            "func main()\n",
            "{\n",
            "    let value: i32 = add(1, 2);\n",
            "    value;\n",
            "}\n",
            "// trailing comment\n",
        ))
    }

    fn document(text: &str) -> DocumentSnapshot {
        let Some(package) = PackageIdentity::try_new("test.package") else {
            panic!("test package identity should be valid");
        };

        let request = CompilationRequest::with_options(
            package.clone(),
            vec![SourceInput::lsp_open_document(
                SourceIdentity::new(1),
                "file:///test.bray",
                SourceVersion::new(1),
                text,
            )],
            CompilationOptions::new(
                WorkerBudget::serial(),
                ProductKind::Library,
                bray_compilation::SelectedTarget::baseline(),
            ),
        );

        let compilation = Compilation::load(request)
            .unwrap_or_else(|error| panic!("test compilation should load: {error:?}"));

        let product = ProductIdentity::try_new(package, "test")
            .unwrap_or_else(|| panic!("test product identity should be valid"));

        let target = bray_compilation::SelectedTarget::baseline()
            .profile()
            .identity()
            .clone();

        DocumentSnapshot {
            compilation: std::sync::Arc::new(compilation),
            source_id: SourceId::new(0),
            uri: String::from("file:///test.bray"),
            revision: 1,
            compilation_revision: CompilationRevision {
                product,
                target,
                generation: 1,
            },
        }
    }

    fn contains_symbol(symbols: &[crate::model::DocumentSymbol], name: &str) -> bool {
        symbols
            .iter()
            .any(|symbol| symbol.name == name || contains_symbol(&symbol.children, name))
    }

    fn absolute_tokens(data: &[u32]) -> Vec<(u32, u32, u32, u32)> {
        let mut line = 0_u32;
        let mut character = 0_u32;
        let mut tokens = Vec::new();

        for token in data.chunks_exact(5) {
            line = line.saturating_add(token[0]);

            character = if token[0] == 0 {
                character.saturating_add(token[1])
            } else {
                token[1]
            };

            tokens.push((line, character, token[2], token[3]));
        }

        tokens
    }
}
