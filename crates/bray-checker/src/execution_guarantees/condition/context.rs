use std::cell::{OnceCell, RefCell};
use std::collections::{BTreeMap, BTreeSet};

use super::ExecutionCondition;

/// Proof preparation belongs to these exact immutable evidence sets.
pub(crate) struct ExecutionProof<'a> {
    trusted: &'a BTreeSet<(ExecutionCondition, bool)>,
    ordinary: &'a BTreeSet<(ExecutionCondition, bool)>,
    known: OnceCell<BTreeSet<(ExecutionCondition, bool)>>,
    premises: RefCell<BTreeMap<ExecutionCondition, BTreeSet<(ExecutionCondition, bool)>>>,
    contraries: RefCell<BTreeMap<ExecutionCondition, BTreeSet<(ExecutionCondition, bool)>>>,
    conclusions: RefCell<BTreeMap<ExecutionCondition, Option<bool>>>,
}

impl<'a> ExecutionProof<'a> {
    pub(crate) fn new(
        trusted: &'a BTreeSet<(ExecutionCondition, bool)>,
        ordinary: &'a BTreeSet<(ExecutionCondition, bool)>,
    ) -> Self {
        Self { trusted, ordinary, known: OnceCell::new(), premises: RefCell::new(BTreeMap::new()),
            contraries: RefCell::new(BTreeMap::new()), conclusions: RefCell::new(BTreeMap::new()) }
    }

    pub(crate) fn prove_trusted(&self, condition: &ExecutionCondition) -> Option<bool> {
        if let Some(value) = self.conclusions.borrow().get(condition) {
            return *value;
        }

        let value = condition.prove(self.trusted, &mut { ExecutionCondition::WORK_LIMIT }).or_else(|| {
            let known = self.known.get_or_init(|| {
                crate::execution_guarantees::trusted_condition_closure(self.trusted.clone(), self.ordinary)
            });

            condition.prove_mixed_with_implications(known, self.ordinary, &mut { ExecutionCondition::WORK_LIMIT }, Some(self))
        });

        self.conclusions.borrow_mut().insert(condition.clone(), value);

        value
    }

    pub(crate) fn prove_mixed(&self, condition: &ExecutionCondition, budget: &mut usize) -> Option<bool> {
        condition.prove_mixed_with_implications(self.trusted, self.ordinary, budget, Some(self))
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

            crate::execution_guarantees::trusted_condition_closure(evidence, &ordinary)
        });

        // Premise preparation cannot recursively search implications.
        if consequence.prove_mixed_with_implications(known, &ordinary, &mut { ExecutionCondition::WORK_LIMIT }, None) == Some(true) {
            return true;
        }

        // Missing trusted evidence must not become an ordinary negative fact.
        let mut contraries = self.contraries.borrow_mut();

        let evidence = contraries.entry(consequence.clone()).or_insert_with(|| {
            let mut evidence = BTreeSet::new();

            consequence.clone().assume_proof(false, false, &mut |condition, holds| {
                evidence.insert((condition, holds));
            });

            evidence
        });

        premise.prove(evidence, &mut { ExecutionCondition::WORK_LIMIT }) == Some(false)
    }
}
