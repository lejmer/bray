use std::cell::{OnceCell, RefCell};
use std::collections::{BTreeMap, BTreeSet};

use super::ExecutionCondition;

pub(super) type ProofMemo = RefCell<BTreeMap<(ExecutionCondition, usize), (Option<bool>, usize)>>;

#[derive(Default)]
struct PreparedFacts {
    conditions: BTreeSet<(ExecutionCondition, bool)>,
    raw: ProofMemo,
    mixed: ProofMemo,
}

impl PreparedFacts {
    fn new(conditions: BTreeSet<(ExecutionCondition, bool)>) -> Self {
        Self { conditions, ..Self::default() }
    }
}

/// Proof preparation belongs to these exact immutable evidence sets.
pub(crate) struct ExecutionProof<'a> {
    trusted: &'a BTreeSet<(ExecutionCondition, bool)>,
    ordinary: &'a BTreeSet<(ExecutionCondition, bool)>,
    known: OnceCell<PreparedFacts>,
    premises: RefCell<BTreeMap<ExecutionCondition, PreparedFacts>>,
    contraries: RefCell<BTreeMap<ExecutionCondition, PreparedFacts>>,
    raw: ProofMemo,
    mixed: ProofMemo,
}

impl<'a> ExecutionProof<'a> {
    pub(crate) fn new(
        trusted: &'a BTreeSet<(ExecutionCondition, bool)>,
        ordinary: &'a BTreeSet<(ExecutionCondition, bool)>,
    ) -> Self {
        Self {
            trusted,
            ordinary,
            known: OnceCell::new(),
            premises: RefCell::new(BTreeMap::new()),
            contraries: RefCell::new(BTreeMap::new()),
            raw: RefCell::new(BTreeMap::new()),
            mixed: RefCell::new(BTreeMap::new()),
        }
    }

    pub(crate) fn prove_trusted(&self, condition: &ExecutionCondition) -> Option<bool> {
        if let Some(value) = condition.prove_with_memo(self.trusted, &mut { ExecutionCondition::WORK_LIMIT }, Some(&self.raw)) {
            return Some(value);
        }

        let known = self.known.get_or_init(|| {
            PreparedFacts::new(crate::execution_guarantees::trusted_condition_closure(self.trusted.clone(), self.ordinary))
        });

        condition.prove_mixed_with_implications(&known.conditions, self.ordinary, &mut { ExecutionCondition::WORK_LIMIT }, Some(self), Some(&known.mixed))
    }

    pub(crate) fn prove_mixed(&self, condition: &ExecutionCondition, budget: &mut usize) -> Option<bool> {
        condition.prove_mixed_with_implications(self.trusted, self.ordinary, budget, Some(self), Some(&self.mixed))
    }

    pub(super) fn implies(&self, premise: &ExecutionCondition, consequence: &ExecutionCondition) -> bool {
        if premise.is_unknown() || consequence.is_unknown() {
            return false;
        }

        if premise == consequence {
            return !premise.contains_unknown();
        }

        let ordinary = BTreeSet::new();
        let mut premises = self.premises.borrow_mut();

        let known = premises.entry(premise.clone()).or_insert_with(|| {
            let mut evidence = BTreeSet::new();

            premise.clone().assume(true, &mut evidence);

            PreparedFacts::new(crate::execution_guarantees::trusted_condition_closure(evidence, &ordinary))
        });

        // Premise preparation cannot recursively search implications.
        if consequence.prove_mixed_with_implications(&known.conditions, &ordinary, &mut { ExecutionCondition::WORK_LIMIT }, None, Some(&known.mixed)) == Some(true) {
            return true;
        }

        // Missing trusted evidence must not become an ordinary negative fact.
        let mut contraries = self.contraries.borrow_mut();

        let evidence = contraries.entry(consequence.clone()).or_insert_with(|| {
            let mut evidence = BTreeSet::new();

            consequence.clone().assume_proof(false, false, &mut |condition, holds| {
                evidence.insert((condition, holds));
            });

            PreparedFacts::new(evidence)
        });

        premise.prove_with_memo(&evidence.conditions, &mut { ExecutionCondition::WORK_LIMIT }, Some(&evidence.raw)) == Some(false)
    }
}
