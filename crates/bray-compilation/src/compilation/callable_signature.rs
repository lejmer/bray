use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_checker::resolve_callable_signature_template;
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{CallableInstanceData, CallableSignatureQuery, SymbolQueryRequest};

use crate::compilation::Compilation;
use crate::compilation::binder::{CompilationBindingContext, binding_query_error};
use crate::fact::FactQueryError;

impl Compilation {
    pub(in crate::compilation) fn resolve_callable_instance_signature(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        instance: CallableInstanceData,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<bray_symbols::CallableSignature>, FactQueryError> {
        let callable = instance.definition().callable_symbol();

        let result = binding_context
            .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(callable))
            .map_err(binding_query_error)?;

        *diagnostics = diagnostics.merged(result.diagnostics());

        let checked = self.checked_constant_terms_for_templates_with_cancellation(
            [result.value().callable_type(), result.value().result()],
            binding_context.cancellation(),
        )?;

        *diagnostics = diagnostics.merged(checked.diagnostics());

        let signature = resolve_callable_signature_template(
            binding_context.semantic_values(),
            result.value(),
            instance.substitution(),
            checked.value(),
        )
        .map_err(FactQueryError::from)?;

        Ok(signature)
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::compilation;

    #[test]
    fn callable_conversions_forget_but_cannot_invent_execution_properties() {
        for (source_contract, target_contract, valid) in [
            ("executes(total)", "", true),
            ("", "executes(total)", false),
        ] {
            let source = r#"
                module app;

                callable Source = func() -> bool SOURCE;
                callable Target = func() -> bool TARGET;

                func convert(pos action: Source) -> Target
                {
                    return action as Target;
                }
            "#
            .replace("SOURCE", source_contract)
            .replace("TARGET", target_contract);

            let compilation = compilation(&source);

            assert_eq!(
                compilation.check_diagnostics().is_empty(),
                valid,
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn named_callable_references_require_the_promised_source_contract() {
        for (properties, valid) in [("executes(total)", true), ("", false)] {
            let source = r#"
                module app;

                callable Action = func() -> bool executes(total);

                func supplied() -> bool PROPERTIES
                {
                    return true;
                }

                func root() -> bool
                {
                    let action: Action = supplied;
                    return action();
                }
            "#
            .replace("PROPERTIES", properties);

            let compilation = compilation(&source);

            assert_eq!(
                compilation.check_diagnostics().is_empty(),
                valid,
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn indirect_execution_uses_purity_but_does_not_invent_termination_evidence() {
        for (supplied, promised, valid) in [
            ("executes(pure)", "pure", true),
            ("", "pure", false),
            ("executes(total)", "total", false),
        ] {
            let source = r#"
                module app;
                callable Action = func() -> bool SUPPLIED;
                func root(pos action: Action) -> bool executes(PROMISED)
                {
                    return action();
                }
            "#
            .replace("SUPPLIED", supplied)
            .replace("PROMISED", promised);

            let compilation = compilation(&source);

            assert_eq!(
                compilation.check_diagnostics().is_empty(),
                valid,
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn generic_callable_substitution_retains_execution_properties() {
        let compilation = compilation(
            r#"
            module app;

            callable Action<T> = func(pos value: T) -> T executes(total);
            callable Plain<T> = func(pos value: T) -> T;

            func root(pos action: Action<bool>) -> bool
            {
                let erased = action as Plain<bool>;
                return erased(true);
            }
        "#,
        );

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );
    }
}
