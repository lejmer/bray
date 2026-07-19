use bray_diagnostics::{DiagnosticBag, DiagnosticKind, DiagnosticResult};
use bray_symbols::CallableAbi;
use bray_syntax::{AbiDirectiveSyntax, CallableDirectivesSyntax, SourceSyntaxNode, SyntaxKind};

use super::core::{TypeExpressionBinder, token_text};
use super::diagnostic::source_diagnostic;

/// Binds the callable ABI selected by an ordered directive surface.
///
/// Invalid or repeated directives retain diagnostics and recover to a stable ABI value.
pub fn bind_callable_abi(
    directives: impl IntoIterator<Item = AbiDirectiveSyntax>,
) -> DiagnosticResult<CallableAbi> {
    let mut directives = directives.into_iter();
    let mut diagnostics = DiagnosticBag::new();

    let Some(directive) = directives.next() else {
        return DiagnosticResult::without_diagnostics(CallableAbi::Bray);
    };

    let abi = parse_abi(&directive).unwrap_or_else(|| {
        diagnostics.add(source_diagnostic(
            &directive,
            DiagnosticKind::BindingInvalidCallableAbi,
        ));

        CallableAbi::Bray
    });

    for duplicate in directives {
        diagnostics.add(source_diagnostic(
            &duplicate,
            DiagnosticKind::BindingDuplicateCallableAbi,
        ));
    }

    DiagnosticResult::new(abi, diagnostics)
}

fn parse_abi(directive: &AbiDirectiveSyntax) -> Option<CallableAbi> {
    let arguments = directive.directive_argument_list();
    let mut arguments = arguments.directive_arguments();

    let mode = arguments.next()?;

    if mode.name_token().is_some() || arguments.next().is_some() {
        return None;
    }

    let expression = mode.expression();
    let mut tokens = expression.tokens().filter(|token| !token.is_missing());

    let token = tokens.next()?;

    if token.kind() != SyntaxKind::IdentifierToken || tokens.next().is_some() {
        return None;
    }

    match token_text(expression.source(), &token) {
        Some("c") => Some(CallableAbi::C),
        Some("system") => Some(CallableAbi::System),
        Some(_) | None => None,
    }
}

impl TypeExpressionBinder<'_> {
    pub(super) fn bind_optional_callable_abi(
        &mut self,
        directives: Option<&CallableDirectivesSyntax>,
    ) -> CallableAbi {
        let result = match directives {
            Some(directives) => bind_callable_abi(directives.abi_directives()),
            None => DiagnosticResult::without_diagnostics(CallableAbi::Bray),
        };

        let (abi, diagnostics) = result.into_parts();

        self.diagnostics.add_range(diagnostics);

        abi
    }
}
