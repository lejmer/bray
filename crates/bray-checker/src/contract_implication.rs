use bray_symbols::CallableConditions;

use std::collections::BTreeMap;

use bray_symbols::{
    CallableConditionSet, CallableContractClause, CallableExecutionGuarantee,
    ConstantBinaryOperation, ConstantTermData, ConstantTermId, ConstantUnaryOperation,
    ProofOutcome, SemanticValueStore, SemanticValueStoreError, SymbolOrdinal,
};

use crate::contract::{
    MAX_CONDITION_STEPS, callable_input_count, instantiate_condition, prove_condition,
};
use crate::guarantee::{check_execution_guarantee_implication, contract_guard_conditions};

/// Checks a representation-preserving callable contract adaptation.
///
/// Parameters, result, execution mode, ABI, ownership, caller-visible phase behavior, and static constraints
/// retain their exact contracts. Implementation-side trust and capability acknowledgments remain
/// properties of the supplied body. Ordinary and conditional guarantees may be weakened when
/// bounded implication establishes substitutability. The supplied callable must already be
/// valid. This comparison does not certify its implementation or execute a contract predicate.
pub fn callable_type_contract_is_compatible(
    values: &SemanticValueStore,
    required: &bray_symbols::CallableTypeData,
    provided: &bray_symbols::CallableTypeData,
) -> Result<bool, SemanticValueStoreError> {
    check_callable_type_contract(values, required, provided).map(|mismatch| mismatch.is_none())
}

/// The part of a callable value's contract that prevents substitution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CallableTypeContractMismatch {
    /// Parameters, result, execution, ABI, dependencies, or static constraints differ.
    Signature,
    /// A caller obligation or guarantee fails bounded implication.
    Condition(CallableConditionMismatch),
}

/// Compares callable value contracts while retaining the exact condition failure.
pub fn check_callable_type_contract(
    values: &SemanticValueStore,
    required: &bray_symbols::CallableTypeData,
    provided: &bray_symbols::CallableTypeData,
) -> Result<Option<CallableTypeContractMismatch>, SemanticValueStoreError> {
    if required.parameters() != provided.parameters()
        || required.is_variadic() != provided.is_variadic()
        || required.result() != provided.result()
        || required.constness() != provided.constness()
        || required.abi() != provided.abi()
        || !phase_contract_matches(
            required.phase_behaviors().invocation(),
            provided.phase_behaviors().invocation(),
        )
        || !match (
            required.phase_behaviors().deferred_execution(),
            provided.phase_behaviors().deferred_execution(),
        ) {
            (Some(required), Some(provided)) => phase_contract_matches(required, provided),
            (None, None) => true,
            _ => false,
        }
        || required.static_constraints() != provided.static_constraints()
    {
        return Ok(Some(CallableTypeContractMismatch::Signature));
    }

    check_callable_condition_implication::<SemanticValueStoreError>(
        values,
        required.conditions(),
        provided.conditions(),
        |condition| Ok(Some(condition)),
        |condition| Ok(Some(condition)),
    )
    .map(|mismatch| mismatch.map(CallableTypeContractMismatch::Condition))
}

fn phase_contract_matches(
    required: &bray_symbols::CallablePhaseBehavior,
    provided: &bray_symbols::CallablePhaseBehavior,
) -> bool {
    required.effects() == provided.effects()
        && required.capabilities() == provided.capabilities()
        && provided.execution_requirements_are_subset_of(required)
        && required.lifecycle_obligations() == provided.lifecycle_obligations()
        && required.dependency_contract() == provided.dependency_contract()
        && required.current_run_cancellation() == provided.current_run_cancellation()
}

/// A declaration contract that cannot be substituted for a required callable contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CallableConditionMismatch {
    /// A provided precondition does not follow from the required preconditions.
    InvocationPrecondition(SymbolOrdinal),
    /// A required normal-completion guarantee does not follow from provided guarantees.
    CompletionPostcondition(SymbolOrdinal),
    /// An execution property is unavailable on its required entry domain.
    ExecutionGuarantee(CallableExecutionGuarantee),
    /// Contract comparison exceeded its bounded reasoning work.
    ReasoningLimit,
}

impl CallableConditionMismatch {
    /// Preserves this condition failure in a locale-neutral diagnostic argument.
    pub fn diagnostic(self) -> bray_diagnostics::DiagnosticCallableContractMismatch {
        use bray_diagnostics::{
            DiagnosticCallableContractMismatch as Mismatch, DiagnosticCallableContractSurface,
            DiagnosticExecutionProperty,
        };

        match self {
            Self::InvocationPrecondition(ordinal) => Mismatch::PredicateImplication {
                surface: DiagnosticCallableContractSurface::InvocationPreconditions,
                index: u64::from(ordinal.raw()),
            },
            Self::CompletionPostcondition(ordinal) => Mismatch::PredicateImplication {
                surface: DiagnosticCallableContractSurface::CompletionPostconditions,
                index: u64::from(ordinal.raw()),
            },
            Self::ExecutionGuarantee(guarantee) => Mismatch::ExecutionGuarantee {
                property: match guarantee.property() {
                    bray_symbols::ExecutionProperty::Pure => DiagnosticExecutionProperty::Pure,
                    bray_symbols::ExecutionProperty::Total => DiagnosticExecutionProperty::Total,
                },
                guard: guarantee.guard().map(|guard| u64::from(guard.raw())),
            },
            Self::ReasoningLimit => Mismatch::ConditionReasoningLimit,
        }
    }
}

/// Checks substitution of ordinary and conditional callable conditions.
///
/// Normalizers supply checked conditions with aligned receiver-first inputs and contextual
/// types. Entry observations are distinct from normal-exit observations, so a mutable entry
/// predicate cannot prove an exit predicate. This does not certify either implementation and
/// does not replace compatibility checks for types, trust, static constraints, or dependencies.
pub fn check_callable_condition_implication<E>(
    values: &SemanticValueStore,
    required: &CallableConditionSet,
    provided: &CallableConditionSet,
    normalize_required: impl FnMut(ConstantTermId) -> Result<Option<ConstantTermId>, E>,
    normalize_provided: impl FnMut(ConstantTermId) -> Result<Option<ConstantTermId>, E>,
) -> Result<Option<CallableConditionMismatch>, E>
where
    E: From<SemanticValueStoreError>,
{
    if required
        .clauses()
        .chain(provided.clauses())
        .take(MAX_CONDITION_STEPS + 1)
        .count()
        > MAX_CONDITION_STEPS
    {
        return Ok(Some(CallableConditionMismatch::ReasoningLimit));
    }

    let required = NormalizedContract::new(required, normalize_required)?;
    let provided = NormalizedContract::new(provided, normalize_provided)?;

    let requirements = required
        .contract
        .invocation_preconditions()
        .iter()
        .filter_map(|clause| required.condition(*clause).map(|term| (term, true)))
        .collect::<Vec<_>>();

    for clause in provided.contract.invocation_preconditions() {
        let Some(condition) = provided.condition(*clause) else {
            return Ok(Some(CallableConditionMismatch::InvocationPrecondition(
                clause.ordinal(),
            )));
        };

        if prove_condition(values, &requirements, condition)? != ProofOutcome::Proven {
            return Ok(Some(CallableConditionMismatch::InvocationPrecondition(
                clause.ordinal(),
            )));
        }
    }

    if let Some(guarantee) = check_execution_guarantee_implication::<E>(
        values,
        required.contract,
        provided.contract,
        |term| Ok(required.meaning(term)),
        |term| Ok(provided.meaning(term)),
    )? {
        return Ok(Some(CallableConditionMismatch::ExecutionGuarantee(
            guarantee,
        )));
    }

    if required
        .contract
        .normal_completion_postconditions()
        .is_empty()
        && required.contract.guarded_postconditions().is_empty()
    {
        return Ok(None);
    }

    let Some(arguments) = entry_observation_arguments(values, &required, &provided)? else {
        return Ok(Some(CallableConditionMismatch::ReasoningLimit));
    };

    let completion = provided.completion_conditions(values, &arguments)?;
    let mut entry = Vec::new();

    for (requirement, truth) in requirements {
        if let Some(requirement) = instantiate_condition(values, requirement, &arguments)? {
            entry.push((requirement, truth));
        }
    }

    for clause in required
        .contract
        .normal_completion_postconditions()
        .iter()
        .chain(required.contract.guarded_postconditions())
    {
        let Some(goal) = required.condition(*clause) else {
            return Ok(Some(CallableConditionMismatch::CompletionPostcondition(
                clause.ordinal(),
            )));
        };

        let Some(guards) = required.entry_conditions(values, clause.guard(), &arguments)? else {
            return Ok(Some(CallableConditionMismatch::CompletionPostcondition(
                clause.ordinal(),
            )));
        };

        let assumptions = entry
            .iter()
            .copied()
            .chain(completion.iter().copied())
            .chain(guards.into_iter().map(|guard| (guard, true)))
            .collect::<Vec<_>>();

        if prove_condition(values, &assumptions, goal)? != ProofOutcome::Proven {
            return Ok(Some(CallableConditionMismatch::CompletionPostcondition(
                clause.ordinal(),
            )));
        }
    }

    Ok(None)
}

struct NormalizedContract<'a> {
    contract: &'a CallableConditionSet,
    conditions: BTreeMap<ConstantTermId, Option<ConstantTermId>>,
}

impl<'a> NormalizedContract<'a> {
    fn new<E>(
        contract: &'a CallableConditionSet,
        mut normalize: impl FnMut(ConstantTermId) -> Result<Option<ConstantTermId>, E>,
    ) -> Result<Self, E> {
        let mut conditions = BTreeMap::new();

        for clause in contract.clauses() {
            if let Some(term) = clause
                .predicate()
                .and_then(|predicate| predicate.condition())
            {
                if let std::collections::btree_map::Entry::Vacant(entry) = conditions.entry(term) {
                    entry.insert(normalize(term)?);
                }
            }
        }

        Ok(Self {
            contract,
            conditions,
        })
    }

    fn meaning(&self, term: ConstantTermId) -> Option<ConstantTermId> {
        self.conditions.get(&term).copied().flatten()
    }

    fn condition(&self, clause: CallableContractClause) -> Option<ConstantTermId> {
        self.meaning(clause.predicate()?.condition()?)
    }

    fn entry_conditions(
        &self,
        values: &SemanticValueStore,
        guard: Option<SymbolOrdinal>,
        arguments: &[ConstantTermId],
    ) -> Result<Option<Vec<ConstantTermId>>, SemanticValueStoreError> {
        let Some(guards) = contract_guard_conditions(self.contract, guard) else {
            return Ok(None);
        };

        let mut conditions = Vec::new();

        for guard in guards {
            let Some(guard) = self.meaning(guard) else {
                return Ok(None);
            };

            let Some(guard) = instantiate_condition(values, guard, arguments)? else {
                return Ok(None);
            };

            conditions.push(guard);
        }

        Ok(Some(conditions))
    }

    fn completion_conditions(
        &self,
        values: &SemanticValueStore,
        arguments: &[ConstantTermId],
    ) -> Result<Vec<(ConstantTermId, bool)>, SemanticValueStoreError> {
        let mut conditions = Vec::new();

        for clause in self
            .contract
            .normal_completion_postconditions()
            .iter()
            .chain(self.contract.guarded_postconditions())
        {
            let Some(mut condition) = self.condition(*clause) else {
                continue;
            };

            let Some(guards) = self.entry_conditions(values, clause.guard(), arguments)? else {
                continue;
            };

            for guard in guards {
                let opposite = values.intern_constant_term(ConstantTermData::Unary {
                    operation: ConstantUnaryOperation::LogicalNot,
                    operand: guard,
                })?;

                condition = values.intern_constant_term(ConstantTermData::Binary {
                    operation: ConstantBinaryOperation::LogicalOr,
                    left: opposite,
                    right: condition,
                })?;
            }

            conditions.push((condition, true));
        }

        Ok(conditions)
    }
}

fn entry_observation_arguments(
    values: &SemanticValueStore,
    required: &NormalizedContract<'_>,
    provided: &NormalizedContract<'_>,
) -> Result<Option<Vec<ConstantTermId>>, SemanticValueStoreError> {
    let roots = required
        .conditions
        .values()
        .chain(provided.conditions.values())
        .filter_map(|condition| *condition);

    let Some(count) = callable_input_count(values, roots)? else {
        return Ok(None);
    };

    // These fresh symbolic input identities exist only during implication checking. No runtime
    // value, snapshot, or ownership transfer is introduced by distinguishing entry observations.
    (0..count)
        .map(|index| {
            values.intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(
                index + count,
            )))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

#[cfg(test)]
mod tests {
    use super::{CallableConditionMismatch, check_callable_condition_implication};
    use bray_symbols::{
        CallableConditionSet, CallableContractClause, CallableContractClauseKind,
        ConstantBinaryOperation, ConstantTermData, ConstantTermId, ConstantUnaryOperation,
        PredicateSemanticSummary, SemanticValueStore, SemanticValueStoreError, SymbolOrdinal,
    };

    #[test]
    fn callable_contract_adaptation_can_forget_but_cannot_invent_guarantees() {
        use bray_symbols::{
            CallableAbi, CallableConditions, CallableConstness, CallableDependencyContracts,
            CallableExecutionGuarantee, CallableTrust, CallableTypeData, ExecutionProperty,
            TypeData,
        };

        let values = SemanticValueStore::try_new().unwrap();
        let unit = values.intern_type(TypeData::Tuple([].into())).unwrap();
        let dependency = values.empty_dependency_contract_template().unwrap();

        let plain = CallableTypeData::new(
            [],
            unit,
            CallableConstness::Runtime,
            CallableTrust::Safe,
            CallableAbi::Bray,
            CallableDependencyContracts::synchronous(dependency),
        );

        let guaranteed = plain.clone().with_execution_guarantees([
            CallableExecutionGuarantee::new(ExecutionProperty::Pure, None),
            CallableExecutionGuarantee::new(ExecutionProperty::Total, None),
        ]);

        assert_eq!(
            super::callable_type_contract_is_compatible(&values, &plain, &guaranteed),
            Ok(true)
        );

        assert_eq!(
            super::callable_type_contract_is_compatible(&values, &guaranteed, &plain),
            Ok(false)
        );

        assert_eq!(
            super::callable_type_contract_is_compatible(&values, &guaranteed, &guaranteed),
            Ok(true)
        );

        let variadic = guaranteed.clone().with_variadic(true);

        assert_eq!(
            super::callable_type_contract_is_compatible(&values, &guaranteed, &variadic),
            Ok(false)
        );

        assert_eq!(
            super::callable_type_contract_is_compatible(&values, &variadic, &guaranteed),
            Ok(false)
        );

        let restricted = plain.clone().with_conditions(contract(
            &values,
            &[(
                CallableContractClauseKind::Requires,
                0,
                None,
                Some(argument(&values, 0)),
            )],
        ));

        assert_eq!(
            super::callable_type_contract_is_compatible(&values, &plain, &restricted),
            Ok(false)
        );
    }

    #[test]
    fn phase_adaptation_allows_fewer_execution_requirements() {
        use bray_symbols::{
            CallableExecutionRequirement, CallablePhaseBehavior, CurrentRunCancellation,
            PredicateSymbolId, SymbolId,
        };

        let values = SemanticValueStore::try_new().unwrap();
        let dependency = values.empty_dependency_contract_template().unwrap();

        let first = CallableExecutionRequirement::new(
            PredicateSymbolId::from_symbol_id(SymbolId::new(1)).into(),
        );

        let second = CallableExecutionRequirement::new(
            PredicateSymbolId::from_symbol_id(SymbolId::new(2)).into(),
        );

        let phase = |requirements: &[CallableExecutionRequirement]| {
            CallablePhaseBehavior::new(
                [],
                [],
                [],
                requirements.iter().copied(),
                [],
                dependency,
                CurrentRunCancellation::NotEntered,
            )
        };

        let unrestricted = phase(&[]);
        let one_lane = phase(&[first]);
        let other_lane = phase(&[second]);
        let both_lanes = phase(&[second, first, second]);

        for (required, provided, accepted) in [
            (&unrestricted, &unrestricted, true),
            (&one_lane, &unrestricted, true),
            (&both_lanes, &one_lane, true),
            (&both_lanes, &both_lanes, true),
            (&unrestricted, &one_lane, false),
            (&one_lane, &both_lanes, false),
            (&one_lane, &other_lane, false),
        ] {
            assert_eq!(super::phase_contract_matches(required, provided), accepted);
        }
    }

    fn argument(values: &SemanticValueStore, ordinal: u32) -> ConstantTermId {
        values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(
                ordinal,
            )))
            .unwrap()
    }

    fn contract(
        values: &SemanticValueStore,
        clauses: &[(
            CallableContractClauseKind,
            u32,
            Option<u32>,
            Option<ConstantTermId>,
        )],
    ) -> CallableConditionSet {
        let dependencies = values.empty_dependency_contract_template().unwrap();

        CallableConditionSet::new(clauses.iter().map(|(kind, ordinal, guard, condition)| {
            CallableContractClause::new(
                SymbolOrdinal::new(*ordinal),
                *kind,
                PredicateSemanticSummary::new(dependencies).with_condition(*condition),
            )
            .with_guard(guard.map(SymbolOrdinal::new))
        }))
    }

    fn compare(
        values: &SemanticValueStore,
        required: &CallableConditionSet,
        provided: &CallableConditionSet,
    ) -> Option<CallableConditionMismatch> {
        check_callable_condition_implication::<SemanticValueStoreError>(
            values,
            required,
            provided,
            |term| Ok(Some(term)),
            |term| Ok(Some(term)),
        )
        .unwrap()
    }

    #[test]
    fn callable_condition_implication_does_not_reuse_mutable_entry_observations_at_exit() {
        let values = SemanticValueStore::try_new().unwrap();
        let first = argument(&values, 0);

        let required = contract(
            &values,
            &[
                (CallableContractClauseKind::Guard, 0, None, Some(first)),
                (CallableContractClauseKind::Ensures, 1, Some(0), Some(first)),
            ],
        );

        let provided = contract(&values, &[]);

        assert_eq!(
            compare(&values, &required, &provided),
            Some(CallableConditionMismatch::CompletionPostcondition(
                SymbolOrdinal::new(1)
            ))
        );

        assert_eq!(compare(&values, &required, &required), None);

        let required = contract(
            &values,
            &[
                (CallableContractClauseKind::Requires, 0, None, Some(first)),
                (CallableContractClauseKind::Ensures, 1, None, Some(first)),
            ],
        );

        assert_eq!(
            compare(&values, &required, &provided),
            Some(CallableConditionMismatch::CompletionPostcondition(
                SymbolOrdinal::new(1)
            ))
        );
    }

    #[test]
    fn callable_condition_implication_combines_all_applicable_completion_guarantees() {
        let values = SemanticValueStore::try_new().unwrap();
        let first = argument(&values, 0);
        let result = argument(&values, 1);

        let opposite = values
            .intern_constant_term(ConstantTermData::Unary {
                operation: ConstantUnaryOperation::LogicalNot,
                operand: first,
            })
            .unwrap();

        let required = contract(
            &values,
            &[(CallableContractClauseKind::Ensures, 0, None, Some(result))],
        );

        let provided = contract(
            &values,
            &[
                (CallableContractClauseKind::Guard, 0, None, Some(first)),
                (
                    CallableContractClauseKind::Ensures,
                    1,
                    Some(0),
                    Some(result),
                ),
                (CallableContractClauseKind::Guard, 2, None, Some(opposite)),
                (
                    CallableContractClauseKind::Ensures,
                    3,
                    Some(2),
                    Some(result),
                ),
            ],
        );

        assert_eq!(compare(&values, &required, &provided), None);

        let both = values
            .intern_constant_term(ConstantTermData::Binary {
                operation: ConstantBinaryOperation::LogicalAnd,
                left: first,
                right: result,
            })
            .unwrap();

        let required = contract(
            &values,
            &[(CallableContractClauseKind::Ensures, 0, None, Some(both))],
        );

        let provided = contract(
            &values,
            &[
                (CallableContractClauseKind::Ensures, 8, None, Some(result)),
                (CallableContractClauseKind::Ensures, 9, None, Some(first)),
            ],
        );

        assert_eq!(compare(&values, &required, &provided), None);
    }

    #[test]
    fn callable_condition_implication_allows_weaker_but_not_stronger_preconditions() {
        let values = SemanticValueStore::try_new().unwrap();
        let first = argument(&values, 0);
        let second = argument(&values, 1);

        let both = values
            .intern_constant_term(ConstantTermData::Binary {
                operation: ConstantBinaryOperation::LogicalAnd,
                left: first,
                right: second,
            })
            .unwrap();

        let narrow = contract(
            &values,
            &[(CallableContractClauseKind::Requires, 2, None, Some(both))],
        );

        let wide = contract(
            &values,
            &[(CallableContractClauseKind::Requires, 0, None, Some(first))],
        );

        assert_eq!(compare(&values, &narrow, &wide), None);

        assert_eq!(
            compare(&values, &wide, &narrow),
            Some(CallableConditionMismatch::InvocationPrecondition(
                SymbolOrdinal::new(2)
            ))
        );
    }

    #[test]
    fn callable_condition_implication_never_replaces_missing_meaning_with_truth() {
        let values = SemanticValueStore::try_new().unwrap();
        let empty = contract(&values, &[]);

        let unavailable = contract(
            &values,
            &[(CallableContractClauseKind::Ensures, 3, None, None)],
        );

        assert_eq!(
            compare(&values, &unavailable, &empty),
            Some(CallableConditionMismatch::CompletionPostcondition(
                SymbolOrdinal::new(3)
            ))
        );

        let unavailable = contract(
            &values,
            &[(CallableContractClauseKind::Requires, 3, None, None)],
        );

        assert_eq!(
            compare(&values, &empty, &unavailable),
            Some(CallableConditionMismatch::InvocationPrecondition(
                SymbolOrdinal::new(3)
            ))
        );
    }
}
