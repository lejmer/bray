use std::collections::{BTreeSet, HashMap, VecDeque};

use bray_bound_tree::BoundOperator;

use super::{ExecutionCondition, ExecutionObservationKind};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct BooleanId(pub(super) usize);

#[derive(Eq, Hash, PartialEq)]
enum BooleanKey {
    Operation(BoundOperator, Vec<BooleanId>),
    Entry(bool, BooleanId),
    Observation(ExecutionObservationKind, BooleanId, BooleanId),
    Borrowed(BooleanId),
    Trusted(BooleanId),
    Atom(ExecutionCondition),
}

impl BooleanKey {
    fn new(condition: &ExecutionCondition, mut child: impl FnMut(&ExecutionCondition) -> Option<BooleanId>) -> Option<Self> {
        Some(match condition {
            ExecutionCondition::Operation(operator @ (BoundOperator::LogicalNot | BoundOperator::LogicalAnd | BoundOperator::LogicalOr), operands) =>
                Self::Operation(*operator, operands.iter().map(&mut child).collect::<Option<_>>()?),
            ExecutionCondition::Entry { captured, condition } => Self::Entry(*captured, child(condition)?),
            ExecutionCondition::Observation { owner, contents, kind } => Self::Observation(*kind, child(owner)?, child(contents)?),
            ExecutionCondition::Borrowed(value) => Self::Borrowed(child(value)?),
            ExecutionCondition::Trusted(value) => Self::Trusted(child(value)?),
            // Only opaque atoms retain structural keys. Boolean keys hash child identities once.
            atom => Self::Atom(atom.clone()),
        })
    }

    fn children(&self) -> Vec<BooleanId> {
        match self {
            Self::Operation(_, children) => children.to_vec(),
            Self::Entry(_, child) | Self::Borrowed(child) | Self::Observation(_, _, child) => vec![*child],
            Self::Trusted(_) | Self::Atom(_) => Vec::new(),
        }
    }
}

pub(super) struct BooleanTerm {
    pub(super) condition: ExecutionCondition,
    pub(super) children: Vec<BooleanId>,
    pub(super) parents: Vec<BooleanId>,
    pub(super) published: bool,
}

/// Shared Boolean structure, with qualified occurrences kept as independent atoms.
pub(super) struct BooleanGraph {
    pub(super) terms: Vec<BooleanTerm>,
    indices: HashMap<BooleanKey, BooleanId>,
}

impl BooleanGraph {
    pub(super) fn new(
        trusted: &BTreeSet<(ExecutionCondition, bool)>,
        ordinary: &BTreeSet<(ExecutionCondition, bool)>,
    ) -> Self {
        let mut graph = Self { terms: Vec::new(), indices: HashMap::new() };

        for (condition, holds) in trusted.iter().chain(ordinary.iter().filter(|(condition, _)| !condition.contains_trusted())) {
            if *holds && matches!(condition, ExecutionCondition::Operation(BoundOperator::LogicalOr, _)) {
                let id = graph.intern(condition);

                graph.terms[id.0].published = true;
            }
        }

        graph
    }

    fn intern(&mut self, condition: &ExecutionCondition) -> BooleanId {
        let key = BooleanKey::new(condition, |child| Some(self.intern(child)))
            .expect("interned Boolean children have identities");

        if let Some(id) = self.indices.get(&key) {
            return *id;
        }

        let children = key.children();

        let id = BooleanId(self.terms.len());

        for child in &children {
            self.terms[child.0].parents.push(id);
        }

        // Graph keys and nodes retain immutable operands independently of the borrowed evidence view.
        self.indices.insert(key, id);
        self.terms.push(BooleanTerm { condition: condition.clone(), children, parents: Vec::new(), published: false });

        id
    }

    pub(super) fn find(&self, condition: &ExecutionCondition) -> Option<BooleanId> {
        let key = BooleanKey::new(condition, |child| self.find(child))?;

        self.indices.get(&key).copied()
    }

    pub(super) fn derived(&self, id: BooleanId, value: impl Fn(BooleanId) -> Option<bool>) -> Option<bool> {
        let term = &self.terms[id.0];

        match (&term.condition, term.children.as_slice()) {
            (ExecutionCondition::Boolean(value), _) => Some(*value),
            (ExecutionCondition::Operation(BoundOperator::LogicalNot, _), [child]) => value(*child).map(|value| !value),
            (ExecutionCondition::Entry { .. } | ExecutionCondition::Observation { .. } | ExecutionCondition::Borrowed(_), [child]) => value(*child),
            (ExecutionCondition::Operation(operator @ (BoundOperator::LogicalAnd | BoundOperator::LogicalOr), _), [left, right]) =>
                prove_boolean(*operator, value(*left), || value(*right)),
            _ => None,
        }
    }
}

/// Find published clauses falsified by a goal's contrary without testing every premise against it.
pub(super) struct DisjunctionProof {
    graph: BooleanGraph,
    related: HashMap<ExecutionCondition, Vec<BooleanId>>,
    baseline: Vec<Option<bool>>,
    values: Vec<(usize, Option<bool>)>,
    asserted: Vec<(usize, Option<bool>)>,
    queued: Vec<usize>,
    epoch: usize,
    queue: VecDeque<BooleanId>,
    candidates: Vec<BooleanId>,
    always_false: Vec<BooleanId>,
    #[cfg(test)]
    pub(super) work: usize,
}

impl DisjunctionProof {
    pub(super) fn new(graph: BooleanGraph) -> Self {
        let mut related = HashMap::<_, Vec<_>>::new();
        let mut baseline = Vec::with_capacity(graph.terms.len());
        let mut always_false = Vec::new();

        // Children precede parents. Intrinsic values need preparation only once per view.
        for (index, term) in graph.terms.iter().enumerate() {
            for subject in related_subjects(&term.condition) {
                // The index retains the immutable subject alongside the graph that uses it.
                related.entry(subject.clone()).or_default().push(BooleanId(index));
            }

            // Without assumptions, only Boolean structure has a truth value. Opaque
            // atoms cannot gain evidence by recursively reproving their empty context.
            let value = graph.derived(BooleanId(index), |child| baseline[child.0]);

            baseline.push(value);
            if term.published && value == Some(false) {
                always_false.push(BooleanId(index));
            }
        }

        let size = graph.terms.len();

        Self {
            graph, related, baseline, always_false,
            values: vec![(0, None); size], asserted: vec![(0, None); size], queued: vec![0; size],
            epoch: 0, queue: VecDeque::new(), candidates: Vec::new(),
            #[cfg(test)]
            work: 0,
        }
    }

    pub(super) fn proves(&mut self, consequence: &ExecutionCondition) -> bool {
        self.epoch = self.epoch.checked_add(1).expect("Boolean proof query epoch fits usize");
        self.queue.clear();
        self.candidates.clear();
        #[cfg(test)]
        { self.work = 0; }

        let mut contrary = BTreeSet::new();

        // Negating a qualified occurrence never establishes an ordinary negative predicate.
        consequence.clone().assume_proof(false, false, &mut |condition, holds| {
            contrary.insert((condition, holds));
        });

        for (condition, holds) in &contrary {
            if !condition.contains_unknown() && let Some(id) = self.graph.find(condition) {
                self.assert(id, *holds);
            }

            // Comparisons can depend on transitive ordering facts. Their raw proof receives
            // the complete contrary, including every connecting edge and NaN restriction.
            for subject in related_subjects(condition) {
                if let Some(ids) = self.related.get(subject) {
                    for id in ids {
                        if self.queued[id.0] != self.epoch {
                            self.queued[id.0] = self.epoch;
                            self.queue.push_back(*id);
                        }
                    }
                }
            }
        }

        while let Some(id) = self.queue.pop_front() {
            self.queued[id.0] = 0;
            #[cfg(test)]
            { self.work += 1; }

            let term = &self.graph.terms[id.0];
            let value = if let ExecutionCondition::Boolean(value) = &term.condition {
                Some(*value)
            } else if self.asserted[id.0].0 == self.epoch {
                self.asserted[id.0].1
            } else {
                self.graph.derived(id, |child| self.value(child)).or_else(|| {
                    term.children.is_empty().then(|| term.condition.prove(&contrary, &mut { ExecutionCondition::WORK_LIMIT })).flatten()
                })
            };

            if self.value(id) != value {
                self.values[id.0] = (self.epoch, value);

                for parent in &term.parents {
                    if self.queued[parent.0] != self.epoch {
                        self.queued[parent.0] = self.epoch;
                        self.queue.push_back(*parent);
                    }
                }
            }

            if term.published && value == Some(false) {
                self.candidates.push(id);
            }
        }

        // The graph selects candidates. The owning bounded rule still decides the implication.
        // This also preserves exact-occurrence precedence when supplied facts conflict.
        self.candidates.iter().chain(&self.always_false).any(|id| {
            self.graph.terms[id.0].condition.prove(&contrary, &mut { ExecutionCondition::WORK_LIMIT }) == Some(false)
        })
    }

    fn assert(&mut self, id: BooleanId, holds: bool) {
        let prior = &mut self.asserted[id.0];

        *prior = (self.epoch, Some(holds || prior.0 == self.epoch && prior.1 == Some(true)));

        if self.queued[id.0] != self.epoch {
            self.queued[id.0] = self.epoch;
            self.queue.push_back(id);
        }
    }

    fn value(&self, id: BooleanId) -> Option<bool> {
        let (epoch, value) = self.values[id.0];

        if epoch == self.epoch { value } else { self.baseline[id.0] }
    }
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

    use super::{BooleanGraph, DisjunctionProof};
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
        let mut small = DisjunctionProof::new(BooleanGraph::new(&facts, &empty));

        assert!(small.proves(&goal));

        let work = small.work;

        for index in 10..1010 {
            facts.insert((or(input(index * 2), input(index * 2 + 1)), true));
        }

        let mut large = DisjunctionProof::new(BooleanGraph::new(&facts, &empty));

        for _ in 0..2 {
            assert!(large.proves(&goal));
            assert_eq!(large.work, work);
            assert!(!large.proves(&or(input(3000), claim.clone())), "unknown guards cannot release authority");
        }

        let mut ordinary = DisjunctionProof::new(BooleanGraph::new(&empty, &facts));

        assert!(!ordinary.proves(&goal), "ordinary occurrences cannot supply qualified evidence");
    }
}
