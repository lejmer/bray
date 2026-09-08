use std::collections::BTreeMap;

use bray_binder::SymbolQueryProvider;
use bray_bound_tree::{BoundExpressionId, BoundUnit, CheckedExpressionSemantics};
use bray_checker::{
    ConstantChecker, ConstantEvaluationInput, DefaultConstantChecker, PredicateConditionResolution,
};
use bray_symbols::{
    AnySymbolId, CallableSignatureQuery, CallableSymbolId, ConstantTermData, ConstantTermId,
    PredicateDefinitionSymbolId, PredicateInstanceData, PredicateSignatureTemplateQuery,
    SymbolOrdinal, SymbolQueryRequest,
};

use super::CompilationConstantCallResolver;
use crate::compilation::Compilation;
use crate::compilation::binder::{CompilationBindingContext, binding_query_error};
use crate::compilation::checker::checker_result;
use crate::compilation::unit::{checker_unit_view, semantic_unit_context_for};
use crate::compilation::{
    SemanticDataKind, SemanticQueryContext, SemanticQueryFailure, SemanticQueryViolation,
};
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(in crate::compilation) fn inert_parameter_default_term(
        &self,
        parameter: bray_symbols::CallableParameterSymbolId,
        provider: bray_symbols::CallableParameterDefaultProviderSymbolId,
        cancellation: &CancellationToken,
    ) -> Result<Option<ConstantTermId>, FactQueryError> {
        cancellation.check()?;
        let checked = self.callable_parameter_default(parameter)?;

        if checked.diagnostics().has_errors() || checked.value().provider() != provider {
            return Ok(None);
        }

        let bray_symbols::CallableParameterDefaultValue::Valid(surface) = checked.value().value()
        else {
            return Ok(None);
        };

        let bray_symbols::RuntimeDefaultTemplateReference::Source(source) =
            surface.template_reference()
        else {
            return Ok(None);
        };

        let key = self.source_runtime_default_key(provider.into(), *source)?;
        let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;
        let unit = bound.result().value();

        let bray_bound_tree::BoundUnitRoot::Expression(root) = unit.root() else {
            return Ok(None);
        };

        // A runtime default can perform effects independently of its selected callee.
        // Only a checked literal with an identity result conversion is inert here.
        if bound.result().diagnostics().has_errors()
            || !matches!(
                unit.tree().expression(root),
                Some(bray_bound_tree::BoundExpression::Literal(_))
            )
        {
            return Ok(None);
        }

        let semantics = self.expression_semantics_with_cancellation(key, cancellation)?;

        if semantics.result().diagnostics().has_errors()
            || semantics
                .result()
                .value()
                .types()
                .expression(root)
                .is_none_or(|result| result.ty() != surface.result())
        {
            return Ok(None);
        }

        let (_, terms) = self.symbolic_expression_terms(
            unit,
            semantics.result().value(),
            &[root],
            cancellation,
        )?;

        let [Some(term)] = terms.as_slice() else {
            return Ok(None);
        };

        // Target-sized literals retain their target-independent representation until realization.
        Ok(Some(*term))
    }

    pub(in crate::compilation) fn expanded_predicate_condition(
        &self,
        condition: ConstantTermId,
        cancellation: &CancellationToken,
        mut select: impl FnMut(
            PredicateInstanceData,
        ) -> Result<Option<PredicateInstanceData>, FactQueryError>,
    ) -> Result<Option<ConstantTermId>, FactQueryError> {
        bray_checker::expand_predicate_condition(
            self.semantic_value_store()?,
            condition,
            |predicate| {
                cancellation.check()?;

                let Some(predicate) = select(predicate)? else {
                    return Ok(PredicateConditionResolution::Unavailable);
                };

                let definition = self.predicate_definition(predicate.definition())?;

                if definition.diagnostics().has_errors() {
                    return Ok(PredicateConditionResolution::Unavailable);
                }

                Ok(match definition.value() {
                    bray_symbols::PredicateDefinitionState::Defined(definition) => {
                        match definition.semantic().condition() {
                            Some(condition) => PredicateConditionResolution::Defined {
                                predicate,
                                condition,
                            },
                            None => PredicateConditionResolution::Unavailable,
                        }
                    }
                    bray_symbols::PredicateDefinitionState::OpaqueTrusted
                    | bray_symbols::PredicateDefinitionState::Required => {
                        PredicateConditionResolution::Opaque(predicate)
                    }
                    _ => PredicateConditionResolution::Unavailable,
                })
            },
        )
    }

    pub(in crate::compilation) fn symbolic_expression_terms(
        &self,
        bound: &BoundUnit,
        semantics: &CheckedExpressionSemantics,
        roots: &[BoundExpressionId],
        cancellation: &CancellationToken,
    ) -> Result<(SymbolOrdinal, Vec<Option<ConstantTermId>>), FactQueryError> {
        let context = self.checker_context_for(bound.key(), cancellation)?;
        let semantic_context = semantic_unit_context_for(context.symbols(), bound)?;
        let binding = self.binding_context(cancellation)?;
        let arguments = predicate_arguments(&binding, bound)?;

        let references =
            self.symbolic_references_with_arguments(bound, semantics.selections(), &arguments)?;

        let patterns = self.patterns_with_cancellation(bound.key().clone(), cancellation)?;
        let values = self.semantic_value_store()?;

        let anonymous_parameters = bound.local_symbols().anonymous_parameters().iter().filter(|parameter| {
            matches!(bound.root(), bray_bound_tree::BoundUnitRoot::AnonymousCallable { callable, .. } if parameter.callable() == callable)
        }).collect::<Vec<_>>();

        let input_count = bound
            .contract_inputs()
            .map_or(arguments.len() + anonymous_parameters.len(), |inputs| {
                inputs.parameters().len()
            });

        let result_ordinal = ordinal(input_count, bound)?;

        let result =
            values.intern_constant_term(ConstantTermData::CallableArgument(result_ordinal))?;

        let mut locals = bound
            .local_symbols()
            .postcondition_results()
            .iter()
            .map(|symbol| (symbol.id().into(), result))
            .collect::<Vec<_>>();

        if let Some(inputs) = bound.contract_inputs() {
            for (index, parameter) in inputs.parameters().iter().enumerate() {
                let term = values.intern_constant_term(ConstantTermData::CallableArgument(
                    ordinal(index, bound)?,
                ))?;

                locals.push(((*parameter).into(), term));
            }
        }

        for parameter in anonymous_parameters {
            let term = values
                .intern_constant_term(ConstantTermData::CallableArgument(parameter.ordinal()))?;

            locals.push((parameter.id().into(), term));
        }

        locals.extend(
            self.symbolic_body_bindings(bound, result_ordinal)?
                .into_iter()
                .map(|(binding, term)| (binding.into(), term)),
        );

        let resolver = CompilationConstantCallResolver::new(self, cancellation);
        let unit = checker_unit_view(bound, &semantic_context, &context)?;
        let mut conditions = Vec::with_capacity(roots.len());

        for root in roots {
            cancellation.check()?;

            let input = ConstantEvaluationInput::new(semantics.types(), semantics.selections())
                .with_root(*root)
                .with_patterns(patterns.result().value())
                .with_references(references.iter().copied())
                .with_local_terms(locals.iter().copied())
                .with_call_resolver(&resolver)
                .with_nested_term_types();

            let checked = checker_result(DefaultConstantChecker.check_constant_term(unit, &input))?;

            // Failure to retain a bounded symbolic expression is lack of evidence. The owning
            // semantic queries, not this optional representation, report source validity errors.
            conditions.push((!checked.diagnostics().has_errors()).then_some(*checked.value()));
        }

        Ok((result_ordinal, conditions))
    }

    pub(in crate::compilation) fn symbolic_body_bindings(
        &self,
        bound: &BoundUnit,
        result_ordinal: SymbolOrdinal,
    ) -> Result<BTreeMap<bray_symbols::LocalBindingSymbolId, ConstantTermId>, FactQueryError> {
        let mut bindings = BTreeMap::new();

        if !matches!(
            bound.root(),
            bray_bound_tree::BoundUnitRoot::CallableBody { .. }
                | bray_bound_tree::BoundUnitRoot::AnonymousCallable { .. }
        ) {
            return Ok(bindings);
        }

        let values = self.semantic_value_store()?;
        let next = u64::from(result_ordinal.raw()) + 1;

        let overflow = |value| {
            SemanticQueryFailure::contract(
                SemanticQueryContext::Unit(bound.key().clone()),
                SemanticQueryViolation::CountOverflow {
                    data: SemanticDataKind::Symbol,
                    value,
                },
            )
        };

        let start = usize::try_from(next).map_err(|_| overflow(next))?;

        for (index, binding) in bound.local_symbols().bindings().iter().enumerate() {
            let index = start.checked_add(index).ok_or_else(|| overflow(u64::MAX))?;
            let parameter = ordinal(index, bound)?;

            let term =
                values.intern_constant_term(ConstantTermData::CallableArgument(parameter))?;

            bindings.insert(binding.id(), term);
        }

        Ok(bindings)
    }

    pub(in crate::compilation) fn symbolic_storage_inputs(
        &self,
        bound: &BoundUnit,
        storage: &bray_bound_tree::StoragePlan,
        locals: &BTreeMap<bray_symbols::LocalBindingSymbolId, ConstantTermId>,
        cancellation: &CancellationToken,
    ) -> Result<BTreeMap<bray_bound_tree::StorageIdentityId, ConstantTermId>, FactQueryError> {
        use bray_bound_tree::{StorageBinding, StorageBindingTarget};

        let binding = self.binding_context(cancellation)?;
        let arguments = predicate_arguments(&binding, bound)?;
        let values = self.semantic_value_store()?;

        let mut observations = BTreeMap::new();

        for (target, binding) in storage.bindings() {
            let identity = match binding {
                StorageBinding::Identity(identity) => Some(*identity),
                StorageBinding::Access(access) if storage.is_root_access(*access) => {
                    storage.root_identity(*access)
                }
                _ => None,
            };

            let Some(identity) = identity else {
                continue;
            };

            let parameter = match target {
                StorageBindingTarget::Parameter(parameter) => {
                    arguments.get(&(*parameter).into()).copied()
                }
                StorageBindingTarget::Receiver(parameter) => {
                    arguments.get(&(*parameter).into()).copied()
                }
                StorageBindingTarget::AnonymousParameter(parameter) => bound
                    .local_symbols()
                    .anonymous_parameters()
                    .iter()
                    .find(|symbol| symbol.id() == *parameter)
                    .map(|symbol| symbol.ordinal()),
                _ => None,
            };

            let term = match target {
                StorageBindingTarget::Local(local) => locals.get(local).copied(),
                _ => parameter
                    .map(|parameter| {
                        values.intern_constant_term(ConstantTermData::CallableArgument(parameter))
                    })
                    .transpose()?,
            };

            if let Some(term) = term {
                observations.insert(identity, term);
            }
        }

        Ok(observations)
    }
}

fn predicate_arguments(
    context: &CompilationBindingContext<'_>,
    bound: &BoundUnit,
) -> Result<BTreeMap<AnySymbolId, SymbolOrdinal>, FactQueryError> {
    if bound.contract_inputs().is_some()
        || matches!(
            bound.root(),
            bray_bound_tree::BoundUnitRoot::AnonymousCallable { .. }
        )
    {
        return Ok(BTreeMap::new());
    }

    let owner = context
        .compilation()
        .symbol_graph()?
        .symbol_for_key(bound.key().declared_owner())
        .ok_or_else(|| {
            SemanticQueryFailure::contract(
                SemanticQueryContext::Unit(bound.key().clone()),
                SemanticQueryViolation::Missing(SemanticDataKind::SymbolKey),
            )
        })?;

    let parameters = if let Some(callable) = CallableSymbolId::try_from_any(owner) {
        let signature = context
            .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(callable))
            .map_err(binding_query_error)?;

        signature
            .value()
            .receiver()
            .map(|receiver| AnySymbolId::from(receiver.parameter()))
            .into_iter()
            .chain(
                signature
                    .value()
                    .parameters()
                    .iter()
                    .copied()
                    .map(AnySymbolId::from),
            )
            .collect::<Vec<_>>()
    } else if let Some(predicate) = PredicateDefinitionSymbolId::try_from_any(owner) {
        let signature = context
            .resolve_symbol_query(SymbolQueryRequest::<PredicateSignatureTemplateQuery>::new(
                predicate,
            ))
            .map_err(binding_query_error)?;

        signature
            .value()
            .parameters()
            .iter()
            .map(|parameter| parameter.parameter().into())
            .collect()
    } else {
        Vec::new()
    };

    parameters
        .into_iter()
        .enumerate()
        .map(|(index, parameter)| Ok((parameter, ordinal(index, bound)?)))
        .collect()
}

fn ordinal(index: usize, bound: &BoundUnit) -> Result<SymbolOrdinal, FactQueryError> {
    u32::try_from(index).map(SymbolOrdinal::new).map_err(|_| {
        SemanticQueryFailure::contract(
            SemanticQueryContext::Unit(bound.key().clone()),
            SemanticQueryViolation::CountOverflow {
                data: SemanticDataKind::Symbol,
                value: u64::try_from(index).unwrap_or(u64::MAX),
            },
        )
        .into()
    })
}

#[cfg(test)]
mod tests {
    use crate::test_support::{compilation, source_function};
    use bray_binder::SymbolQueryProvider;
    use bray_symbols::CallableConditions;
    use bray_symbols::{
        CallableContractsQuery, CallableSymbolId, ConstantTermData, PredicateDefinitionState,
        PredicateDefinitionSymbolId, ProofOutcome, SymbolOrdinal, SymbolQueryRequest,
    };

    #[test]
    fn guarded_predicate_meaning_keeps_entry_arguments_separate_from_results() {
        let compilation = compilation(
            "module app; func identity(pos ready: bool) -> bool when(ready) { executes(pure, total) ensures(result) } { return ready; }",
        );

        let binding = compilation
            .binding_context(&compilation.state.cancellation)
            .unwrap();

        let owner = CallableSymbolId::from(source_function(&compilation, "identity"));

        let contract = binding
            .resolve_symbol_query(SymbolQueryRequest::<CallableContractsQuery>::new(owner))
            .unwrap();

        assert!(
            contract.diagnostics().is_empty(),
            "{:?}",
            contract.diagnostics()
        );

        let guard = contract.value().conditions().entry_guards()[0]
            .predicate()
            .unwrap()
            .condition()
            .unwrap();

        let postcondition = contract.value().conditions().guarded_postconditions()[0]
            .predicate()
            .unwrap()
            .condition()
            .unwrap();

        let values = compilation.semantic_value_store().unwrap();

        let argument = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .unwrap();

        let result = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(1)))
            .unwrap();

        assert_eq!(
            bray_checker::prove_condition(values, &[(argument, true)], guard),
            Ok(ProofOutcome::Proven)
        );

        assert_eq!(
            bray_checker::prove_condition(values, &[(argument, true)], postcondition),
            Ok(ProofOutcome::Unknown)
        );

        assert_eq!(
            bray_checker::prove_condition(values, &[(result, true)], postcondition),
            Ok(ProofOutcome::Proven)
        );
    }

    #[test]
    fn guarded_predicate_meaning_retains_shared_observations_without_address_values() {
        let compilation = compilation(
            "module app; struct Flag { ready: bool; predicate complete(value: &Self) = value.ready; } func check(pos value: Flag) when(Flag.complete(&value)) { executes(pure) } {}",
        );

        let symbols = compilation.symbol_graph().unwrap();

        let predicate = symbols
            .predicates()
            .iter()
            .find(|predicate| predicate.origin() == bray_symbols::SymbolOrigin::Source)
            .unwrap();

        let definition = compilation
            .predicate_definition(PredicateDefinitionSymbolId::Predicate(predicate.id()))
            .unwrap();

        assert!(
            definition.diagnostics().is_empty(),
            "{:?}",
            definition.diagnostics()
        );

        let PredicateDefinitionState::Defined(definition) = definition.value() else {
            panic!("source predicate must retain its definition");
        };

        assert!(definition.semantic().condition().is_some());

        let binding = compilation
            .binding_context(&compilation.state.cancellation)
            .unwrap();

        let owner = CallableSymbolId::from(source_function(&compilation, "check"));

        let contract = binding
            .resolve_symbol_query(SymbolQueryRequest::<CallableContractsQuery>::new(owner))
            .unwrap();

        assert!(
            contract.diagnostics().is_empty(),
            "{:?}",
            contract.diagnostics()
        );

        assert!(
            contract.value().conditions().entry_guards()[0]
                .predicate()
                .unwrap()
                .condition()
                .is_some()
        );

        let guard = contract.value().conditions().entry_guards()[0]
            .predicate()
            .unwrap()
            .condition()
            .unwrap();

        let expanded = compilation
            .expanded_predicate_condition(guard, &compilation.state.cancellation, |predicate| {
                Ok(Some(predicate))
            })
            .unwrap()
            .unwrap();

        assert_ne!(expanded, guard);

        assert_eq!(
            compilation
                .expanded_predicate_condition(
                    expanded,
                    &compilation.state.cancellation,
                    |predicate| Ok(Some(predicate))
                )
                .unwrap(),
            Some(expanded)
        );
    }
}
