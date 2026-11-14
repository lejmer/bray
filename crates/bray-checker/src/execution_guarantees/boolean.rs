use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use bray_bound_tree::BoundOperator;

use super::ExecutionCondition;
use super::condition::{ConditionId, ConditionIndex, ConditionMemo, ConditionNode, cached_condition};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct BooleanId(pub(super) usize);

pub(super) struct BooleanTerm {
    pub(super) condition: Arc<ConditionNode>,
    pub(super) children: Vec<BooleanId>,
    pub(super) parents: Vec<BooleanId>,
    pub(super) published: Option<bool>,
}

/// Shared Boolean structure, with qualified occurrences kept as independent atoms.
pub(super) struct BooleanGraph {
    pub(super) terms: Vec<BooleanTerm>,
    indices: HashMap<ConditionId, BooleanId>,
    conditions: ConditionIndex,
}

impl BooleanGraph {
    pub(super) fn new(
        trusted: &BTreeSet<(ExecutionCondition, bool)>,
        ordinary: &BTreeSet<(ExecutionCondition, bool)>,
        conditions: ConditionIndex,
    ) -> Self {
        let mut graph = Self { terms: Vec::new(), indices: HashMap::new(), conditions };

        for ((condition, holds), qualified) in trusted.iter().map(|fact| (fact, true)).chain(ordinary.iter().map(|fact| (fact, false))) {
            if alternative_value(condition) == Some(*holds) && (qualified || !condition.contains_trusted()) {
                let key = graph.conditions.intern(condition);
                let id = graph.intern(key);

                graph.terms[id.0].published = Some(*holds);
            }
        }

        graph
    }

    fn intern(&mut self, key: ConditionId) -> BooleanId {
        if let Some(id) = self.indices.get(&key) { return *id; }

        let condition = self.conditions.node(key);
        let children = &condition.children;

        let children = match &condition.value {
            ExecutionCondition::Operation(BoundOperator::LogicalNot, _) if children.len() == 1 => vec![self.intern(children[0])],
            ExecutionCondition::Operation(BoundOperator::LogicalAnd | BoundOperator::LogicalOr, _) if children.len() == 2 =>
                vec![self.intern(children[0]), self.intern(children[1])],
            ExecutionCondition::ValueAlias(..) => vec![self.intern(children[0]), self.intern(children[1])],
            ExecutionCondition::Entry { .. } | ExecutionCondition::Borrowed(_) => vec![self.intern(children[0])],
            ExecutionCondition::Observation { .. } => {
                self.intern(children[0]);

                vec![self.intern(children[1])]
            }
            ExecutionCondition::Trusted(_) => {
                self.intern(children[0]);

                Vec::new()
            }
            _ => Vec::new(),
        };

        let id = BooleanId(self.terms.len());

        for child in &children { self.terms[child.0].parents.push(id); }

        self.indices.insert(key, id);

        // The immutable structural node is shared. Scope-specific facts stay on this graph.
        self.terms.push(BooleanTerm {
            condition, children, parents: Vec::new(), published: None,
        });

        id
    }

    pub(super) fn find(&self, condition: &ExecutionCondition) -> Option<BooleanId> {
        self.indices.get(&self.conditions.intern(condition)).copied()
    }

    pub(super) fn assertions(&self, assumptions: &BTreeSet<(ExecutionCondition, bool)>) -> Vec<Option<bool>> {
        let mut assertions = vec![None; self.terms.len()];

        for (condition, holds) in assumptions {
            if let Some(id) = self.find(condition) {
                let asserted = &mut assertions[id.0];

                *asserted = Some(asserted.unwrap_or(false) || *holds);
            }
        }

        assertions
    }

    pub(super) fn derived(&self, id: BooleanId, value: impl Fn(BooleanId) -> Option<bool>) -> Option<bool> {
        let term = &self.terms[id.0];

        match (&term.condition.value, term.children.as_slice()) {
            (ExecutionCondition::Boolean(value), _) => Some(*value),
            (ExecutionCondition::Operation(BoundOperator::LogicalNot, _), [child]) => value(*child).map(|value| !value),
            (ExecutionCondition::Entry { .. } | ExecutionCondition::Observation { .. } | ExecutionCondition::Borrowed(_), [child]) => value(*child),
            (ExecutionCondition::Operation(operator @ (BoundOperator::LogicalAnd | BoundOperator::LogicalOr), _), [left, right]) =>
                prove_boolean(*operator, value(*left), || value(*right)),
            (ExecutionCondition::ValueAlias(_, _), [left, right]) => alias_value(value(*left), || value(*right)),
            _ => None,
        }
    }

    pub(super) fn consequences(&self, id: BooleanId, held: bool, value: impl Fn(BooleanId) -> Option<bool>) -> [Option<(BooleanId, bool)>; 2] {
        let term = &self.terms[id.0];

        let [left, right] = term.children.as_slice() else { return [None, None]; };

        if matches!(term.condition.value, ExecutionCondition::ValueAlias(_, _)) && held {
            return [value(*left).map(|value| (*right, value)), value(*right).map(|value| (*left, value))];
        }

        [
            (value(*left) == Some(!held)).then_some((*right, held)),
            (value(*right) == Some(!held)).then_some((*left, held)),
        ]
    }
}

/// Find published alternatives contradicted by a goal's contrary without testing every premise against it.
pub(super) struct AlternativeProof {
    graph: BooleanGraph,
    related: HashMap<ConditionId, Vec<BooleanId>>,
    answers: ConditionMemo<bool>,
    baseline: Vec<Option<bool>>,
    baseline_conflicts: Vec<BooleanId>,
}

/// Only this query can change its hypothetical facts and reached-node values.
struct AlternativeQuery<'a> {
    proof: &'a AlternativeProof,
    nodes: QueryNodes,
    queue: VecDeque<BooleanId>,
    candidates: Vec<BooleanId>,
    #[cfg(test)]
    work: usize,
}

#[derive(Clone, Copy, Default)]
struct QueryNode {
    // An absent override reads the immutable baseline. An explicit None is unknown.
    value: Option<Option<bool>>,
    asserted: Option<bool>,
    queued: bool,
}

/// Bound initialization for small graphs, while large graphs allocate only reached nodes.
enum QueryNodes {
    Dense(Vec<QueryNode>),
    Sparse(BTreeMap<BooleanId, QueryNode>),
}

impl QueryNodes {
    fn new(size: usize) -> Self {
        const DENSE_LIMIT: usize = 1024;

        if size <= DENSE_LIMIT { Self::Dense(vec![QueryNode::default(); size]) }
        else { Self::Sparse(BTreeMap::new()) }
    }

    fn get(&self, id: BooleanId) -> Option<&QueryNode> {
        match self {
            Self::Dense(nodes) => Some(&nodes[id.0]),
            Self::Sparse(nodes) => nodes.get(&id),
        }
    }

    fn entry(&mut self, id: BooleanId) -> &mut QueryNode {
        match self {
            Self::Dense(nodes) => &mut nodes[id.0],
            Self::Sparse(nodes) => nodes.entry(id).or_default(),
        }
    }

    #[cfg(test)]
    fn allocated(&self) -> usize {
        match self { Self::Dense(nodes) => nodes.len(), Self::Sparse(nodes) => nodes.len() }
    }
}

impl AlternativeProof {
    pub(super) fn new(graph: BooleanGraph) -> Self {
        let mut related = HashMap::<_, Vec<_>>::new();
        let mut baseline = Vec::with_capacity(graph.terms.len());
        let mut baseline_conflicts = Vec::new();

        // Children precede parents. Intrinsic values need preparation only once per view.
        for (index, term) in graph.terms.iter().enumerate() {
            for subject in related_subjects(&term.condition.value) {
                // Subject identities share structure without sharing established facts.
                related.entry(graph.conditions.intern(subject)).or_default().push(BooleanId(index));
            }

            // Without assumptions, only Boolean structure has a truth value. Opaque
            // atoms cannot gain evidence by recursively reproving their empty context.
            let value = graph.derived(BooleanId(index), |child| baseline[child.0]);

            baseline.push(value);

            if let Some(held) = term.published && value == Some(!held) {
                baseline_conflicts.push(BooleanId(index));
            }
        }

        let answers = Mutex::new(graph.conditions.map());

        Self { graph, related, baseline, baseline_conflicts, answers }
    }

    pub(super) fn proves(&self, consequence: &ExecutionCondition) -> bool {
        if consequence.is_unknown() { return false; }

        cached_condition(&self.answers, consequence, || AlternativeQuery::new(self).proves(consequence))
    }
}

impl<'a> AlternativeQuery<'a> {
    fn new(proof: &'a AlternativeProof) -> Self {
        Self {
            proof, nodes: QueryNodes::new(proof.graph.terms.len()),
            queue: VecDeque::new(), candidates: Vec::new(),
            #[cfg(test)]
            work: 0,
        }
    }

    fn proves(&mut self, consequence: &ExecutionCondition) -> bool {
        let mut contrary = BTreeSet::new();

        // Negating a qualified occurrence never establishes an ordinary negative predicate.
        consequence.clone().assume_proof(false, false, &mut |condition, holds| {
            contrary.insert((condition, holds));
        });

        for (condition, holds) in &contrary {
            self.seed(condition, *holds);
        }

        while let Some(id) = self.queue.pop_front() {
            let asserted = {
                let node = self.nodes.entry(id);

                node.queued = false;

                node.asserted
            };

            #[cfg(test)]
            { self.work += 1; }

            let proof = self.proof;
            let term = &proof.graph.terms[id.0];

            let value = if let ExecutionCondition::Boolean(value) = &term.condition.value {
                Some(*value)
            } else if let Some(value) = asserted {
                Some(value)
            } else {
                self.proof.graph.derived(id, |child| self.value(child)).or_else(|| {
                    term.children.is_empty().then(|| term.condition.value.prove(&contrary, &mut { ExecutionCondition::WORK_LIMIT })).flatten()
                })
            };

            if self.value(id) != value {
                self.nodes.entry(id).value = Some(value);

                for parent in &term.parents { self.enqueue(*parent); }
            }

            if let Some(held) = term.published && value == Some(!held) {
                self.candidates.push(id);
            }

            // Only an established ordinary data correspondence can extend the hypothesis.
            // Never infer alias identity merely because two conditions have matching truth.
            let copied = if matches!(term.condition.value, ExecutionCondition::ValueAlias(_, _)) && term.published == Some(true) {
                self.proof.graph.consequences(id, true, |child| self.value(child)).map(|copy| {
                    copy.filter(|(child, _)| self.value(*child).is_none()).map(|(child, value)| {
                        // The hypothesis owns these fact keys while operands stay shared with the graph.
                        (self.proof.graph.terms[child.0].condition.value.clone(), value)
                    })
                })
            } else { [None, None] };

            for (condition, held) in copied.into_iter().flatten() {
                let mut derived = Vec::new();

                condition.assume_proof(held, false, &mut |condition, value| {
                    if contrary.insert((condition.clone(), value)) { derived.push((condition, value)); }
                });

                for (condition, held) in derived { self.seed(&condition, held); }
            }
        }

        // The graph selects candidates. The owning bounded rule still decides the implication.
        // This also preserves exact-occurrence precedence when supplied facts conflict.
        self.candidates.iter().chain(&self.proof.baseline_conflicts).any(|id| {
            let term = &self.proof.graph.terms[id.0];
            let held = term.published.expect("implication candidates have an asserted alternative");

            term.condition.value.prove(&contrary, &mut { ExecutionCondition::WORK_LIMIT })
                == Some(!held)
        })
    }

    fn assert(&mut self, id: BooleanId, holds: bool) {
        let node = self.nodes.entry(id);

        node.asserted = Some(holds || node.asserted == Some(true));
        self.enqueue(id);
    }

    fn seed(&mut self, condition: &ExecutionCondition, held: bool) {
        if let Some(id) = self.proof.graph.find(condition) && !self.proof.graph.terms[id.0].condition.value.contains_unknown() {
            self.assert(id, held);
        }

        // Include comparison and variant dependencies when a copied hypothesis changes too.
        let proof = self.proof;

        for subject in related_subjects(condition) {
            if let Some(ids) = proof.related.get(&proof.graph.conditions.intern(subject)) {
                for id in ids { self.enqueue(*id); }
            }
        }
    }

    fn enqueue(&mut self, id: BooleanId) {
        let node = self.nodes.entry(id);

        if !node.queued {
            node.queued = true;
            self.queue.push_back(id);
        }
    }

    fn value(&self, id: BooleanId) -> Option<bool> {
        self.nodes.get(id).and_then(|node| node.value).unwrap_or(self.proof.baseline[id.0])
    }
}

pub(super) fn alternative_value(condition: &ExecutionCondition) -> Option<bool> {
    match condition {
        ExecutionCondition::Operation(BoundOperator::LogicalOr, _) => Some(true),
        ExecutionCondition::Operation(BoundOperator::LogicalAnd, _) => Some(false),
        ExecutionCondition::ValueAlias(_, _) => Some(true),
        _ => None,
    }
}

pub(super) fn alias_value(left: Option<bool>, right: impl FnOnce() -> Option<bool>) -> Option<bool> {
    let left = left?;

    (left != right()?).then_some(false)
}

pub(super) fn prove_boolean(operator: BoundOperator, left: Option<bool>, right: impl FnOnce() -> Option<bool>) -> Option<bool> {
    let decisive = match operator {
        BoundOperator::LogicalOr => true,
        BoundOperator::LogicalAnd => false,
        _ => unreachable!("Boolean proof requires conjunction or disjunction"),
    };

    match left {
        Some(value) if value == decisive => Some(value),
        Some(_) => right(),
        None => right().filter(|value| *value == decisive),
    }
}

fn related_subjects(condition: &ExecutionCondition) -> &[ExecutionCondition] {
    match condition {
        ExecutionCondition::Operation(BoundOperator::Equal | BoundOperator::NotEqual | BoundOperator::Less | BoundOperator::LessEqual | BoundOperator::Greater | BoundOperator::GreaterEqual, operands) => operands,
        ExecutionCondition::VariantTest(_, subject) => std::slice::from_ref(subject.as_ref()),
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use bray_bound_tree::BoundOperator::{LogicalAnd, LogicalNot, LogicalOr};
    use bray_symbols::SymbolOrdinal;

    use super::{BooleanGraph, AlternativeProof, AlternativeQuery};
    use crate::{ExecutionCondition, ExecutionPlace};

    #[test]
    fn irrelevant_clauses_do_not_multiply_query_work_or_lend_authority() {
        let input = |index| ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(index)));
        let not = |value| ExecutionCondition::operation(LogicalNot, vec![value]);
        let or = |left, right| ExecutionCondition::operation(LogicalOr, vec![left, right]);
        let claim = ExecutionCondition::Trusted(Arc::new(input(2)));
        let active = ExecutionCondition::operation(LogicalAnd, vec![input(0), input(1)]);
        let premise = or(not(active), claim.clone());
        let goal = or(not(input(0)), or(not(input(1)), claim.clone()));
        let empty = BTreeSet::new();
        let mut facts = BTreeSet::from([(premise, true)]);
        let small = AlternativeProof::new(BooleanGraph::new(&facts, &empty, Default::default()));

        let mut query = AlternativeQuery::new(&small);

        assert!(query.proves(&goal));

        let work = query.work;

        for index in 10..1010 {
            facts.insert((or(input(index * 2), input(index * 2 + 1)), true));
        }

        let large = AlternativeProof::new(BooleanGraph::new(&facts, &empty, Default::default()));

        for _ in 0..2 {
            let mut query = AlternativeQuery::new(&large);

            assert!(query.proves(&goal));
            assert_eq!(query.work, work);
            assert!(query.nodes.allocated() <= work, "query storage follows reached nodes, not the full graph");
            assert!(large.proves(&goal));
            assert!(!large.proves(&or(input(3000), claim.clone())), "unknown guards cannot release authority");
        }

        let parallel = AlternativeProof::new(BooleanGraph::new(&facts, &empty, Default::default()));

        std::thread::scope(|threads| {
            for (goal, expected) in [(goal.clone(), true), (or(input(3000), claim.clone()), false)] {
                let parallel = &parallel;

                threads.spawn(move || {
                    for _ in 0..4 { assert_eq!(parallel.proves(&goal), expected); }
                });
            }
        });

        let ordinary = AlternativeProof::new(BooleanGraph::new(&empty, &facts, Default::default()));

        assert!(!ordinary.proves(&goal), "ordinary occurrences cannot supply qualified evidence");
    }
}
