use bray_symbols::CallableConditions;

use bray_symbols::{
    CallableConditionSet, CallableExecutionGuarantee, ConstantBinaryOperation, ConstantTermData,
    ConstantTermId, ProofOutcome, SemanticValueStore, SemanticValueStoreError, SymbolOrdinal,
};

use crate::contract::{MAX_CONDITION_STEPS, prove_condition};

/// Checks projection promises and retains the selected implementation obligations.
pub fn storage_projection_proof_dependencies(
    values: &SemanticValueStore,
    conditions: &CallableConditionSet,
    callable: bray_symbols::CallableInstanceData,
    site: bray_bound_tree::AnyBoundNodeId,
) -> Result<Option<Vec<bray_bound_tree::CallableProofDependency>>, SemanticValueStoreError> {
    let required = CallableConditionSet::new([]).with_execution_guarantees(
        [
            bray_symbols::ExecutionProperty::Pure,
            bray_symbols::ExecutionProperty::Total,
        ]
        .map(|property| CallableExecutionGuarantee::new(property, None)),
    );

    if check_execution_guarantee_implication::<SemanticValueStoreError>(
        values,
        &required,
        conditions,
        |term| Ok(Some(term)),
        |term| Ok(Some(term)),
    )?
    .is_some()
    {
        return Ok(None);
    }

    Ok(Some(
        conditions
            .execution_guarantees()
            .iter()
            .map(|guarantee| {
                bray_bound_tree::CallableProofDependency::new(
                    bray_bound_tree::CallableProofTarget::Implicit { site, callable },
                    bray_bound_tree::CallableProofObligation::Execution(*guarantee),
                )
            })
            .collect(),
    ))
}

/// Returns closed execution evidence for compiler-provided storage projection implementations.
/// Source implementations establish these properties through ordinary body checking.
pub fn compiler_projection_guarantees<C: crate::CheckerRequestContext + ?Sized>(
    context: &C,
    callable: bray_symbols::CallableSymbolId,
) -> Result<Vec<CallableExecutionGuarantee>, crate::CheckerQueryError<C::UpstreamError>> {
    let available = context.available_compiler_known_symbols();

    let heap = ["HeapStorageBorrow", "HeapStorageBorrowMut"]
        .into_iter()
        .any(|name| {
            bray_compiler_known::CompilerKnownDeclarationKey::try_new(name)
                .and_then(|key| {
                    available
                        .declaration_symbol::<bray_symbols::TraitCallableFulfillmentSymbolId>(&key)
                })
                .is_some_and(|symbol| bray_symbols::CallableSymbolId::from(symbol) == callable)
        });

    let anchored = context
        .implementation_hook(callable.into_any())?
        .is_some_and(|resolution| {
            resolution.is_available()
                && matches!(
                    resolution.hook(),
                    bray_compiler_known::ImplementationHook::BorrowFrom
                        | bray_compiler_known::ImplementationHook::BorrowMutFrom
                        | bray_compiler_known::ImplementationHook::UninitPointer
                        | bray_compiler_known::ImplementationHook::UninitPointerMut
                )
        });

    Ok(if heap || anchored {
        [
            bray_symbols::ExecutionProperty::Pure,
            bray_symbols::ExecutionProperty::Total,
        ]
        .map(|property| CallableExecutionGuarantee::new(property, None))
        .into()
    } else {
        Vec::new()
    })
}

/// Returns the first required execution promise not implied by the provided contract.
///
/// The normalizers align generic arguments, contextual types, and transparent predicates with
/// the same checked receiver-first input identities. Missing meaning and bounded reasoning
/// exhaustion are not evidence. This checks contract implication, not implementation validity.
/// Callers must separately verify provided bodies and ordinary precondition compatibility.
pub fn check_execution_guarantee_implication<E>(
    values: &SemanticValueStore,
    required: &CallableConditionSet,
    provided: &CallableConditionSet,
    mut normalize_required: impl FnMut(ConstantTermId) -> Result<Option<ConstantTermId>, E>,
    mut normalize_provided: impl FnMut(ConstantTermId) -> Result<Option<ConstantTermId>, E>,
) -> Result<Option<CallableExecutionGuarantee>, E>
where
    E: From<SemanticValueStoreError>,
{
    if required.execution_guarantees().is_empty() {
        return Ok(None);
    }

    let mut remaining = MAX_CONDITION_STEPS;

    if required.invocation_preconditions().len() > remaining
        || required.entry_guards().len() > remaining
        || provided.entry_guards().len() > remaining
    {
        return Ok(required.execution_guarantees().first().copied());
    }

    let mut preconditions = Vec::new();

    for clause in required.invocation_preconditions() {
        if let Some(condition) = clause
            .predicate()
            .and_then(|predicate| predicate.condition())
        {
            if let Some(condition) = normalize_required(condition)? {
                preconditions.push((condition, true));
            }
        }
    }

    for guarantee in required.execution_guarantees() {
        let Some(guards) = contract_guard_conditions(required, guarantee.guard()) else {
            return Ok(Some(*guarantee));
        };

        // Each domain adds independent entry assumptions to these copyable term identities.
        let mut conditions = preconditions.clone();

        for guard in guards {
            let Some(guard) = normalize_required(guard)? else {
                return Ok(Some(*guarantee));
            };

            conditions.push((guard, true));
        }

        if execution_property_coverage(
            values,
            provided,
            guarantee.property(),
            &conditions,
            &mut normalize_provided,
            &mut remaining,
        )?
        .is_none()
        {
            return Ok(Some(*guarantee));
        }
    }

    Ok(None)
}

/// Selects declared domains covering one execution point. Each returned declaration still needs
/// implementation evidence. Domains excluded by the supplied conditions create no dependency.
pub(crate) fn execution_property_coverage<E>(
    values: &SemanticValueStore,
    provided: &CallableConditionSet,
    property: bray_symbols::ExecutionProperty,
    assumptions: &[(ConstantTermId, bool)],
    normalize: &mut impl FnMut(ConstantTermId) -> Result<Option<ConstantTermId>, E>,
    remaining: &mut usize,
) -> Result<Option<Vec<CallableExecutionGuarantee>>, E>
where
    E: From<SemanticValueStoreError>,
{
    if crate::contract::conditions_are_inconsistent(values, assumptions)? {
        return Ok(Some(Vec::new()));
    }

    let mut domain = None;
    let mut dependencies = Vec::new();

    for candidate in provided.execution_guarantees() {
        let Some(next) = remaining.checked_sub(1) else {
            return Ok(None);
        };

        *remaining = next;

        if candidate.property() != property {
            continue;
        }

        let Some(guards) = contract_guard_conditions(provided, candidate.guard()) else {
            continue;
        };

        let Some(next) = remaining.checked_sub(guards.len()) else {
            return Ok(None);
        };

        *remaining = next;

        if guards.is_empty() {
            return Ok(Some(vec![*candidate]));
        }

        let mut candidate_domain = None;
        let mut unavailable = false;

        for guard in guards {
            let Some(guard) = normalize(guard)? else {
                unavailable = true;

                break;
            };

            candidate_domain = Some(combine(
                values,
                candidate_domain,
                guard,
                ConstantBinaryOperation::LogicalAnd,
            )?);
        }

        if !unavailable && let Some(candidate_domain) = candidate_domain {
            match prove_condition(values, assumptions, candidate_domain)? {
                ProofOutcome::Proven => return Ok(Some(vec![*candidate])),
                ProofOutcome::Disproven => continue,
                ProofOutcome::Unknown => {}
                ProofOutcome::Recovered => continue,
            }

            domain = Some(combine(
                values,
                domain,
                candidate_domain,
                ConstantBinaryOperation::LogicalOr,
            )?);

            dependencies.push(*candidate);
        }
    }

    let covered = match domain {
        Some(domain) => prove_condition(values, assumptions, domain)? == ProofOutcome::Proven,
        None => false,
    };

    Ok(covered.then_some(dependencies))
}

/// Collects an entry guard and all enclosing guards, without treating unavailable meaning as true.
///
/// Guard ordinals must refer backwards to existing guards in the same contract. An unconditional
/// domain has no conditions. Invalid ancestry or a bounded-work overflow returns no evidence.
pub fn contract_guard_conditions(
    contract: &CallableConditionSet,
    mut guard: Option<SymbolOrdinal>,
) -> Option<Vec<ConstantTermId>> {
    let mut conditions = Vec::new();

    while let Some(ordinal) = guard {
        if conditions.len() >= MAX_CONDITION_STEPS {
            return None;
        }

        let clause = contract
            .entry_guards()
            .iter()
            .find(|clause| clause.ordinal() == ordinal)?;

        if clause.guard().is_some_and(|parent| parent >= ordinal) {
            return None;
        }

        conditions.push(clause.predicate()?.condition()?);
        guard = clause.guard();
    }

    Some(conditions)
}

fn combine(
    values: &SemanticValueStore,
    previous: Option<ConstantTermId>,
    next: ConstantTermId,
    operation: ConstantBinaryOperation,
) -> Result<ConstantTermId, SemanticValueStoreError> {
    match previous {
        Some(previous) => values.intern_constant_term(ConstantTermData::Binary {
            operation,
            left: previous,
            right: next,
        }),
        None => Ok(next),
    }
}

impl crate::ExecutionCallInput {
    /// Whether selected inputs can supply execution-contract observations before querying promises.
    pub fn supports_selected_call(
        call: &bray_bound_tree::SelectedCall,
        inputs: &[(
            crate::ExecutionCallArgument,
            Option<&bray_bound_tree::SelectedConversion>,
        )],
    ) -> bool {
        matches!(
            call.resolution().result(),
            bray_bound_tree::BoundCallResult::Immediate(_)
        ) && inputs.iter().all(|(_, conversion)| {
            conversion.is_none_or(|conversion| {
                matches!(
                    conversion.target(),
                    bray_bound_tree::ConversionTarget::Identity
                )
            })
        }) && (matches!(
            call.target(),
            bray_bound_tree::BoundCallableTarget::Indirect(_)
        ) || !call.arguments().iter().any(|argument| {
            matches!(
                argument,
                bray_bound_tree::SelectedArgument::Explicit {
                    parameter: None,
                    ..
                }
            )
        }))
    }

    /// Returns candidate contracts for a nonvariadic indirect callable, without certifying them.
    pub fn indirect_conditions(
        values: &SemanticValueStore,
        ty: bray_symbols::TypeId,
    ) -> Result<Option<CallableConditionSet>, SemanticValueStoreError> {
        let data = values.type_data(ty)?;

        let bray_symbols::TypeData::Callable(callable) = data.as_ref() else {
            return Ok(None);
        };

        // Normalization owns an Arc-backed copy. The semantic callable type remains immutable.
        Ok((!callable.is_variadic()).then(|| callable.conditions().clone()))
    }

    /// Interprets selected inputs as execution observations after their contracts are normalized.
    pub fn from_selected_call(
        values: &SemanticValueStore,
        bound: &bray_bound_tree::BoundUnit,
        expressions: &bray_bound_tree::CheckedExpressionSemantics,
        call: &bray_bound_tree::SelectedCall,
        inputs: &[(
            crate::ExecutionCallArgument,
            Option<&bray_bound_tree::SelectedConversion>,
        )],
        conditions: CallableConditionSet,
    ) -> Result<Self, SemanticValueStoreError> {
        let mut borrowed = std::collections::BTreeSet::new();
        let mut arguments = Vec::with_capacity(inputs.len());

        for (argument, conversion) in inputs {
            let crate::ExecutionCallArgument::Expression(argument) = argument else {
                arguments.push(*argument);
                continue;
            };

            let ty = conversion
                .map(|conversion| conversion.target_type())
                .or_else(|| {
                    call.receiver()
                        .filter(|receiver| receiver.expression() == *argument)
                        .map(|receiver| receiver.target_type())
                })
                .or_else(|| {
                    expressions
                        .types()
                        .expression(*argument)
                        .map(|result| result.ty())
                });

            let is_borrowed = match ty {
                Some(ty) => matches!(
                    &*values.type_data(ty)?,
                    bray_symbols::TypeData::Borrow { .. }
                ),
                None => false,
            } || call.receiver().is_some_and(|receiver| {
                receiver.expression() == *argument
                    && matches!(
                        receiver.mode(),
                        bray_symbols::ReceiverMode::Shared | bray_symbols::ReceiverMode::Mutable
                    )
            });

            let mut observed = *argument;

            if is_borrowed {
                // Execution contracts observe the borrowed referent, not a materialized address.
                // Keep this interpretation out of constant-value and exported-template evaluation.
                if let Some(bray_bound_tree::BoundExpression::Structured(expression)) =
                    bound.tree().expression(*argument)
                    && expression.kind() == bray_bound_tree::BoundStructuredExpressionKind::Borrow
                    && let [referent] = expression.operands()
                {
                    observed = *referent;
                }

                borrowed.insert(observed);
            }

            arguments.push(crate::ExecutionCallArgument::Expression(observed));
        }

        Ok(Self::new(conditions, arguments, borrowed))
    }
}

#[cfg(test)]
mod tests {
    use super::{check_execution_guarantee_implication, contract_guard_conditions};
    use bray_symbols::CallableConditions;
    use bray_symbols::{
        CallableConditionSet, CallableContractClause, CallableContractClauseKind,
        CallableExecutionGuarantee, ConstantTermData, ConstantTermId, ExecutionProperty,
        PredicateSemanticSummary, SemanticValueStore, SemanticValueStoreError, SymbolOrdinal,
    };

    fn contract(
        values: &SemanticValueStore,
        guards: &[(u32, Option<u32>, Option<ConstantTermId>)],
        properties: &[(ExecutionProperty, Option<u32>)],
    ) -> CallableConditionSet {
        let dependency = values.empty_dependency_contract_template().unwrap();

        let clauses = guards.iter().map(|(ordinal, parent, condition)| {
            let predicate = PredicateSemanticSummary::new(dependency);

            let predicate = match condition {
                Some(condition) => predicate.with_condition(Some(*condition)),
                None => predicate,
            };

            CallableContractClause::new(
                SymbolOrdinal::new(*ordinal),
                CallableContractClauseKind::Guard,
                predicate,
            )
            .with_guard(parent.map(SymbolOrdinal::new))
        });

        CallableConditionSet::new(clauses).with_execution_guarantees(properties.iter().map(
            |(property, guard)| {
                CallableExecutionGuarantee::new(*property, guard.map(SymbolOrdinal::new))
            },
        ))
    }

    fn mismatch(
        values: &SemanticValueStore,
        required: &CallableConditionSet,
        provided: &CallableConditionSet,
    ) -> Option<CallableExecutionGuarantee> {
        check_execution_guarantee_implication::<SemanticValueStoreError>(
            values,
            required,
            provided,
            |term| Ok(Some(term)),
            |term| Ok(Some(term)),
        )
        .unwrap()
    }

    #[test]
    fn execution_guarantee_implication_uses_domains_not_clause_positions() {
        let values = SemanticValueStore::try_new().unwrap();

        let first = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .unwrap();

        let second = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(1)))
            .unwrap();

        let required = contract(
            &values,
            &[(0, None, Some(first)), (1, Some(0), Some(second))],
            &[(ExecutionProperty::Total, Some(1))],
        );

        let provided = contract(
            &values,
            &[(7, None, Some(second))],
            &[
                (ExecutionProperty::Total, Some(7)),
                (ExecutionProperty::Pure, None),
            ],
        );

        assert_eq!(mismatch(&values, &required, &provided), None);

        assert_eq!(
            mismatch(&values, &provided, &required),
            Some(CallableExecutionGuarantee::new(
                ExecutionProperty::Pure,
                None
            ))
        );

        assert_eq!(
            contract_guard_conditions(&required, Some(SymbolOrdinal::new(1))),
            Some(vec![second, first])
        );
    }

    #[test]
    fn execution_guarantee_implication_never_invents_a_property_or_entry_fact() {
        let values = SemanticValueStore::try_new().unwrap();

        let condition = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .unwrap();

        let required = contract(&values, &[], &[(ExecutionProperty::Total, None)]);

        let guarded = contract(
            &values,
            &[(0, None, Some(condition))],
            &[(ExecutionProperty::Total, Some(0))],
        );

        let missing = contract(&values, &[], &[]);
        let pure = contract(&values, &[], &[(ExecutionProperty::Pure, None)]);

        for provided in [&guarded, &missing, &pure] {
            assert_eq!(
                mismatch(&values, &required, provided),
                Some(CallableExecutionGuarantee::new(
                    ExecutionProperty::Total,
                    None
                ))
            );
        }
    }

    #[test]
    fn execution_coverage_retains_only_needed_domains() {
        let values = SemanticValueStore::try_new().unwrap();

        let ready = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .unwrap();

        let not_ready = values
            .intern_constant_term(ConstantTermData::Unary {
                operation: bray_symbols::ConstantUnaryOperation::LogicalNot,
                operand: ready,
            })
            .unwrap();

        let provided = contract(
            &values,
            &[(0, None, Some(not_ready)), (1, None, Some(ready))],
            &[
                (ExecutionProperty::Total, Some(0)),
                (ExecutionProperty::Total, Some(1)),
            ],
        );

        for (assumptions, expected) in [
            (
                vec![(ready, true)],
                vec![CallableExecutionGuarantee::new(
                    ExecutionProperty::Total,
                    Some(SymbolOrdinal::new(1)),
                )],
            ),
            (vec![], provided.execution_guarantees().to_vec()),
            (vec![(ready, true), (ready, false)], vec![]),
        ] {
            let mut remaining = crate::contract::MAX_CONDITION_STEPS;

            let coverage = super::execution_property_coverage::<SemanticValueStoreError>(
                &values,
                &provided,
                ExecutionProperty::Total,
                &assumptions,
                &mut |term| Ok(Some(term)),
                &mut remaining,
            )
            .unwrap();

            assert_eq!(coverage, Some(expected));
        }
    }

    #[test]
    fn execution_guarantee_implication_rejects_unavailable_or_invalid_guard_ancestry() {
        let values = SemanticValueStore::try_new().unwrap();

        let condition = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .unwrap();

        let required = contract(&values, &[], &[(ExecutionProperty::Total, None)]);

        for guards in [
            vec![(0, None, None)],
            vec![(0, Some(0), Some(condition))],
            vec![(1, Some(0), Some(condition))],
        ] {
            let provided = contract(
                &values,
                &guards,
                &[(
                    ExecutionProperty::Total,
                    guards.first().map(|guard| guard.0),
                )],
            );

            assert!(mismatch(&values, &required, &provided).is_some());
        }
    }
}
