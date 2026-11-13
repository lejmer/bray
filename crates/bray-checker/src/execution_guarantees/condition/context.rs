use std::cell::{OnceCell, RefCell};
use std::collections::{BTreeMap, BTreeSet};

use super::ExecutionCondition;
use crate::execution_guarantees::boolean::DisjunctionProof;

pub(super) type ProofMemo = RefCell<BTreeMap<(ExecutionCondition, usize), (Option<bool>, usize)>>;

struct PreparedFacts {
    conditions: BTreeSet<(ExecutionCondition, bool)>,
    mixed: ProofMemo,
    disjunctions: RefCell<DisjunctionProof>,
}

/// Proof preparation belongs to these exact immutable evidence sets.
pub(crate) struct ExecutionProof<'a> {
    #[cfg(test)]
    pub(in crate::execution_guarantees) reference_scan: bool,
    trusted: &'a BTreeSet<(ExecutionCondition, bool)>,
    ordinary: &'a BTreeSet<(ExecutionCondition, bool)>,
    known: OnceCell<PreparedFacts>,
    raw: ProofMemo,
}

impl<'a> ExecutionProof<'a> {
    pub(crate) fn new(
        trusted: &'a BTreeSet<(ExecutionCondition, bool)>,
        ordinary: &'a BTreeSet<(ExecutionCondition, bool)>,
    ) -> Self {
        Self {
            #[cfg(test)]
            reference_scan: false,
            trusted,
            ordinary,
            known: OnceCell::new(),
            raw: RefCell::new(BTreeMap::new()),
        }
    }

    pub(crate) fn prove_trusted(&self, condition: &ExecutionCondition) -> Option<bool> {
        if let Some(value) = condition.explicit_value(self.trusted, None) {
            return Some(value);
        }
        if let Some(value) = condition.prove_with_memo(self.trusted, &mut { ExecutionCondition::WORK_LIMIT }, Some(&self.raw)) {
            return Some(value);
        }

        let known = self.known();

        condition.prove_mixed_with_implications(&known.conditions, self.ordinary, &mut { ExecutionCondition::WORK_LIMIT }, Some(self), Some(&known.mixed))
    }

    pub(crate) fn prove_mixed(&self, condition: &ExecutionCondition, budget: &mut usize) -> Option<bool> {
        if *budget == 0 { return None; }

        if let Some(value) = condition.explicit_value(self.trusted, Some(self.ordinary)) {
            *budget -= 1;

            return Some(value);
        }

        if condition.is_unknown() {
            *budget -= 1;

            return None;
        }
        let known = self.known();

        condition.prove_mixed_with_implications(&known.conditions, self.ordinary, budget, Some(self), Some(&known.mixed))
    }

    fn known(&self) -> &PreparedFacts {
        self.known.get_or_init(|| {
            // Preparation owns the expanded facts while all immutable operand storage stays shared.
            let (conditions, graph) = crate::execution_guarantees::trusted_implication::prepare_trusted_conditions(self.trusted.clone(), self.ordinary);

            PreparedFacts { conditions, mixed: RefCell::new(BTreeMap::new()), disjunctions: RefCell::new(DisjunctionProof::new(graph)) }
        })
    }

    pub(super) fn proves_disjunction(&self, consequence: &ExecutionCondition) -> bool {
        #[cfg(test)]
        if self.reference_scan {
            return self.known().conditions.iter().chain(self.ordinary.iter().filter(|(condition, _)| !condition.contains_trusted()))
                .any(|(premise, holds)| *holds && matches!(premise, ExecutionCondition::Operation(bray_bound_tree::BoundOperator::LogicalOr, _)) && Self::implies(premise, consequence));
        }

        self.known().disjunctions.borrow_mut().proves(consequence)
    }

    pub(super) fn implies(premise: &ExecutionCondition, consequence: &ExecutionCondition) -> bool {
        if premise.is_unknown() || consequence.is_unknown() {
            return false;
        }

        if premise == consequence {
            return !premise.contains_unknown();
        }

        let ordinary = BTreeSet::new();
        let mut evidence = BTreeSet::new();

        // This standalone comparison owns its premise facts, sharing their immutable operands.
        premise.clone().assume(true, &mut evidence);

        let known = crate::execution_guarantees::trusted_condition_closure(evidence, &ordinary);

        // Premise preparation cannot recursively search implications.
        if consequence.prove_mixed_with_implications(&known, &ordinary, &mut { ExecutionCondition::WORK_LIMIT }, None, None) == Some(true) {
            return true;
        }

        // Missing trusted evidence must not become an ordinary negative fact.
        let mut evidence = BTreeSet::new();

        // The hypothetical clause owns its fact keys, without copying operand trees.
        consequence.clone().assume_proof(false, false, &mut |condition, holds| {
            evidence.insert((condition, holds));
        });

        premise.prove(&evidence, &mut { ExecutionCondition::WORK_LIMIT }) == Some(false)
    }
}
