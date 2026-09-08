use bray_bound_tree::CallableProofObligation;
use bray_compiler_known::RepresentationRole;
use bray_symbols::{
    CallableConditionSet, CallableConditions, ConstantTermData, ConstantTermId, ConstantTest,
    ExecutionProperty, GenericArgument, ProofOutcome, SemanticValueStore, SemanticValueStoreError,
    SymbolOrdinal, TypeData, TypeId, UnionVariantSymbolId,
};

use crate::contract::{
    MAX_CONDITION_STEPS, applicable_postconditions_with, callable_input_count,
    instantiate_condition, preconditions_hold_with, prove_condition,
};
use crate::guarantee::execution_property_coverage;
use crate::{CheckerInfrastructureError, CheckerRequestContext};

pub(crate) fn unresolved_finalization_diagnostic<C: CheckerRequestContext + ?Sized>(
    request: crate::CheckerUnitView<'_, C>,
    exit: bray_bound_tree::AnyBoundNodeId,
    ty: TypeId,
    kind: bray_diagnostics::DiagnosticKind,
    ordinal: usize,
) -> Result<bray_diagnostics::Diagnostic, crate::CheckerQueryError<C::UpstreamError>> {
    let source = crate::diagnostic::bound_node_origin(request, exit).map_or_else(
        || request.unit().key().source(),
        |origin| origin.source_anchor(),
    );

    let span = request.source(source)?.span();
    let ty = crate::diagnostic::diagnostic_type(request.context(), ty)?;

    Ok(bray_diagnostics::Diagnostic::new(
        crate::diagnostic::diagnostic_id(ordinal),
        kind,
        bray_diagnostics::SeverityKind::Error,
    )
    .with_primary_span(span)
    .with_arg(bray_diagnostics::DiagnosticArg::actual_type(ty)))
}

/// Proves successful no-work completion of one selected whole-value finalizer.
///
/// Conditions must be normalized for the selected instance, and certified obligations must come
/// from that declaration's dependency-validated implementation. Arguments and assumptions describe
/// the current receiver state. Returned obligations identify the evidence used by this decision.
/// Receiver acquisition, represented parts, destruction, release, and other owners remain separate.
pub fn prove_finalizer_completion<C: CheckerRequestContext + ?Sized>(
    request: &C,
    conditions: &CallableConditionSet,
    certified: &[CallableProofObligation],
    arguments: &[ConstantTermId],
    result: TypeId,
    assumptions: &[(ConstantTermId, bool)],
) -> Result<Option<Vec<CallableProofObligation>>, CheckerInfrastructureError> {
    finalizer_completion_evidence(
        request.semantic_values(),
        request.available_compiler_known_symbols(),
        conditions,
        Some(certified),
        arguments,
        result,
        assumptions,
        &mut |condition, arguments| {
            instantiate_condition(request.semantic_values(), condition, arguments)
        },
    )
}

/// Identifies the implementation obligations needed to discharge a selected finalizer.
/// The result is a proof candidate. Each returned obligation must be dependency-validated before
/// lowering can omit the whole-value finalization step.
pub(crate) fn finalizer_completion_requirements(
    values: &SemanticValueStore,
    available: &bray_symbols::AvailableCompilerKnownSymbols,
    conditions: &CallableConditionSet,
    arguments: &[ConstantTermId],
    result: TypeId,
    assumptions: &[(ConstantTermId, bool)],
    normalize: &mut impl FnMut(
        ConstantTermId,
        &[ConstantTermId],
    ) -> Result<Option<ConstantTermId>, SemanticValueStoreError>,
) -> Result<Option<Vec<CallableProofObligation>>, CheckerInfrastructureError> {
    finalizer_completion_evidence(
        values,
        available,
        conditions,
        None,
        arguments,
        result,
        assumptions,
        normalize,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "completion checking shares the selected contract and normalization across certified proofs and proof candidates"
)]
fn finalizer_completion_evidence(
    values: &SemanticValueStore,
    available: &bray_symbols::AvailableCompilerKnownSymbols,
    conditions: &CallableConditionSet,
    certified: Option<&[CallableProofObligation]>,
    arguments: &[ConstantTermId],
    result: TypeId,
    assumptions: &[(ConstantTermId, bool)],
    normalize: &mut impl FnMut(
        ConstantTermId,
        &[ConstantTermId],
    ) -> Result<Option<ConstantTermId>, SemanticValueStoreError>,
) -> Result<Option<Vec<CallableProofObligation>>, CheckerInfrastructureError> {
    let representation =
        crate::representation::type_representation_for_values(values, available, result)?;

    let success = match representation {
        Some(RepresentationRole::Unit) => None,
        Some(RepresentationRole::Result) => {
            let data = values
                .type_data(result)
                .map_err(CheckerInfrastructureError::SemanticValueStore)?;

            let TypeData::Named { substitution, .. } = data.as_ref() else {
                return Ok(None);
            };

            let substitution = values
                .generic_substitution_data(*substitution)
                .map_err(CheckerInfrastructureError::SemanticValueStore)?;

            let Some(GenericArgument::Type(success)) = substitution
                .bindings()
                .first()
                .map(|binding| binding.argument())
            else {
                return Ok(None);
            };

            if crate::representation::type_representation_for_values(values, available, success)?
                != Some(RepresentationRole::Unit)
            {
                return Ok(None);
            }

            let result = available.result_representation().ok_or(
                CheckerInfrastructureError::CompilerKnownRepresentationUnavailable {
                    role: RepresentationRole::Result,
                },
            )?;

            Some(result.success_variant())
        }
        _ => return Ok(None),
    };

    completion_evidence_with(
        values,
        conditions,
        certified,
        arguments,
        success,
        assumptions,
        normalize,
    )
    .map_err(CheckerInfrastructureError::SemanticValueStore)
}

fn completion_evidence_with(
    values: &SemanticValueStore,
    conditions: &CallableConditionSet,
    certified: Option<&[CallableProofObligation]>,
    arguments: &[ConstantTermId],
    success: Option<UnionVariantSymbolId>,
    assumptions: &[(ConstantTermId, bool)],
    normalize: &mut impl FnMut(
        ConstantTermId,
        &[ConstantTermId],
    ) -> Result<Option<ConstantTermId>, SemanticValueStoreError>,
) -> Result<Option<Vec<CallableProofObligation>>, SemanticValueStoreError> {
    if !preconditions_hold_with(values, conditions, assumptions, &mut |condition| {
        normalize(condition, arguments)
    })? {
        return Ok(None);
    }

    // Retain immutable clause groups while restricting available promises to certified evidence.
    let provided = conditions.clone().with_execution_guarantees(
        conditions
            .execution_guarantees()
            .iter()
            .copied()
            .filter(|guarantee| {
                certified.is_none_or(|certified| {
                    certified.contains(&CallableProofObligation::Execution(*guarantee))
                })
            }),
    );

    let mut remaining = MAX_CONDITION_STEPS;
    let mut evidence = Vec::new();

    for property in [ExecutionProperty::Pure, ExecutionProperty::Total] {
        let Some(coverage) = execution_property_coverage(
            values,
            &provided,
            property,
            assumptions,
            &mut |condition| normalize(condition, arguments),
            &mut remaining,
        )?
        else {
            return Ok(None);
        };

        evidence.extend(coverage.into_iter().map(CallableProofObligation::Execution));
    }

    let Some(success) = success else {
        return Ok(Some(evidence));
    };

    let Some(fresh) = callable_input_count(
        values,
        arguments
            .iter()
            .copied()
            .chain(assumptions.iter().map(|(term, _)| *term)),
    )?
    else {
        return Ok(None);
    };

    let result = values.intern_constant_term(ConstantTermData::CallableArgument(
        SymbolOrdinal::new(fresh),
    ))?;

    let goal = values.intern_constant_term(ConstantTermData::Test {
        subject: result,
        kind: ConstantTest::ActiveUnionVariant(success),
    })?;

    let completion_arguments = arguments
        .iter()
        .copied()
        .chain([result])
        .collect::<Vec<_>>();

    let mut completion = assumptions.to_vec();

    for clause in applicable_postconditions_with(
        values,
        conditions,
        assumptions,
        &mut remaining,
        &mut |condition, _| normalize(condition, arguments),
    )? {
        let obligation = CallableProofObligation::Postcondition(clause.ordinal());

        if certified.is_some_and(|certified| !certified.contains(&obligation)) {
            continue;
        }

        let Some(condition) = clause
            .predicate()
            .and_then(|predicate| predicate.condition())
        else {
            continue;
        };

        let Some(condition) = normalize(condition, &completion_arguments)? else {
            continue;
        };

        completion.push((condition, true));
        evidence.push(obligation);
    }

    Ok((prove_condition(values, &completion, goal)? == ProofOutcome::Proven).then_some(evidence))
}

#[cfg(test)]
mod tests {
    use crate::test_support::compiler_known_symbol;
    use bray_bound_tree::CallableProofObligation;
    use bray_symbols::{
        CallableConditionSet, CallableConditions, CallableContractClause,
        CallableContractClauseKind, CallableExecutionGuarantee, ConstantTermData, ConstantTest,
        ExecutionProperty, PredicateSemanticSummary, SemanticValueStore, SymbolOrdinal,
        UnionVariantSymbolId,
    };

    fn completion_evidence(
        values: &SemanticValueStore,
        conditions: &CallableConditionSet,
        certified: Option<&[CallableProofObligation]>,
        arguments: &[bray_symbols::ConstantTermId],
        success: Option<UnionVariantSymbolId>,
        assumptions: &[(bray_symbols::ConstantTermId, bool)],
    ) -> Result<Option<Vec<CallableProofObligation>>, bray_symbols::SemanticValueStoreError> {
        super::completion_evidence_with(
            values,
            conditions,
            certified,
            arguments,
            success,
            assumptions,
            &mut |condition, arguments| crate::instantiate_condition(values, condition, arguments),
        )
    }

    #[test]
    fn completion_requires_both_execution_proofs_and_the_successful_result_proof() {
        let values = SemanticValueStore::try_new().unwrap();

        let ready = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .unwrap();

        let result = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(1)))
            .unwrap();

        let success = compiler_known_symbol::<UnionVariantSymbolId>("ResultVariant0Ok");

        let success_condition = values
            .intern_constant_term(ConstantTermData::Test {
                subject: result,
                kind: ConstantTest::ActiveUnionVariant(success),
            })
            .unwrap();

        let dependency = values.empty_dependency_contract_template().unwrap();

        let pure =
            CallableExecutionGuarantee::new(ExecutionProperty::Pure, Some(SymbolOrdinal::new(0)));

        let total =
            CallableExecutionGuarantee::new(ExecutionProperty::Total, Some(SymbolOrdinal::new(0)));

        let conditions = CallableConditionSet::new([
            CallableContractClause::new(
                SymbolOrdinal::new(0),
                CallableContractClauseKind::Guard,
                PredicateSemanticSummary::new(dependency).with_condition(Some(ready)),
            ),
            CallableContractClause::new(
                SymbolOrdinal::new(1),
                CallableContractClauseKind::Ensures,
                PredicateSemanticSummary::new(dependency).with_condition(Some(success_condition)),
            )
            .with_guard(Some(SymbolOrdinal::new(0))),
        ])
        .with_execution_guarantees([pure, total]);

        let certified = [
            CallableProofObligation::Execution(pure),
            CallableProofObligation::Execution(total),
            CallableProofObligation::Postcondition(SymbolOrdinal::new(1)),
        ];

        assert_eq!(
            completion_evidence(
                &values,
                &conditions,
                Some(&certified),
                &[ready],
                Some(success),
                &[(ready, true)]
            )
            .unwrap(),
            Some(certified.to_vec())
        );

        assert_eq!(
            completion_evidence(
                &values,
                &conditions,
                None,
                &[ready],
                Some(success),
                &[(ready, true)]
            )
            .unwrap(),
            Some(certified.to_vec())
        );

        assert!(
            completion_evidence(
                &values,
                &conditions,
                Some(&certified),
                &[ready],
                Some(success),
                &[]
            )
            .unwrap()
            .is_none()
        );

        for missing in 0..certified.len() {
            let incomplete = certified
                .iter()
                .enumerate()
                .filter_map(|(index, proof)| (index != missing).then_some(*proof))
                .collect::<Vec<_>>();

            assert!(
                completion_evidence(
                    &values,
                    &conditions,
                    Some(&incomplete),
                    &[ready],
                    Some(success),
                    &[(ready, true)]
                )
                .unwrap()
                .is_none()
            );
        }

        assert_eq!(
            completion_evidence(
                &values,
                &conditions,
                Some(&certified[..2]),
                &[ready],
                None,
                &[(ready, true)]
            )
            .unwrap(),
            Some(certified[..2].to_vec())
        );
    }

    #[test]
    fn completion_does_not_waive_an_unproven_invocation_requirement() {
        let values = SemanticValueStore::try_new().unwrap();

        let ready = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .unwrap();

        let predicate =
            PredicateSemanticSummary::new(values.empty_dependency_contract_template().unwrap())
                .with_condition(Some(ready));

        let pure = CallableExecutionGuarantee::new(ExecutionProperty::Pure, None);
        let total = CallableExecutionGuarantee::new(ExecutionProperty::Total, None);

        let conditions = CallableConditionSet::new([CallableContractClause::new(
            SymbolOrdinal::new(0),
            CallableContractClauseKind::Requires,
            predicate,
        )])
        .with_execution_guarantees([pure, total]);

        let certified = [
            CallableProofObligation::Execution(pure),
            CallableProofObligation::Execution(total),
        ];

        assert!(
            completion_evidence(&values, &conditions, Some(&certified), &[ready], None, &[])
                .unwrap()
                .is_none()
        );

        assert!(
            completion_evidence(
                &values,
                &conditions,
                Some(&certified),
                &[ready],
                None,
                &[(ready, true)]
            )
            .unwrap()
            .is_some()
        );
    }
}
