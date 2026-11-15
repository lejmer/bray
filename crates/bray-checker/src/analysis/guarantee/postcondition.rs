use super::flow::{ExecutionFlow, ExecutionFlowDomain};
use super::state::ExecutionState;
use crate::{CheckerRequestContext, ExecutionCondition};

pub(super) fn unproven_postcondition<C: CheckerRequestContext + ?Sized>(
    flow: &ExecutionFlow<'_, '_, C>,
    postconditions: &[(ExecutionCondition, bray_source::SourceSpan)],
) -> Option<(bray_source::SourceSpan, crate::UnprovenExecutionCondition)> {
    if postconditions.is_empty() {
        return None;
    }

    let mut failed = vec![None; postconditions.len()];

    for exit in flow.domain.graph.completion_exits() {
        let output = flow.output(exit.block());

        let Some(state) = output else {
            continue;
        };

        check_postconditions(&flow.domain, state, postconditions, &mut failed);
    }

    postconditions
        .iter()
        .zip(failed)
        .find_map(|((_, span), failed)| failed.map(|failed| (*span, failed)))
}

pub(in crate::analysis::guarantee) fn check_postconditions<C: CheckerRequestContext + ?Sized>(
    domain: &ExecutionFlowDomain<'_, '_, C>,
    state: &ExecutionState,
    postconditions: &[(ExecutionCondition, bray_source::SourceSpan)],
    failed: &mut [Option<crate::UnprovenExecutionCondition>],
) {
    // Replay each exit once, sharing any implication closure across its clauses.
    let trusted = std::cell::OnceCell::new();
    let normalized = std::cell::OnceCell::new();
    let normalized_proof = std::cell::OnceCell::new();
    let proof = state.proof();
    let mut integer_ranges = super::integer::IntegerRanges::new();

    for ((required, _), failed) in postconditions.iter().zip(failed) {
        if failed.is_some() {
            continue;
        }

        let condition = required.substitute(
            &|input| {
                input
                    .value_in(&state.current)
                    .unwrap_or_else(ExecutionCondition::unknown)
            },
            &domain.returned_value(state),
            &mut { ExecutionCondition::WORK_LIMIT },
        ).with_reference_origins(domain.request.semantic_values(), domain.dependencies, &state.assumptions);

        if reflexive_comparison(domain, state, &condition) {
            continue;
        }

        if proof.prove_mixed(&condition, &mut {
            ExecutionCondition::WORK_LIMIT
        }) == Some(true)
        {
            continue;
        }

        if domain.integer_condition_truth(state, &condition, &mut integer_ranges) == Some(true) {
            continue;
        }

        let (equalities, known) = normalized.get_or_init(|| {
            let equalities = state.proofs.equalities(&state.assumptions, None);

            let known = state.assumptions.iter()
                .map(|(condition, holds)| (condition.with_equalities(&equalities), *holds))
                .collect();

            (equalities, known)
        });

        let trusted = trusted.get_or_init(|| {
            crate::execution_guarantees::trusted_condition_closure(
                state.trusted_assumptions.iter().map(|(condition, holds)| (
                    condition.with_reference_origins(domain.request.semantic_values(), domain.dependencies, &state.assumptions)
                        .with_equalities(equalities), *holds,
                )).collect(),
                known,
            )
        });

        let proof = normalized_proof.get_or_init(|| crate::execution_guarantees::ExecutionProof::new(trusted, known));

        if proof.prove_mixed(&condition.with_equalities(equalities), &mut {
            ExecutionCondition::WORK_LIMIT
        }) == Some(true)
        {
            continue;
        }

        if !super::integer::unsigned_arithmetic_is_bounded(
            &condition.with_equalities(equalities), known, &|value| domain.unsigned_value(value),
        ) {
            *failed = Some(crate::UnprovenExecutionCondition::new(required.clone(), condition));
        }
    }
}

fn reflexive_comparison<C: CheckerRequestContext + ?Sized>(
    domain: &ExecutionFlowDomain<'_, '_, C>,
    state: &ExecutionState,
    condition: &ExecutionCondition,
) -> bool {
    use bray_bound_tree::BoundOperator;

    let crate::ExecutionConditionKind::Operation(
        BoundOperator::Equal | BoundOperator::LessEqual | BoundOperator::GreaterEqual,
        operands,
    ) = condition.kind()
    else {
        return false;
    };

    let [left, right] = operands.as_ref() else {
        return false;
    };

    if left != right {
        return false;
    }

    // Untyped normalization cannot assume IEEE equality or weak ordering for NaN.
    // Boolean operations and established integer values have reflexive comparisons.
    matches!((
        left).kind(),
        crate::ExecutionConditionKind::Boolean(_)
            | crate::ExecutionConditionKind::Predicate(_, _, _)
            | crate::ExecutionConditionKind::Operation(
                BoundOperator::LogicalNot
                    | BoundOperator::LogicalAnd
                    | BoundOperator::LogicalOr
                    | BoundOperator::Equal
                    | BoundOperator::NotEqual
                    | BoundOperator::Less
                    | BoundOperator::LessEqual
                    | BoundOperator::Greater
                    | BoundOperator::GreaterEqual,
                _
            )
    ) || domain
        .observation_type(left)
        .and_then(|ty| crate::representation::type_representation(domain.request, ty))
        == Some(bray_compiler_known::RepresentationRole::ScalarBool)
        || domain
            .observed_integer_range(state, left, &mut Default::default(), &mut {
                ExecutionCondition::WORK_LIMIT
            })
            .is_some()
}
