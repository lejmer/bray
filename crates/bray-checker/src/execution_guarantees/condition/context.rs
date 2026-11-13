use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, OnceLock};

use super::ExecutionCondition;
use super::memo::{ExecutionMemo, ConditionMemo, cached_condition, memoized};
use crate::execution_guarantees::boolean::AlternativeProof;

pub(super) type ProofMemo = ExecutionMemo<ExecutionCondition, Option<bool>>;

struct DerivedFacts {
    conditions: BTreeSet<(ExecutionCondition, bool)>,
    mixed: ProofMemo,
    alternatives: Mutex<AlternativeProof>,
    guards: OnceLock<BTreeMap<ExecutionCondition, ExecutionCondition>>,
    refined: Mutex<BTreeMap<ExecutionCondition, Arc<[ExecutionCondition]>>>,
}

impl DerivedFacts {
    fn new(trusted: &BTreeSet<(ExecutionCondition, bool)>, ordinary: &BTreeSet<(ExecutionCondition, bool)>) -> Self {
        // Preparation owns expanded facts, sharing immutable condition operands with the inputs.
        let (conditions, graph) = crate::execution_guarantees::trusted_implication::prepare_trusted_conditions(trusted.clone(), ordinary);

        Self { conditions, mixed: ProofMemo::default(), alternatives: Mutex::new(AlternativeProof::new(graph)), guards: OnceLock::new(), refined: Mutex::new(BTreeMap::new()) }
    }
}

struct PreparedFacts {
    trusted: BTreeSet<(ExecutionCondition, bool)>,
    ordinary: BTreeSet<(ExecutionCondition, bool)>,
    raw: ProofMemo,
    requested: ProofMemo,
    trusted_results: ConditionMemo<Option<bool>>,
    derived: OnceLock<DerivedFacts>,
}

impl PreparedFacts {
    fn new(trusted: &BTreeSet<(ExecutionCondition, bool)>, ordinary: &BTreeSet<(ExecutionCondition, bool)>) -> Self {
        // One exact snapshot owns all pure query memos and their lazy Boolean closure.
        Self { trusted: trusted.clone(), ordinary: ordinary.clone(), raw: ProofMemo::default(), requested: ProofMemo::default(), trusted_results: ConditionMemo::default(), derived: OnceLock::new() }
    }

    fn derived(&self) -> &DerivedFacts {
        self.derived.get_or_init(|| DerivedFacts::new(&self.trusted, &self.ordinary))
    }
}

/// Analysis-local preparation, reusable only for two exactly equal immutable evidence sets.
#[derive(Default)]
pub(crate) struct ExecutionProofCache {
    facts: Mutex<HashMap<u64, Vec<Arc<PreparedFacts>>>>,
}

impl std::fmt::Debug for ExecutionProofCache {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("ExecutionProofCache").finish_non_exhaustive()
    }
}

// Memoization is not part of semantic flow-state identity.
impl PartialEq for ExecutionProofCache {
    fn eq(&self, _: &Self) -> bool { true }
}

impl Eq for ExecutionProofCache {}

impl ExecutionProofCache {
    pub(crate) fn ordinary_proof<'a>(&'a self, ordinary: &'a BTreeSet<(ExecutionCondition, bool)>) -> ExecutionProof<'a> {
        static EMPTY: BTreeSet<(ExecutionCondition, bool)> = BTreeSet::new();

        self.proof(&EMPTY, ordinary)
    }

    pub(crate) fn proof<'a>(&'a self, trusted: &'a BTreeSet<(ExecutionCondition, bool)>, ordinary: &'a BTreeSet<(ExecutionCondition, bool)>) -> ExecutionProof<'a> {
        let mut proof = ExecutionProof::new(trusted, ordinary);
        proof.cache = Some(self);

        proof
    }

    pub(crate) fn closure(&self, trusted: &BTreeSet<(ExecutionCondition, bool)>, ordinary: &BTreeSet<(ExecutionCondition, bool)>) -> BTreeSet<(ExecutionCondition, bool)> {
        // Publication needs an owned set because its destination can mutate the source evidence.
        self.prepare(trusted, ordinary).derived().conditions.clone()
    }

    fn prepare(&self, trusted: &BTreeSet<(ExecutionCondition, bool)>, ordinary: &BTreeSet<(ExecutionCondition, bool)>) -> Arc<PreparedFacts> {
        let mut hash = std::hash::DefaultHasher::new();
        // Bucket selection needs only a cheap shape. Full immutable sets below
        // establish identity, without repeatedly hashing expanded operand trees.
        for facts in [trusted, ordinary] {
            facts.len().hash(&mut hash);

            for (condition, holds) in facts {
                std::mem::discriminant(condition).hash(&mut hash);
                holds.hash(&mut hash);
            }
        }
        let key = hash.finish();

        let mut facts = self.facts.lock().expect("analysis proof cache lock is not poisoned");
        let bucket = facts.entry(key).or_default();

        if let Some(cached) = bucket.iter().find(|cached| cached.trusted == *trusted && cached.ordinary == *ordinary) {
            return Arc::clone(cached);
        }

        let prepared = Arc::new(PreparedFacts::new(trusted, ordinary));

        // A hash only selects the bucket. Retained complete snapshots establish cache identity.
        bucket.push(Arc::clone(&prepared));

        prepared
    }
}

/// Proof preparation belongs to these exact immutable evidence sets.
pub(crate) struct ExecutionProof<'a> {
    #[cfg(test)]
    pub(in crate::execution_guarantees) reference_scan: bool,
    trusted: &'a BTreeSet<(ExecutionCondition, bool)>,
    ordinary: &'a BTreeSet<(ExecutionCondition, bool)>,
    cache: Option<&'a ExecutionProofCache>,
    prepared: OnceCell<Arc<PreparedFacts>>,
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
            cache: None,
            prepared: OnceCell::new(),
        }
    }

    pub(crate) fn prove_trusted(&self, condition: &ExecutionCondition) -> Option<bool> {
        if self.cache.is_none() || matches!(condition, ExecutionCondition::Boolean(_)) {
            return self.prove_trusted_inner(condition);
        }

        cached_condition(&self.facts().trusted_results, condition, || self.prove_trusted_inner(condition))
    }

    fn prove_trusted_inner(&self, condition: &ExecutionCondition) -> Option<bool> {
        if let Some(value) = condition.explicit_value(self.trusted, None) {
            return Some(value);
        }
        if let Some(value) = condition.prove_with_memo(self.trusted, &mut { ExecutionCondition::WORK_LIMIT }, Some(&self.facts().raw)) {
            return Some(value);
        }

        let known = self.known();

        condition.prove_mixed_with_implications(&known.conditions, self.ordinary, &mut { ExecutionCondition::WORK_LIMIT }, Some(self), Some(&known.mixed))
    }

    pub(crate) fn prove_mixed(&self, condition: &ExecutionCondition, budget: &mut usize) -> Option<bool> {
        if *budget == 0 { return None; }

        if self.cache.is_none() || matches!(condition, ExecutionCondition::Boolean(_)) {
            return self.prove_mixed_inner(condition, budget);
        }

        memoized(Some(&self.facts().requested), condition, budget, |budget| self.prove_mixed_inner(condition, budget))
    }

    fn prove_mixed_inner(&self, condition: &ExecutionCondition, budget: &mut usize) -> Option<bool> {

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

    /// Read guard-discharge consequences from this exact carrier's established clauses.
    pub(crate) fn refined_observations(&self, condition: &ExecutionCondition) -> Arc<[ExecutionCondition]> {
        let known = self.known();
        let mut refined = known.refined.lock().expect("analysis refinement memo lock is not poisoned");

        Arc::clone(refined.entry(condition.clone()).or_insert_with(|| {
            if !known.conditions.contains(&(condition.clone(), true)) { return Arc::from([]); }

            let ordinary = known.guards.get_or_init(|| known.conditions.iter().chain(self.ordinary.iter())
                .filter(|(condition, _)| !condition.contains_trusted())
                .map(|(condition, holds)| (condition.clone(), ExecutionCondition::Boolean(*holds)))
                .collect());
            let normalized = condition.with_guard_facts(ordinary);
            let normalized = (normalized != *condition && normalized.contains_trusted()).then_some(normalized);

            normalized.iter().chain(condition.observations().filter(|observed| {
                (!observed.contains_trusted() || matches!(observed, ExecutionCondition::Trusted(_)))
                    && Some(*observed) != normalized.as_ref()
                    && known.conditions.contains(&((*observed).clone(), true))
            })).cloned().collect::<Vec<_>>().into()
        }))
    }

    fn known(&self) -> &DerivedFacts { self.facts().derived() }

    fn facts(&self) -> &PreparedFacts {
        self.prepared.get_or_init(|| {
            if let Some(cache) = self.cache { return cache.prepare(self.trusted, self.ordinary); }

            Arc::new(PreparedFacts::new(self.trusted, self.ordinary))
        })
    }

    pub(super) fn proves_alternative(&self, consequence: &ExecutionCondition) -> bool {
        #[cfg(test)]
        if self.reference_scan {
            return self.known().conditions.iter().chain(self.ordinary.iter().filter(|(condition, _)| !condition.contains_trusted()))
                .any(|(premise, holds)| *holds && matches!(premise, ExecutionCondition::Operation(bray_bound_tree::BoundOperator::LogicalOr, _)) && Self::implies(premise, consequence));
        }

        self.known().alternatives.lock().expect("analysis alternative proof lock is not poisoned").proves(consequence)
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use bray_bound_tree::BoundOperator::{Less, LogicalAnd, LogicalNot, LogicalOr};
    use bray_symbols::SymbolOrdinal;

    use super::{ExecutionCondition, ExecutionProofCache};
    use crate::ExecutionPlace;

    #[test]
    fn analysis_preparation_reuses_exact_inputs_without_reusing_changed_authority() {
        let input = |index| ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(index)));
        let claim = ExecutionCondition::Trusted(Arc::new(input(1)));
        let guard = input(0);
        let clause = ExecutionCondition::operation(LogicalOr, vec![
            ExecutionCondition::operation(LogicalNot, vec![guard.clone()]), claim.clone(),
        ]);
        let facts = BTreeSet::from([(clause.clone(), true)]);
        let ordinary = BTreeSet::from([
            (guard.clone(), true),
            (ExecutionCondition::operation(Less, vec![input(11), input(12)]), true),
            (ExecutionCondition::operation(Less, vec![input(12), input(13)]), true),
        ]);
        let cache = ExecutionProofCache::default();
        let proof = cache.proof(&facts, &ordinary);

        assert_eq!(proof.prove_trusted(&claim), Some(true));
        let copied = facts.clone();
        let prepared = proof.prepared.get().expect("guarded claim prepares its evidence");
        let same = cache.proof(&copied, &ordinary);
        assert_eq!(same.prove_trusted(&claim), Some(true));
        assert!(Arc::ptr_eq(prepared, same.prepared.get().expect("same inputs find preparation")));
        let observations = proof.refined_observations(&clause);
        assert!(observations.contains(&claim));
        assert!(Arc::ptr_eq(&observations, &same.refined_observations(&clause)));

        for incoming in [0, 1, 2, 8, ExecutionCondition::WORK_LIMIT, 1] {
            let mut actual_budget = incoming;
            let mut expected_budget = incoming;
            let expected = super::ExecutionProof::new(&facts, &ordinary).prove_mixed(&claim, &mut expected_budget);

            assert_eq!(cache.proof(&facts, &ordinary).prove_mixed(&claim, &mut actual_budget), expected);
            assert_eq!(actual_budget, expected_budget, "cache hits retain the exact work budget");
        }

        let alternative = ExecutionCondition::operation(LogicalOr, vec![
            ExecutionCondition::Trusted(Arc::new(input(7))), claim.clone(),
        ]);
        let combined = ExecutionCondition::operation(LogicalAnd, vec![guard.clone(), alternative]);
        let reference = super::ExecutionProof::new(&facts, &ordinary);

        for goal in [combined.clone(), ExecutionCondition::operation(LogicalNot, vec![combined]),
            ExecutionCondition::operation(Less, vec![input(11), input(13)])] {
            for incoming in (0..=ExecutionCondition::WORK_LIMIT).chain([2, 1, ExecutionCondition::WORK_LIMIT]) {
                let mut actual_budget = incoming;
                let mut expected_budget = incoming;
                let expected = goal.prove_mixed_with_implications(
                    &reference.known().conditions, &ordinary, &mut expected_budget, Some(&reference), None);

                assert_eq!(cache.proof(&facts, &ordinary).prove_mixed(&goal, &mut actual_budget), expected,
                    "cached and uncached inference differ at budget{incoming}, goal={goal:?}");
                assert_eq!(actual_budget, expected_budget, "semantic work cost differs at budget{incoming}");
            }
        }

        std::thread::scope(|threads| {
            for active in [false, true] {
                let cache = &cache;
                let facts = &facts;
                let claim = &claim;
                let guard = guard.clone();
                let clause = &clause;
                threads.spawn(move || {
                    let ordinary = BTreeSet::from([(guard, active)]);
                    let proof = cache.proof(facts, &ordinary);
                    assert_eq!(proof.prove_trusted(claim) == Some(true), active);
                    assert_eq!(proof.refined_observations(clause).contains(claim), active);
                });
            }
        });

        let empty = BTreeSet::new();
        assert_ne!(cache.proof(&empty, &ordinary).prove_trusted(&claim), Some(true), "removing qualified evidence revokes authority");
        assert_ne!(cache.proof(&ordinary, &facts).prove_trusted(&claim), Some(true), "swapping evidence roles cannot qualify an ordinary occurrence");
        assert_ne!(cache.proof(&facts, &ordinary).prove_trusted(&ExecutionCondition::Trusted(Arc::new(input(2)))), Some(true), "another owner has no matching evidence");

        let other_claim = ExecutionCondition::Trusted(Arc::new(input(2)));
        let other_clause = ExecutionCondition::operation(LogicalOr, vec![
            ExecutionCondition::operation(LogicalNot, vec![guard.clone()]), other_claim.clone(),
        ]);
        let colliding = BTreeSet::from([(other_clause, true)]);
        let other_owner = cache.proof(&colliding, &ordinary);

        assert_eq!(other_owner.prove_trusted(&other_claim), Some(true));
        assert_ne!(other_owner.prove_trusted(&claim), Some(true), "identical bucket shapes cannot merge owner evidence");
        assert!(!Arc::ptr_eq(prepared, other_owner.prepared.get().expect("another owner prepares distinct evidence")));

        let independent = ExecutionProofCache::default();
        let other = independent.proof(&facts, &ordinary);
        assert_eq!(other.prove_trusted(&claim), Some(true));
        assert!(!Arc::ptr_eq(prepared, other.prepared.get().expect("independent analysis prepares its own evidence")));
    }
}
