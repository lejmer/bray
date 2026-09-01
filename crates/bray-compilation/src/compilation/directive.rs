use bray_declarations::SyntaxAnchor;
use bray_source::SourceSnapshot;
use bray_symbols::{DirectiveKind, DirectiveSurface, DirectiveTemplate, SymbolName};
use bray_syntax::{ExpressionSyntax, SyntaxTree};

use super::Compilation;
use super::{ForeignDataKind, ForeignQueryContext, ForeignQueryFailure};
use crate::fact::FactQueryError;

pub(super) fn first_directive(
    directives: &DirectiveSurface,
    kind: DirectiveKind,
) -> Option<&DirectiveTemplate> {
    directives
        .directives()
        .iter()
        .find(|directive| directive.kind() == kind)
}

pub(super) fn directives_of_kind(
    directives: &DirectiveSurface,
    kind: DirectiveKind,
) -> Vec<DirectiveTemplate> {
    directives
        .directives()
        .iter()
        .filter(|directive| directive.kind() == kind)
        .cloned()
        .collect()
}

pub(super) fn bare_directive_argument_name(
    syntax: &SyntaxTree,
    source: &SourceSnapshot,
    anchor: SyntaxAnchor,
) -> Option<SymbolName> {
    let expression = anchor.find_descendant::<ExpressionSyntax>(syntax)?;
    let access = expression.primary_expression()?.access_expression()?;
    let identifier = access.identifier_token()?;

    if identifier.range() != expression.full_range() {
        return None;
    }

    identifier.text(source.text()).and_then(SymbolName::try_new)
}

pub(super) fn directive_source_text<'compilation>(
    compilation: &'compilation Compilation,
    directive: &DirectiveTemplate,
) -> Result<&'compilation str, FactQueryError> {
    let syntax = directive.syntax();

    compilation
        .source(syntax.source_id())
        .and_then(|source| source.text_slice(syntax.full_range()))
        .ok_or_else(|| {
            ForeignQueryFailure::missing(
                ForeignQueryContext::Directive(syntax),
                ForeignDataKind::SourceText,
            )
            .into()
        })
}
