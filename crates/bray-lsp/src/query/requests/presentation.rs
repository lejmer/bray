use bray_bound_tree::BoundSourceAnchor;
use bray_declarations::{DeclarationKind, DeclarationName, DeclarationRecord};
use bray_source::{LineIndex, LspPosition, SourceSnapshot, TextRange, TextSize};
use bray_symbols::SymbolKind;

use crate::model::{Location, Position, Range};
use crate::workspace::DocumentSnapshot;

use super::QueryError;

pub(super) fn location_for_anchor(
    compilation: &bray_compilation::Compilation,
    anchor: BoundSourceAnchor,
) -> Option<Location> {
    let source = source_for_anchor(compilation, anchor).ok()?;
    let uri = source.origin().document_uri().ok().flatten()?;
    let range = lsp_range(source, anchor.syntax().full_range())?;

    Some(Location { uri, range })
}

fn source_for_anchor(
    compilation: &bray_compilation::Compilation,
    anchor: BoundSourceAnchor,
) -> Result<&SourceSnapshot, QueryError> {
    compilation
        .source(anchor.syntax().source_id())
        .filter(|source| source.version() == anchor.source_version())
        .ok_or(QueryError::MissingSource {
            source: anchor.syntax().source_id(),
            expected_version: Some(anchor.source_version().raw()),
        })
}

pub(in crate::query) fn source(document: &DocumentSnapshot) -> Result<&SourceSnapshot, QueryError> {
    document
        .compilation
        .source(document.source_id)
        .ok_or(QueryError::MissingSource {
            source: document.source_id,
            expected_version: None,
        })
}

pub(in crate::query) fn lsp_range(source: &SourceSnapshot, range: TextRange) -> Option<Range> {
    let index = LineIndex::new(source.text()).ok()?;
    let start = index.lsp_position(range.start())?;
    let end = index.lsp_position(range.end())?;

    Some(Range {
        start: position(start),
        end: position(end),
    })
}

const fn position(position: LspPosition) -> Position {
    Position {
        line: position.line(),
        character: position.character(),
    }
}

pub(super) fn declaration_label(
    source: &SourceSnapshot,
    declaration: &DeclarationRecord,
) -> String {
    let text = source
        .text_slice(declaration.full_range())
        .unwrap_or_default()
        .trim();

    text.split(['{', ';'])
        .next()
        .unwrap_or(text)
        .trim()
        .to_owned()
}

pub(super) fn declaration_name(name: Option<&DeclarationName>) -> Option<String> {
    match name? {
        DeclarationName::Identifier(name) => Some(name.clone()),
        DeclarationName::Keyword(kind) => Some(
            kind.as_str()
                .strip_suffix("_keyword")
                .unwrap_or(kind.as_str())
                .to_owned(),
        ),
        DeclarationName::Path(path) => Some(path.dotted()),
        DeclarationName::Implementation(name) => {
            let subject = name.subject().dotted();

            Some(match name.trait_path() {
                Some(trait_path) => format!("{subject}: {}", trait_path.dotted()),
                None => subject,
            })
        }
    }
}

pub(super) const fn symbol_completion_kind(kind: SymbolKind) -> u32 {
    match kind {
        SymbolKind::Module => 9,
        SymbolKind::Struct | SymbolKind::Union | SymbolKind::Trait => 7,
        SymbolKind::Function
        | SymbolKind::Predicate
        | SymbolKind::CallableContract
        | SymbolKind::CallableOverload
        | SymbolKind::AnonymousCallable => 3,
        SymbolKind::CallableParameter
        | SymbolKind::PredicateParameter
        | SymbolKind::ReceiverParameter
        | SymbolKind::AnonymousCallableParameter
        | SymbolKind::GenericTypeParameter
        | SymbolKind::GenericConstParameter => 5,
        SymbolKind::Constant
        | SymbolKind::TraitConstantMember
        | SymbolKind::TraitConstantFulfillment
        | SymbolKind::LocalConstant
        | SymbolKind::StructField
        | SymbolKind::UnionPayloadField => 21,
        _ => 6,
    }
}

pub(super) fn range_covers_cursor(range: TextRange, offset: TextSize) -> bool {
    range.contains(offset) || range.end() == offset
}

pub(super) const fn declaration_symbol_kind(kind: DeclarationKind) -> u32 {
    match kind {
        DeclarationKind::Module => 2,
        DeclarationKind::Struct => 23,
        DeclarationKind::Union => 10,
        DeclarationKind::Trait => 11,
        DeclarationKind::Function
        | DeclarationKind::Predicate
        | DeclarationKind::CallableContract => 12,
        DeclarationKind::TypeCallableMember
        | DeclarationKind::TraitCallableMember
        | DeclarationKind::TypeConstructorMember
        | DeclarationKind::FinalizerMember
        | DeclarationKind::DestructorMember
        | DeclarationKind::ScopeEnterMember
        | DeclarationKind::ScopeExitMember => 6,
        DeclarationKind::StructField | DeclarationKind::UnionPayloadField => 8,
        DeclarationKind::Constant | DeclarationKind::TraitConstantMember => 14,
        _ => 13,
    }
}
