use std::collections::BTreeMap;

use bray_bound_tree::{BoundExpressionId, BoundReferenceTarget, SelectedCall};
use bray_symbols::ReceiverMode;

use super::flow::{ExecutionFlow, ExecutionFlowDomain};
use super::state::ExecutionState;
use crate::{
    CheckerRequestContext, ExecutionCompletionContract, ExecutionCondition, ExecutionPlace,
};

pub(super) fn unproven_postconditions<C: CheckerRequestContext + ?Sized>(
    flow: &ExecutionFlow<'_, '_, C>,
    postconditions: &[(ExecutionCondition, bray_source::SourceSpan)],
) -> Vec<bray_source::SourceSpan> {
    if postconditions.is_empty() {
        return Vec::new();
    }

    let mut failed = vec![false; postconditions.len()];

    for exit in flow.domain.graph.exits().iter().filter(|exit| {
        matches!(
            exit.kind(),
            super::super::model::AnalysisExitKind::Return
                | super::super::model::AnalysisExitKind::NormalFallthrough
                | super::super::model::AnalysisExitKind::ResultErrorPropagation
        )
    }) {
        let Some(state) = flow.output(exit.block()) else {
            continue;
        };

        // Replay each exit once, sharing any implication closure across its clauses.
        let mut trusted = None;
        let mut unsigned = None;

        for ((condition, _), failed) in postconditions.iter().zip(&mut failed) {
            if *failed {
                continue;
            }

            let condition = condition.substitute(
                &|input| {
                    input
                        .value_in(&state.current)
                        .unwrap_or_else(|| ExecutionCondition::Input(input.clone()))
                },
                &state.result,
                &mut { ExecutionCondition::WORK_LIMIT },
            );

            if condition.prove_mixed(&state.trusted_assumptions, &state.assumptions, &mut {
                ExecutionCondition::WORK_LIMIT
            }) == Some(true)
            {
                continue;
            }

            let trusted = trusted.get_or_insert_with(|| {
                crate::execution_guarantees::trusted_condition_closure(
                    state.trusted_assumptions.clone(),
                    &state.assumptions,
                )
            });

            if condition.prove_mixed(trusted, &state.assumptions, &mut {
                ExecutionCondition::WORK_LIMIT
            }) == Some(true)
            {
                continue;
            }

            if !matches!(&condition, ExecutionCondition::Operation(bray_bound_tree::BoundOperator::LessEqual, operands)
                if matches!(operands.first(), Some(ExecutionCondition::Operation(bray_bound_tree::BoundOperator::Add, _))))
            {
                *failed = true;
                continue;
            }

            let (equalities, known) = unsigned.get_or_insert_with(|| {
                let equalities = ExecutionCondition::equalities(&state.assumptions, None);

                let known = state
                    .assumptions
                    .iter()
                    .map(|(condition, holds)| (condition.with_equalities(&equalities), *holds))
                    .collect();

                (equalities, known)
            });

            *failed =
                !unsigned_sum_is_bounded(&condition.with_equalities(equalities), known, &|value| {
                    flow.domain.unsigned_value(value)
                });
        }
    }

    postconditions
        .iter()
        .zip(failed)
        .filter_map(|((_, span), failed)| failed.then_some(*span))
        .collect()
}

fn unsigned_sum_is_bounded(
    condition: &ExecutionCondition,
    known: &std::collections::BTreeSet<(ExecutionCondition, bool)>,
    unsigned: &impl Fn(&ExecutionCondition) -> bool,
) -> bool {
    use bray_bound_tree::BoundOperator;

    let ExecutionCondition::Operation(BoundOperator::LessEqual, operands) = condition else {
        return false;
    };

    let [
        ExecutionCondition::Operation(BoundOperator::Add, sum),
        limit,
    ] = operands.as_ref()
    else {
        return false;
    };

    let [left, right] = sum.as_ref() else {
        return false;
    };

    if ![left, right, limit].into_iter().all(unsigned) {
        return false;
    }

    // A bounded unsigned subtraction cannot wrap. The resulting spare-capacity
    // bound proves the sum without applying integer algebra to floating-point values.
    // Each comparison uses the existing indexed proof set, not another scan.
    [(left, right), (right, left)]
        .into_iter()
        .any(|(used, count)| {
            let available = ExecutionCondition::operation(
                BoundOperator::Subtract,
                vec![limit.clone(), used.clone()],
            );

            [
                ExecutionCondition::operation(
                    BoundOperator::LessEqual,
                    vec![used.clone(), limit.clone()],
                ),
                ExecutionCondition::operation(
                    BoundOperator::LessEqual,
                    vec![count.clone(), available],
                ),
            ]
            .iter()
            .all(|condition| {
                condition.prove(known, &mut { ExecutionCondition::WORK_LIMIT }) == Some(true)
            })
        })
}

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(super) fn receiver_post_state(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
        call: &SelectedCall,
        contracts: &[&ExecutionCompletionContract],
    ) -> BTreeMap<ExecutionPlace, ExecutionCondition> {
        let mut inputs = BTreeMap::new();

        let Some(receiver) = call.receiver().filter(|receiver| {
            matches!(
                receiver.mode(),
                ReceiverMode::Shared | ReceiverMode::Mutable
            )
        }) else {
            return inputs;
        };

        let input = BoundReferenceTarget::Surface(receiver.parameter().into());

        if !contracts
            .iter()
            .flat_map(|contract| &contract.postconditions)
            .any(|(condition, _)| {
                condition
                    .inputs()
                    .iter()
                    .any(|place| place.reference() == Some(input))
            })
        {
            return inputs;
        }

        let Some(place) = crate::execution_guarantees::expression_place(
            self.request.unit(),
            self.semantics,
            self.request.semantic_values(),
            receiver.expression(),
        ) else {
            return inputs;
        };

        let value = ExecutionCondition::PostState(expression, input);

        // The caller's receiver and callee predicate refer to the same normal-exit observation.
        state.assign(place, value.clone());
        inputs.insert(input.into(), value);

        inputs
    }
}
