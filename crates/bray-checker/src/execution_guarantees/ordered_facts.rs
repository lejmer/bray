use std::collections::BTreeSet;

use hashbrown::{HashMap, HashSet};
use bray_bound_tree::BoundOperator;

use super::{ConditionIndex, ExecutionCondition};
use super::condition::ConditionId;

struct Edge {
    position: usize,
    target: ConditionId,
    strict: bool,
}

struct Node {
    value: ExecutionCondition,
    ordered: usize,
    edges: Vec<Edge>,
}

/// One immutable evidence scope owns its ordered edges and their original scan positions.
pub(super) struct OrderedFacts {
    conditions: ConditionIndex,
    nodes: HashMap<ConditionId, Node>,
    length: usize,
    has_edges: bool,
}

impl OrderedFacts {
    pub(super) fn new(facts: &BTreeSet<(ExecutionCondition, bool)>, conditions: &ConditionIndex) -> Self {
        let mut result = Self { conditions: conditions.clone(), nodes: HashMap::new(), length: facts.len(), has_edges: false };

        for (position, (condition, holds)) in facts.iter().enumerate() {
            let ExecutionCondition::Operation(operator, operands) = condition else { continue; };

            // Only positive ordered comparisons and false inequality exclude NaN.
            if !matches!((operator, holds), (BoundOperator::Equal | BoundOperator::Less | BoundOperator::LessEqual
                | BoundOperator::Greater | BoundOperator::GreaterEqual, true) | (BoundOperator::NotEqual, false)) { continue; }

            let operands = operands.iter().map(|value| {
                let id = conditions.intern(value);

                result.nodes.entry(id).or_insert_with(|| Node {
                    value: value.clone(), ordered: position + 1, edges: Vec::new(),
                });

                id
            }).collect::<Vec<_>>();

            if !holds { continue; }

            let [left, right] = operands.as_slice() else { continue; };

            let (from, to, strict) = match operator {
                BoundOperator::Less => (*left, *right, true),
                BoundOperator::LessEqual | BoundOperator::Equal => (*left, *right, false),
                BoundOperator::Greater => (*right, *left, true),
                BoundOperator::GreaterEqual => (*right, *left, false),
                _ => unreachable!("positive ordered evidence has an ordered operator"),
            };

            result.has_edges = true;

            result.nodes.get_mut(&from).expect("ordered source was indexed").edges.push(Edge {
                position: position + 1, target: to, strict,
            });

            if *operator == BoundOperator::Equal && left != right {
                result.nodes.get_mut(&to).expect("equality target was indexed").edges.push(Edge {
                    position: position + 1, target: from, strict: false,
                });
            }
        }

        result
    }

    pub(super) fn positively_ordered(&self, value: &ExecutionCondition, budget: &mut usize) -> bool {
        if self.nodes.is_empty() {
            *budget = budget.saturating_sub(self.length);

            return false;
        }

        let key = self.conditions.intern(value);
        let position = self.nodes.get(&key).map(|node| node.ordered);

        if let Some(position) = position && *budget >= position {
            *budget -= position;

            return true;
        }

        *budget = budget.saturating_sub(self.length);

        false
    }

    pub(super) fn path(&self, lower: &ExecutionCondition, upper: &ExecutionCondition, requires_strict: bool, budget: &mut usize) -> bool {
        if self.length < 2 { return false; }

        if !self.has_edges {
            let result = literal_path(lower, upper, requires_strict);

            if !result { *budget = budget.saturating_sub(self.length); }

            return result;
        }

        let upper_id = self.conditions.intern(upper);
        let mut pending = vec![(lower, self.conditions.intern(lower), false)];
        let mut visited = HashSet::new();

        while let Some((current, key, strict)) = pending.pop() {
            if literal_path(current, upper, requires_strict && !strict) { return true; }
            if !visited.insert((key, strict)) { continue; }

            let mut consumed = 0;

            for edge in self.nodes.get(&key).into_iter().flat_map(|node| &node.edges) {
                let cost = edge.position - consumed;

                let Some(remaining) = budget.checked_sub(cost) else { *budget = 0; return false; };

                *budget = remaining;
                consumed = edge.position;

                let strict = strict || edge.strict;

                if edge.target == upper_id && (strict || !requires_strict) { return true; }

                pending.push((&self.nodes.get(&edge.target).expect("ordered target was indexed").value, edge.target, strict));
            }

            // Skipped facts still consume exactly the original bounded scan's work.
            let Some(remaining) = budget.checked_sub(self.length - consumed) else { *budget = 0; return false; };

            *budget = remaining;
        }

        false
    }
}

fn literal_path(current: &ExecutionCondition, upper: &ExecutionCondition, strict: bool) -> bool {
    let (ExecutionCondition::Literal(current), ExecutionCondition::Literal(upper)) = (current, upper) else { return false; };

    matches!((current.kind(), upper.kind()),
        (bray_symbols::ConstantValueKind::Integer(_), bray_symbols::ConstantValueKind::Integer(_)))
        && matches!(crate::constant::fold_binary(if strict { BoundOperator::Less } else { BoundOperator::LessEqual },
            current.kind(), upper.kind(), crate::ConstantEvaluationLimits::default().integer_bits()),
            Ok(bray_symbols::ConstantValueKind::Boolean(true)))
}

#[cfg(test)]
mod tests {
    use super::{BoundOperator, BTreeSet, ConditionIndex, ExecutionCondition, OrderedFacts};
    use std::sync::Arc;
    use bray_symbols::{ConstantValueData, ConstantValueKind, IntegerConstant, SymbolOrdinal};

    #[test]
    fn indexed_edges_preserve_scan_results_and_each_budget() {
        let input = |ordinal| ExecutionCondition::Input(super::super::ExecutionPlace::argument(SymbolOrdinal::new(ordinal)));

        let integer = |value| ExecutionCondition::Literal(Arc::new(ConstantValueData::new(crate::test_support::error_type(),
            ConstantValueKind::Integer(IntegerConstant::from_u64(value)))));

        let values = [input(0), input(1), integer(2), integer(4)];

        let relation = |op, left: usize, right: usize| ExecutionCondition::Operation(op,
            vec![values[left].clone(), values[right].clone()].into());

        let candidates = [
            (relation(BoundOperator::LessEqual, 0, 1), true),
            (relation(BoundOperator::Less, 1, 2), true),
            (relation(BoundOperator::Equal, 2, 0), true),
            (relation(BoundOperator::Less, 1, 3), false),
            (relation(BoundOperator::NotEqual, 1, 3), false),
            (relation(BoundOperator::Greater, 3, 1), true),
            (ExecutionCondition::Unknown, true),
        ];

        for mask in 0..(1 << candidates.len()) {
            let facts = candidates.iter().enumerate().filter(|(bit, _)| mask & (1 << bit) != 0)
                .map(|(_, fact)| fact.clone()).collect::<BTreeSet<_>>();

            let indexed = OrderedFacts::new(&facts, &ConditionIndex::default());

            for budget in 0..=64 {
                for lower in &values {
                    let (mut actual, mut expected) = (budget, budget);

                    assert_eq!(indexed.positively_ordered(lower, &mut actual), positively_ordered(lower, &facts, &mut expected));
                    assert_eq!(actual, expected, "ordered-domain budget at subset {mask}, limit {budget}");

                    for upper in &values {
                        for strict in [false, true] {
                            let (mut actual, mut expected) = (budget, budget);

                            assert_eq!(indexed.path(lower, upper, strict, &mut actual),
                                ordered_path(lower, upper, strict, &facts, &mut expected),
                                "path differs at subset {mask}, limit {budget}, strict={strict}, {lower:?} -> {upper:?}");

                            assert_eq!(actual, expected, "path budget at subset {mask}, limit {budget}");
                        }
                    }
                }
            }
        }
    }

    // Independent pre-index traversal protects edge order, NaN rules and exact exhausted budgets.
    fn positively_ordered(
        value: &ExecutionCondition,
        assumptions: &std::collections::BTreeSet<(ExecutionCondition, bool)>,
        budget: &mut usize,
    ) -> bool {
        let limit = *budget;

        assumptions.iter().take(limit).any(|(condition, holds)| {
            let Some(remaining) = budget.checked_sub(1) else {
                return false;
            };

            *budget = remaining;

            let ExecutionCondition::Operation(operator, operands) = condition else {
                return false;
            };

            // A held ordering or equality excludes NaN. A false ordering and a true
            // inequality do not. This uses value evidence, without assuming total order.
            matches!(
                (operator, holds),
                (
                    bray_bound_tree::BoundOperator::Equal
                        | bray_bound_tree::BoundOperator::Less
                        | bray_bound_tree::BoundOperator::LessEqual
                        | bray_bound_tree::BoundOperator::Greater
                        | bray_bound_tree::BoundOperator::GreaterEqual,
                    true
                ) | (bray_bound_tree::BoundOperator::NotEqual, false)
            ) && operands.iter().any(|operand| operand == value)
        })
    }

    fn ordered_path(
        lower: &ExecutionCondition,
        upper: &ExecutionCondition,
        requires_strict: bool,
        assumptions: &std::collections::BTreeSet<(ExecutionCondition, bool)>,
        budget: &mut usize,
    ) -> bool {
        use bray_bound_tree::BoundOperator;

        if assumptions.len() < 2 {
            return false;
        }

        let mut pending = vec![(lower, false)];
        let mut visited = std::collections::BTreeSet::new();

        while let Some((current, strict)) = pending.pop() {
            if let (ExecutionCondition::Literal(current), ExecutionCondition::Literal(upper)) =
                (current, upper)
                && matches!(
                    (current.kind(), upper.kind()),
                    (
                        bray_symbols::ConstantValueKind::Integer(_),
                        bray_symbols::ConstantValueKind::Integer(_)
                    )
                )
                && matches!(
                    crate::constant::fold_binary(
                        if requires_strict && !strict {
                            BoundOperator::Less
                        } else {
                            BoundOperator::LessEqual
                        },
                        current.kind(),
                        upper.kind(),
                        crate::ConstantEvaluationLimits::default().integer_bits(),
                    ),
                    Ok(bray_symbols::ConstantValueKind::Boolean(true))
                )
            {
                return true;
            }

            if !visited.insert((current, strict)) {
                continue;
            }

            for (condition, value) in assumptions {
                let Some(remaining) = budget.checked_sub(1) else {
                    return false;
                };

                *budget = remaining;

                let ExecutionCondition::Operation(operator, operands) = condition else {
                    continue;
                };

                let [left, right] = operands.as_ref() else {
                    continue;
                };

                // Negative comparisons cannot supply ordered edges because of NaN.
                if !value {
                    continue;
                }

                let (from, to, edge_strict) = match operator {
                    BoundOperator::Less => (left, right, true),
                    BoundOperator::LessEqual => (left, right, false),
                    BoundOperator::Greater => (right, left, true),
                    BoundOperator::GreaterEqual => (right, left, false),
                    BoundOperator::Equal if right == current => (right, left, false),
                    BoundOperator::Equal => (left, right, false),
                    _ => continue,
                };

                if from != current {
                    continue;
                }

                let strict = strict || edge_strict;

                if to == upper && (strict || !requires_strict) {
                    return true;
                }

                pending.push((to, strict));
            }
        }

        false
    }

}
