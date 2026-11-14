use super::super::{ExecutionCondition, ExecutionConditionKind as K};
use crate::execution_guarantees::ExecutionObservationKind;
use bray_bound_tree::BoundOperator;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::scope::{MapRole, RewriteMap, RewriteMemo};
use super::super::memo::{ExecutionMemo, memoized};

impl ExecutionCondition {
    pub(crate) fn has_value_changes(replacements: &BTreeMap<Self, Self>) -> bool {
        replacements.iter().any(|(before, after)| before != after)
    }

    pub(crate) fn substitute(
        &self,
        input: &impl Fn(&crate::execution_guarantees::ExecutionPlace) -> Self,
        result: &Self,
        budget: &mut usize,
    ) -> Self {
        self.substitute_values(
            &|condition| match condition.kind() {
                K::Input(place) => Some(input(place)),
                K::Result => Some(result.clone()),
                _ => None,
            },
            budget,
            None,
        )
    }

    /// Inserts predecessor snapshots once. A snapshot can contain the same
    /// surviving owner, so recursively resolving it as an equality would cycle.
    pub(crate) fn with_snapshots(&self, snapshots: &BTreeMap<Self, Self>) -> Self {
        super::scope::SnapshotSubstitution::new(snapshots).apply(self)
    }

    /// Copy correlations apply to values. They cannot relocate borrowed observations.
    pub(crate) fn with_copy_origins(&self, snapshots: &BTreeMap<Self, Self>) -> Self {
        if !Self::has_value_changes(snapshots) { return self.clone(); }

        self.substitute_values(
            &|condition| match condition.kind() {
                K::Borrowed(_) | K::Observation { kind: ExecutionObservationKind::Reference, .. } =>
                    Some(condition.clone()),
                _ => snapshot_value(condition, snapshots),
            },
            &mut { Self::WORK_LIMIT },
            None,
        )
    }

    pub(super) fn substitute_values(
        &self,
        replace: &impl Fn(&Self) -> Option<Self>,
        budget: &mut usize,
        memo: Option<&ExecutionMemo<Self>>,
    ) -> Self {
        memoized(memo, self, budget, |budget| self.substitute_values_inner(replace, budget, memo))
    }

    fn substitute_values_inner(
        &self,
        replace: &impl Fn(&Self) -> Option<Self>,
        budget: &mut usize,
        memo: Option<&ExecutionMemo<Self>>,
    ) -> Self {
        let Some(next) = budget.checked_sub(1) else {
            return Self::unknown();
        };

        *budget = next;

        if let Some(value) = replace(self) {
            return self.reuse_unchanged(value);
        }

        let rewritten = match self.kind() {
            K::Discriminant(value) => value.substitute_values(replace, budget, memo).discriminant(),
            K::Descriptor(kind, value) => value.substitute_values(replace, budget, memo).descriptor(*kind),
            K::ValueAlias(left, right) => Self::value_alias(
                left.substitute_values(replace, budget, memo), right.substitute_values(replace, budget, memo),
            ),
            K::Call(callable, implementation, witnesses, operands, reference_type) => Self::call(
                *callable,
                *implementation,
                witnesses.iter().copied(),
                operands
                    .iter()
                    .map(|operand| operand.substitute_values(replace, budget, memo))
                    .collect(),
                *reference_type,
            ),
            K::Predicate(predicate, substitution, operands) => Self::predicate(
                *predicate,
                *substitution,
                operands
                    .iter()
                    .map(|operand| operand.substitute_values(replace, budget, memo).argument_value(operand.is_reference()))
                    .collect(),
            ),
            K::Trusted(value) => {
                Self::new(K::Trusted(Arc::new(value.substitute_values(replace, budget, memo))))
            }
            K::Borrowed(value) => value.substitute_values(replace, budget, memo).borrowed(),
            K::Present(value) => Self::new(K::Present(Arc::new(value.substitute_values(replace, budget, memo)))),
            K::Tagged(variant, value) => {
                Self::new(K::Tagged(*variant, Arc::new(value.substitute_values(replace, budget, memo))))
            }
            K::VariantTest(variant, value) => value
                .substitute_values(replace, budget, memo)
                .matches_variant(*variant),
            K::Entry {
                condition,
                captured: false,
            } => Self::new(K::Entry {
                condition: Arc::new(condition.substitute_values(replace, budget, memo)),
                captured: false,
            }),
            K::Projection(field, value) => {
                value.substitute_values(replace, budget, memo).component(*field)
            }
            K::CapturedInput {
                owner,
                invocation,
                ordinal,
            } => Self::new(K::CapturedInput {
                owner: Arc::new(owner.substitute_values(replace, budget, memo)),
                invocation: *invocation,
                ordinal: *ordinal,
            }),
            K::Observation { owner, contents, kind } => Self::observation(
                owner.substitute_values(replace, budget, memo),
                contents.substitute_values(replace, budget, memo),
                *kind,
            ),
            K::Constructed(base, fields) => Self::constructed(base.substitute_values(replace, budget, memo),
                fields
                    .iter()
                    .map(|(field, value)| (*field, value.substitute_values(replace, budget, memo)))
                    .collect::<Vec<_>>()
                    .into(),
            ),
            K::Operation(operator, operands) => Self::operation(
                *operator,
                operands
                    .iter()
                    .map(|operand| operand.substitute_values(replace, budget, memo))
                    .collect(),
            ),
            _ => self.clone(),
        };

        self.reuse_unchanged(rewritten)
    }

    pub(crate) fn capture_entry(
        &self,
        input: &impl Fn(&crate::execution_guarantees::ExecutionPlace) -> Self,
        trusted: &BTreeSet<(Self, bool)>,
        ordinary: &BTreeSet<(Self, bool)>,
        budget: &mut usize,
    ) -> Self {
        let Some(next) = budget.checked_sub(1) else {
            return Self::unknown();
        };

        *budget = next;

        let rewritten = match self.kind() {
            K::Entry {
                condition,
                captured: false,
            } => {
                let condition = condition.substitute(input, &Self::unknown(), budget);

                let condition = match condition.prove_trusted(trusted, ordinary) {
                    Some(value) => Self::boolean(value),
                    None if condition.observes_live(None) => Self::unknown(),
                    None => condition,
                };

                Self::new(K::Entry {
                    condition: Arc::new(condition),
                    captured: true,
                })
            }
            K::Operation(operator, operands) => Self::operation(
                *operator,
                operands
                    .iter()
                    .map(|operand| operand.capture_entry(input, trusted, ordinary, budget))
                    .collect(),
            ),
            _ => self.clone(),
        };

        self.reuse_unchanged(rewritten)
    }

    pub(crate) fn equalities(
        assumptions: &BTreeSet<(Self, bool)>,
        expiring: Option<bray_bound_tree::BoundExpressionId>,
    ) -> BTreeMap<Self, Self> {
        Self::equalities_with(assumptions, expiring, || std::borrow::Cow::Owned(
            crate::execution_guarantees::trusted_condition_closure(BTreeSet::new(), assumptions)))
    }

    pub(in crate::execution_guarantees) fn equalities_with<'a>(
        assumptions: &'a BTreeSet<(Self, bool)>,
        expiring: Option<bray_bound_tree::BoundExpressionId>,
        derive: impl FnOnce() -> std::borrow::Cow<'a, BTreeSet<(Self, bool)>>,
    ) -> BTreeMap<Self, Self> {
        let mut replacements = BTreeMap::new();
        let mut budget = Self::WORK_LIMIT;

        let derived = assumptions.iter().any(|(condition, _)| matches!(condition.kind(), K::Operation(
            BoundOperator::LogicalAnd | BoundOperator::LogicalOr | BoundOperator::LogicalNot, _)) && condition.contains_equivalence())
            .then(derive);

        for (condition, value) in assumptions.iter().chain(derived.iter().flat_map(|facts| facts.iter())
            .filter(|(condition, _)| matches!(condition.kind(), K::ValueAlias(_, _)
                | K::Operation(BoundOperator::Equal | BoundOperator::NotEqual, _)))).take(Self::WORK_LIMIT) {
            let (left, right) = match condition.kind() {
                K::ValueAlias(left, right) if *value => (left.as_ref(), right.as_ref()),
                K::Operation(operator, operands) if matches!((operator, value),
                    (BoundOperator::Equal, true) | (BoundOperator::NotEqual, false)) => {
                    let [left, right] = operands.as_ref() else { continue; };

                    (left, right)
                }
                _ => continue,
            };

            if left == &Self::unknown() || right == &Self::unknown() {
                continue;
            }

            let left = resolve_equality(left, &replacements, &mut budget);
            let right = resolve_equality(right, &replacements, &mut budget);

            // An explicit exit transfer must name the surviving value rather than an alias
            // whose scoped authority is ending. Keep the same bounded equality solver.
            let left_order = (
                expiring.is_some_and(|scope| {
                    left.depends_on(bray_bound_tree::StorageMutation::Operation(scope.into()))
                }),
                &left,
            );

            let right_order = (
                expiring.is_some_and(|scope| {
                    right.depends_on(bray_bound_tree::StorageMutation::Operation(scope.into()))
                }),
                &right,
            );

            if left_order < right_order {
                replacements.insert(right, left);
            } else if right_order < left_order {
                replacements.insert(left, right);
            }
        }

        replacements
    }

    pub(crate) fn with_equalities(&self, replacements: &BTreeMap<Self, Self>) -> Self {
        if !Self::has_value_changes(replacements) { return self.clone(); }

        rewrite_equalities(self, replacements, None, &mut { Self::WORK_LIMIT }, None)
    }

    pub(crate) fn with_joined_values(
        &self,
        replacements: &BTreeMap<Self, Self>,
        identities: &BTreeMap<Self, Self>,
    ) -> Self {
        joined_value(self, RewriteMap::new(replacements), RewriteMap::new(identities), None)
    }
}

pub(super) fn snapshot_value(
    condition: &ExecutionCondition,
    snapshots: &BTreeMap<ExecutionCondition, ExecutionCondition>,
) -> Option<ExecutionCondition> {
    if let Some(value) = snapshots.get(condition) {
        return Some(value.clone());
    }

    let crate::ExecutionConditionKind::Input(place) = condition.kind() else { return None; };

    for length in (0..place.projections.len()).rev() {
        let mut prefix = place.clone();

        prefix.projections = place.projections[..length].into();

        if let Some(value) = snapshots.get(&ExecutionCondition::new(crate::ExecutionConditionKind::Input(prefix))) {
            return Some(place.projections[length..].iter()
                .fold(value.clone(), |value, field| value.component(*field)));
        }
    }

    None
}

fn resolve_equality(
    value: &ExecutionCondition,
    replacements: &std::collections::BTreeMap<ExecutionCondition, ExecutionCondition>,
    budget: &mut usize,
) -> ExecutionCondition {
    let mut value = value.clone();

    while let Some(replacement) = replacements.get(&value) {
        if replacement == &value {
            break;
        }

        let Some(next) = budget.checked_sub(1) else {
            return ExecutionCondition::unknown();
        };

        *budget = next;
        value = replacement.clone();
    }

    value
}

pub(super) fn joined_value(
    value: &ExecutionCondition,
    values: RewriteMap<'_>,
    owners: RewriteMap<'_>,
    memo: Option<&RewriteMemo>,
) -> ExecutionCondition {
    if !values.changed && !owners.changed {
        return value.clone();
    }

    rewrite_equalities(value, values.entries, Some(owners), &mut { ExecutionCondition::WORK_LIMIT },
        memo.map(|memo| (memo, MapRole::Value)))
}

pub(super) fn joined_owner(
    value: &ExecutionCondition,
    values: RewriteMap<'_>,
    owners: RewriteMap<'_>,
    memo: Option<&RewriteMemo>,
) -> ExecutionCondition {
    let owner = value.owner_identity();

    let component = matches!(owner.kind(), K::Projection(_, _))
        || matches!(owner.kind(), K::Input(place) if !place.projections.is_empty());

    let (replacements, role) = if component { (values, MapRole::Value) } else { (owners, MapRole::Owner) };

    if !replacements.changed && !owners.changed {
        return owner.clone();
    }

    rewrite_equalities(owner, replacements.entries, Some(owners), &mut { ExecutionCondition::WORK_LIMIT },
        memo.map(|memo| (memo, role))).owner_identity().clone()
}

fn rewrite_identity(
    owner: &ExecutionCondition,
    identities: Option<RewriteMap<'_>>,
    budget: &mut usize,
    memo: Option<(&RewriteMemo, MapRole)>,
) -> ExecutionCondition {
    identities.map_or_else(|| owner.clone(), |identities| {
        if !identities.changed { return owner.clone(); }

        rewrite_equalities(owner, identities.entries, Some(identities), budget, memo.map(|(memo, _)| (memo, MapRole::Owner)))
    })
}

fn rewrite_equalities(
    value: &ExecutionCondition,
    replacements: &std::collections::BTreeMap<ExecutionCondition, ExecutionCondition>,
    identities: Option<RewriteMap<'_>>,
    budget: &mut usize,
    memo: Option<(&RewriteMemo, MapRole)>,
) -> ExecutionCondition {
    if replacements.is_empty() && identities.is_none_or(|map| map.entries.is_empty()) {
        return value.clone();
    }

    // An unchanged complete observation shields its components from ancestor renaming.
    if replacements.get(value) == Some(value) {
        return value.clone();
    }

    let compute = |budget: &mut usize| {
        value.reuse_unchanged(rewrite_equalities_inner(value, replacements, identities, budget, memo))
    };

    let Some((cache, role)) = memo else { return compute(budget); };

    let cache = match role { MapRole::Value => &cache.values, MapRole::Owner => &cache.owners };

    memoized(Some(cache), value, budget, compute)
}

fn rewrite_equalities_inner(
    value: &ExecutionCondition,
    replacements: &BTreeMap<ExecutionCondition, ExecutionCondition>,
    identities: Option<RewriteMap<'_>>,
    budget: &mut usize,
    memo: Option<(&RewriteMemo, MapRole)>,
) -> ExecutionCondition {
    let Some(next) = budget.checked_sub(1) else {
        return ExecutionCondition::unknown();
    };

    *budget = next;

    let value = resolve_equality(value, replacements, budget);

    let value = match value.kind() {
        K::Discriminant(value) => rewrite_equalities(&value, replacements, identities, budget, memo).discriminant(),
        K::Descriptor(kind, value) => rewrite_identity(&value, identities, budget, memo).descriptor(*kind),
        K::ValueAlias(left, right) => ExecutionCondition::value_alias(
            rewrite_equalities(&left, replacements, identities, budget, memo),
            rewrite_equalities(&right, replacements, identities, budget, memo),
        ),
        K::Trusted(value) if identities.is_some() => ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(
            Arc::new(rewrite_equalities(&value, replacements, identities, budget, memo)),
        )),
        K::Trusted(value) => {
            // Ordinary Boolean equalities cannot rename the predicate that owns trusted authority.
            let value = match value.kind() {
                K::Predicate(predicate, substitution, operands) => {
                    ExecutionCondition::predicate(
                        *predicate,
                        *substitution,
                        operands
                            .iter()
                            .map(|operand| {
                                rewrite_equalities(operand, replacements, identities, budget, memo)
                            })
                            .collect(),
                    )
                }
                _ => value.as_ref().clone(),
            };

            ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(value)))
        }
        K::Tagged(variant, value) => ExecutionCondition::new(crate::ExecutionConditionKind::Tagged(
            *variant,
            Arc::new(rewrite_equalities(&value, replacements, identities, budget, memo)),
        )),
        K::VariantTest(variant, value) => {
            rewrite_equalities(&value, replacements, identities, budget, memo).matches_variant(*variant)
        }
        K::Present(value) => {
            ExecutionCondition::new(crate::ExecutionConditionKind::Present(Arc::new(rewrite_equalities(&value, replacements, identities, budget, memo))))
        }
        K::Borrowed(value) if let Some(identities) = identities => {
            rewrite_equalities(&value, replacements, Some(identities), budget, memo).borrowed()
        }
        K::Entry { captured: true, .. } if identities.is_some() => value.clone(),
        K::Borrowed(child) => {
            // Equal contents do not equate the physical referents of borrowed observations.
            ExecutionCondition::new(K::Borrowed(Arc::clone(child)))
        }
        K::Entry {
            condition,
            captured,
        } => ExecutionCondition::new(crate::ExecutionConditionKind::Entry {
            condition: Arc::new(rewrite_equalities(
                &condition,
                replacements,
                identities,
                budget, memo)),
            captured: *captured,
        }),
        K::Input(place) if identities.is_some() => {
            let mut rewritten = None;

            // Input paths compact projections into their place. An owned root
            // rename must reach those fields just as it reaches Projection nodes.
            for length in (0..place.projections.len()).rev() {
                let mut prefix = place.clone();

                prefix.projections = place.projections[..length].into();

                let prefix = ExecutionCondition::new(crate::ExecutionConditionKind::Input(prefix));

                if replacements.contains_key(&prefix) {
                    let root = rewrite_equalities(&prefix, replacements, identities, budget, memo);

                    rewritten = Some(
                        place.projections[length..]
                            .iter()
                            .fold(root, |value, field| value.component(*field)),
                    );

                    break;
                }
            }

            rewritten.unwrap_or_else(|| ExecutionCondition::new(crate::ExecutionConditionKind::Input(place.clone())))
        }
        K::Projection(field, base) => {
            rewrite_equalities(&base, replacements, identities, budget, memo).component(*field)
        }
        K::CapturedInput {
            owner,
            invocation,
            ordinal,
        } if let Some(identities) = identities => ExecutionCondition::new(crate::ExecutionConditionKind::CapturedInput {
            owner: Arc::new(rewrite_identity(&owner, Some(identities), budget, memo)),
            invocation: *invocation,
            ordinal: *ordinal,
        }),
        K::Observation { owner, contents, kind } => ExecutionCondition::observation(
            rewrite_identity(&owner, identities, budget, memo),
            rewrite_equalities(&contents, replacements, identities, budget, memo),
            *kind,
        ),
        K::Constructed(base, fields) => ExecutionCondition::constructed(rewrite_equalities(&base, replacements, identities, budget, memo),
            fields
                .iter()
                .map(|(field, value)| {
                    (
                        *field,
                        rewrite_equalities(value, replacements, identities, budget, memo),
                    )
                })
                .collect::<Vec<_>>()
                .into(),
        ),
        K::Operation(operator, operands) => ExecutionCondition::operation(
            *operator,
            operands
                .iter()
                .map(|operand| rewrite_equalities(operand, replacements, identities, budget, memo))
                .collect(),
        ),
        K::Predicate(predicate, substitution, operands) => {
            ExecutionCondition::predicate(
                *predicate,
                *substitution,
                operands
                    .iter()
                    .map(|operand| rewrite_equalities(operand, replacements, identities, budget, memo))
                    .collect(),
            )
        }
        K::Call(callable, implementation, witnesses, operands, reference_type) => {
            ExecutionCondition::call(
                *callable,
                *implementation,
                witnesses.iter().copied(),
                operands
                    .iter()
                    .map(|operand| rewrite_equalities(operand, replacements, identities, budget, memo))
                    .collect(),
                *reference_type,
            )
        }
        _ => value.clone(),
    };

    resolve_equality(&value, replacements, budget)
}
