use std::collections::{BTreeMap, BTreeSet};

use bray_symbols::{
    ConstantBinaryOperation, ConstantTermData, ConstantTermId, ConstantUnaryOperation,
    ConstantValueKind, ProofOutcome, SemanticValueStore, SemanticValueStoreError,
};

pub(crate) const MAX_CONDITION_STEPS: usize = 4096;

pub(crate) fn applicable_postconditions(
    values: &SemanticValueStore,
    conditions: &bray_symbols::CallableConditionSet,
    arguments: &[ConstantTermId],
    assumptions: &[(ConstantTermId, bool)],
    remaining: &mut usize,
) -> Result<Vec<bray_symbols::CallableContractClause>, SemanticValueStoreError> {
    applicable_postconditions_with(
        values,
        conditions,
        assumptions,
        remaining,
        &mut |condition, remaining| {
            instantiate_condition_with_budget(values, condition, arguments, remaining)
        },
    )
}

pub(crate) fn applicable_postconditions_with(
    values: &SemanticValueStore,
    conditions: &bray_symbols::CallableConditionSet,
    assumptions: &[(ConstantTermId, bool)],
    remaining: &mut usize,
    normalize: &mut impl FnMut(
        ConstantTermId,
        &mut usize,
    ) -> Result<Option<ConstantTermId>, SemanticValueStoreError>,
) -> Result<Vec<bray_symbols::CallableContractClause>, SemanticValueStoreError> {
    use bray_symbols::CallableConditions;

    let mut applicable = Vec::new();

    for clause in conditions
        .normal_completion_postconditions()
        .iter()
        .chain(conditions.guarded_postconditions())
    {
        let Some(next) = remaining.checked_sub(1) else {
            break;
        };

        *remaining = next;

        let Some(guards) = crate::contract_guard_conditions(conditions, clause.guard()) else {
            continue;
        };

        let mut active = true;

        for guard in guards {
            let Some(guard) = normalize(guard, remaining)? else {
                active = false;
                break;
            };

            if prove_condition(values, assumptions, guard)? != ProofOutcome::Proven {
                active = false;
                break;
            }
        }

        if active {
            applicable.push(*clause);
        }
    }

    Ok(applicable)
}

pub(crate) fn preconditions_hold(
    values: &SemanticValueStore,
    conditions: &bray_symbols::CallableConditionSet,
    arguments: &[ConstantTermId],
    assumptions: &[(ConstantTermId, bool)],
) -> Result<bool, SemanticValueStoreError> {
    preconditions_hold_with(values, conditions, assumptions, &mut |condition| {
        instantiate_condition(values, condition, arguments)
    })
}

pub(crate) fn preconditions_hold_with(
    values: &SemanticValueStore,
    conditions: &bray_symbols::CallableConditionSet,
    assumptions: &[(ConstantTermId, bool)],
    normalize: &mut impl FnMut(
        ConstantTermId,
    ) -> Result<Option<ConstantTermId>, SemanticValueStoreError>,
) -> Result<bool, SemanticValueStoreError> {
    use bray_symbols::CallableConditions;

    if conditions.invocation_preconditions().len() > MAX_CONDITION_STEPS {
        return Ok(false);
    }

    for clause in conditions.invocation_preconditions() {
        let Some(condition) = clause
            .predicate()
            .and_then(|predicate| predicate.condition())
        else {
            return Ok(false);
        };

        let Some(condition) = normalize(condition)? else {
            return Ok(false);
        };

        if prove_condition(values, assumptions, condition)? != ProofOutcome::Proven {
            return Ok(false);
        }
    }

    Ok(true)
}

pub(crate) fn callable_input_count(
    values: &SemanticValueStore,
    roots: impl IntoIterator<Item = ConstantTermId>,
) -> Result<Option<u32>, SemanticValueStoreError> {
    let mut pending = roots.into_iter().collect::<Vec<_>>();
    let mut visited = BTreeSet::new();
    let mut count = 0;
    let mut remaining = MAX_CONDITION_STEPS;

    while let Some(term) = pending.pop() {
        let Some(next) = remaining.checked_sub(1) else {
            return Ok(None);
        };

        remaining = next;

        if !visited.insert(term) {
            continue;
        }

        let data = values.constant_term_data(term)?;

        if let ConstantTermData::CallableArgument(ordinal) = &*data {
            let Some(next) = ordinal.raw().checked_add(1) else {
                return Ok(None);
            };

            count = count.max(next);
        }

        if data
            .try_map_terms(|child| {
                if pending.len() >= remaining {
                    return Err(());
                }

                pending.push(child);

                Ok(child)
            })
            .is_err()
        {
            return Ok(None);
        }
    }

    Ok(usize::try_from(count)
        .ok()
        .filter(|count| *count <= MAX_CONDITION_STEPS)
        .map(|_| count))
}

/// Instantiates a checked condition's receiver-first value inputs without executing it.
///
/// Generic substitution is a separate operation. Arguments must already have compatible checked
/// types and valid observation identities. Missing arguments or exhausted work return no evidence.
/// Replacement terms are inserted once, not recursively interpreted as formal inputs.
pub fn instantiate_condition(
    values: &SemanticValueStore,
    condition: ConstantTermId,
    arguments: &[ConstantTermId],
) -> Result<Option<ConstantTermId>, SemanticValueStoreError> {
    let mut remaining = MAX_CONDITION_STEPS;

    instantiate_condition_with_budget(values, condition, arguments, &mut remaining)
}

pub(crate) fn instantiate_condition_with_budget(
    values: &SemanticValueStore,
    condition: ConstantTermId,
    arguments: &[ConstantTermId],
    remaining: &mut usize,
) -> Result<Option<ConstantTermId>, SemanticValueStoreError> {
    rewrite_condition_with_budget(values, condition, remaining, |_, data| {
        Ok(match data {
            ConstantTermData::CallableArgument(ordinal) => std::ops::ControlFlow::Break(
                usize::try_from(ordinal.raw())
                    .ok()
                    .and_then(|index| arguments.get(index))
                    .copied(),
            ),
            _ => std::ops::ControlFlow::Continue(()),
        })
    })
}

/// Replaces selected observations before traversing their operands. A missing replacement
/// supplies no evidence, and inserted observations are never rewritten a second time.
pub(crate) fn rewrite_condition_with_budget(
    values: &SemanticValueStore,
    condition: ConstantTermId,
    remaining: &mut usize,
    mut replace: impl FnMut(
        ConstantTermId,
        &ConstantTermData,
    ) -> Result<
        std::ops::ControlFlow<Option<ConstantTermId>>,
        SemanticValueStoreError,
    >,
) -> Result<Option<ConstantTermId>, SemanticValueStoreError> {
    let mut pending = vec![(condition, false)];
    let mut mapped = BTreeMap::new();

    while let Some((term, expanded)) = pending.pop() {
        if mapped.contains_key(&term) {
            continue;
        }

        let Some(next) = remaining.checked_sub(1) else {
            return Ok(None);
        };

        *remaining = next;

        let data = values.constant_term_data(term)?;

        if let std::ops::ControlFlow::Break(replacement) = replace(term, &data)? {
            let Some(argument) = replacement else {
                return Ok(None);
            };

            values.constant_term_data(argument)?;
            mapped.insert(term, argument);

            continue;
        }

        if !expanded {
            pending.push((term, true));

            if data
                .try_map_terms(|child| {
                    if pending.len() >= *remaining {
                        return Err(());
                    }

                    pending.push((child, false));

                    Ok(child)
                })
                .is_err()
            {
                return Ok(None);
            }

            continue;
        }

        let Ok(rewritten) = data.try_map_terms(|child| mapped.get(&child).copied().ok_or(()))
        else {
            return Ok(None);
        };

        let projected = matches!(rewritten, ConstantTermData::Projection(_));
        let rewritten = values.intern_constant_term(rewritten)?;

        let rewritten = if projected {
            let Some(rewritten) =
                crate::constant::shape::observation_subject(values, rewritten, remaining)?
            else {
                return Ok(None);
            };

            rewritten
        } else {
            rewritten
        };

        mapped.insert(term, rewritten);
    }

    Ok(mapped.get(&condition).copied())
}

/// Proves a checked Boolean predicate from currently valid checked Boolean conditions.
///
/// The caller owns storage/version validity and must omit invalidated conditions. This service
/// never treats absent evidence as true and never executes a callable to obtain evidence.
/// Work beyond the bounded Boolean reasoning budget returns an unknown outcome.
pub fn prove_condition(
    values: &SemanticValueStore,
    conditions: &[(ConstantTermId, bool)],
    goal: ConstantTermId,
) -> Result<ProofOutcome, SemanticValueStoreError> {
    if conditions.len() >= MAX_CONDITION_STEPS {
        return Ok(ProofOutcome::Unknown);
    }

    let mut remaining = MAX_CONDITION_STEPS;

    match satisfiable(values, conditions, (goal, false), &mut remaining)? {
        Satisfiability::Impossible => return Ok(ProofOutcome::Proven),
        Satisfiability::Unknown => return Ok(ProofOutcome::Unknown),
        Satisfiability::Recovered => return Ok(ProofOutcome::Recovered),
        Satisfiability::Possible => {}
    }

    Ok(
        match satisfiable(values, conditions, (goal, true), &mut remaining)? {
            Satisfiability::Impossible => ProofOutcome::Disproven,
            Satisfiability::Recovered => ProofOutcome::Recovered,
            Satisfiability::Possible | Satisfiability::Unknown => ProofOutcome::Unknown,
        },
    )
}

enum Satisfiability {
    Possible,
    Impossible,
    Unknown,
    Recovered,
}

pub(crate) fn conditions_are_inconsistent(
    values: &SemanticValueStore,
    conditions: &[(ConstantTermId, bool)],
) -> Result<bool, SemanticValueStoreError> {
    let Some(condition) = conditions.first().copied() else {
        return Ok(false);
    };

    let mut remaining = MAX_CONDITION_STEPS;

    Ok(matches!(
        satisfiable(values, conditions, condition, &mut remaining)?,
        Satisfiability::Impossible
    ))
}

struct ConditionBranch {
    pending: Vec<(ConstantTermId, bool)>,
    known: BTreeMap<ConstantTermId, bool>,
    active_variants: BTreeMap<ConstantTermId, bray_symbols::UnionVariantSymbolId>,
}

fn satisfiable(
    values: &SemanticValueStore,
    conditions: &[(ConstantTermId, bool)],
    goal: (ConstantTermId, bool),
    remaining: &mut usize,
) -> Result<Satisfiability, SemanticValueStoreError> {
    let mut initial = conditions.to_vec();

    initial.push(goal);

    let mut branches = vec![ConditionBranch {
        pending: initial,
        known: BTreeMap::new(),
        active_variants: BTreeMap::new(),
    }];

    'branch: while let Some(mut branch) = branches.pop() {
        while let Some((term, truth)) = branch.pending.pop() {
            let Some(next) = remaining.checked_sub(1) else {
                return Ok(Satisfiability::Unknown);
            };

            *remaining = next;

            if let Some(previous) = branch.known.insert(term, truth) {
                if previous != truth {
                    continue 'branch;
                }

                continue;
            }

            let data = values.constant_term_data(term)?;

            match &*data {
                ConstantTermData::Typed { term, .. } => branch.pending.push((*term, truth)),
                ConstantTermData::Projection(_) => {
                    let Some(observed) =
                        crate::constant::shape::observation_subject(values, term, remaining)?
                    else {
                        return Ok(Satisfiability::Unknown);
                    };

                    if observed != term {
                        branch.pending.push((observed, truth));
                    }
                }
                ConstantTermData::Unary {
                    operation: ConstantUnaryOperation::LogicalNot,
                    operand,
                } => branch.pending.push((*operand, !truth)),
                ConstantTermData::Binary {
                    operation:
                        operation @ (ConstantBinaryOperation::Equal | ConstantBinaryOperation::NotEqual),
                    left,
                    right,
                } => {
                    let Some(left) =
                        crate::constant::shape::observation_subject(values, *left, remaining)?
                    else {
                        return Ok(Satisfiability::Unknown);
                    };

                    let Some(right) =
                        crate::constant::shape::observation_subject(values, *right, remaining)?
                    else {
                        return Ok(Satisfiability::Unknown);
                    };

                    if let Some(equal) = crate::constant::shape::literal_observations_equal(
                        values, left, right, remaining,
                    )? {
                        if equal != (truth == (*operation == ConstantBinaryOperation::Equal)) {
                            continue 'branch;
                        }

                        continue;
                    }

                    let boolean =
                        match crate::constant::shape::observation_boolean(values, left, remaining)?
                        {
                            Some(value) => Some((right, value)),
                            None => crate::constant::shape::observation_boolean(
                                values, right, remaining,
                            )?
                            .map(|value| (left, value)),
                        };

                    if let Some((operand, value)) = boolean {
                        let equal = truth == (*operation == ConstantBinaryOperation::Equal);

                        branch.pending.push((operand, value == equal));

                        continue;
                    }

                    let (left, right) = if left <= right {
                        (left, right)
                    } else {
                        (right, left)
                    };

                    let equal = values.intern_constant_term(ConstantTermData::Binary {
                        operation: ConstantBinaryOperation::Equal,
                        left,
                        right,
                    })?;

                    if equal != term || *operation == ConstantBinaryOperation::NotEqual {
                        branch.pending.push((
                            equal,
                            truth == (*operation == ConstantBinaryOperation::Equal),
                        ));
                    }
                }
                ConstantTermData::Binary {
                    operation:
                        operation @ (ConstantBinaryOperation::LogicalAnd
                        | ConstantBinaryOperation::LogicalOr),
                    left,
                    right,
                } => {
                    if (*operation == ConstantBinaryOperation::LogicalAnd) == truth {
                        branch.pending.extend([(*left, truth), (*right, truth)]);
                    } else {
                        let cost = branch
                            .pending
                            .len()
                            .saturating_add(branch.known.len())
                            .saturating_add(branch.active_variants.len());

                        let Some(next) = remaining.checked_sub(cost) else {
                            return Ok(Satisfiability::Unknown);
                        };

                        *remaining = next;

                        // Alternatives own independent bounded maps of copyable condition identities.
                        let mut alternative = branch.pending.clone();
                        alternative.push((*right, truth));

                        branches.push(ConditionBranch {
                            pending: alternative,
                            known: branch.known.clone(),
                            active_variants: branch.active_variants.clone(),
                        });

                        branch.pending.push((*left, truth));
                    }
                }
                ConstantTermData::Test { subject, kind } => {
                    let Some(observed) =
                        crate::constant::shape::observation_subject(values, *subject, remaining)?
                    else {
                        return Ok(Satisfiability::Unknown);
                    };

                    if observed != *subject {
                        let test = values.intern_constant_term(ConstantTermData::Test {
                            subject: observed,
                            kind: *kind,
                        })?;

                        branch.pending.push((test, truth));

                        continue;
                    }

                    if let Some(actual) =
                        crate::constant::shape::test_term_shape(values, *subject, *kind, remaining)?
                    {
                        if actual != truth {
                            continue 'branch;
                        }
                    }

                    if truth
                        && let bray_symbols::ConstantTest::ActiveUnionVariant(variant) = kind
                        && branch
                            .active_variants
                            .insert(*subject, *variant)
                            .is_some_and(|previous| previous != *variant)
                    {
                        continue 'branch;
                    }
                }
                ConstantTermData::Value(value) => {
                    match values.constant_value_data(*value)?.kind() {
                        ConstantValueKind::Boolean(actual) if *actual != truth => continue 'branch,
                        ConstantValueKind::Error => return Ok(Satisfiability::Recovered),
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        return Ok(Satisfiability::Possible);
    }

    Ok(Satisfiability::Impossible)
}

#[cfg(test)]
mod tests {
    use super::{MAX_CONDITION_STEPS, instantiate_condition, prove_condition};
    use bray_symbols::{
        ConstantBinaryOperation, ConstantTermData, ConstantUnaryOperation, ConstantValueKind,
        ProofOutcome, SemanticValueStore, SymbolOrdinal,
    };

    #[test]
    fn boolean_projection_facts_share_typed_roots_but_preserve_distinct_paths() {
        use bray_symbols::{ConstantProjection, ConstantProjectionKind, TypeData};

        let values = SemanticValueStore::try_new().unwrap();
        let ty = values.intern_type(TypeData::tuple([])).unwrap();

        let root = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .unwrap();

        let typed = values
            .intern_constant_term(ConstantTermData::Typed { term: root, ty })
            .unwrap();

        let kind = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0));

        let condition = values
            .intern_constant_term(ConstantTermData::Projection(ConstantProjection::new(
                typed, kind,
            )))
            .unwrap();

        let goal = values
            .intern_constant_term(ConstantTermData::Projection(ConstantProjection::new(
                root, kind,
            )))
            .unwrap();

        let sibling = values
            .intern_constant_term(ConstantTermData::Projection(ConstantProjection::new(
                root,
                ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1)),
            )))
            .unwrap();

        assert_eq!(
            prove_condition(&values, &[(condition, true)], goal).unwrap(),
            ProofOutcome::Proven
        );

        assert_eq!(
            prove_condition(&values, &[(condition, false)], goal).unwrap(),
            ProofOutcome::Disproven
        );

        assert_eq!(
            prove_condition(&values, &[(condition, true)], sibling).unwrap(),
            ProofOutcome::Unknown
        );

        let aggregate = values
            .intern_constant_term(ConstantTermData::tuple([root]))
            .unwrap();

        let selected = values
            .intern_constant_term(ConstantTermData::Projection(ConstantProjection::new(
                aggregate, kind,
            )))
            .unwrap();

        assert_eq!(
            prove_condition(&values, &[(root, true)], selected).unwrap(),
            ProofOutcome::Proven
        );

        assert_eq!(
            prove_condition(&values, &[(root, false)], selected).unwrap(),
            ProofOutcome::Disproven
        );
    }

    #[test]
    fn shape_facts_share_typed_observations_without_equating_different_values() {
        let values = SemanticValueStore::try_new().unwrap();

        let ty = values
            .intern_type(bray_symbols::TypeData::tuple([]))
            .unwrap();

        let first = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .unwrap();

        let second = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(1)))
            .unwrap();

        let typed = values
            .intern_constant_term(ConstantTermData::Typed { term: first, ty })
            .unwrap();

        let success = crate::test_support::compiler_known_symbol::<
            bray_symbols::UnionVariantSymbolId,
        >("ResultVariant0Ok");

        for kind in [
            bray_symbols::ConstantTest::NullablePresent,
            bray_symbols::ConstantTest::ActiveUnionVariant(success),
        ] {
            let condition = values
                .intern_constant_term(ConstantTermData::Test {
                    subject: typed,
                    kind,
                })
                .unwrap();

            let goal = values
                .intern_constant_term(ConstantTermData::Test {
                    subject: first,
                    kind,
                })
                .unwrap();

            let unrelated = values
                .intern_constant_term(ConstantTermData::Test {
                    subject: second,
                    kind,
                })
                .unwrap();

            assert_eq!(
                prove_condition(&values, &[(condition, true)], goal).unwrap(),
                ProofOutcome::Proven
            );

            assert_eq!(
                prove_condition(&values, &[(condition, false)], goal).unwrap(),
                ProofOutcome::Disproven
            );

            assert_eq!(
                prove_condition(&values, &[(condition, true)], unrelated).unwrap(),
                ProofOutcome::Unknown
            );
        }
    }

    #[test]
    fn structural_equalities_normalize_typed_projected_operands() {
        use bray_symbols::{ConstantProjection, ConstantProjectionKind, TypeData};

        let values = SemanticValueStore::try_new().unwrap();
        let ty = values.intern_type(TypeData::tuple([])).unwrap();

        let root = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .unwrap();

        let other = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(1)))
            .unwrap();

        let typed = values
            .intern_constant_term(ConstantTermData::typed(root, ty))
            .unwrap();

        let kind = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0));

        let projected = values
            .intern_constant_term(ConstantTermData::Projection(ConstantProjection::new(
                typed, kind,
            )))
            .unwrap();

        let plain = values
            .intern_constant_term(ConstantTermData::Projection(ConstantProjection::new(
                root, kind,
            )))
            .unwrap();

        let condition = values
            .intern_constant_term(ConstantTermData::Binary {
                operation: ConstantBinaryOperation::Equal,
                left: projected,
                right: other,
            })
            .unwrap();

        let goal = values
            .intern_constant_term(ConstantTermData::Binary {
                operation: ConstantBinaryOperation::Equal,
                left: other,
                right: plain,
            })
            .unwrap();

        assert_eq!(
            prove_condition(&values, &[(condition, true)], goal).unwrap(),
            ProofOutcome::Proven
        );

        assert_eq!(
            prove_condition(&values, &[(condition, false)], goal).unwrap(),
            ProofOutcome::Disproven
        );
    }

    #[test]
    fn distinct_active_variants_are_exclusive_for_the_same_observation() {
        use bray_symbols::{ConstantTest, UnionVariantSymbolId};

        let values = SemanticValueStore::try_new().unwrap();

        let first = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .unwrap();

        let second = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(1)))
            .unwrap();

        let ok =
            crate::test_support::compiler_known_symbol::<UnionVariantSymbolId>("ResultVariant0Ok");

        let error = crate::test_support::compiler_known_symbol::<UnionVariantSymbolId>(
            "ResultVariant1Error",
        );

        let test = |subject, variant| {
            values
                .intern_constant_term(ConstantTermData::Test {
                    subject,
                    kind: ConstantTest::ActiveUnionVariant(variant),
                })
                .unwrap()
        };

        let success = test(first, ok);
        let failure = test(first, error);
        let unrelated = test(second, error);

        assert_eq!(
            prove_condition(&values, &[(success, true)], failure).unwrap(),
            ProofOutcome::Disproven
        );

        assert_eq!(
            prove_condition(&values, &[(failure, true)], success).unwrap(),
            ProofOutcome::Disproven
        );

        assert_eq!(
            prove_condition(&values, &[(success, false)], failure).unwrap(),
            ProofOutcome::Unknown
        );

        assert_eq!(
            prove_condition(&values, &[(success, true)], unrelated).unwrap(),
            ProofOutcome::Unknown
        );
    }

    #[test]
    fn condition_instantiation_replaces_formals_once_and_requires_every_used_input() {
        let values = SemanticValueStore::try_new().unwrap();

        let left = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .unwrap();

        let right = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(1)))
            .unwrap();

        let both = values
            .intern_constant_term(ConstantTermData::Binary {
                operation: ConstantBinaryOperation::LogicalAnd,
                left,
                right,
            })
            .unwrap();

        let swapped = values
            .intern_constant_term(ConstantTermData::Binary {
                operation: ConstantBinaryOperation::LogicalAnd,
                left: right,
                right: left,
            })
            .unwrap();

        assert_eq!(
            instantiate_condition(&values, both, &[right, left]),
            Ok(Some(swapped))
        );

        assert_eq!(instantiate_condition(&values, both, &[left]), Ok(None));

        assert_eq!(
            instantiate_condition(&values, left, &[right]),
            Ok(Some(right))
        );
    }

    #[test]
    fn comparisons_with_boolean_literals_preserve_logical_facts() {
        let values = SemanticValueStore::try_new().unwrap();

        let ty = values
            .intern_type(bray_symbols::TypeData::tuple([]))
            .unwrap();

        let input = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .unwrap();

        for literal in [false, true] {
            let value = values
                .intern_constant_value(bray_symbols::ConstantValueData::new(
                    ty,
                    ConstantValueKind::Boolean(literal),
                ))
                .unwrap();

            let literal_term = values
                .intern_constant_term(ConstantTermData::Value(value))
                .unwrap();

            for operation in [
                ConstantBinaryOperation::Equal,
                ConstantBinaryOperation::NotEqual,
            ] {
                for (left, right) in [(input, literal_term), (literal_term, input)] {
                    let comparison = values
                        .intern_constant_term(ConstantTermData::Binary {
                            operation,
                            left,
                            right,
                        })
                        .unwrap();

                    for truth in [false, true] {
                        let expected =
                            literal == (truth == (operation == ConstantBinaryOperation::Equal));

                        assert_eq!(
                            prove_condition(&values, &[(comparison, truth)], input).unwrap(),
                            if expected {
                                ProofOutcome::Proven
                            } else {
                                ProofOutcome::Disproven
                            }
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn boolean_conditions_compose_without_deciding_unknown_atoms() {
        let values = SemanticValueStore::try_new().unwrap();

        let left = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .unwrap();

        let right = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(1)))
            .unwrap();

        let both = values
            .intern_constant_term(ConstantTermData::Binary {
                operation: ConstantBinaryOperation::LogicalAnd,
                left,
                right,
            })
            .unwrap();

        let either = values
            .intern_constant_term(ConstantTermData::Binary {
                operation: ConstantBinaryOperation::LogicalOr,
                left,
                right,
            })
            .unwrap();

        let not_left = values
            .intern_constant_term(ConstantTermData::Unary {
                operation: ConstantUnaryOperation::LogicalNot,
                operand: left,
            })
            .unwrap();

        assert_eq!(
            prove_condition(&values, &[(both, true)], left),
            Ok(ProofOutcome::Proven)
        );

        assert_eq!(
            prove_condition(&values, &[(either, false)], not_left),
            Ok(ProofOutcome::Proven)
        );

        assert_eq!(
            prove_condition(&values, &[(left, true)], both),
            Ok(ProofOutcome::Unknown)
        );

        assert_eq!(
            prove_condition(&values, &[(left, true)], either),
            Ok(ProofOutcome::Proven)
        );

        assert_eq!(
            prove_condition(&values, &[(left, false)], both),
            Ok(ProofOutcome::Disproven)
        );

        assert_eq!(
            prove_condition(&values, &[(both, false)], left),
            Ok(ProofOutcome::Unknown)
        );

        assert_eq!(
            prove_condition(&values, &[(either, true)], left),
            Ok(ProofOutcome::Unknown)
        );

        assert_eq!(
            prove_condition(&values, &[], not_left),
            Ok(ProofOutcome::Unknown)
        );
    }

    #[test]
    fn contradictory_entry_domains_are_unreachable() {
        let values = SemanticValueStore::try_new().unwrap();

        let input = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .unwrap();

        let opposite = values
            .intern_constant_term(ConstantTermData::Unary {
                operation: ConstantUnaryOperation::LogicalNot,
                operand: input,
            })
            .unwrap();

        assert_eq!(
            prove_condition(&values, &[(input, true), (opposite, true)], input),
            Ok(ProofOutcome::Proven)
        );

        assert_eq!(
            prove_condition(&values, &[(input, true), (opposite, true)], opposite),
            Ok(ProofOutcome::Proven)
        );
    }

    #[test]
    fn condition_implication_combines_alternative_domains_without_assuming_an_atom() {
        let values = SemanticValueStore::try_new().unwrap();

        let left = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .unwrap();

        let right = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(1)))
            .unwrap();

        let not_left = values
            .intern_constant_term(ConstantTermData::Unary {
                operation: ConstantUnaryOperation::LogicalNot,
                operand: left,
            })
            .unwrap();

        let either = values
            .intern_constant_term(ConstantTermData::Binary {
                operation: ConstantBinaryOperation::LogicalOr,
                left,
                right,
            })
            .unwrap();

        let complete = values
            .intern_constant_term(ConstantTermData::Binary {
                operation: ConstantBinaryOperation::LogicalOr,
                left,
                right: not_left,
            })
            .unwrap();

        let impossible = values
            .intern_constant_term(ConstantTermData::Binary {
                operation: ConstantBinaryOperation::LogicalAnd,
                left,
                right: not_left,
            })
            .unwrap();

        assert_eq!(
            prove_condition(&values, &[], complete),
            Ok(ProofOutcome::Proven)
        );

        assert_eq!(
            prove_condition(&values, &[], impossible),
            Ok(ProofOutcome::Disproven)
        );

        assert_eq!(
            prove_condition(&values, &[(either, true), (left, false)], right),
            Ok(ProofOutcome::Proven)
        );

        assert_eq!(
            prove_condition(&values, &[(either, true)], right),
            Ok(ProofOutcome::Unknown)
        );
    }

    #[test]
    fn unequal_complements_equal_without_equating_distinct_operands() {
        let values = SemanticValueStore::try_new().unwrap();

        let operands = (0..3)
            .map(|index| {
                values
                    .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(
                        index,
                    )))
                    .unwrap()
            })
            .collect::<Vec<_>>();

        let comparison = |operation, right| {
            values
                .intern_constant_term(ConstantTermData::Binary {
                    operation,
                    left: operands[0],
                    right,
                })
                .unwrap()
        };

        let equal = comparison(ConstantBinaryOperation::Equal, operands[1]);
        let unequal = comparison(ConstantBinaryOperation::NotEqual, operands[1]);
        let other = comparison(ConstantBinaryOperation::NotEqual, operands[2]);

        assert_eq!(
            prove_condition(&values, &[(equal, true)], unequal).unwrap(),
            ProofOutcome::Disproven
        );

        assert_eq!(
            prove_condition(&values, &[(equal, false)], unequal).unwrap(),
            ProofOutcome::Proven
        );

        assert_eq!(
            prove_condition(&values, &[(unequal, false)], equal).unwrap(),
            ProofOutcome::Proven
        );

        assert_eq!(
            prove_condition(&values, &[(equal, true)], other).unwrap(),
            ProofOutcome::Unknown
        );
    }

    #[test]
    fn deep_conditions_use_bounded_iterative_work() {
        let values = SemanticValueStore::try_new().unwrap();

        let input = values
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .unwrap();

        let mut nested = input;

        for _ in 0..MAX_CONDITION_STEPS {
            nested = values
                .intern_constant_term(ConstantTermData::Unary {
                    operation: ConstantUnaryOperation::LogicalNot,
                    operand: nested,
                })
                .unwrap();
        }

        assert_eq!(
            prove_condition(&values, &[], nested),
            Ok(ProofOutcome::Unknown)
        );

        assert_eq!(
            prove_condition(&values, &[(nested, true)], input),
            Ok(ProofOutcome::Unknown)
        );

        assert_eq!(instantiate_condition(&values, nested, &[input]), Ok(None));
    }
}
