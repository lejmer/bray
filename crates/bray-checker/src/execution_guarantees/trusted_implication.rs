use std::collections::{BTreeMap, BTreeSet, VecDeque};

use bray_bound_tree::BoundOperator;

use super::ExecutionCondition;

#[derive(Default)]
struct ProofTerm {
    value: Option<bool>,
    asserted: Option<bool>,
    parents: Vec<usize>,
    asserted_disjunction: bool,
}

/// Derive conditional trusted evidence once, following only affected Boolean terms.
pub(crate) fn trusted_condition_closure(
    mut known: BTreeSet<(ExecutionCondition, bool)>,
    ordinary: &BTreeSet<(ExecutionCondition, bool)>,
) -> BTreeSet<(ExecutionCondition, bool)> {
    let trusted = &known;
    let mut indices = BTreeMap::new();
    let mut conditions = Vec::new();

    let mut pending = trusted
        .iter()
        .take(ExecutionCondition::WORK_LIMIT)
        .filter_map(|(condition, holds)| {
            (*holds
                && matches!(
                    condition,
                    ExecutionCondition::Operation(BoundOperator::LogicalOr, _)
                ))
            .then_some(condition.clone())
        })
        .collect::<Vec<_>>();

    while let Some(condition) = pending.pop() {
        // Each bounded input clause can contain a bounded Boolean expression.
        if indices.contains_key(&condition)
            || conditions.len() == ExecutionCondition::WORK_LIMIT * ExecutionCondition::WORK_LIMIT
        {
            continue;
        }

        indices.insert(condition.clone(), conditions.len());
        conditions.push(condition.clone());

        match condition {
            ExecutionCondition::Operation(
                BoundOperator::LogicalNot | BoundOperator::LogicalAnd | BoundOperator::LogicalOr,
                operands,
            ) => pending.extend(operands.iter().cloned()),
            ExecutionCondition::Entry { condition, .. }
            | ExecutionCondition::Trusted(condition) => pending.push(condition.as_ref().clone()),
            _ => {}
        }
    }

    let mut terms = (0..conditions.len())
        .map(|_| ProofTerm::default())
        .collect::<Vec<_>>();

    let mut queue = VecDeque::new();

    for (index, condition) in conditions.iter().enumerate() {
        let children = boolean_children(condition);

        for child in children {
            if let Some(child) = indices.get(child) {
                terms[*child].parents.push(index);
            }
        }

        let explicit = |assumptions: &BTreeSet<_>| {
            [true, false]
                .into_iter()
                .find(|value| assumptions.contains(&(condition.clone(), *value)))
        };

        terms[index].asserted_disjunction = matches!(
            condition,
            ExecutionCondition::Operation(BoundOperator::LogicalOr, _)
        ) && explicit(trusted) == Some(true);

        terms[index].asserted = match condition {
            ExecutionCondition::Unknown => None,
            ExecutionCondition::Boolean(value) => Some(*value),
            ExecutionCondition::Entry { .. } => None,
            ExecutionCondition::Trusted(_) => explicit(trusted),
            _ if !boolean_children(condition).is_empty() => explicit(trusted),
            _ => condition
                .prove(trusted, &mut { ExecutionCondition::WORK_LIMIT })
                .or_else(|| condition.prove(ordinary, &mut { ExecutionCondition::WORK_LIMIT })),
        };

        terms[index].value = terms[index].asserted;

        queue.push_back(index);
    }

    propagate(&conditions, &indices, &mut terms, &mut queue, &mut known);

    known
}

fn boolean_children(condition: &ExecutionCondition) -> &[ExecutionCondition] {
    match condition {
        ExecutionCondition::Operation(
            BoundOperator::LogicalNot | BoundOperator::LogicalAnd | BoundOperator::LogicalOr,
            operands,
        ) => operands,
        ExecutionCondition::Entry { condition, .. } => std::slice::from_ref(condition.as_ref()),
        _ => &[],
    }
}

fn propagate(
    conditions: &[ExecutionCondition],
    indices: &BTreeMap<ExecutionCondition, usize>,
    terms: &mut [ProofTerm],
    queue: &mut VecDeque<usize>,
    known: &mut BTreeSet<(ExecutionCondition, bool)>,
) {
    while let Some(index) = queue.pop_front() {
        let condition = &conditions[index];
        let children = boolean_children(condition);

        let value =
            |child: &ExecutionCondition| indices.get(child).and_then(|index| terms[*index].value);

        let derived = match (condition, children) {
            (ExecutionCondition::Operation(BoundOperator::LogicalNot, _), [child]) => {
                value(child).map(|value| !value)
            }
            (ExecutionCondition::Entry { .. }, [child]) => value(child),
            (ExecutionCondition::Operation(operator, _), [left, right]) => {
                match (operator, value(left), value(right)) {
                    (BoundOperator::LogicalAnd, Some(false), _)
                    | (BoundOperator::LogicalAnd, _, Some(false)) => Some(false),
                    (BoundOperator::LogicalAnd, Some(true), Some(true)) => Some(true),
                    (BoundOperator::LogicalOr, Some(true), _)
                    | (BoundOperator::LogicalOr, _, Some(true)) => Some(true),
                    (BoundOperator::LogicalOr, Some(false), Some(false)) => Some(false),
                    _ => None,
                }
            }
            _ => None,
        };

        let next = terms[index].asserted.or(derived);

        if terms[index].value != next {
            terms[index].value = next;
            queue.extend(terms[index].parents.iter().copied());
        }

        if terms[index].asserted_disjunction
            && let [left, right] = children
        {
            let established = [
                (indices.get(left).and_then(|index| terms[*index].value) == Some(false))
                    .then_some(right),
                (indices.get(right).and_then(|index| terms[*index].value) == Some(false))
                    .then_some(left),
            ];

            for condition in established.into_iter().flatten() {
                // Share the normal assumption rules, including qualified predicates and negation.
                condition
                    .clone()
                    .assume_with(true, &mut |condition, value| {
                        if !known.insert((condition.clone(), value)) {
                            return;
                        }

                        if let Some(index) = indices.get(&condition) {
                            let term = &mut terms[*index];

                            term.asserted = Some(term.asserted.unwrap_or(false) || value);

                            if term.value != term.asserted {
                                term.value = term.asserted;
                                queue.extend(term.parents.iter().copied());
                            }

                            if value
                                && matches!(
                                    condition,
                                    ExecutionCondition::Operation(BoundOperator::LogicalOr, _)
                                )
                            {
                                term.asserted_disjunction = true;
                                queue.push_back(*index);
                            }
                        }
                    });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use bray_bound_tree::{BoundOperator, BoundReferenceTarget};
    use bray_symbols::{CallableParameterSymbolId, SymbolId};

    use super::{ExecutionCondition, trusted_condition_closure};

    fn input(index: u32) -> ExecutionCondition {
        ExecutionCondition::Input(
            BoundReferenceTarget::Surface(
                CallableParameterSymbolId::from_symbol_id(SymbolId::new(index)).into(),
            )
            .into(),
        )
    }

    #[test]
    fn chained_disjunctions_follow_newly_established_negations() {
        let mut trusted = BTreeSet::new();
        let mut ordinary = BTreeSet::new();

        input(0).assume(false, &mut ordinary);

        for index in 0..128 {
            ExecutionCondition::operation(
                BoundOperator::LogicalOr,
                vec![
                    input(index),
                    ExecutionCondition::operation(
                        BoundOperator::LogicalNot,
                        vec![input(index + 1)],
                    ),
                ],
            )
            .assume(true, &mut trusted);
        }

        let known = trusted_condition_closure(trusted, &ordinary);

        assert_eq!(
            input(128).prove_mixed(&known, &ordinary, &mut { ExecutionCondition::WORK_LIMIT }),
            Some(false)
        );
    }

    #[test]
    fn ordinary_facts_do_not_create_qualified_evidence() {
        let qualified = ExecutionCondition::Trusted(Arc::new(input(1)));
        let mut ordinary = BTreeSet::new();

        qualified.clone().assume(true, &mut ordinary);

        assert_eq!(qualified.prove_trusted(&BTreeSet::new(), &ordinary), None);

        let mut trusted = BTreeSet::new();

        ExecutionCondition::operation(BoundOperator::LogicalOr, vec![qualified.clone(), input(2)])
            .assume(true, &mut trusted);

        input(2).assume(false, &mut ordinary);

        assert_eq!(qualified.prove_trusted(&trusted, &ordinary), Some(true));
    }
}
