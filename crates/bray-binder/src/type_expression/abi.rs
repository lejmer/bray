use bray_symbols::CallableAbi;
use bray_syntax::{AbiDirectiveSyntax, CallableDirectivesSyntax, SourceSyntaxNode, SyntaxKind};

use super::binding::{TypeExpressionBinder, token_text};
use crate::{BinderFactError, BinderFactResult};

/// Binds the callable ABI selected by an ordered directive surface.
pub fn bind_callable_abi(
    directives: impl IntoIterator<Item = AbiDirectiveSyntax>,
) -> BinderFactResult<CallableAbi> {
    // TODO(BRA-202): Report malformed and duplicate directives through checker diagnostics.
    let mut directives = directives.into_iter();

    let Some(directive) = directives.next() else {
        return Ok(CallableAbi::Bray);
    };

    if directives.next().is_some() {
        return Err(BinderFactError::DependencyUnavailable);
    }

    let arguments = directive.directive_argument_list();
    let mut arguments = arguments.directive_arguments();

    let Some(mode) = arguments.next() else {
        return Err(BinderFactError::DependencyUnavailable);
    };

    if mode.name_token().is_some() {
        return Err(BinderFactError::DependencyUnavailable);
    }

    let expression = mode.expression();
    let mut tokens = expression.tokens().filter(|token| !token.is_missing());

    let Some(token) = tokens.next() else {
        return Err(BinderFactError::DependencyUnavailable);
    };

    if token.kind() != SyntaxKind::IdentifierToken || tokens.next().is_some() {
        return Err(BinderFactError::DependencyUnavailable);
    }

    match token_text(expression.source(), &token) {
        Some("c") => Ok(CallableAbi::C),
        Some("system") => Ok(CallableAbi::System),
        Some(_) | None => Err(BinderFactError::DependencyUnavailable),
    }
}

impl TypeExpressionBinder<'_> {
    pub(super) fn bind_optional_callable_abi(
        &self,
        directives: Option<&CallableDirectivesSyntax>,
    ) -> BinderFactResult<CallableAbi> {
        match directives {
            Some(directives) => bind_callable_abi(directives.abi_directives()),
            None => Ok(CallableAbi::Bray),
        }
    }
}
