use bray_bound_tree::BoundReferenceTarget;
use bray_compilation::{CancellationToken, QueryPriority, SemanticAvailability};
use bray_declarations::DeclarationKind;
use bray_diagnostics::{DiagnosticBag, SeverityKind};
use bray_messages::DiagnosticRenderer;
use bray_source::SourceSnapshot;
use bray_symbols::SymbolKind;
use bray_syntax::SyntaxKind;

use crate::model::{Diagnostic, DocumentDiagnosticReport, SemanticTokens};
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
        .ok_or(QueryError::Compiler)?;

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

    Ok(DocumentDiagnosticReport {
        kind: "full",
        items: lsp_diagnostics(source(document)?, &diagnostics),
    })
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

fn lsp_diagnostics(source: &SourceSnapshot, diagnostics: &DiagnosticBag) -> Vec<Diagnostic> {
    let renderer = DiagnosticRenderer::english();

    diagnostics
        .iter()
        .filter_map(|diagnostic| {
            let span = diagnostic.primary_span()?;

            if span.source_id() != source.source_id() {
                return None;
            }

            let range = lsp_range(source, span.range())?;
            let rendered = renderer.render(diagnostic);

            Some(Diagnostic {
                range,
                severity: diagnostic_severity(diagnostic.severity()),
                code: diagnostic.kind().code().raw(),
                source: "bray",
                message: rendered.message().to_owned(),
            })
        })
        .collect()
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
        SemanticAvailability::Recovered(None) | SemanticAvailability::Unavailable => None,
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
