use bray_bound_tree::BoundReferenceTarget;
use bray_compilation::{CancellationToken, QueryPriority, SemanticAvailability};
use bray_declarations::DeclarationKind;
use bray_diagnostics::{
    DiagnosticBag, DiagnosticLabelStyle, DiagnosticSuggestionApplicability, SeverityKind,
};
use bray_messages::DiagnosticRenderer;
use bray_source::{SourceSnapshot, SourceSpan, SourceStore};
use bray_symbols::SymbolKind;
use bray_syntax::SyntaxKind;

use crate::model::{
    CodeAction, CodeActionContext, Diagnostic, DiagnosticData, DiagnosticRelatedInformation,
    DocumentDiagnosticReport, Location, Range, SemanticTokens, TextEdit, WorkspaceEdit,
};
use crate::workspace::DocumentSnapshot;

use super::requests::{QueryError, lsp_range, source};

pub(super) fn semantic_tokens(
    document: &DocumentSnapshot,
    cancellation: &CancellationToken,
    multiline_support: bool,
) -> Result<SemanticTokens, QueryError> {
    let source = source(document)?;

    let syntax = document
        .compilation
        .source_unit_syntax(document.source_id)
        .ok_or(QueryError::MissingSyntax(document.source_id))?;

    let mut absolute = Vec::new();

    for token in syntax.source_unit().tokens() {
        if cancellation.is_cancelled() {
            return Err(QueryError::Cancelled);
        }

        for trivia in token
            .leading_trivia()
            .iter()
            .chain(token.trailing_trivia())
            .filter(|trivia| is_comment(trivia.kind()))
        {
            push_semantic_token(
                &mut absolute,
                source,
                trivia.range(),
                Some(15),
                multiline_support,
            );
        }

        if token.is_missing() || token.range().is_empty() {
            continue;
        }

        let token_type = if token.kind() == SyntaxKind::IdentifierToken {
            identifier_token_type(document, token.range().start(), cancellation)?
        } else {
            syntax_token_type(token.kind())
        };

        push_semantic_token(
            &mut absolute,
            source,
            token.range(),
            token_type,
            multiline_support,
        );
    }

    absolute.sort_unstable();
    absolute.dedup();

    Ok(SemanticTokens {
        data: encode_semantic_tokens(absolute),
    })
}

pub(super) fn diagnostics(
    document: &DocumentSnapshot,
    cancellation: &CancellationToken,
) -> Result<DocumentDiagnosticReport, QueryError> {
    let diagnostics = document_diagnostics(document, cancellation)?;

    Ok(DocumentDiagnosticReport {
        kind: "full",
        items: lsp_diagnostics(
            source(document)?,
            document.compilation.sources(),
            &diagnostics,
        ),
    })
}

pub(super) fn code_actions(
    document: &DocumentSnapshot,
    requested_range: Range,
    context: &CodeActionContext,
    cancellation: &CancellationToken,
) -> Result<Vec<CodeAction>, QueryError> {
    let source = source(document)?;
    let sources = document.compilation.sources();
    let diagnostics = document_diagnostics(document, cancellation)?;

    Ok(suggestion_actions(
        source,
        sources,
        &diagnostics,
        requested_range,
        context,
    ))
}

fn suggestion_actions(
    source: &SourceSnapshot,
    sources: &SourceStore,
    diagnostics: &DiagnosticBag,
    requested_range: Range,
    context: &CodeActionContext,
) -> Vec<CodeAction> {
    if context
        .only
        .as_ref()
        .is_some_and(|only| !only.iter().any(|kind| kind == "quickfix"))
    {
        return Vec::new();
    }

    let renderer = DiagnosticRenderer::english();
    let mut actions = Vec::new();

    for (diagnostic, diagnostic_ordinal) in diagnostics.iter().zip(0_u64..) {
        let Some(span) = diagnostic.primary_span() else {
            continue;
        };

        if span.source_id() != source.source_id() {
            continue;
        }

        let Some(range) = lsp_range(source, span.range()) else {
            continue;
        };

        if !ranges_overlap(range, requested_range) {
            continue;
        }

        let data = DiagnosticData {
            source_version: source.version().raw(),
            diagnostic_ordinal,
        };

        let Some(originating_diagnostic) = lsp_diagnostic(source, sources, diagnostic, data) else {
            continue;
        };

        if !context.diagnostics.iter().any(|candidate| {
            candidate.range == range
                && candidate.code == diagnostic.kind().code().raw()
                && candidate.data == Some(data)
        }) {
            continue;
        }

        let rendered = renderer.render(diagnostic);

        for suggestion in rendered.suggestions() {
            let Some(edit) = workspace_edit(sources, suggestion.edits()) else {
                continue;
            };

            actions.push(CodeAction {
                title: suggestion.message().to_owned(),
                kind: "quickfix",
                is_preferred: suggestion.applicability()
                    == DiagnosticSuggestionApplicability::MachineApplicable,
                diagnostics: vec![originating_diagnostic.clone()],
                edit,
            });
        }
    }

    actions
}

fn document_diagnostics(
    document: &DocumentSnapshot,
    cancellation: &CancellationToken,
) -> Result<DiagnosticBag, QueryError> {
    let compilation = document.compilation.as_ref();

    let mut diagnostics = compilation.diagnostics_for_source(
        document.source_id,
        cancellation,
        QueryPriority::Interactive,
    )?;

    for unit in compilation.bound_units_for_source(
        document.source_id,
        cancellation,
        QueryPriority::Interactive,
    )? {
        let unit_diagnostics = compilation.diagnostics_for_unit(
            unit.value().key().clone(),
            cancellation,
            QueryPriority::Interactive,
        )?;

        diagnostics = diagnostics.merged(&unit_diagnostics);
    }

    Ok(diagnostics)
}

fn encode_semantic_tokens(tokens: Vec<(u32, u32, u32, u32)>) -> Vec<u32> {
    let mut data = Vec::with_capacity(tokens.len() * 5);
    let mut previous_line = 0;
    let mut previous_character = 0;

    for (line, character, length, token_type) in tokens {
        let delta_line = line - previous_line;

        let delta_character = if delta_line == 0 {
            character - previous_character
        } else {
            character
        };

        data.extend([delta_line, delta_character, length, token_type, 0]);

        previous_line = line;
        previous_character = character;
    }

    data
}

fn lsp_diagnostics(
    source: &SourceSnapshot,
    sources: &SourceStore,
    diagnostics: &DiagnosticBag,
) -> Vec<Diagnostic> {
    diagnostics
        .iter()
        .zip(0_u64..)
        .filter_map(|(diagnostic, diagnostic_ordinal)| {
            lsp_diagnostic(
                source,
                sources,
                diagnostic,
                DiagnosticData {
                    source_version: source.version().raw(),
                    diagnostic_ordinal,
                },
            )
        })
        .collect()
}

fn lsp_diagnostic(
    source: &SourceSnapshot,
    sources: &SourceStore,
    diagnostic: &bray_diagnostics::Diagnostic,
    data: DiagnosticData,
) -> Option<Diagnostic> {
    let span = diagnostic.primary_span()?;

    if span.source_id() != source.source_id() {
        return None;
    }

    let range = lsp_range(source, span.range())?;
    let rendered = DiagnosticRenderer::english().render(diagnostic);

    let mut related_information = Vec::new();

    for related in rendered.related_locations() {
        let Some(location) = lsp_location(sources, related.span()) else {
            continue;
        };

        push_related_information(
            &mut related_information,
            DiagnosticRelatedInformation {
                location,
                message: related.message().to_owned(),
            },
        );
    }

    let mut message = rendered.message().to_owned();

    for label in rendered.labels() {
        if label.style() == DiagnosticLabelStyle::Primary || label.span() == span {
            append_distinct_message(&mut message, label.message());
        } else if let Some(location) = lsp_location(sources, label.span()) {
            push_related_information(
                &mut related_information,
                DiagnosticRelatedInformation {
                    location,
                    message: label.message().to_owned(),
                },
            );
        }
    }

    for note in rendered.notes() {
        append_distinct_message(&mut message, note.message());
    }

    for suggestion in rendered.suggestions() {
        append_distinct_message(&mut message, suggestion.message());
    }

    Some(Diagnostic {
        range,
        severity: diagnostic_severity(diagnostic.severity()),
        code: diagnostic.kind().code().raw(),
        source: "bray",
        message,
        data,
        related_information,
    })
}

fn push_related_information(
    related_information: &mut Vec<DiagnosticRelatedInformation>,
    information: DiagnosticRelatedInformation,
) {
    if !related_information.contains(&information) {
        related_information.push(information);
    }
}

fn append_distinct_message(message: &mut String, addition: &str) {
    if message.lines().any(|line| line == addition) {
        return;
    }

    message.push('\n');
    message.push_str(addition);
}

fn lsp_location(sources: &SourceStore, span: SourceSpan) -> Option<Location> {
    let source = sources.get(span.source_id())?;
    let uri = source.origin().document_uri().ok()??;
    let range = lsp_range(source, span.range())?;

    Some(Location { uri, range })
}

fn workspace_edit(
    sources: &SourceStore,
    edits: &[bray_diagnostics::DiagnosticSourceEdit],
) -> Option<WorkspaceEdit> {
    let mut changes = std::collections::BTreeMap::<String, Vec<TextEdit>>::new();

    for edit in edits {
        let location = lsp_location(sources, edit.span())?;

        changes.entry(location.uri).or_default().push(TextEdit {
            range: location.range,
            new_text: edit.replacement().to_owned(),
        });
    }

    if changes.is_empty() {
        return None;
    }

    Some(WorkspaceEdit { changes })
}

fn ranges_overlap(left: Range, right: Range) -> bool {
    match (left.start == left.end, right.start == right.end) {
        (true, true) => left.start == right.start,
        (true, false) => right.start <= left.start && left.start < right.end,
        (false, true) => left.start <= right.start && right.start < left.end,
        (false, false) => left.start < right.end && right.start < left.end,
    }
}

fn identifier_token_type(
    document: &DocumentSnapshot,
    position: bray_source::TextSize,
    cancellation: &CancellationToken,
) -> Result<Option<u32>, QueryError> {
    let compilation = document.compilation.as_ref();

    let symbol = compilation.symbol_at(
        document.source_id,
        position,
        cancellation,
        QueryPriority::Interactive,
    )?;

    let kind = match symbol {
        SemanticAvailability::Available(BoundReferenceTarget::Local(symbol))
        | SemanticAvailability::Recovered(Some(BoundReferenceTarget::Local(symbol))) => {
            Some(symbol.kind())
        }
        SemanticAvailability::Available(BoundReferenceTarget::Surface(symbol))
        | SemanticAvailability::Recovered(Some(BoundReferenceTarget::Surface(symbol))) => {
            Some(symbol.kind())
        }
        SemanticAvailability::Available(BoundReferenceTarget::TypeQualifier(_))
        | SemanticAvailability::Recovered(Some(BoundReferenceTarget::TypeQualifier(_)))
        | SemanticAvailability::Recovered(None)
        | SemanticAvailability::Unavailable => None,
    };

    if let Some(kind) = kind {
        return Ok(Some(symbol_token_type(kind)));
    }

    let declaration = compilation.declaration_at(
        document.source_id,
        position,
        cancellation,
        QueryPriority::Interactive,
    )?;

    let kind = match declaration {
        SemanticAvailability::Available(declaration)
        | SemanticAvailability::Recovered(Some(declaration)) => compilation
            .declaration_table()
            .declaration(declaration)
            .map(|declaration| declaration_token_type(declaration.kind())),
        SemanticAvailability::Recovered(None) | SemanticAvailability::Unavailable => None,
    };

    Ok(Some(kind.unwrap_or(8)))
}

const fn syntax_token_type(kind: SyntaxKind) -> Option<u32> {
    if kind.is_keyword() {
        return Some(13);
    }

    match kind {
        SyntaxKind::StringLiteralToken | SyntaxKind::CharacterLiteralToken => Some(16),
        SyntaxKind::DecimalIntegerLiteralToken
        | SyntaxKind::BinaryIntegerLiteralToken
        | SyntaxKind::HexadecimalIntegerLiteralToken
        | SyntaxKind::RealLiteralToken
        | SyntaxKind::ImaginaryLiteralToken
        | SyntaxKind::TupleElementIndexToken => Some(17),
        _ if kind.is_expression_operator() => Some(18),
        _ => None,
    }
}

const fn symbol_token_type(kind: SymbolKind) -> u32 {
    match kind {
        SymbolKind::Module => 0,
        SymbolKind::Struct => 5,
        SymbolKind::Union => 3,
        SymbolKind::Trait => 4,
        SymbolKind::GenericTypeParameter => 6,
        SymbolKind::CallableParameter
        | SymbolKind::PredicateParameter
        | SymbolKind::ReceiverParameter
        | SymbolKind::AnonymousCallableParameter => 7,
        SymbolKind::StructField
        | SymbolKind::UnionPayloadField
        | SymbolKind::InherentTypeMember
        | SymbolKind::TraitTypeMember
        | SymbolKind::TraitTypeFulfillment => 9,
        SymbolKind::UnionVariant => 10,
        SymbolKind::Function
        | SymbolKind::Predicate
        | SymbolKind::CallableContract
        | SymbolKind::CallableOverload
        | SymbolKind::AnonymousCallable => 11,
        SymbolKind::TypeCallableMember
        | SymbolKind::Constructor
        | SymbolKind::Finalizer
        | SymbolKind::Destructor
        | SymbolKind::ScopeEnter
        | SymbolKind::ScopeExit
        | SymbolKind::TraitCallableMember
        | SymbolKind::TraitCallableFulfillment
        | SymbolKind::TraitPredicateMember
        | SymbolKind::TraitPredicateFulfillment
        | SymbolKind::TraitFinalizerRequirement
        | SymbolKind::TraitDestructorRequirement
        | SymbolKind::TraitScopeEnterRequirement
        | SymbolKind::TraitScopeExitRequirement
        | SymbolKind::TraitScopeEnterFulfillment
        | SymbolKind::TraitScopeExitFulfillment => 12,
        _ => 8,
    }
}

const fn declaration_token_type(kind: DeclarationKind) -> u32 {
    match kind {
        DeclarationKind::Module => 0,
        DeclarationKind::Struct => 5,
        DeclarationKind::Union => 3,
        DeclarationKind::Trait => 4,
        DeclarationKind::GenericTypeParameter => 6,
        DeclarationKind::CallableParameter | DeclarationKind::PredicateParameter => 7,
        DeclarationKind::StructField
        | DeclarationKind::UnionPayloadField
        | DeclarationKind::TraitTypeMember
        | DeclarationKind::ImplementationTypeMemberBinding => 9,
        DeclarationKind::UnionVariant => 10,
        DeclarationKind::Function
        | DeclarationKind::Predicate
        | DeclarationKind::CallableContract
        | DeclarationKind::CallableOverload => 11,
        DeclarationKind::TypeCallableMember
        | DeclarationKind::TypeConstructorMember
        | DeclarationKind::FinalizerMember
        | DeclarationKind::DestructorMember
        | DeclarationKind::ScopeEnterMember
        | DeclarationKind::ScopeExitMember
        | DeclarationKind::TraitCallableMember
        | DeclarationKind::TraitPredicateMember
        | DeclarationKind::TraitFinalizerRequirement
        | DeclarationKind::TraitDestructorRequirement
        | DeclarationKind::TraitScopeEnterRequirement
        | DeclarationKind::TraitScopeExitRequirement => 12,
        _ => 8,
    }
}

fn push_semantic_token(
    tokens: &mut Vec<(u32, u32, u32, u32)>,
    source: &SourceSnapshot,
    text_range: bray_source::TextRange,
    token_type: Option<u32>,
    multiline_support: bool,
) {
    let Some(token_type) = token_type else {
        return;
    };

    let Some(range) = lsp_range(source, text_range) else {
        return;
    };

    if range.start.line == range.end.line {
        tokens.push((
            range.start.line,
            range.start.character,
            range.end.character.saturating_sub(range.start.character),
            token_type,
        ));

        return;
    }

    let Some(text) = source.text_slice(text_range) else {
        return;
    };

    if multiline_support {
        tokens.push((
            range.start.line,
            range.start.character,
            u32::try_from(text.encode_utf16().count()).unwrap_or(u32::MAX),
            token_type,
        ));

        return;
    }

    for (line_offset, segment) in text.split('\n').enumerate() {
        let segment = segment.strip_suffix('\r').unwrap_or(segment);
        let length = u32::try_from(segment.encode_utf16().count()).unwrap_or(u32::MAX);

        if length == 0 {
            continue;
        }

        tokens.push((
            range
                .start
                .line
                .saturating_add(u32::try_from(line_offset).unwrap_or(u32::MAX)),
            if line_offset == 0 {
                range.start.character
            } else {
                0
            },
            length,
            token_type,
        ));
    }
}

const fn is_comment(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::LineCommentTrivia
            | SyntaxKind::BlockCommentTrivia
            | SyntaxKind::DocumentationLineCommentTrivia
            | SyntaxKind::DocumentationBlockCommentTrivia
    )
}

const fn diagnostic_severity(severity: SeverityKind) -> u32 {
    match severity {
        SeverityKind::Error => 1,
        SeverityKind::Warning => 2,
        SeverityKind::Note => 3,
        SeverityKind::Help => 4,
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_source::{SourceId, SourceIdentity, SourceOrigin, SourceStore, SourceVersion};

    use super::{lsp_diagnostics, ranges_overlap, suggestion_actions};
    use crate::model::{CodeActionContext, CodeActionContextDiagnostic, Position, Range};

    #[test]
    fn parser_diagnostic_publishes_labels_and_safe_code_actions() {
        let mut sources = SourceStore::new();
        let text = "module main; extern func main()\nusing core;";

        sources
            .insert(
                SourceIdentity::new(0),
                SourceOrigin::lsp_document("file:///main.bray"),
                SourceVersion::new(1),
                text,
            )
            .unwrap_or_else(|error| panic!("test LSP source should load: {error:?}"));

        let source = sources
            .get(SourceId::new(0))
            .unwrap_or_else(|| panic!("test source should exist"));

        let parsed = bray_parser::parse_source_unit(source);
        let published = lsp_diagnostics(source, &sources, parsed.diagnostics());

        let diagnostic = published
            .iter()
            .find(|diagnostic| diagnostic.code == DiagnosticKind::SyntaxExpectedToken.code().raw())
            .unwrap_or_else(|| panic!("parser should publish the missing semicolon"));

        let source_diagnostic = parsed
            .diagnostics()
            .by_kind(DiagnosticKind::SyntaxExpectedToken)
            .next()
            .unwrap_or_else(|| panic!("parser diagnostic should remain available"));

        let rendered = bray_messages::DiagnosticRenderer::english().render(source_diagnostic);

        let primary_label = rendered
            .labels()
            .iter()
            .find(|label| label.style() == bray_diagnostics::DiagnosticLabelStyle::Primary)
            .unwrap_or_else(|| panic!("parser diagnostic should retain its primary label"));

        let context = CodeActionContext {
            diagnostics: vec![CodeActionContextDiagnostic {
                range: diagnostic.range,
                code: diagnostic.code,
                data: Some(diagnostic.data),
            }],
            only: Some(vec!["quickfix".to_owned()]),
        };

        assert_ne!(primary_label.message(), rendered.message());

        assert!(
            diagnostic
                .message
                .lines()
                .any(|line| line == primary_label.message())
        );

        let actions = suggestion_actions(
            source,
            &sources,
            parsed.diagnostics(),
            diagnostic.range,
            &context,
        );

        assert_eq!(actions.len(), 1);
        assert!(!actions[0].is_preferred);
        assert_eq!(actions[0].kind, "quickfix");
        assert_eq!(actions[0].diagnostics, [diagnostic.clone()]);

        assert_eq!(
            actions[0].edit.changes["file:///main.bray"][0].new_text,
            ";"
        );

        let excluded = suggestion_actions(
            source,
            &sources,
            parsed.diagnostics(),
            diagnostic.range,
            &CodeActionContext {
                diagnostics: context.diagnostics,
                only: Some(vec!["source.organizeImports".to_owned()]),
            },
        );

        assert!(excluded.is_empty());
    }

    #[test]
    fn declaration_diagnostic_publishes_the_actual_related_origin() {
        let mut sources = SourceStore::new();
        let text = "module main; struct Point {} struct Point {}";

        sources
            .insert(
                SourceIdentity::new(0),
                SourceOrigin::lsp_document("file:///main.bray"),
                SourceVersion::new(1),
                text,
            )
            .unwrap_or_else(|error| panic!("test LSP source should load: {error:?}"));

        let source = sources
            .get(SourceId::new(0))
            .unwrap_or_else(|| panic!("test source should exist"));

        let parsed = bray_parser::parse_source_unit(source);
        let discovered = bray_declarations::discover_source_unit_declarations(parsed.source_unit());
        let declarations = bray_declarations::merge_declaration_chunks([&discovered]);

        bray_testing::assert_goal_state_diagnostic_kind(
            declarations.diagnostics(),
            DiagnosticKind::DeclarationDuplicateName,
        );

        let source_diagnostic = declarations
            .diagnostics()
            .by_kind(DiagnosticKind::DeclarationDuplicateName)
            .next()
            .unwrap_or_else(|| panic!("declaration producer should report the duplicate name"));

        let [related] = source_diagnostic.related_locations() else {
            panic!("declaration producer should retain the first declaration");
        };

        let rendered = bray_messages::DiagnosticRenderer::english().render(source_diagnostic);
        let published = lsp_diagnostics(source, &sources, declarations.diagnostics());

        let diagnostic = published
            .iter()
            .find(|diagnostic| {
                diagnostic.code == DiagnosticKind::DeclarationDuplicateName.code().raw()
            })
            .unwrap_or_else(|| panic!("LSP should publish the duplicate declaration"));

        assert_eq!(diagnostic.related_information.len(), 1);

        assert_eq!(
            diagnostic.related_information[0].message,
            rendered.related_locations()[0].message()
        );

        assert_eq!(
            diagnostic.related_information[0].location.range,
            super::lsp_range(source, related.span().range())
                .unwrap_or_else(|| panic!("related declaration range should map to LSP"))
        );
    }

    #[test]
    fn stale_or_unmatched_code_action_context_never_offers_edits() {
        let mut sources = SourceStore::new();

        sources
            .insert(
                SourceIdentity::new(0),
                SourceOrigin::lsp_document("file:///main.bray"),
                SourceVersion::new(4),
                "module main; extern func main()\nusing core;",
            )
            .unwrap_or_else(|error| panic!("test LSP source should load: {error:?}"));

        let source = sources
            .get(SourceId::new(0))
            .unwrap_or_else(|| panic!("test source should exist"));

        let parsed = bray_parser::parse_source_unit(source);
        let published = lsp_diagnostics(source, &sources, parsed.diagnostics());

        let [diagnostic] = published.as_slice() else {
            panic!("parser should publish one missing semicolon diagnostic");
        };

        let stale = CodeActionContext {
            diagnostics: vec![CodeActionContextDiagnostic {
                range: diagnostic.range,
                code: diagnostic.code,
                data: Some(crate::model::DiagnosticData {
                    source_version: diagnostic.data.source_version - 1,
                    diagnostic_ordinal: diagnostic.data.diagnostic_ordinal,
                }),
            }],
            only: Some(vec!["quickfix".to_owned()]),
        };

        assert!(
            suggestion_actions(
                source,
                &sources,
                parsed.diagnostics(),
                diagnostic.range,
                &stale,
            )
            .is_empty()
        );
    }

    #[test]
    fn repeated_secondary_label_information_is_published_once() {
        let mut sources = SourceStore::new();

        sources
            .insert(
                SourceIdentity::new(0),
                SourceOrigin::lsp_document("file:///main.bray"),
                SourceVersion::new(1),
                "main value",
            )
            .unwrap_or_else(|error| panic!("test LSP source should load: {error:?}"));

        let source = sources
            .get(SourceId::new(0))
            .unwrap_or_else(|| panic!("test source should exist"));

        let primary = bray_source::SourceSpan::new(
            SourceId::new(0),
            bray_source::TextRange::new(bray_source::TextSize::ZERO, bray_source::TextSize::new(4)),
        );

        let secondary = bray_source::SourceSpan::new(
            SourceId::new(0),
            bray_source::TextRange::new(
                bray_source::TextSize::new(5),
                bray_source::TextSize::new(10),
            ),
        );

        let label = bray_diagnostics::DiagnosticLabel::secondary(
            bray_diagnostics::DiagnosticLabelKind::InvalidIdentifier,
            secondary,
        );

        let diagnostic = bray_diagnostics::Diagnostic::new(
            bray_diagnostics::DiagnosticId::new(0),
            DiagnosticKind::SyntaxExpectedExpression,
            bray_diagnostics::SeverityKind::Error,
        )
        .with_primary_span(primary)
        .with_arg(bray_diagnostics::DiagnosticArg::expected_syntax_kind(
            bray_syntax::SyntaxKind::Expression,
        ))
        .with_label(label.clone())
        .with_label(label);

        let published = lsp_diagnostics(
            source,
            &sources,
            &bray_diagnostics::DiagnosticBag::single(diagnostic),
        );

        assert_eq!(published[0].related_information.len(), 1);

        assert_eq!(
            published[0].related_information[0].message,
            "invalid identifier"
        );
    }

    #[test]
    fn code_action_ranges_use_half_open_overlap_with_explicit_insertions() {
        let position = |character| Position { line: 0, character };

        let range = |start, end| Range {
            start: position(start),
            end: position(end),
        };

        assert!(!ranges_overlap(range(0, 2), range(2, 4)));
        assert!(ranges_overlap(range(0, 2), range(1, 3)));
        assert!(ranges_overlap(range(2, 2), range(0, 3)));
        assert!(ranges_overlap(range(2, 2), range(2, 2)));
        assert!(ranges_overlap(range(0, 3), range(2, 2)));
        assert!(!ranges_overlap(range(3, 3), range(0, 3)));
    }
}
