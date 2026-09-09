use bray_binder::SymbolQueryProvider;
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{
    AnySymbolId, CallableConditions, CallableContractObligation, CallableContractsQuery,
    CallableSymbolId, ForeignCallableDirection, SymbolQueryRequest,
};

use crate::compilation::Compilation;
use crate::compilation::binder::binding_query_error;
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(in crate::compilation) fn foreign_callable_assertions(
        &self,
        callable: CallableSymbolId,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Vec<CallableContractObligation>>, FactQueryError> {
        let AnySymbolId::Function(function) = callable.into_any() else {
            return Ok(DiagnosticResult::without_diagnostics(Vec::new()));
        };

        let foreign = self.foreign_callable_contract_with_cancellation(function, cancellation)?;

        if foreign.diagnostics().has_errors()
            || !foreign
                .value()
                .as_ref()
                .is_some_and(|boundary| boundary.direction() == ForeignCallableDirection::Import)
        {
            return Ok(DiagnosticResult::new(
                Vec::new(),
                foreign.diagnostics().clone(),
            ));
        }

        let binding = self.binding_context(cancellation)?;

        let contracts = binding
            .resolve_symbol_query(SymbolQueryRequest::<CallableContractsQuery>::new(callable))
            .map_err(binding_query_error)?;

        let diagnostics = foreign.diagnostics().merged(contracts.diagnostics());

        if diagnostics.has_errors() {
            return Ok(DiagnosticResult::new(Vec::new(), diagnostics));
        }

        let conditions = contracts.value().conditions();

        let promises = conditions
            .execution_guarantees()
            .iter()
            .copied()
            .map(CallableContractObligation::Execution)
            .chain(
                conditions
                    .normal_completion_postconditions()
                    .iter()
                    .chain(conditions.guarded_postconditions())
                    .map(|clause| CallableContractObligation::Postcondition(clause.ordinal())),
            )
            .collect();

        Ok(DiagnosticResult::new(promises, diagnostics))
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::compilation_with_native_link;
    use bray_symbols::NativeLinkKind;

    #[test]
    fn foreign_assertions_preserve_boundary_and_caller_obligations() {
        for (
            module_trust,
            external,
            trusted,
            capability,
            wrapper_trust,
            wrapper_capability,
            call_trust,
            body,
            accepted,
        ) in [
            (
                "trusted",
                "extern",
                "trusted",
                "uses(foreign_call)",
                "trusted",
                "uses(foreign_call)",
                "trusted",
                ";",
                true,
            ),
            (
                "trusted",
                "extern",
                "trusted",
                "uses(foreign_call)",
                "trusted",
                "uses(foreign_call)",
                "",
                ";",
                true,
            ),
            (
                "",
                "extern",
                "trusted",
                "uses(foreign_call)",
                "trusted",
                "uses(foreign_call)",
                "trusted",
                ";",
                false,
            ),
            (
                "trusted",
                "extern",
                "",
                "uses(foreign_call)",
                "trusted",
                "uses(foreign_call)",
                "trusted",
                ";",
                false,
            ),
            (
                "trusted",
                "extern",
                "trusted",
                "",
                "trusted",
                "uses(foreign_call)",
                "trusted",
                ";",
                false,
            ),
            (
                "trusted",
                "extern",
                "trusted",
                "uses(foreign_call)",
                "",
                "uses(foreign_call)",
                "trusted",
                ";",
                false,
            ),
            (
                "trusted",
                "extern",
                "trusted",
                "uses(foreign_call)",
                "trusted",
                "",
                "trusted",
                ";",
                false,
            ),
            (
                "trusted",
                "",
                "trusted",
                "uses(foreign_call)",
                "trusted",
                "uses(foreign_call)",
                "trusted",
                "{ panic(1); }",
                false,
            ),
        ] {
            let source = format!(
                r#"
                {module_trust} module app;
                @link(name = "native") @symbol(name = "native_value") @abi(c) {external} {trusted} func native_value() -> i32 {capability} executes(pure, total) {body} {wrapper_trust} func root() -> i32 {wrapper_capability} executes(pure, total)
                {{
                    return {call_trust} native_value();
                }}
                "#
            );

            let compilation =
                compilation_with_native_link(&source, "native", NativeLinkKind::Dynamic);

            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                accepted,
                "{source}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn foreign_assertions_require_each_property_and_an_applicable_entry_guard() {
        for (foreign_contract, caller_contract, accepted) in [
            (
                "executes(pure, total) ensures(result == value)",
                "executes(pure, total) ensures(result == value)",
                true,
            ),
            ("executes(pure)", "executes(total)", false),
            ("executes(total)", "executes(pure)", false),
            ("", "executes(pure, total)", false),
            (
                "when(value == 0) { executes(pure, total) ensures(result == value) }",
                "when(value == 0) { executes(pure, total) ensures(result == value) }",
                true,
            ),
            (
                "when(value == 0) { executes(pure, total) }",
                "executes(pure, total)",
                false,
            ),
        ] {
            let source = format!(
                r#"
                trusted module app;
                @link(name = "native") @symbol(name = "native_value") @abi(c) extern trusted func native_value(pos value: i32) -> i32 uses(foreign_call) {foreign_contract};
                trusted func root(pos value: i32) -> i32 uses(foreign_call) {caller_contract}
                {{
                    return native_value(value);
                }}
                "#
            );

            let compilation =
                compilation_with_native_link(&source, "native", NativeLinkKind::Dynamic);

            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                accepted,
                "{source}: {diagnostics:?}"
            );
        }
    }
}
