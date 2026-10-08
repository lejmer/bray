use std::collections::BTreeSet;

use super::super::flow::ExecutionFlowDomain;
use crate::{CheckerRequestContext, ExecutionCondition};

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(super) fn trusted_requirements_proven(
        &self,
        entry: &crate::ExecutionCallEvidence,
        requirements: &[ExecutionCondition],
    ) -> bool {
        if entry.proves_trusted(requirements) {
            return true;
        }

        let equalities = ExecutionCondition::equalities(&entry.assumptions, None);

        let ordinary = entry
            .assumptions
            .iter()
            .map(|(condition, value)| (condition.with_equalities(&equalities), *value))
            .collect::<BTreeSet<_>>();

        requirements.iter().all(|requirement| {
            if entry.proves_trusted(std::slice::from_ref(requirement)) {
                return true;
            }

            let condition = requirement
                .substitute(
                    &|place| {
                        place
                            .value_in(&entry.arguments)
                            .unwrap_or(ExecutionCondition::Unknown)
                    },
                    &ExecutionCondition::Unknown,
                    &mut { ExecutionCondition::WORK_LIMIT },
                )
                .with_equalities(&equalities);

            let ExecutionCondition::Trusted(condition) = condition else {
                return false;
            };

            let ExecutionCondition::Predicate(predicate, substitution, arguments) =
                condition.as_ref()
            else {
                return false;
            };

            let [pointer, count] = arguments.as_ref() else {
                return false;
            };

            if !self.spatial_predicates.contains(predicate) {
                return false;
            }

            entry
                .trusted_assumptions
                .iter()
                .take(ExecutionCondition::WORK_LIMIT)
                .any(|(known, value)| {
                    if !value {
                        return false;
                    }

                    let known = known.with_equalities(&equalities);

                    let ExecutionCondition::Trusted(known) = known else {
                        return false;
                    };

                    let ExecutionCondition::Predicate(
                        known_predicate,
                        known_substitution,
                        known_arguments,
                    ) = known.as_ref()
                    else {
                        return false;
                    };

                    let [known_pointer, known_count] = known_arguments.as_ref() else {
                        return false;
                    };

                    predicate == known_predicate
                        && substitution == known_substitution
                        && pointer == known_pointer
                        && range_count_follows(count, known_count, &ordinary)
                })
        })
    }
}

fn range_count_follows(
    count: &ExecutionCondition,
    extent: &ExecutionCondition,
    assumptions: &BTreeSet<(ExecutionCondition, bool)>,
) -> bool {
    use bray_bound_tree::BoundOperator;

    if ExecutionCondition::operation(
        BoundOperator::LessEqual,
        vec![count.clone(), extent.clone()],
    )
    .prove(assumptions, &mut { ExecutionCondition::WORK_LIMIT })
        == Some(true)
    {
        return true;
    }

    let ExecutionCondition::Literal(value) = count else {
        return false;
    };

    let bray_symbols::ConstantValueKind::Integer(integer) = value.kind() else {
        return false;
    };

    if integer.to_u64() != Some(1) {
        return false;
    }

    if let ExecutionCondition::Operation(BoundOperator::Subtract, operands) = extent
        && let [end, start] = operands.as_ref()
    {
        return ExecutionCondition::operation(
            BoundOperator::Less,
            vec![start.clone(), end.clone()],
        )
        .prove(assumptions, &mut { ExecutionCondition::WORK_LIMIT })
            == Some(true)
            || assumptions.contains(&(
                ExecutionCondition::operation(
                    BoundOperator::GreaterEqual,
                    vec![start.clone(), end.clone()],
                ),
                false,
            ));
    }

    assumptions.iter().any(|(condition, holds)| {
        if !holds { return false; }

        let ExecutionCondition::Operation(operator, operands) = condition else { return false; };

        let [left, right] = operands.as_ref() else { return false; };

        let lower = match operator {
            BoundOperator::Less if right == extent => left,
            BoundOperator::Greater if left == extent => right,
            _ => return false,
        };

        matches!(lower, ExecutionCondition::Literal(value)
            if matches!(value.kind(), bray_symbols::ConstantValueKind::Integer(integer) if integer.is_zero()))
    })
}
