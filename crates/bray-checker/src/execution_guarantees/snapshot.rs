use crate::execution_guarantees::ExecutionObservationKind as ObservationKind;
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

    if let (
        ExecutionCondition::Observation { owner: left_owner, contents: left_value, kind: ObservationKind::Scalar },
        ExecutionCondition::Observation { owner: right_owner, contents: right_value, kind: ObservationKind::Scalar },
    ) = (left, right) {
        let identity = if left_owner == right_owner { Some(left_owner.as_ref().clone()) } else { result.clone() };

        return match identity {
            Some(identity) => ExecutionCondition::scalar_observation(identity,
                common_components(left_value, right_value, result, conditions, facts, budget)),
            None => ExecutionCondition::Unknown,
        };
    }

    let left_owner = left.ownership_identity();
    let right_owner = right.ownership_identity();
    let same_owner = matches!((left_owner, right_owner), (Some((role, owner)), Some((other_role, other))) if role == other_role && owner == other);

    if left_owner.map(|(role, _)| role) != right_owner.map(|(role, _)| role) {
        return mixed_role_components(left, right, result, conditions, facts, budget);
    }

    if left_owner.is_some_and(|(role, _)| role == ObservationKind::Reference) {
        let contents = common_components(
            &left.clone().into_referent(), &right.clone().into_referent(),
            if same_owner { result.clone() } else { None }, conditions, facts, budget,
        );

        return if same_owner {
            ExecutionCondition::reference_value(left_owner.expect("shared reference owner exists").1.clone(), contents)
        } else if let Some(identity) = result {
            ExecutionCondition::reference_value(identity, contents)
        } else { ExecutionCondition::Unknown };
    }

    if same_owner
        && (left.observation_kind() == Some(ObservationKind::Owned)
            || right.observation_kind() == Some(ObservationKind::Owned))
    {
        let contents = |value: &ExecutionCondition| match value {
            ExecutionCondition::Observation { contents, kind: ObservationKind::Owned, .. } => contents.as_ref().clone(),
            value => value.clone(),
        };

        return ExecutionCondition::owned_value(
            left_owner.expect("shared owned context exists").1.clone(),
            common_components(&contents(left), &contents(right), result, conditions, facts, budget),
        );
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
    let identity = if same_owner {
        left_owner.expect("shared owner exists").1.clone()
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

    ExecutionCondition::constructed(identity, fields)
}

fn mixed_role_components(
    left: &ExecutionCondition,
    right: &ExecutionCondition,
    result: Option<ExecutionCondition>,
    conditions: (&BTreeSet<(ExecutionCondition, bool)>, &BTreeSet<(ExecutionCondition, bool)>),
    facts: &mut Vec<(ExecutionCondition, bool)>,
    budget: &mut usize,
) -> ExecutionCondition {
    let Some(identity) = result else { return ExecutionCondition::Unknown; };

    let data = |value: &ExecutionCondition| match value {
        ExecutionCondition::Observation { contents, .. } => contents.as_ref().clone(),
        ExecutionCondition::Borrowed(_) => value.clone().into_referent(),
        value => value.clone(),
    };

    let common = common_components(&data(left), &data(right), Some(identity.clone()), conditions, facts, budget);

    // This finite phi region owns the selected object. Equal copied data never
    // makes the original source carrier common to both predecessors.
    let joined = owned_snapshot(&common, identity.clone(), budget);

    if joined.ownership_identity() == Some((ObservationKind::Owned, &identity)) {
        joined
    } else {
        ExecutionCondition::owned_value(identity, joined)
    }
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

    if source.ownership_identity().is_some_and(|(role, _)| role == ObservationKind::Reference) {
        return source.clone();
    }

    match source {
        ExecutionCondition::Observation { kind: ObservationKind::Scalar, .. } => source.clone(),
        ExecutionCondition::Observation { contents, kind: ObservationKind::Reference, .. } => {
            ExecutionCondition::reference_value(identity, contents.as_ref().clone())
        }
        ExecutionCondition::Observation { contents, kind: ObservationKind::Owned, .. } => {
            let data = owned_snapshot(contents, identity.clone(), budget);

            ExecutionCondition::owned_value(identity, data)
        }
        ExecutionCondition::Constructed(base, fields) => {
            let base = if **base == ExecutionCondition::Unknown { identity } else { owned_snapshot(base, identity, budget) };
            let parent = base.ownership_identity().filter(|(role, _)| *role == ObservationKind::Owned)
                .map(|(_, owner)| owner.clone()).unwrap_or(ExecutionCondition::Unknown);

            let fields = fields
                .iter()
                .map(|(field, value)| {
                    (
                        *field,
                        owned_snapshot(
                            value,
                            parent.clone().project(*field),
                            budget,
                        ),
                    )
                })
                .collect();

            ExecutionCondition::constructed(base, fields)
        }
        ExecutionCondition::Tagged(variant, value) => {
            ExecutionCondition::Tagged(*variant, std::sync::Arc::new(owned_snapshot(value, identity, budget)))
        }
        ExecutionCondition::Present(value) => {
            let payload = owned_snapshot(
                value,
                identity.clone().project(bray_bound_tree::StorageProjection::NullableValue),
                budget,
            );

            ExecutionCondition::owned_value(identity, ExecutionCondition::Present(std::sync::Arc::new(payload)))
        }
        ExecutionCondition::Borrowed(_)
        | ExecutionCondition::Boolean(_)
        | ExecutionCondition::Literal(_)
        | ExecutionCondition::InitializedSnapshot(_, _, _)
        | ExecutionCondition::Unknown => source.clone(),
        _ => identity,
    }
}

