use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_bound_tree::{
    BoundExpression, BoundUnit, CheckedExpressionTypes, SemanticSelection, SemanticSelectionEntry,
};
use bray_checker::check_generic_arguments;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticLabel,
    DiagnosticLabelKind, SeverityKind,
};
use bray_source::SourceSpan;
use bray_symbols::{
    AnySymbolId, CallableConditions, CallableConditionsQuery, CallableDefinitionId,
    CallableInstanceData, CallableSignatureQuery, GenericSubstitutionData, GenericSubstitutionId,
    StaticSymbolId, SymbolQueryRequest, TypeData, TypeId,
};
use bray_syntax::GenericArgumentListSyntax;

use super::support::{checker_unit_view, unit_contract_failure};
use crate::compilation::binder::{binding_query_error, generic_parameter_ids, type_binder};
use crate::compilation::checker::checker_result;
use crate::compilation::state::Compilation;
use crate::fact::{CancellationToken, FactQueryError};

enum NamedReference {
    Callable(CallableDefinitionId),
    Static(StaticSymbolId),
}

#[cfg(test)]
mod tests {
    use crate::test_support::{compilation, source_function_body_key};

    #[test]
    fn named_callable_values_preserve_declared_guarantees_and_generic_inputs() {
        for source in [
            "module app; func checked() executes(pure, total) {} func example() { let operation: func() = checked; }",
            "module app; func checked() executes(pure, total) {} func example() { let operation: func() executes(pure, total) = checked; }",
            "module app; func identity<T>(pos value: T) -> T executes(pure, total) { return value; } func example() { let operation: func(pos value: bool) -> bool executes(pure, total) = identity<bool>; }",
            "module app; func require_ready(pos ready: bool) requires(ready) {} func example() { let operation: func(pos ready: bool) requires(ready) = require_ready; }",
        ] {
            let compilation = compilation(source);
            let diagnostics = compilation.check_diagnostics();

            assert!(!diagnostics.has_errors(), "{source}: {diagnostics:?}");
        }
    }

    #[test]
    fn named_callable_values_cannot_invent_guarantees_or_erase_preconditions() {
        use bray_diagnostics::{
            DiagnosticArg, DiagnosticCallableContractMismatch as Mismatch,
            DiagnosticCallableContractSurface, DiagnosticExecutionProperty, DiagnosticKind,
        };

        for (source, mismatch) in [
            (
                "module app; func unchecked() {} func example() { let operation: func() executes(pure) = unchecked; }",
                Mismatch::ExecutionGuarantee {
                    property: DiagnosticExecutionProperty::Pure,
                    guard: None,
                },
            ),
            (
                "module app; func unchecked<T>(pos value: T) -> T { return value; } func example() { let operation: func(pos value: bool) -> bool executes(total) = unchecked<bool>; }",
                Mismatch::ExecutionGuarantee {
                    property: DiagnosticExecutionProperty::Total,
                    guard: None,
                },
            ),
            (
                "module app; func unchecked(pos ready: bool) requires(ready) {} func example() { let operation: func(pos ready: bool) = unchecked; }",
                Mismatch::PredicateImplication {
                    surface: DiagnosticCallableContractSurface::InvocationPreconditions,
                    index: 0,
                },
            ),
        ] {
            let compilation = compilation(source);
            let diagnostics = compilation.check_diagnostics();

            let diagnostic = diagnostics
                .iter()
                .find(|diagnostic| {
                    diagnostic.kind() == DiagnosticKind::CheckingCallableContractMismatch
                })
                .unwrap_or_else(|| panic!("{source}: {diagnostics:?}"));

            assert!(diagnostic.primary_span().is_some());

            assert!(
                diagnostic
                    .args()
                    .contains(&DiagnosticArg::referenced_name("unchecked"))
            );

            assert!(
                diagnostic
                    .args()
                    .contains(&DiagnosticArg::callable_contract_mismatch(mismatch))
            );
        }
    }

    #[test]
    fn callable_values_can_weaken_guarantees_in_returns_conversions_and_arguments() {
        let compilation = compilation(
            "module app; \
             func pass(operation: func() executes(pure, total)) -> func() { return operation; } \
             func convert(operation: func() executes(pure, total)) -> func() { return operation as func(); } \
             func invoke(operation: func() executes(pure, total)) { take(operation = operation); } \
             func take(operation: func()) {}",
        );

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{:?}",
            compilation.check_diagnostics()
        );

        for name in ["pass", "convert", "invoke"] {
            let result = compilation
                .lowered_unit(source_function_body_key(&compilation, name))
                .unwrap();

            assert!(
                result.diagnostics().is_empty(),
                "{name}: {:?}",
                result.diagnostics()
            );

            assert!(
                result
                    .value()
                    .as_ref()
                    .and_then(|unit| unit.mir())
                    .is_some(),
                "{name}: MIR expected"
            );
        }
    }

    #[test]
    fn callable_values_cannot_gain_guarantees_through_a_return() {
        let compilation = compilation(
            "module app; func strengthen(operation: func()) -> func() executes(pure, total) { return operation; }",
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(
            diagnostics.iter().any(|diagnostic| {
                diagnostic.kind()
                    == bray_diagnostics::DiagnosticKind::CheckingIncompatibleExpressionType
                    && diagnostic.primary_span().is_some()
            }),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn callable_values_preserve_caller_obligations_without_exporting_implementation_capabilities() {
        for source in [
            "trusted module app; func forget(operation: trusted func() uses(foreign_call)) -> trusted func() { return operation; }",
            "trusted module app; trusted func hint() uses(intrinsic) { trusted core.target.spin_loop_hint(); } func example() { let operation: func() = hint; }",
            "trusted module app; trusted func work() requires(blocking_execution(), compute_execution()) uses(intrinsic) { trusted core.target.spin_loop_hint(); } func example() { let operation: trusted func() requires(blocking_execution(), compute_execution()) = work; }",
        ] {
            let compilation = compilation(source);
            let diagnostics = compilation.check_diagnostics();

            assert!(!diagnostics.has_errors(), "{source}: {diagnostics:?}");
        }

        for source in [
            "trusted module app; trusted predicate valid(); trusted func work() requires(trusted valid()) {} func example() { let operation: func() = work; }",
            "module app; func work() requires(blocking_execution()) {} func example() { let operation: func() = work; }",
        ] {
            let compilation = compilation(source);

            let diagnostics = compilation.check_diagnostics();

            assert!(
                diagnostics.iter().any(|diagnostic| {
                    matches!(
                        diagnostic.kind(),
                        bray_diagnostics::DiagnosticKind::CheckingIncompatibleExpressionType
                            | bray_diagnostics::DiagnosticKind::CheckingCallableContractMismatch
                    ) && diagnostic.primary_span().is_some()
                }),
                "{diagnostics:?}"
            );
        }
    }
}

impl Compilation {
    pub(super) fn named_reference_selections(
        &self,
        key: &bray_bound_tree::BoundUnitKey,
        cancellation: &CancellationToken,
        bound: &BoundUnit,
        types: &CheckedExpressionTypes,
        semantic_context: &bray_checker::SemanticUnitContext,
        checker_context: &impl bray_checker::CheckerRequestContext<UpstreamError = FactQueryError>,
    ) -> Result<(Vec<SemanticSelectionEntry>, DiagnosticBag), FactQueryError> {
        let binding_context = self.binding_context_for(key, cancellation)?;

        let owner = binding_context
            .symbols()
            .symbol_for_key(key.declared_owner())
            .ok_or_else(|| {
                unit_contract_failure(
                    key,
                    crate::compilation::SemanticQueryViolation::Missing(
                        crate::compilation::SemanticDataKind::Symbol,
                    ),
                )
            })?;

        let unit = checker_unit_view(bound, semantic_context, checker_context)?;

        let mut entries = Vec::new();
        let mut diagnostics = DiagnosticBag::new();

        for (expression, node) in bound.tree().expressions() {
            let BoundExpression::Name(name) = node else {
                continue;
            };

            let bray_bound_tree::BoundReferenceTarget::Surface(symbol) = name.target() else {
                continue;
            };

            let reference = match symbol {
                AnySymbolId::Static(declaration) => NamedReference::Static(declaration),
                symbol => {
                    let Some(definition) = CallableDefinitionId::try_new(symbol) else {
                        continue;
                    };

                    NamedReference::Callable(definition)
                }
            };

            let Some(substitution) = self.named_reference_substitution(
                &binding_context,
                owner,
                symbol,
                name.generic_argument_list(),
                unit,
                &mut diagnostics,
            )?
            else {
                continue;
            };

            let selection = match reference {
                NamedReference::Callable(definition) => {
                    let instance = CallableInstanceData::new(definition, substitution);
                    let ty = types.expression(expression).filter(|ty| !ty.is_recovered());

                    let parent = bound
                        .tree()
                        .expression_parent(expression)
                        .and_then(|parent| bound.tree().expression(parent));

                    let is_callee = match parent {
                        Some(BoundExpression::Call(call)) => call.callee() == expression,
                        Some(BoundExpression::ErrorCall(call)) => call.callee() == expression,
                        _ => false,
                    };

                    if !is_callee && let Some(ty) = ty {
                        let checked = self.check_named_callable_reference(
                            &binding_context,
                            checker_context,
                            instance,
                            ty.ty(),
                            name.origin().source_anchor().syntax(),
                            &mut diagnostics,
                        )?;

                        if !checked {
                            continue;
                        }
                    }

                    SemanticSelection::CallableReference(instance)
                }
                NamedReference::Static(declaration) => {
                    let (selection, selection_diagnostics) = self.static_reference_selection(
                        declaration,
                        substitution,
                        cancellation,
                        &binding_context,
                        name.origin().source_anchor().syntax(),
                    )?;

                    diagnostics = diagnostics.merged(&selection_diagnostics);

                    SemanticSelection::StaticReference(selection)
                }
            };

            entries.push(SemanticSelectionEntry::new(expression, selection));
        }

        Ok((entries, diagnostics))
    }

    fn check_named_callable_reference(
        &self,
        binding_context: &crate::compilation::binder::CompilationBindingContext<'_>,
        checker_context: &impl bray_checker::CheckerRequestContext<UpstreamError = FactQueryError>,
        instance: CallableInstanceData,
        expected: TypeId,
        source: bray_declarations::SyntaxAnchor,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<bool, FactQueryError> {
        let values = binding_context.semantic_values();

        let required = values
            .type_data(expected)
            .map_err(FactQueryError::SemanticValueStore)?;

        let TypeData::Callable(required) = required.as_ref() else {
            return Ok(true);
        };

        let Some(actual) =
            self.named_callable_reference_type(binding_context, instance, diagnostics)?
        else {
            return Ok(false);
        };

        let provided = values
            .type_data(actual)
            .map_err(FactQueryError::SemanticValueStore)?;

        let TypeData::Callable(provided) = provided.as_ref() else {
            return Err(crate::compilation::SemanticQueryFailure::contract(
                crate::compilation::SemanticQueryContext::Type(actual),
                crate::compilation::SemanticQueryViolation::Unsupported(
                    crate::compilation::SemanticDataKind::CallableSignature,
                ),
            )
            .into());
        };

        let Some(mismatch) = bray_checker::check_callable_type_contract(values, required, provided)
            .map_err(FactQueryError::SemanticValueStore)?
        else {
            return Ok(true);
        };

        let span = SourceSpan::new(source.source_id(), source.full_range());

        let diagnostic = match mismatch {
            bray_checker::CallableTypeContractMismatch::Signature => Diagnostic::new(
                DiagnosticId::from_index(diagnostics.len()),
                DiagnosticKind::CheckingIncompatibleExpressionType,
                SeverityKind::Error,
            )
            .with_arg(DiagnosticArg::expected_type(bray_checker::diagnostic_type(
                checker_context,
                expected,
            )?))
            .with_arg(DiagnosticArg::actual_type(bray_checker::diagnostic_type(
                checker_context,
                actual,
            )?)),
            bray_checker::CallableTypeContractMismatch::Condition(mismatch) => {
                let symbol = instance.definition().callable_symbol().into_any();

                let name = binding_context
                    .symbols()
                    .member_name(symbol)
                    .ok_or_else(|| {
                        crate::compilation::SemanticQueryFailure::contract(
                            crate::compilation::SemanticQueryContext::Symbol(symbol),
                            crate::compilation::SemanticQueryViolation::Missing(
                                crate::compilation::SemanticDataKind::Symbol,
                            ),
                        )
                    })?;

                Diagnostic::new(
                    DiagnosticId::from_index(diagnostics.len()),
                    DiagnosticKind::CheckingCallableContractMismatch,
                    SeverityKind::Error,
                )
                .with_arg(DiagnosticArg::referenced_name(name.as_str()))
                .with_arg(DiagnosticArg::callable_contract_mismatch(
                    mismatch.diagnostic(),
                ))
            }
        };

        diagnostics.add(
            diagnostic
                .with_primary_span(span)
                .with_label(DiagnosticLabel::primary(
                    DiagnosticLabelKind::IncompatibleExpressionType,
                    span,
                )),
        );

        Ok(false)
    }

    fn named_callable_reference_type(
        &self,
        context: &crate::compilation::binder::CompilationBindingContext<'_>,
        instance: CallableInstanceData,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<TypeId>, FactQueryError> {
        let owner = instance.definition().callable_symbol();

        let signature = context
            .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(owner))
            .map_err(binding_query_error)?;

        let constants = self.checked_constant_terms_for_templates_with_cancellation(
            [signature.value().callable_type()],
            context.cancellation(),
        )?;

        let conditions = context
            .resolve_symbol_query(SymbolQueryRequest::<CallableConditionsQuery>::new(owner))
            .map_err(binding_query_error)?;

        *diagnostics = DiagnosticBag::merged_all([
            diagnostics,
            signature.diagnostics(),
            constants.diagnostics(),
            conditions.diagnostics(),
        ]);

        let values = context.semantic_values();

        let Some(ty) = bray_checker::resolve_type_expression_template(
            values,
            signature.value().callable_type(),
            constants.value(),
        )?
        else {
            return Ok(None);
        };

        let data = values
            .type_data(ty)
            .map_err(FactQueryError::SemanticValueStore)?;

        let TypeData::Callable(callable) = data.as_ref() else {
            return Ok(Some(ty));
        };

        let behavior = crate::compilation::binder::bind_declared_callable_phase_behaviors(
            context,
            owner,
            callable.execution(),
        )
        .map_err(binding_query_error)?;

        *diagnostics = diagnostics.merged(behavior.diagnostics());

        let (behavior, _) = behavior.into_parts();

        // Share immutable signature storage and attach declaration contracts before substituting once.
        let callable = callable
            .clone()
            .with_conditions(conditions.value().clone())
            .with_phase_behaviors(behavior);

        let ty = values
            .intern_type(TypeData::Callable(callable))
            .map_err(FactQueryError::SemanticValueStore)?;

        values
            .substitute_type(ty, instance.substitution())
            .map(Some)
            .map_err(FactQueryError::SemanticValueStore)
    }

    fn named_reference_substitution<C>(
        &self,
        binding_context: &crate::compilation::binder::CompilationBindingContext<'_>,
        owner: AnySymbolId,
        symbol: AnySymbolId,
        generic_arguments: Option<bray_declarations::SyntaxAnchor>,
        unit: bray_checker::CheckerUnitView<'_, C>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<GenericSubstitutionId>, FactQueryError>
    where
        C: bray_checker::CheckerRequestContext + ?Sized,
        C::UpstreamError: Into<FactQueryError>,
    {
        let parameters = generic_parameter_ids(binding_context.symbols(), symbol)
            .map_err(binding_query_error)?;

        let resolved = match generic_arguments {
            Some(anchor) => {
                let arguments = anchor
                    .find_descendant::<GenericArgumentListSyntax>(self.syntax_tree())
                    .ok_or_else(|| {
                        crate::compilation::SemanticQueryFailure::located_contract(
                            crate::compilation::SemanticQueryContext::Symbol(symbol),
                            crate::compilation::SemanticQueryViolation::Missing(
                                crate::compilation::SemanticDataKind::Syntax,
                            ),
                            SourceSpan::new(anchor.source_id(), anchor.full_range()),
                        )
                    })?;

                let arguments = arguments.generic_arguments().collect::<Vec<_>>();

                let bound_arguments = type_binder(binding_context, owner)
                    .map_err(binding_query_error)?
                    .bind_call_generic_arguments(&arguments, &parameters)
                    .map_err(binding_query_error)?;

                let (bound_arguments, argument_diagnostics) = bound_arguments.into_parts();

                *diagnostics = diagnostics.merged(&argument_diagnostics);

                let resolved = checker_result(check_generic_arguments(unit, &bound_arguments))?;

                let (resolved, resolution_diagnostics) = resolved.into_parts();

                *diagnostics = diagnostics.merged(&resolution_diagnostics);

                let Some(resolved) = resolved else {
                    return Ok(None);
                };

                resolved
            }
            None if parameters.is_empty() => Vec::new(),
            None => return Ok(None),
        };

        let owner = crate::compilation::substitution::generic_owner(symbol)?;

        let substitution =
            GenericSubstitutionData::try_new(owner, parameters, resolved).map_err(|cause| {
                crate::compilation::SemanticQueryFailure::GenericSubstitution {
                    owner: Some(owner),
                    cause,
                }
            })?;

        binding_context
            .semantic_values()
            .intern_generic_substitution(substitution)
            .map(Some)
            .map_err(FactQueryError::SemanticValueStore)
    }
}
