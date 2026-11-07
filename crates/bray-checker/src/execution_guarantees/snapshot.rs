use std::collections::BTreeSet;

use super::ExecutionCondition;
use bray_bound_tree::BoundOperator;
use bray_symbols::ConstantValueKind;

pub(crate) fn common_components(
    left: &ExecutionCondition,
    right: &ExecutionCondition,
    result: Option<ExecutionCondition>,
    conditions: (
        &BTreeSet<(ExecutionCondition, bool)>,
        &BTreeSet<(ExecutionCondition, bool)>,
    ),
    facts: &mut Vec<(ExecutionCondition, bool)>,
    budget: &mut usize,
) -> ExecutionCondition {
    if left == right {
        return left.clone();
    }

    let Some(next) = budget.checked_sub(1) else {
        return ExecutionCondition::Unknown;
    };

    *budget = next;

    if left == &ExecutionCondition::Unknown || right == &ExecutionCondition::Unknown {
        return ExecutionCondition::Unknown;
    }

    match (
        matches!(left.owner_identity(), ExecutionCondition::Borrowed(_)),
        matches!(right.owner_identity(), ExecutionCondition::Borrowed(_)),
    ) {
        (true, true) => {
            let same_owner = left.has_same_owner(right);

            let contents = common_components(
                &left.clone().into_referent(),
                &right.clone().into_referent(),
                if same_owner { result } else { None },
                conditions,
                facts,
                budget,
            );

            return if same_owner {
                left.owner_identity().with_referent(contents)
            } else {
                // Joining an owned result does not identify different referents.
                contents.borrowed()
            };
        }
        (true, false) | (false, true) => return ExecutionCondition::Unknown,
        (false, false) => {}
    }

    if let (ExecutionCondition::StorageValue(left_owner, left_value), ExecutionCondition::StorageValue(right_owner, right_value)) = (left, right) {
        if left_owner == right_owner {
            return ExecutionCondition::storage_value(
                left_owner.as_ref().clone(),
                common_components(left_value, right_value, result, conditions, facts, budget),
            );
        }

        return ExecutionCondition::Unknown;
    }

    let fields = |mut value: &ExecutionCondition| {
        while let Some(snapshot) = value.nullable_payload_snapshot() {
            value = snapshot;
        }

        match value {
            ExecutionCondition::Constructed(_, fields) => fields
                .iter()
                .map(|(field, _)| *field)
                .collect::<std::collections::BTreeSet<_>>(
            ),
            ExecutionCondition::Present(_) => {
                [bray_bound_tree::StorageProjection::NullableValue].into()
            }
            ExecutionCondition::Literal(value)
                if matches!(value.kind(), ConstantValueKind::NullableAbsent) =>
            {
                [bray_bound_tree::StorageProjection::NullableValue].into()
            }
            _ => Default::default(),
        }
    };

    let mut projections = fields(left);

    projections.extend(fields(right));

    if projections.is_empty() {
        let joined = result.unwrap_or(ExecutionCondition::Unknown);

        if joined != ExecutionCondition::Unknown
            && let (ExecutionCondition::Literal(left), ExecutionCondition::Literal(right)) =
                (left, right)
            && matches!(
                (left.kind(), right.kind()),
                (ConstantValueKind::Integer(_), ConstantValueKind::Integer(_))
            )
            && let Ok(ConstantValueKind::Boolean(ordered)) = crate::constant::fold_binary(
                BoundOperator::LessEqual,
                left.kind(),
                right.kind(),
                crate::ConstantEvaluationLimits::default().integer_bits(),
            )
        {
            // A yielded result keeps these bounds for this evaluation. Literals
            // remain independent values and are never renamed to the result.
            let (lower, upper) = if ordered {
                (left, right)
            } else {
                (right, left)
            };

            for (operator, bound) in [
                (BoundOperator::GreaterEqual, lower),
                (BoundOperator::LessEqual, upper),
            ] {
                facts.push((
                    ExecutionCondition::operation(
                        operator,
                        vec![
                            joined.clone(),
                            ExecutionCondition::Literal(std::sync::Arc::clone(bound)),
                        ],
                    ),
                    true,
                ));
            }
        }

        return joined;
    }

    // A changed observation does not replace its existing descriptor or loan.
    // Distinct owners instead acquire the selected result region's identity.
    let identity = if left.owner_identity() != &ExecutionCondition::Unknown && left.has_same_owner(right) {
        left.owner_identity().clone()
    } else {
        result.clone().unwrap_or(ExecutionCondition::Unknown)
    };

    let fields = projections
        .into_iter()
        .map(|projection| {
            let inactive =
                |value: &ExecutionCondition, conditions: &BTreeSet<(ExecutionCondition, bool)>| {
                    match projection {
                        bray_bound_tree::StorageProjection::NullableValue => {
                            matches!(value, ExecutionCondition::Literal(value)
                    if matches!(value.kind(), ConstantValueKind::NullableAbsent))
                                || conditions.iter().any(|(condition, holds)| {
                                    condition.nullable_absence(*holds).is_some_and(
                                        |(subject, _)| {
                                            subject == value || subject == value.owner_identity()
                                        },
                                    )
                                })
                        }
                        bray_bound_tree::StorageProjection::ActiveUnionPayloadField {
                            variant,
                            ..
                        } => {
                            value
                                .clone()
                                .matches_variant(variant)
                                .prove(conditions, &mut { ExecutionCondition::WORK_LIMIT })
                                == Some(false)
                        }
                        _ => false,
                    }
                };

            let left_inactive = inactive(left, conditions.0);
            let right_inactive = inactive(right, conditions.1);

            if left_inactive != right_inactive {
                // Only the active predecessor initializes this payload. Retain
                // its components under the joined conditional owner.
                let initialized = if left_inactive { right } else { left };

                return (
                    projection,
                    owned_snapshot(
                        &initialized.clone().project(projection),
                        identity.clone().project(projection),
                        budget,
                    ),
                );
            }

            let left = left.clone().project(projection);
            let right = right.clone().project(projection);

            let result = result
                .as_ref()
                .or_else(|| (identity != ExecutionCondition::Unknown).then_some(&identity))
                .map(|result| if projection == bray_bound_tree::StorageProjection::OwnedTarget {
                    // Contents have their own current identity. OwnedTarget is
                    // not a source-level projection of the retained descriptor.
                    result.clone()
                } else { result.clone().project(projection) });

            (
                projection,
                common_components(&left, &right, result, conditions, facts, budget),
            )
        })
        .collect();

    ExecutionCondition::Constructed(std::sync::Arc::new(identity), fields)
}

/// Reanchor initialized owned components under their destination owner.
pub(crate) fn owned_snapshot(
    source: &ExecutionCondition,
    identity: ExecutionCondition,
    budget: &mut usize,
) -> ExecutionCondition {
    let Some(remaining) = budget.checked_sub(1) else {
        return ExecutionCondition::Unknown;
    };

    *budget = remaining;

    if let Some(snapshot) = source.nullable_payload_snapshot() {
        return owned_snapshot(snapshot, identity, budget);
    }

    if matches!(source.owner_identity(), ExecutionCondition::Borrowed(_)) {
        return source.clone();
    }

    match source {
        ExecutionCondition::StorageValue(_, _) => source.clone(),
        ExecutionCondition::Constructed(base, fields) => {
            let base = owned_snapshot(base, identity, budget);

            let fields = fields
                .iter()
                .map(|(field, value)| {
                    (
                        *field,
                        owned_snapshot(
                            value,
                            base.owner_identity().clone().project(*field),
                            budget,
                        ),
                    )
                })
                .collect();

            ExecutionCondition::Constructed(std::sync::Arc::new(base), fields)
        }
        ExecutionCondition::Tagged(variant, _) => {
            ExecutionCondition::Tagged(*variant, std::sync::Arc::new(identity))
        }
        ExecutionCondition::Present(value) => {
            ExecutionCondition::Present(std::sync::Arc::new(owned_snapshot(
                value,
                identity.project(bray_bound_tree::StorageProjection::NullableValue),
                budget,
            )))
        }
        ExecutionCondition::Borrowed(_)
        | ExecutionCondition::Boolean(_)
        | ExecutionCondition::Literal(_)
        | ExecutionCondition::Unknown => source.clone(),
        _ => identity,
    }
}

