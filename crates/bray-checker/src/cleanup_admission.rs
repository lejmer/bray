//! Type-wide proof that a selected local lifecycle action needs no cleanup storage.

use crate::{CheckerInfrastructureError, CheckerRequestContext};
use bray_bound_tree::CallableProofObligation;
use bray_compiler_known::RepresentationRole;
use bray_symbols::{
    AvailableCompilerKnownSymbols, CallableConditionSet, CallableExecution, ConstantTermData,
    ExecutionProperty, SemanticValueStore, SemanticValueStoreError, SymbolOrdinal, TypeId,
};

/// Proves that a selected action needs neither activation nor outgoing incident storage.
/// Conditions describe the selected instance. Certified obligations must be dependency-validated.
/// This proof uses an arbitrary receiver, never facts about a particular value's completion.
pub fn prove_cleanup_admission_free<C: CheckerRequestContext + ?Sized>(
    request: &C,
    conditions: &CallableConditionSet,
    certified: &[CallableProofObligation],
    execution: CallableExecution,
    result: TypeId,
) -> Result<Option<Vec<CallableProofObligation>>, CheckerInfrastructureError> {
    let values = request.semantic_values();

    cleanup_admission_evidence(
        values,
        request.available_compiler_known_symbols(),
        conditions,
        Some(certified),
        execution,
        result,
    )
}

/// Returns candidate dependencies for a type-wide absence of local cleanup storage.
/// Every returned obligation must be certified before omitting the action's uniform allowance.
pub(crate) fn cleanup_admission_requirements(
    values: &SemanticValueStore,
    available: &AvailableCompilerKnownSymbols,
    conditions: &CallableConditionSet,
    execution: CallableExecution,
    result: TypeId,
) -> Result<Option<Vec<CallableProofObligation>>, CheckerInfrastructureError> {
    cleanup_admission_evidence(values, available, conditions, None, execution, result)
}

fn cleanup_admission_evidence(
    values: &SemanticValueStore,
    available: &AvailableCompilerKnownSymbols,
    conditions: &CallableConditionSet,
    certified: Option<&[CallableProofObligation]>,
    execution: CallableExecution,
    result: TypeId,
) -> Result<Option<Vec<CallableProofObligation>>, CheckerInfrastructureError> {
    let representation =
        crate::representation::type_representation_for_values(values, available, result)?;

    admission_evidence(values, conditions, certified, execution, representation)
        .map_err(CheckerInfrastructureError::SemanticValueStore)
}

fn admission_evidence(
    values: &SemanticValueStore,
    conditions: &CallableConditionSet,
    certified: Option<&[CallableProofObligation]>,
    execution: CallableExecution,
    result: Option<RepresentationRole>,
) -> Result<Option<Vec<CallableProofObligation>>, SemanticValueStoreError> {
    if execution != CallableExecution::Synchronous || result != Some(RepresentationRole::Unit) {
        return Ok(None);
    }

    let receiver =
        values.intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))?;

    let mut remaining = crate::contract::MAX_CONDITION_STEPS;

    crate::finalization::execution_evidence_with(
        values,
        conditions,
        certified,
        &[receiver],
        &[],
        &[ExecutionProperty::Total],
        &mut remaining,
        &mut |condition, arguments| {
            crate::contract::instantiate_condition(values, condition, arguments)
        },
    )
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::CallableProofObligation;
    use bray_compiler_known::RepresentationRole;
    use bray_symbols::{
        CallableConditionSet, CallableConditions, CallableContractClause,
        CallableContractClauseKind, CallableExecution, CallableExecutionGuarantee,
        ConstantTermData, ExecutionProperty, PredicateSemanticSummary, SemanticValueStore,
        SymbolOrdinal,
    };

    #[test]
    fn uniform_allowance_requires_unit_synchronous_and_certified_unconditional_total() {
        let values = SemanticValueStore::try_new().unwrap();
        let total = CallableExecutionGuarantee::new(ExecutionProperty::Total, None);
        let proof = CallableProofObligation::Execution(total);
        let unconditional = CallableConditionSet::new([]).with_execution_guarantees([total]);

        let receiver = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .unwrap();

        let predicate =
            PredicateSemanticSummary::new(values.empty_dependency_contract_template().unwrap())
                .with_condition(Some(receiver));

        let conditional = CallableConditionSet::new([CallableContractClause::new(
            SymbolOrdinal::new(0),
            CallableContractClauseKind::Guard,
            predicate.clone(),
        )])
        .with_execution_guarantees([CallableExecutionGuarantee::new(
            ExecutionProperty::Total,
            Some(SymbolOrdinal::new(0)),
        )]);

        let required = CallableConditionSet::new([CallableContractClause::new(
            SymbolOrdinal::new(0),
            CallableContractClauseKind::Requires,
            predicate,
        )])
        .with_execution_guarantees([total]);

        let check = |conditions: &CallableConditionSet,
                     certified: Option<&[CallableProofObligation]>,
                     execution,
                     result| {
            super::admission_evidence(&values, conditions, certified, execution, Some(result))
                .unwrap()
        };

        assert_eq!(
            check(
                &unconditional,
                None,
                CallableExecution::Synchronous,
                RepresentationRole::Unit
            ),
            Some(vec![proof])
        );

        assert_eq!(
            check(
                &unconditional,
                Some(&[proof]),
                CallableExecution::Synchronous,
                RepresentationRole::Unit
            ),
            Some(vec![proof])
        );

        assert_eq!(
            check(
                &unconditional,
                Some(&[]),
                CallableExecution::Synchronous,
                RepresentationRole::Unit
            ),
            None
        );

        assert_eq!(
            check(
                &unconditional,
                None,
                CallableExecution::Asynchronous,
                RepresentationRole::Unit
            ),
            None
        );

        assert_eq!(
            check(
                &unconditional,
                None,
                CallableExecution::Synchronous,
                RepresentationRole::Result
            ),
            None
        );

        assert_eq!(
            check(
                &conditional,
                None,
                CallableExecution::Synchronous,
                RepresentationRole::Unit
            ),
            None
        );

        assert_eq!(
            check(
                &required,
                None,
                CallableExecution::Synchronous,
                RepresentationRole::Unit
            ),
            None
        );
    }
}
