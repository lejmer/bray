use super::{SemanticDataKind, SemanticQueryContext, SemanticQueryFailure, SemanticQueryViolation};
use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_checker::{resolve_callable_signature_template, resolve_type_expression_template};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    CallableInstanceData, CallableSignatureQuery, ImplementationSubjectQuery, SymbolQueryRequest,
    TypeId,
};

use crate::compilation::Compilation;
use crate::compilation::binder::{CompilationBindingContext, binding_query_error};
use crate::fact::FactQueryError;

impl Compilation {
    pub(in crate::compilation) fn inherited_callable_substitution(
        &self,
        binding: &CompilationBindingContext<'_>,
        instance: CallableInstanceData,
    ) -> Result<
        bray_diagnostics::DiagnosticResult<Option<bray_symbols::GenericSubstitutionId>>,
        FactQueryError,
    > {
        let values = binding.semantic_values();

        let owner = binding
            .containing_symbol(instance.definition().symbol())
            .map_err(binding_query_error)?;

        let Some(bray_symbols::AnySymbolId::InherentImplementation(implementation)) = owner else {
            return Ok(bray_diagnostics::DiagnosticResult::without_diagnostics(
                Some(instance.substitution()),
            ));
        };

        let mut diagnostics = DiagnosticBag::new();

        let pattern = self.resolve_implementation_self_type(
            binding,
            implementation.into(),
            &mut diagnostics,
        )?;

        let data = values.type_data(pattern);

        let bray_symbols::TypeData::Named { definition, .. } = data.as_ref() else {
            return Ok(bray_diagnostics::DiagnosticResult::new(
                Some(instance.substitution()),
                diagnostics,
            ));
        };

        let named_owner = bray_symbols::GenericOwnerId::try_new(definition.into_any())
            .expect("named type is a generic owner");

        let named = binding
            .resolve_symbol_query(SymbolQueryRequest::<
                bray_symbols::GenericDeclarationTemplateQuery,
            >::new(named_owner))
            .map_err(binding_query_error)?;

        diagnostics.add_range(named.diagnostics().iter().cloned());

        let incoming = values.generic_substitution_data(instance.substitution());

        if !incoming
            .bindings()
            .iter()
            .any(|binding| named.value().parameters().contains(&binding.parameter()))
        {
            return Ok(bray_diagnostics::DiagnosticResult::new(
                Some(instance.substitution()),
                diagnostics,
            ));
        }

        let implementation_owner = bray_symbols::GenericOwnerId::try_new(implementation.into())
            .expect("inherent implementation is a generic owner");

        let generic = binding
            .resolve_symbol_query(SymbolQueryRequest::<
                bray_symbols::GenericDeclarationTemplateQuery,
            >::new(implementation_owner))
            .map_err(binding_query_error)?;

        diagnostics.add_range(generic.diagnostics().iter().cloned());

        if incoming
            .bindings()
            .iter()
            .any(|binding| generic.value().parameters().contains(&binding.parameter()))
        {
            return Ok(bray_diagnostics::DiagnosticResult::new(
                Some(instance.substitution()),
                diagnostics,
            ));
        }

        let open = super::substitution::identity_substitution(
            values,
            named_owner,
            named.value().parameters(),
        )?;

        let actual = values
            .intern_type(bray_symbols::TypeData::Named {
                definition: *definition,
                substitution: open,
            })
            .map_err(FactQueryError::SemanticValueStore)?;

        let actual = values
            .substitute_type(actual, instance.substitution())
            .map_err(FactQueryError::SemanticValueStore)?;

        let matched = super::implementation::match_implementation_subject(
            implementation.into(),
            generic.value().parameters(),
            pattern,
            actual,
            values,
        )
        .map_err(|error| {
            super::implementation::implementation_match_query_error(implementation.into(), error)
        })?;

        let substitution = matched
            .map(|matched| {
                super::substitution::substitution_for_owner(
                    values,
                    instance.definition().symbol(),
                    [instance.substitution(), matched],
                )
            })
            .transpose()?;

        Ok(bray_diagnostics::DiagnosticResult::new(
            substitution,
            diagnostics,
        ))
    }

    pub(in crate::compilation) fn resolve_implementation_self_type(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        implementation: bray_symbols::ImplementationSymbolId,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<TypeId, FactQueryError> {
        let subject = binding_context
            .resolve_symbol_query(SymbolQueryRequest::<ImplementationSubjectQuery>::new(
                implementation,
            ))
            .map_err(binding_query_error)?;

        *diagnostics = diagnostics.merged(subject.diagnostics());

        let checked = self.checked_constant_terms_for_templates_with_cancellation(
            [subject.value().ty()],
            binding_context.cancellation(),
        )?;

        *diagnostics = diagnostics.merged(checked.diagnostics());

        resolve_type_expression_template(
            binding_context.semantic_values(),
            subject.value().ty(),
            checked.value(),
        )
        .ok_or_else(|| {
            SemanticQueryFailure::contract(
                SemanticQueryContext::Symbol(implementation.into_any()),
                SemanticQueryViolation::Unsupported(SemanticDataKind::Type),
            )
            .into()
        })
    }

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
        );

        let Some(signature) = signature else {
            return Ok(None);
        };

        let checker = super::checker::CompilationCheckerContext::new(*binding_context);

        let signature = bray_checker::normalize_callable_signature(
            &checker,
            signature,
            Some(instance.substitution()),
            diagnostics,
        )
        .map_err(FactQueryError::from)?;

        let predicates = binding_context
            .resolve_symbol_query(SymbolQueryRequest::<
                bray_symbols::CallablePredicateContractsQuery,
            >::new(callable))
            .map_err(binding_query_error)?;

        diagnostics.add_range(predicates.diagnostics().iter().cloned());

        signature
            .with_predicate_contracts(
                binding_context.semantic_values(),
                predicates.value(),
                instance.substitution(),
            )
            .map(Some)
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::compilation;

    #[test]
    fn inherent_constructor_self_resolves_with_its_generic_subject() {
        for (subject, construct_result, call, caller_result) in [
            ("Value", "Self", "Value(true)", "Value"),
            ("Value<T>", "Self", "Value<bool>(true)", "Value<bool>"),
            (
                "Value<T>",
                "Result<Self, bool>",
                "Value<bool>(true)",
                "Result<Value<bool>, bool>",
            ),
        ] {
            let field = if subject == "Value" { "bool" } else { "T" };

            let result = if construct_result == "Self" {
                "value"
            } else {
                "Ok(value)"
            };

            let source = format!(
                r#"
                module app;
                struct {subject} {{ byte: {field}; }}
                impl {subject}
                {{
                    construct(pos byte: {field}) -> {construct_result}
                    {{
                        let value: {subject} = {{ byte = byte }};
                        return {result};
                    }}
                }}
                func caller() -> {caller_result} {{ return {call}; }}
            "#
            );

            let compilation = compilation(&source);

            assert!(
                compilation.check_diagnostics().is_empty(),
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

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
