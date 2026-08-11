use bray_diagnostics::{
    DiagnosticBag, DiagnosticKind, DiagnosticNote, DiagnosticNoteKind,
    DiagnosticRelatedLocation, DiagnosticRelatedLocationKind, DiagnosticResult,
};
use bray_source::SourceSpan;
use bray_symbols::CallableAbi;
use bray_syntax::{AbiDirectiveSyntax, CallableDirectivesSyntax, SourceSyntaxNode, SyntaxKind};

use super::core::{TypeExpressionBinder, token_text};
use super::diagnostic::callable_abi_diagnostic;

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

    let first_span = SourceSpan::new(directive.source().source_id(), directive.full_range());

    let abi = parse_abi(&directive).unwrap_or_else(|| {
        diagnostics.add(
            callable_abi_diagnostic(&directive, DiagnosticKind::BindingInvalidCallableAbi)
                .with_note(DiagnosticNote::new(
                    DiagnosticNoteKind::CallableAbiDirectiveMustNameSupportedAbi,
                )),
        );

        CallableAbi::Bray
    });

    for duplicate in directives {
        diagnostics.add(
            callable_abi_diagnostic(&duplicate, DiagnosticKind::BindingDuplicateCallableAbi)
                .with_related_location(DiagnosticRelatedLocation::new(
                    DiagnosticRelatedLocationKind::FirstDirective,
                    first_span,
                )),
        );
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

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_testing::{test_source_at, test_source_store};

    use super::bind_callable_abi;

    #[test]
    fn repeated_callable_abi_directives_publish_exact_structured_diagnostics() {
        let sources = test_source_store([concat!(
            "module app;\n",
            "@abi(c)\n",
            "@abi(system)\n",
            "func main()\n",
            "{\n",
            "}\n",
        )]);

        let parsed = bray_parser::parse_source_unit(test_source_at(&sources, 0));

        let declarations = parsed
            .source_unit()
            .function_declarations()
            .collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("test source must contain one function declaration");
        };

        let result = bind_callable_abi(declaration.function_directives().abi_directives());

        bray_testing::assert_goal_state_diagnostic_kind(
            result.diagnostics(),
            DiagnosticKind::BindingDuplicateCallableAbi,
        );

        assert_eq!(
            result
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.kind())
                .collect::<Vec<_>>(),
            [DiagnosticKind::BindingDuplicateCallableAbi]
        );
    }
}
