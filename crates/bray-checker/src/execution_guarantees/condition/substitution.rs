use super::ExecutionCondition;
use bray_bound_tree::BoundOperator;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

impl ExecutionCondition {
    pub(crate) fn substitute(
        &self,
        input: &impl Fn(&crate::execution_guarantees::ExecutionPlace) -> Self,
        result: &Self,
        budget: &mut usize,
    ) -> Self {
        self.substitute_values(
            &|condition| match condition {
                Self::Input(place) => Some(input(place)),
                Self::Result => Some(result.clone()),
                _ => None,
            },
            budget,
        )
    }

    /// Inserts predecessor snapshots once. A snapshot can contain the same
    /// surviving owner, so recursively resolving it as an equality would cycle.
    pub(crate) fn with_snapshots(&self, snapshots: &BTreeMap<Self, Self>) -> Self {
        self.substitute_values(
            &|condition| {
                if let Some(value) = snapshots.get(condition) {
                    return Some(value.clone());
                }

                let Self::Input(place) = condition else {
                    return None;
                };

                for length in (0..place.projections.len()).rev() {
                    let mut prefix = place.clone();

                    prefix.projections = place.projections[..length].into();

                    if let Some(value) = snapshots.get(&Self::Input(prefix)) {
                        return Some(place.projections[length..].iter().fold(
                            value.clone(),
                            |value, field| value.component(*field),
                        ));
                    }
                }

                None
            },
            &mut { Self::WORK_LIMIT },
        )
    }

    fn substitute_values(
        &self,
        replace: &impl Fn(&Self) -> Option<Self>,
        budget: &mut usize,
    ) -> Self {
        let Some(next) = budget.checked_sub(1) else {
            return Self::Unknown;
        };

        *budget = next;

        if let Some(value) = replace(self) {
            return value;
        }

        match self {
            Self::Call(callable, operands) => Self::call(
                *callable,
                operands
                    .iter()
                    .map(|operand| operand.substitute_values(replace, budget))
                    .collect(),
            ),
            Self::Predicate(predicate, substitution, operands) => Self::predicate(
                *predicate,
                *substitution,
                operands
                    .iter()
                    .map(|operand| operand.substitute_values(replace, budget))
                    .collect(),
            ),
            Self::Trusted(value) => {
                Self::Trusted(Arc::new(value.substitute_values(replace, budget)))
            }
            Self::Borrowed(value) => value.substitute_values(replace, budget).borrowed(),
            Self::Present(value) => value.substitute_values(replace, budget).present(),
            Self::Tagged(variant, value) => {
                Self::Tagged(*variant, Arc::new(value.substitute_values(replace, budget)))
            }
            Self::VariantTest(variant, value) => value
                .substitute_values(replace, budget)
                .matches_variant(*variant),
            Self::Entry {
                condition,
                captured: false,
            } => Self::Entry {
                condition: Arc::new(condition.substitute_values(replace, budget)),
                captured: false,
            },
            Self::Projection(field, value) => {
                value.substitute_values(replace, budget).component(*field)
            }
            Self::Constructed(base, fields) => Self::Constructed(
                Arc::new(base.substitute_values(replace, budget)),
                fields
                    .iter()
                    .map(|(field, value)| (*field, value.substitute_values(replace, budget)))
                    .collect::<Vec<_>>()
                    .into(),
            ),
            Self::Operation(operator, operands) => Self::operation(
                *operator,
                operands
                    .iter()
                    .map(|operand| operand.substitute_values(replace, budget))
                    .collect(),
            ),
            _ => self.clone(),
        }
    }

    pub(crate) fn capture_entry(
        &self,
        input: &impl Fn(&crate::execution_guarantees::ExecutionPlace) -> Self,
        trusted: &BTreeSet<(Self, bool)>,
        ordinary: &BTreeSet<(Self, bool)>,
        budget: &mut usize,
    ) -> Self {
        let Some(next) = budget.checked_sub(1) else {
            return Self::Unknown;
        };

        *budget = next;

        match self {
            Self::Entry {
                condition,
                captured: false,
            } => {
                let condition = condition.substitute(input, &Self::Unknown, budget);

                let condition = match condition.prove_trusted(trusted, ordinary) {
                    Some(value) => Self::Boolean(value),
                    None if condition.observes_borrowed(None) => Self::Unknown,
                    None => condition,
                };

                Self::Entry {
                    condition: Arc::new(condition),
                    captured: true,
                }
            }
            Self::Operation(operator, operands) => Self::operation(
                *operator,
                operands
                    .iter()
                    .map(|operand| operand.capture_entry(input, trusted, ordinary, budget))
                    .collect(),
            ),
            value => value.clone(),
        }
    }

    pub(crate) fn equalities(
        assumptions: &BTreeSet<(Self, bool)>,
        expiring: Option<bray_bound_tree::BoundExpressionId>,
    ) -> BTreeMap<Self, Self> {
        let mut replacements = BTreeMap::new();
        let mut budget = Self::WORK_LIMIT;

        for (condition, value) in assumptions.iter().take(Self::WORK_LIMIT) {
            let Self::Operation(operator, operands) = condition else {
                continue;
            };

            if !matches!(
                (operator, value),
                (BoundOperator::Equal, true) | (BoundOperator::NotEqual, false)
            ) {
                continue;
            }

            let [left, right] = operands.as_ref() else {
                continue;
            };

            if left == &Self::Unknown || right == &Self::Unknown {
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
        rewrite_equalities(self, replacements, None, &mut { Self::WORK_LIMIT })
    }

    pub(crate) fn with_joined_values(
        &self,
        replacements: &BTreeMap<Self, Self>,
        identities: &BTreeMap<Self, Self>,
    ) -> Self {
        rewrite_equalities(self, replacements, Some(identities), &mut {
            Self::WORK_LIMIT
        })
    }
}

fn resolve_equality(
    value: &ExecutionCondition,
    replacements: &std::collections::BTreeMap<ExecutionCondition, ExecutionCondition>,
    budget: &mut usize,
) -> ExecutionCondition {
    let mut value = value.clone();

    while let Some(replacement) = replacements.get(&value) {
        let Some(next) = budget.checked_sub(1) else {
            return ExecutionCondition::Unknown;
        };

        *budget = next;
        value = replacement.clone();
    }

    value
}

fn rewrite_equalities(
    value: &ExecutionCondition,
    replacements: &std::collections::BTreeMap<ExecutionCondition, ExecutionCondition>,
    identities: Option<&BTreeMap<ExecutionCondition, ExecutionCondition>>,
    budget: &mut usize,
) -> ExecutionCondition {
    let Some(next) = budget.checked_sub(1) else {
        return ExecutionCondition::Unknown;
    };

    *budget = next;

    let value = resolve_equality(value, replacements, budget);

    let value = match value {
        ExecutionCondition::Trusted(value) if identities.is_some() => ExecutionCondition::Trusted(
            Arc::new(rewrite_equalities(&value, replacements, identities, budget)),
        ),
        ExecutionCondition::Trusted(value) => {
            // Ordinary Boolean equalities cannot rename the predicate that owns trusted authority.
            let value = match value.as_ref() {
                ExecutionCondition::Predicate(predicate, substitution, operands) => {
                    ExecutionCondition::predicate(
                        *predicate,
                        *substitution,
                        operands
                            .iter()
                            .map(|operand| {
                                rewrite_equalities(operand, replacements, identities, budget)
                            })
                            .collect(),
                    )
                }
                value => value.clone(),
            };

            ExecutionCondition::Trusted(Arc::new(value))
        }
        ExecutionCondition::Tagged(variant, value) => ExecutionCondition::Tagged(
            variant,
            Arc::new(rewrite_equalities(&value, replacements, identities, budget)),
        ),
        ExecutionCondition::VariantTest(variant, value) => {
            rewrite_equalities(&value, replacements, identities, budget).matches_variant(variant)
        }
        ExecutionCondition::Present(value) => {
            rewrite_equalities(&value, replacements, identities, budget).present()
        }
        ExecutionCondition::Borrowed(value) if let Some(identities) = identities => {
            rewrite_equalities(&value, identities, Some(identities), budget).borrowed()
        }
        ExecutionCondition::Entry { captured: true, .. } if identities.is_some() => value,
        ExecutionCondition::Borrowed(value) => {
            // Equal contents do not equate the physical referents of borrowed observations.
            ExecutionCondition::Borrowed(value)
        }
        ExecutionCondition::Entry {
            condition,
            captured,
        } => ExecutionCondition::Entry {
            condition: Arc::new(rewrite_equalities(
                &condition,
                replacements,
                identities,
                budget,
            )),
            captured,
        },
        ExecutionCondition::Input(place) if identities.is_some() => {
            let mut rewritten = None;

            // Input paths compact projections into their place. An owned root
            // rename must reach those fields just as it reaches Projection nodes.
            for length in (0..place.projections.len()).rev() {
                let mut prefix = place.clone();

                prefix.projections = place.projections[..length].into();

                let prefix = ExecutionCondition::Input(prefix);

                if replacements.contains_key(&prefix) {
                    let root = rewrite_equalities(&prefix, replacements, identities, budget);

                    rewritten = Some(
                        place.projections[length..]
                            .iter()
                            .fold(root, |value, field| value.component(*field)),
                    );

                    break;
                }
            }

            rewritten.unwrap_or(ExecutionCondition::Input(place))
        }
        ExecutionCondition::Projection(field, base) => {
            rewrite_equalities(&base, replacements, identities, budget).component(field)
        }
        ExecutionCondition::Constructed(base, fields) => ExecutionCondition::Constructed(
            Arc::new(rewrite_equalities(&base, replacements, identities, budget)),
            fields
                .iter()
                .map(|(field, value)| {
                    (
                        *field,
                        rewrite_equalities(value, replacements, identities, budget),
                    )
                })
                .collect::<Vec<_>>()
                .into(),
        ),
        ExecutionCondition::Operation(operator, operands) => ExecutionCondition::operation(
            operator,
            operands
                .iter()
                .map(|operand| rewrite_equalities(operand, replacements, identities, budget))
                .collect(),
        ),
        ExecutionCondition::Predicate(predicate, substitution, operands) => {
            ExecutionCondition::predicate(
                predicate,
                substitution,
                operands
                    .iter()
                    .map(|operand| rewrite_equalities(operand, replacements, identities, budget))
                    .collect(),
            )
        }
        ExecutionCondition::Call(callable, operands) => ExecutionCondition::call(
            callable,
            operands
                .iter()
                .map(|operand| rewrite_equalities(operand, replacements, identities, budget))
                .collect(),
        ),
        value => value,
    };

    resolve_equality(&value, replacements, budget)
}
