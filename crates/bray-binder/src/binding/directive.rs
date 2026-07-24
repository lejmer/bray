use bray_declarations::SyntaxAnchor;
use bray_diagnostics::{
    Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticResult, SeverityKind,
};
use bray_source::SourceSpan;
use bray_symbols::{
    AnySymbolId, DeclarationExpressionTemplate, DirectiveArgumentName, DirectiveArgumentTemplate,
    DirectiveAttachment, DirectiveKind, DirectiveSurface, DirectiveTemplate, SymbolName,
};
use bray_syntax::{
    CallableDirectivesSyntax, DirectiveArgumentSyntax, SourceSyntaxNode, SyntaxCast, SyntaxKind,
    SyntaxNodeView, SyntaxWalkControl, SyntaxWalkEvent, syntax_node_view, walk_direct_child_nodes,
    walk_syntax_node,
};

use crate::{BinderFactError, BinderFactResult};

/// Binds one directive occurrence without applying directive-specific policy.
pub fn bind_directive_template(
    owner: AnySymbolId,
    attachment: DirectiveAttachment,
    syntax: SyntaxNodeView<'_>,
) -> BinderFactResult<DiagnosticResult<DirectiveTemplate>> {
    let kind = DirectiveKind::try_from_syntax_kind(syntax.kind())
        .ok_or(BinderFactError::DependencyUnavailable)?;

    let directive_syntax = SyntaxAnchor::from_node(&syntax);
    let mut arguments = Vec::new();
    let mut diagnostics = DiagnosticBag::new();
    let mut failed = false;

    walk_syntax_node(&syntax, |event| {
        let SyntaxWalkEvent::EnterNode(node) = event else {
            return SyntaxWalkControl::Continue;
        };

        if node.kind() != SyntaxKind::DirectiveArgument {
            return SyntaxWalkControl::Continue;
        }

        let Some(argument) = DirectiveArgumentSyntax::cast_from(node) else {
            failed = true;

            return SyntaxWalkControl::Stop;
        };

        if directive_argument_is_malformed(&argument) {
            diagnostics.add(malformed_directive_argument(&argument));
        }

        arguments.push(bind_directive_argument(owner, &argument));

        SyntaxWalkControl::SkipChildren
    });

    if failed {
        return Err(BinderFactError::DependencyUnavailable);
    }

    Ok(DiagnosticResult::new(
        DirectiveTemplate::new(kind, directive_syntax, attachment, arguments),
        diagnostics,
    ))
}

/// Binds directives attached to one callable type syntax occurrence.
pub fn bind_callable_type_directives(
    owner: AnySymbolId,
    syntax: SyntaxNodeView<'_>,
) -> BinderFactResult<DiagnosticResult<DirectiveSurface>> {
    let attachment = DirectiveAttachment::CallableType(SyntaxAnchor::from_node(&syntax));
    let mut directives = Vec::new();
    let mut diagnostics = DiagnosticBag::new();
    let mut failed = None;

    walk_direct_child_nodes(&syntax, |child| {
        let Some(callable_directives) = child.cast::<CallableDirectivesSyntax>() else {
            return SyntaxWalkControl::Continue;
        };

        for directive in callable_directives.abi_directives() {
            match bind_directive_template(owner, attachment, syntax_node_view(&directive)) {
                Ok(result) => {
                    let (directive, result_diagnostics) = result.into_parts();

                    diagnostics.add_range(result_diagnostics);
                    directives.push(directive);
                }
                Err(error) => {
                    failed = Some(error);

                    return SyntaxWalkControl::Stop;
                }
            }
        }

        SyntaxWalkControl::Stop
    });

    if let Some(error) = failed {
        return Err(error);
    }

    Ok(DiagnosticResult::new(
        DirectiveSurface::new(directives),
        diagnostics,
    ))
}

fn bind_directive_argument(
    owner: AnySymbolId,
    syntax: &DirectiveArgumentSyntax,
) -> DirectiveArgumentTemplate {
    let name = match (syntax.name_token(), syntax.equals_token()) {
        (None, None) => DirectiveArgumentName::Positional,
        (Some(name), Some(equals)) if !name.is_missing() && !equals.is_missing() => syntax
            .source()
            .text_slice(name.range())
            .and_then(SymbolName::try_new)
            .map_or(
                DirectiveArgumentName::Recovered,
                DirectiveArgumentName::Named,
            ),
        _ => DirectiveArgumentName::Recovered,
    };

    let expression =
        DeclarationExpressionTemplate::new(owner, SyntaxAnchor::from_node(&syntax.expression()));

    DirectiveArgumentTemplate::new(name, expression)
}

fn directive_argument_is_malformed(syntax: &DirectiveArgumentSyntax) -> bool {
    let name_is_malformed = match (syntax.name_token(), syntax.equals_token()) {
        (None, None) => false,
        (Some(name), Some(equals)) => name.is_missing() || equals.is_missing(),
        _ => true,
    };

    name_is_malformed || syntax.expression().is_recovered()
}

fn malformed_directive_argument(syntax: &DirectiveArgumentSyntax) -> Diagnostic {
    let span = SourceSpan::new(syntax.source().source_id(), syntax.full_range());

    Diagnostic::new(
        DiagnosticId::new(span.range().start().bytes()),
        DiagnosticKind::BindingMalformedDirectiveArgument,
        SeverityKind::Error,
    )
    .with_primary_span(span)
}
