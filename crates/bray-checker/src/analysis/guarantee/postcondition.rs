use std::collections::BTreeMap;

use bray_bound_tree::{BoundReferenceTarget, SelectedCall};
use bray_symbols::ReceiverMode;

use super::flow::{ExecutionFlow, ExecutionFlowDomain};
use super::state::ExecutionState;
use crate::{CheckerRequestContext, ExecutionCondition, ExecutionPlace};

pub(super) fn unproven_postconditions<C: CheckerRequestContext + ?Sized>(
    flow: &ExecutionFlow<'_, '_, C>,
    postconditions: &[(ExecutionCondition, bray_source::SourceSpan)],
) -> Vec<bray_source::SourceSpan> {
    if postconditions.is_empty() {
        return Vec::new();
    }

    let mut failed = vec![false; postconditions.len()];

    for exit in flow.domain.graph.completion_exits() {
        let output = flow.output(exit.block());

        let Some(state) = output else {
            continue;
        };

        check_postconditions(&flow.domain, &state, postconditions, &mut failed);
    }

    postconditions
        .iter()
        .zip(failed)
        .filter_map(|((_, span), failed)| failed.then_some(*span))
        .collect()
}

pub(in crate::analysis::guarantee) fn check_postconditions<C: CheckerRequestContext + ?Sized>(
    domain: &ExecutionFlowDomain<'_, '_, C>,
    state: &ExecutionState,
    postconditions: &[(ExecutionCondition, bray_source::SourceSpan)],
    failed: &mut [bool],
) {
    // Replay each exit once, sharing any implication closure across its clauses.
    let mut trusted = None;
    let mut unsigned = None;
    let mut integer_ranges = super::integer::IntegerRanges::new();

    for ((condition, _), failed) in postconditions.iter().zip(failed) {
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

        if reflexive_comparison(domain, state, &condition) {
            continue;
        }

        if condition.prove_mixed(&state.trusted_assumptions, &state.assumptions, &mut {
            ExecutionCondition::WORK_LIMIT
        }) == Some(true)
        {
            continue;
        }

        if domain.integer_condition_truth(state, &condition, &mut integer_ranges) == Some(true) {
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

        let (equalities, known) = unsigned.get_or_insert_with(|| {
            let equalities = ExecutionCondition::equalities(&state.assumptions, None);

            let known = state
                .assumptions
                .iter()
                .map(|(condition, holds)| (condition.with_equalities(&equalities), *holds))
                .collect();

            (equalities, known)
        });

        *failed = !super::integer::unsigned_arithmetic_is_bounded(
            &condition.with_equalities(equalities),
            known,
            &|value| domain.unsigned_value(value),
        );
    }
}

fn reflexive_comparison<C: CheckerRequestContext + ?Sized>(
    domain: &ExecutionFlowDomain<'_, '_, C>,
    state: &ExecutionState,
    condition: &ExecutionCondition,
) -> bool {
    use bray_bound_tree::BoundOperator;

    let ExecutionCondition::Operation(
        BoundOperator::Equal | BoundOperator::LessEqual | BoundOperator::GreaterEqual,
        operands,
    ) = condition
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
    matches!(
        left,
        ExecutionCondition::Boolean(_)
            | ExecutionCondition::Predicate(_, _, _)
            | ExecutionCondition::Operation(
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

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(super) fn call_post_state(
        &self,
        state: &ExecutionState,
        expression: bray_bound_tree::BoundExpressionId,
        call: &SelectedCall,
    ) -> BTreeMap<ExecutionPlace, ExecutionCondition> {
        let mut inputs = BTreeMap::new();

        for argument in call.arguments() {
            let (parameter, ordinal, ty) = match argument {
                bray_bound_tree::SelectedArgument::Explicit {
                    parameter,
                    ordinal,
                    conversion,
                    ..
                } => (*parameter, *ordinal, conversion.target_type()),
                bray_bound_tree::SelectedArgument::Default {
                    parameter,
                    ordinal,
                    ty,
                    ..
                } => (Some(*parameter), *ordinal, *ty),
            };

            if !matches!(
                self.request.semantic_values().type_data(ty).as_ref(),
                bray_symbols::TypeData::Borrow { .. }
            ) {
                continue;
            }

            let root = bray_symbols::DependencySubjectRoot::Parameter(
                bray_symbols::SymbolOrdinal::new(ordinal),
            );

            let input = call
                .input_site(expression, root)
                .expect("postcondition has its evaluated input");

            let value = self.current_input(state, input);

            if let Some(parameter) = parameter {
                inputs.insert(
                    BoundReferenceTarget::Surface(parameter.into()).into(),
                    value.clone(),
                );
            }

            inputs.insert(
                ExecutionPlace::argument(
                    call.input_ordinal(root)
                        .expect("formal input has its ordinal"),
                ),
                value,
            );
        }

        if let Some(receiver) = call.receiver().filter(|receiver| {
            matches!(
                receiver.mode(),
                ReceiverMode::Shared | ReceiverMode::Mutable
            )
        }) {
            let input = call
                .input_site(expression, bray_symbols::DependencySubjectRoot::Receiver)
                .expect("borrowed receiver has its input");

            let value = self.current_input(state, input);

            inputs.insert(
                BoundReferenceTarget::Surface(receiver.parameter().into()).into(),
                value.clone(),
            );

            inputs.insert(
                ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0)),
                value,
            );
        }

        inputs
    }
}
