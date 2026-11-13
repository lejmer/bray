use std::collections::{BTreeSet, VecDeque};

use bray_bound_tree::BoundOperator;

use super::ExecutionCondition;
use super::boolean::{BooleanGraph, BooleanId, alternative_value};

#[derive(Default)]
struct ProofTerm {
    value: Option<bool>,
    asserted: Option<bool>,
    asserted_alternative: Option<bool>,
}

/// Derive Boolean consequences without promoting ordinary occurrences to trusted evidence.
pub(crate) fn trusted_condition_closure(
    known: BTreeSet<(ExecutionCondition, bool)>,
    ordinary: &BTreeSet<(ExecutionCondition, bool)>,
) -> BTreeSet<(ExecutionCondition, bool)> {
    prepare_trusted_conditions(known, ordinary).0
}

pub(super) fn prepare_trusted_conditions(
    mut known: BTreeSet<(ExecutionCondition, bool)>,
    ordinary: &BTreeSet<(ExecutionCondition, bool)>,
) -> (BTreeSet<(ExecutionCondition, bool)>, BooleanGraph) {
    let mut graph = BooleanGraph::new(&known, ordinary);
    let mut terms = (0..graph.terms.len()).map(|_| ProofTerm::default()).collect::<Vec<_>>();
    let mut queue = VecDeque::new();

    for (index, node) in graph.terms.iter().enumerate() {
        let condition = &node.condition;
        let explicit = |assumptions: &BTreeSet<_>| {
            [true, false].into_iter().find(|value| assumptions.contains(&(condition.clone(), *value)))
        };

        terms[index].asserted_alternative = alternative_value(condition).filter(|held| {
            explicit(&known) == Some(*held)
                || (!condition.contains_trusted() && explicit(ordinary) == Some(*held))
        });
        terms[index].asserted = match condition {
            ExecutionCondition::Unknown => None,
            ExecutionCondition::Boolean(value) => Some(*value),
            ExecutionCondition::Entry { .. } => None,
            ExecutionCondition::Trusted(_) => explicit(&known),
            _ if !node.children.is_empty() => explicit(&known).or_else(|| {
                (!condition.contains_trusted()).then(|| explicit(ordinary)).flatten()
            }),
            _ => condition.prove(&known, &mut { ExecutionCondition::WORK_LIMIT })
                .or_else(|| condition.prove(ordinary, &mut { ExecutionCondition::WORK_LIMIT })),
        };
        terms[index].value = terms[index].asserted;
        queue.push_back(BooleanId(index));
    }

    propagate(&graph, &mut terms, &mut queue, &mut known);

    // Refinement needs both positive guard facts and exclusions of inactive alternatives.
    for (node, term) in graph.terms.iter_mut().zip(terms) {
        if !node.condition.contains_trusted() && let Some(value) = term.value {
            // Facts and graph retain the same immutable condition operands.
            known.insert((node.condition.clone(), value));
            if alternative_value(&node.condition) == Some(value) {
                node.published = Some(value);
            }
        }
        if let Some(held) = term.asserted_alternative {
            node.published = Some(held);
        }
    }

    (known, graph)
}

fn propagate(
    graph: &BooleanGraph,
    terms: &mut [ProofTerm],
    queue: &mut VecDeque<BooleanId>,
    known: &mut BTreeSet<(ExecutionCondition, bool)>,
) {
    while let Some(id) = queue.pop_front() {
        let node = &graph.terms[id.0];
        let derived = graph.derived(id, |child| terms[child.0].value);
        let next = terms[id.0].asserted.or(derived);

        if terms[id.0].value != next {
            terms[id.0].value = next;
            queue.extend(node.parents.iter().copied());
        }

        if let Some(held) = terms[id.0].asserted_alternative && let [left, right] = node.children.as_slice() {
            let established = [
                (terms[left.0].value == Some(!held)).then_some(*right),
                (terms[right.0].value == Some(!held)).then_some(*left),
            ];

            for child in established.into_iter().flatten() {
                // Normal assumptions retain qualified predicates and negation polarity.
                graph.terms[child.0].condition.clone().assume_with(held, &mut |condition, value| {
                    if !known.insert((condition.clone(), value)) { return; }

                    if let Some(id) = graph.find(&condition) {
                        let term = &mut terms[id.0];

                        term.asserted = Some(term.asserted.unwrap_or(false) || value);

                        if term.value != term.asserted {
                            term.value = term.asserted;
                            queue.extend(graph.terms[id.0].parents.iter().copied());
                        }

                        if alternative_value(&condition) == Some(value) {
                            term.asserted_alternative = Some(value);
                            queue.push_back(id);
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
    fn indexed_queries_preserve_single_premise_boolean_implications() {
        let not = |value| ExecutionCondition::operation(BoundOperator::LogicalNot, vec![value]);
        let and = |left, right| ExecutionCondition::operation(BoundOperator::LogicalAnd, vec![left, right]);
        let or = |left, right| ExecutionCondition::operation(BoundOperator::LogicalOr, vec![left, right]);
        let a = input(0);
        let b = input(1);
        let qualified = ExecutionCondition::Trusted(Arc::new(input(2)));
        let atoms = [a.clone(), b.clone(), qualified.clone(), not(a.clone()), not(b.clone()),
            and(a.clone(), b.clone()), not(and(a.clone(), b.clone())),
            or(not(a.clone()), not(b.clone())), ExecutionCondition::Unknown];
        let clauses = atoms.iter().flat_map(|left| atoms.iter().map(|right| or(left.clone(), right.clone()))).collect::<BTreeSet<_>>();
        let ordinary = BTreeSet::new();

        for premise in &clauses {
            if !matches!(premise, ExecutionCondition::Operation(BoundOperator::LogicalOr, _)) { continue; }

            let facts = BTreeSet::from([(premise.clone(), true)]);
            let proof = crate::execution_guarantees::ExecutionProof::new(&facts, &ordinary);
            let mut reference = crate::execution_guarantees::ExecutionProof::new(&facts, &ordinary);

            reference.reference_scan = true;

            for goal in &clauses {
                if !matches!(goal, ExecutionCondition::Operation(BoundOperator::LogicalOr, _)) { continue; }

                assert_eq!(proof.prove_trusted(goal), reference.prove_trusted(goal),
                    "premise={premise:?}, goal={goal:?}");
            }
        }
    }

    #[test]
    fn indexed_queries_keep_numeric_and_variant_relationships() {
        let not = |value| ExecutionCondition::operation(BoundOperator::LogicalNot, vec![value]);
        let or = |left, right| ExecutionCondition::operation(BoundOperator::LogicalOr, vec![left, right]);
        let comparison = |operator| ExecutionCondition::operation(operator, vec![input(0), input(1)]);
        let claim = ExecutionCondition::Trusted(Arc::new(input(2)));
        let variant = |ordinal| bray_symbols::UnionVariantSymbolId::from_symbol_id(SymbolId::new(ordinal));
        let active = input(3).matches_variant(variant(900));
        let other = input(3).matches_variant(variant(901));
        let empty = BTreeSet::new();

        for (premise, goal, expected) in [
            (or(not(comparison(BoundOperator::LessEqual)), claim.clone()),
                or(not(comparison(BoundOperator::Less)), claim.clone()), true),
            (or(other, claim.clone()), or(not(active), claim.clone()), true),
            (or(not(comparison(BoundOperator::Less)), claim.clone()),
                or(comparison(BoundOperator::GreaterEqual), claim), false),
        ] {
            assert_eq!(premise.implies(&goal), expected);

            let facts = BTreeSet::from([(premise, true)]);
            let proof = crate::execution_guarantees::ExecutionProof::new(&facts, &empty);

            assert_eq!(proof.prove_trusted(&goal) == Some(true), expected);
        }
    }

    #[test]
    fn guarded_trusted_disjunctions_prove_weaker_requirements_without_selecting_an_alternative() {
        let ordinary = input(0);
        let promised = ExecutionCondition::Trusted(Arc::new(input(2)));
        let extra = ExecutionCondition::Trusted(Arc::new(input(1)));

        let established = ExecutionCondition::operation(BoundOperator::LogicalOr,
            vec![ordinary, promised.clone()]);

        let required = ExecutionCondition::operation(BoundOperator::LogicalOr,
            vec![established.clone(), extra.clone()]);

        let known = BTreeSet::from([(established.clone(), true)]);

        assert!(established.implies(&required));
        assert_eq!(required.prove_mixed(&known, &BTreeSet::new(), &mut { ExecutionCondition::WORK_LIMIT }), Some(true));
        assert_eq!(promised.prove_mixed(&known, &BTreeSet::new(), &mut { ExecutionCondition::WORK_LIMIT }), None);
        assert_eq!(extra.prove_mixed(&known, &BTreeSet::new(), &mut { ExecutionCondition::WORK_LIMIT }), None);
        assert_eq!(required.prove_mixed(&BTreeSet::new(), &known, &mut { ExecutionCondition::WORK_LIMIT }), None);

        let mut unrelated = known;

        for index in 0..256 {
            let clause = ExecutionCondition::operation(BoundOperator::LogicalOr,
                vec![input(10 + index), ExecutionCondition::Trusted(Arc::new(input(1000 + index)))]);

            unrelated.insert((clause, true));
        }

        let ordinary = BTreeSet::from([(input(3000), true)]);
        let decisive = ExecutionCondition::operation(BoundOperator::LogicalOr, vec![input(3000), extra]);

        assert_eq!(decisive.prove_mixed(&unrelated, &ordinary, &mut { ExecutionCondition::WORK_LIMIT }), Some(true));
    }

    #[test]
    fn derived_variant_exclusions_survive_without_establishing_guarded_authority() {
        let variant = |ordinal| bray_symbols::UnionVariantSymbolId::from_symbol_id(SymbolId::new(ordinal));
        let success = input(0).matches_variant(variant(900));
        let failure = input(0).matches_variant(variant(901));
        let claim = ExecutionCondition::Trusted(Arc::new(input(1)));

        let nullable = ExecutionCondition::operation(BoundOperator::LogicalOr,
            vec![input(2), claim.clone()]);

        let guarantee = ExecutionCondition::operation(BoundOperator::LogicalOr,
            vec![failure.clone(), nullable]);

        let known = trusted_condition_closure(BTreeSet::from([(guarantee, true)]),
            &BTreeSet::from([(success, true)]));

        assert!(known.contains(&(failure, false)));
        assert!(!known.contains(&(claim.clone(), true)));
        assert_eq!(claim.prove(&known, &mut { ExecutionCondition::WORK_LIMIT }), None);
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
    fn unrelated_facts_and_clauses_do_not_limit_disjunction_elimination() {
        let mut ordinary = BTreeSet::new();

        input(0).assume(false, &mut ordinary);

        ExecutionCondition::operation(BoundOperator::LogicalOr, vec![input(0), input(1)])
            .assume(true, &mut ordinary);

        for ordinal in 50_000..50_150 {
            input(ordinal).assume(false, &mut ordinary);
        }

        for ordinal in 0..6_000 {
            ExecutionCondition::operation(
                BoundOperator::LogicalOr,
                vec![input(1_000 + ordinal * 2), input(1_001 + ordinal * 2)],
            )
            .assume(true, &mut ordinary);
        }

        let known = trusted_condition_closure(BTreeSet::new(), &ordinary);

        assert_eq!(
            input(1).prove_mixed(&known, &ordinary, &mut { ExecutionCondition::WORK_LIMIT }),
            Some(true)
        );

        assert_eq!(
            ExecutionCondition::Trusted(Arc::new(input(1)))
                .prove_mixed(&known, &ordinary, &mut { ExecutionCondition::WORK_LIMIT }),
            None
        );
    }

    #[test]
    fn ordinary_disjunction_elimination_preserves_occurrence_authority() {
        let mut ordinary = BTreeSet::new();
        let qualified = ExecutionCondition::Trusted(Arc::new(input(1)));

        ExecutionCondition::operation(BoundOperator::LogicalOr, vec![input(0), input(1)])
            .assume(true, &mut ordinary);

        input(0).assume(false, &mut ordinary);

        let known = trusted_condition_closure(BTreeSet::new(), &ordinary);

        assert_eq!(
            input(1).prove_mixed(&known, &ordinary, &mut { ExecutionCondition::WORK_LIMIT }),
            Some(true)
        );

        assert_eq!(
            qualified.prove_mixed(&known, &ordinary, &mut { ExecutionCondition::WORK_LIMIT }),
            None
        );

        let mut forged = BTreeSet::new();

        ExecutionCondition::operation(BoundOperator::LogicalOr, vec![input(0), qualified.clone()])
            .assume(true, &mut forged);

        input(0).assume(false, &mut forged);

        let known = trusted_condition_closure(BTreeSet::new(), &forged);

        assert_eq!(
            qualified.prove_mixed(&known, &forged, &mut { ExecutionCondition::WORK_LIMIT }),
            None
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

    #[test]
    fn false_conjunctions_preserve_qualified_negative_authority() {
        let qualified = ExecutionCondition::Trusted(Arc::new(input(1)));
        let alternative = ExecutionCondition::operation(BoundOperator::LogicalAnd, vec![input(0), qualified.clone()]);
        let ordinary = BTreeSet::from([(input(0), true)]);
        let trusted = BTreeSet::from([(alternative.clone(), false)]);
        let known = trusted_condition_closure(trusted, &ordinary);

        assert_eq!(qualified.prove_trusted(&known, &ordinary), Some(false));
        assert_eq!(input(1).prove(&known, &mut { ExecutionCondition::WORK_LIMIT }), Some(false));

        let ordinary = BTreeSet::from([(alternative, false), (input(0), true)]);
        let known = trusted_condition_closure(BTreeSet::new(), &ordinary);

        assert_eq!(qualified.prove_trusted(&known, &ordinary), None,
            "an ordinary negative alternative cannot supply qualified negative evidence");
    }
}
