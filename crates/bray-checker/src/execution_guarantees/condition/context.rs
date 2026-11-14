use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex, OnceLock};

use super::ExecutionCondition;
use super::substitution::GuardSubstitution;
use super::interning::{ConditionId, ConditionIndex, ConditionMap};
use super::memo::{ExecutionMemo, ConditionMemo, cached_condition, memoized};
use crate::execution_guarantees::boolean::AlternativeProof;
use crate::execution_guarantees::{ExecutionAssumptions, ExecutionWitness};
use crate::execution_guarantees::witness::WitnessConditions;

pub(in crate::execution_guarantees) struct ProofMemo {
    pub(super) queries: ExecutionMemo<Option<bool>>,
    index: ConditionIndex,
    explicit: OnceLock<ConditionMap<bool>>,
    single: OnceLock<[Arc<ProofMemo>; 2]>,
    ordered: OnceLock<crate::execution_guarantees::ordered_facts::OrderedFacts>,
}

impl ProofMemo {
    pub(in crate::execution_guarantees) fn new(index: &ConditionIndex) -> Self {
        Self { queries: ExecutionMemo::new(index), index: index.clone(), explicit: OnceLock::new(), single: OnceLock::new(), ordered: OnceLock::new() }
    }

    pub(in crate::execution_guarantees) fn ordered(&self, facts: &BTreeSet<(ExecutionCondition, bool)>) -> &crate::execution_guarantees::ordered_facts::OrderedFacts {
        self.ordered.get_or_init(|| crate::execution_guarantees::ordered_facts::OrderedFacts::new(facts, &self.index))
    }

    /// Raw queries share only exactly equal input sets, independently of mixed proof roles.
    pub(in crate::execution_guarantees) fn single_views(&self) -> &[Arc<Self>; 2] {
        self.single.get_or_init(|| std::array::from_fn(|_| Arc::new(Self::new(&self.index))))
    }

    fn with_single(index: &ConditionIndex, single: [Arc<Self>; 2]) -> Self {
        Self { single: OnceLock::from(single), ..Self::new(index) }
    }

    pub(super) fn explicit_value(&self, condition: &ExecutionCondition, trusted: &BTreeSet<(ExecutionCondition, bool)>, ordinary: Option<&BTreeSet<(ExecutionCondition, bool)>>) -> Option<bool> {
        let facts = self.explicit.get_or_init(|| {
            let mut facts = self.index.map();

            for (condition, holds) in trusted.iter().chain(ordinary.into_iter().flatten().filter(|(condition, _)| !condition.contains_trusted())) {
                if condition.contains_unknown() { continue; }
                *facts.get_or_insert_with(condition, || false) |= *holds;
            }

            facts
        });

        facts.get(condition).copied()
    }
}

struct DerivedFacts {
    conditions: BTreeSet<(ExecutionCondition, bool)>,
    mixed: ProofMemo,
    alternatives: AlternativeProof,
    guards: OnceLock<GuardSubstitution>,
    positive: OnceLock<ConditionMap<()>>,
    refined: ConditionMemo<Arc<[ExecutionCondition]>>,
}

impl DerivedFacts {
    fn new(trusted: &BTreeSet<(ExecutionCondition, bool)>, ordinary: &BTreeSet<(ExecutionCondition, bool)>, index: &ConditionIndex, single: &[Arc<ProofMemo>; 2]) -> Self {
        // Preparation owns expanded facts, sharing immutable condition operands with the inputs.
        let (conditions, graph) = crate::execution_guarantees::trusted_implication::prepare_trusted_conditions(trusted.clone(), ordinary, index.clone(), single);

        // Closure changes the trusted premises. Its ordinary input remains exactly the same.
        let mixed = ProofMemo::with_single(index, [Arc::new(ProofMemo::new(index)), Arc::clone(&single[1])]);

        Self { conditions, mixed, alternatives: AlternativeProof::new(graph), guards: OnceLock::new(), positive: OnceLock::new(), refined: Mutex::new(index.map()) }
    }

    fn establishes(&self, condition: &ExecutionCondition) -> bool {
        self.positive.get_or_init(|| {
            let mut positive = self.mixed.index.map();

            for (condition, holds) in &self.conditions {
                if *holds { positive.insert(condition, ()); }
            }

            positive
        }).get(condition).is_some()
    }

}

struct PreparedFacts {
    witnesses: Mutex<HashMap<usize, (Arc<ExecutionWitness>, Arc<WitnessConditions>)>>,
    index: ConditionIndex,
    trusted: ExecutionAssumptions,
    ordinary: ExecutionAssumptions,
    direct: ProofMemo,
    equalities: Mutex<HashMap<Option<bray_bound_tree::BoundExpressionId>, Arc<BTreeMap<ExecutionCondition, ExecutionCondition>>>>,
    requested: ExecutionMemo<Option<bool>>,
    trusted_results: ConditionMemo<Option<bool>>,
    derived: OnceLock<DerivedFacts>,
}

impl PreparedFacts {
    fn new(trusted: ExecutionAssumptions, ordinary: ExecutionAssumptions, index: &ConditionIndex, single: [Arc<ProofMemo>; 2]) -> Self {
        // One exact snapshot owns all pure query memos and their lazy Boolean closure.
        Self { witnesses: Mutex::default(), index: index.clone(), trusted, ordinary, direct: ProofMemo::with_single(index, single), equalities: Mutex::default(), requested: ExecutionMemo::new(index), trusted_results: Mutex::new(index.map()), derived: OnceLock::new() }
    }

    fn derived(&self) -> &DerivedFacts {
        self.derived.get_or_init(|| DerivedFacts::new(&self.trusted, &self.ordinary, &self.index, self.direct.single_views()))
    }
}

/// Analysis-local preparation, reusable only for two exactly equal immutable evidence sets.
#[derive(Default)]
pub(crate) struct ExecutionProofCache {
    pub(crate) conditions: ConditionIndex,
    facts: Mutex<HashMap<[usize; 2], Arc<PreparedFacts>>>,
    inputs: Mutex<HashMap<Vec<(ConditionId, bool)>, Arc<ProofMemo>>>,
    aliases: Mutex<HashMap<usize, (ExecutionAssumptions, Arc<ProofMemo>)>>,
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
    pub(crate) fn memoize_condition<T: Clone, Q: Fn(&ExecutionCondition) -> T>(&self, query: Q) -> impl Fn(&ExecutionCondition) -> T + use<T, Q> {
        super::memo::memoize_condition(self.conditions.map(), query)
    }

    pub(crate) fn ordinary_proof<'a>(&'a self, ordinary: &'a ExecutionAssumptions) -> ExecutionProof<'a> {
        static EMPTY: std::sync::LazyLock<ExecutionAssumptions> = std::sync::LazyLock::new(ExecutionAssumptions::default);

        self.proof(&EMPTY, ordinary)
    }

    pub(crate) fn proof<'a>(&'a self, trusted: &'a ExecutionAssumptions, ordinary: &'a ExecutionAssumptions) -> ExecutionProof<'a> {
        let mut proof = ExecutionProof::new(trusted, ordinary);

        proof.cache = Some((self, trusted, ordinary));

        proof
    }

    /// Equality bindings belong to this exact ordinary snapshot and expiry preference.
    pub(crate) fn equalities(
        &self,
        ordinary: &ExecutionAssumptions,
        expiring: Option<bray_bound_tree::BoundExpressionId>,
    ) -> Arc<BTreeMap<ExecutionCondition, ExecutionCondition>> {
        let proof = self.ordinary_proof(ordinary);
        let facts = proof.facts();

        if let Some(result) = facts.equalities.lock().expect("equality cache lock is not poisoned").get(&expiring) {
            return Arc::clone(result);
        }

        let result = Arc::new(ExecutionCondition::equalities_with(ordinary, expiring,
            || std::borrow::Cow::Borrowed(&proof.known().conditions)));

        facts.equalities.lock().expect("equality cache lock is not poisoned").insert(expiring, Arc::clone(&result));

        result
    }

    pub(crate) fn refined_witness(
        &self,
        witness: &Arc<ExecutionWitness>,
        trusted: &ExecutionAssumptions,
        ordinary: &ExecutionAssumptions,
    ) -> Arc<WitnessConditions> {
        let proof = self.proof(trusted, ordinary);
        let facts = proof.facts();
        let key = Arc::as_ptr(witness).addr();

        if let Some((_, result)) = facts.witnesses.lock().expect("witness refinement cache lock is not poisoned").get(&key) {
            return Arc::clone(result);
        }

        let result = Arc::new(witness.derived_conditions(&proof, ordinary, self));

        // Retaining the immutable source prevents address reuse and forces later changes onto another allocation.
        facts.witnesses.lock().expect("witness refinement cache lock is not poisoned")
            .insert(key, (Arc::clone(witness), Arc::clone(&result)));

        result
    }

    fn prepare_input(&self, input: &ExecutionAssumptions) -> Arc<ProofMemo> {
        let storage = input.storage_key();

        if let Some((_, prepared)) = self.aliases.lock().expect("proof input aliases lock is not poisoned").get(&storage) {
            return Arc::clone(prepared);
        }

        let key = input.iter().map(|(condition, holds)| (self.conditions.intern(condition), *holds)).collect();

        let prepared = Arc::clone(self.inputs.lock().expect("proof input index lock is not poisoned")
            .entry(key).or_insert_with(|| Arc::new(ProofMemo::new(&self.conditions))));

        // Retain every physical alias, including equal independently allocated inputs.
        self.aliases.lock().expect("proof input aliases lock is not poisoned")
            .insert(storage, (input.clone(), Arc::clone(&prepared)));

        prepared
    }

    fn prepare(&self, trusted: &ExecutionAssumptions, ordinary: &ExecutionAssumptions) -> Arc<PreparedFacts> {
        let single = [trusted, ordinary].map(|input| self.prepare_input(input));
        let key = single.each_ref().map(|input| Arc::as_ptr(input).addr());

        // Canonical raw inputs carry no mixed result. Their ordered pair fixes both evidence roles.
        Arc::clone(self.facts.lock().expect("analysis proof cache lock is not poisoned")
            .entry(key).or_insert_with(|| Arc::new(PreparedFacts::new(
                trusted.clone(), ordinary.clone(), &self.conditions, single))))
    }

}

/// Proof preparation belongs to these exact immutable evidence sets.
pub(crate) struct ExecutionProof<'a> {
    #[cfg(test)]
    pub(in crate::execution_guarantees) reference_scan: bool,
    trusted: &'a BTreeSet<(ExecutionCondition, bool)>,
    ordinary: &'a BTreeSet<(ExecutionCondition, bool)>,
    cache: Option<(&'a ExecutionProofCache, &'a ExecutionAssumptions, &'a ExecutionAssumptions)>,
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

    /// Read ordinary consequences without publishing them as new flow premises.
    pub(crate) fn ordinary_facts(&self) -> impl Iterator<Item = &(ExecutionCondition, bool)> {
        self.ordinary.union(&self.known().conditions)
            .filter(|(condition, _)| !condition.contains_trusted())
    }

    pub(crate) fn can_assume(&self, condition: &ExecutionCondition, value: bool) -> bool {
        if condition.is_unknown() { return true; }

        if let Some(known) = self.prove_mixed(condition, &mut { ExecutionCondition::WORK_LIMIT }) {
            return known == value;
        }

        let contrary = if value {
            ExecutionCondition::operation(bray_bound_tree::BoundOperator::LogicalNot, vec![condition.clone()])
        } else { condition.clone() };

        // Atomic feasibility also needs the indexed alternative relation.
        // Disjunction queries already consult it during ordinary proof.
        !(self.prove_trusted(&contrary) == Some(true)
            || !matches!(contrary, ExecutionCondition::Operation(bray_bound_tree::BoundOperator::LogicalOr, _))
                && self.proves_alternative(&contrary))
    }

    /// Owning authority must require trusted evidence under this same ordinary scope.
    pub(crate) fn owns_condition(&self, condition: &ExecutionCondition) -> bool {
        self.prove_trusted(condition) == Some(true)
            && condition.prove_with_memo(self.ordinary, &mut { ExecutionCondition::WORK_LIMIT },
                Some(&self.facts().direct.single_views()[1])) != Some(true)
    }

    pub(crate) fn prove_trusted(&self, condition: &ExecutionCondition) -> Option<bool> {
        if self.cache.is_none() || matches!(condition, ExecutionCondition::Boolean(_)) {
            return self.prove_trusted_inner(condition);
        }

        cached_condition(&self.facts().trusted_results, condition, || self.prove_trusted_inner(condition))
    }

    fn prove_trusted_inner(&self, condition: &ExecutionCondition) -> Option<bool> {
        if let Some(value) = condition.explicit_value(self.trusted, None, self.cache.map(|_| self.facts().direct.single_views()[0].as_ref())) {
            return Some(value);
        }

        if let Some(value) = condition.prove_with_memo(self.trusted, &mut { ExecutionCondition::WORK_LIMIT }, Some(&self.facts().direct.single_views()[0])) {
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

    /// Evaluate the original inputs without Boolean closure or alternative inference.
    pub(super) fn prove_direct(&self, condition: &ExecutionCondition, budget: &mut usize) -> Option<bool> {
        condition.prove_mixed_with_implications(self.trusted, self.ordinary, budget, None,
            self.cache.map(|_| &self.facts().direct))
    }

    fn prove_mixed_inner(&self, condition: &ExecutionCondition, budget: &mut usize) -> Option<bool> {

        if let Some(value) = condition.explicit_value(self.trusted, Some(self.ordinary), self.cache.map(|_| &self.facts().direct)) {
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

        cached_condition(&known.refined, condition, || {
            if !known.establishes(condition) { return Arc::from([]); }

            let ordinary = known.guards.get_or_init(|| {
                GuardSubstitution::new(&known.mixed.index,
                    known.conditions.iter().chain(self.ordinary.iter())
                        .filter(|(condition, _)| !condition.contains_trusted())
                        .map(|(condition, holds)| (condition, ExecutionCondition::Boolean(*holds))))
            });

            let normalized = ordinary.apply(condition);
            let normalized = (normalized != *condition && normalized.contains_trusted()).then_some(normalized);

            normalized.iter().chain(condition.observations().filter(|observed| {
                known.establishes(observed)
                    && (!observed.contains_trusted() || matches!(observed, ExecutionCondition::Trusted(_)))
                    && Some(*observed) != normalized.as_ref()
            })).cloned().collect::<Vec<_>>().into()
        })
    }

    fn known(&self) -> &DerivedFacts { self.facts().derived() }

    fn facts(&self) -> &PreparedFacts {
        self.prepared.get_or_init(|| {
            if let Some((cache, trusted, ordinary)) = self.cache { return cache.prepare(trusted, ordinary); }

            let index = ConditionIndex::default();
            let single = std::array::from_fn(|_| Arc::new(ProofMemo::new(&index)));

            Arc::new(PreparedFacts::new(self.trusted.clone().into(), self.ordinary.clone().into(), &index, single))
        })
    }

    pub(super) fn proves_alternative(&self, consequence: &ExecutionCondition) -> bool {
        #[cfg(test)]
        if self.reference_scan {
            return self.known().conditions.iter().chain(self.ordinary.iter().filter(|(condition, _)| !condition.contains_trusted()))
                .any(|(premise, holds)| *holds && matches!(premise, ExecutionCondition::Operation(bray_bound_tree::BoundOperator::LogicalOr, _)) && Self::implies(premise, consequence));
        }

        self.known().alternatives.proves(consequence)
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
    fn equality_results_retain_exact_ordinary_scope_and_expiry_preference() {
        let (_, expressions) = crate::test_support::expression_unit(bray_bound_tree::BoundUnitId::new(0), |tree, origin| {
            (0..2).map(|_| crate::test_support::push_expression(tree,
                crate::test_support::integer_literal_expression(origin, None))).collect()
        });

        let left = ExecutionCondition::Expression(expressions[0]);
        let right = ExecutionCondition::Expression(expressions[1]);
        let guard = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
        let alias = ExecutionCondition::value_alias(left.clone(), right.clone());

        let clause = ExecutionCondition::operation(LogicalOr, vec![
            ExecutionCondition::operation(LogicalNot, vec![guard.clone()]), alias,
        ]);

        let cache = ExecutionProofCache::default();
        let mut ordinary = super::ExecutionAssumptions::from([(clause, true)]);
        let inactive = cache.equalities(&ordinary, None);

        assert!(inactive.is_empty());
        guard.assume(true, &mut ordinary);

        let active = cache.equalities(&ordinary, None);
        let expiring = cache.equalities(&ordinary, Some(expressions[0]));

        assert_eq!(right.with_equalities(&active), left);
        assert_eq!(left.with_equalities(&expiring), right);
        assert!(inactive.is_empty(), "later evidence cannot mutate an earlier result");

        for expiry in [None, Some(expressions[0]), Some(expressions[1])] {
            let expected = ExecutionCondition::equalities(&ordinary, expiry);
            let actual = cache.equalities(&ordinary, expiry);
            let copied = ordinary.iter().cloned().collect();

            assert_eq!(*actual, expected);
            assert!(Arc::ptr_eq(&actual, &cache.equalities(&copied, expiry)));
        }

        ordinary.retain(|(condition, _)| matches!(condition, ExecutionCondition::Operation(LogicalOr, _)));
        assert!(cache.equalities(&ordinary, None).is_empty(), "removed guard truth cannot reuse active bindings");
    }

    #[test]
    fn analysis_preparation_reuses_exact_inputs_without_reusing_changed_authority() {
        let input = |index| ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(index)));
        let claim = ExecutionCondition::Trusted(Arc::new(input(1)));
        let guard = input(0);

        let clause = ExecutionCondition::operation(LogicalOr, vec![
            ExecutionCondition::operation(LogicalNot, vec![guard.clone()]), claim.clone(),
        ]);

        let facts = super::ExecutionAssumptions::from([(clause.clone(), true)]);

        let ordinary = super::ExecutionAssumptions::from([
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

        let rebuilt = facts.iter().cloned().collect::<super::ExecutionAssumptions>();
        let alias = cache.proof(&rebuilt, &ordinary);

        assert!(!rebuilt.shares_storage(&facts));
        assert_eq!(alias.prove_trusted(&claim), Some(true));
        assert!(Arc::ptr_eq(prepared, alias.prepared.get().expect("equal rebuilt inputs reuse preparation")));

        let mut unchanged = copied.clone();

        unchanged.extend(copied.iter().cloned());
        unchanged.retain(|_| true);

        assert!(unchanged.shares_storage(&copied), "no-op updates retain the immutable scope");

        let mut changed_inputs = ordinary.clone();

        changed_inputs.retain(|(condition, _)| condition != &guard);
        changed_inputs.insert((guard.clone(), false));

        assert!(!changed_inputs.shares_storage(&ordinary));
        assert_ne!(cache.proof(&facts, &changed_inputs).prove_trusted(&claim), Some(true));
        assert_eq!(proof.prove_trusted(&claim), Some(true), "updating a clone cannot mutate cached authority");

        let inactive = cache.proof(&facts, &changed_inputs);
        let reversed = cache.proof(&ordinary, &facts);
        let mut extra_facts = facts.clone();

        extra_facts.insert((ExecutionCondition::Trusted(Arc::new(input(7))), true));

        let extra = cache.proof(&extra_facts, &ordinary);
        let raw = prepared.direct.single_views();

        assert!(Arc::ptr_eq(&raw[0], &inactive.facts().direct.single_views()[0]), "unchanged trusted input shares its raw queries");
        assert!(!Arc::ptr_eq(&raw[1], &inactive.facts().direct.single_views()[1]), "changed guard polarity creates a new raw input");
        assert!(Arc::ptr_eq(&raw[1], &extra.facts().direct.single_views()[1]), "unchanged ordinary input shares its raw queries");
        assert!(Arc::ptr_eq(&raw[0], &reversed.facts().direct.single_views()[1]));
        assert_ne!(reversed.prove_trusted(&claim), Some(true), "raw input reuse cannot transfer mixed evidence roles");

        for (trusted, ordinary) in [(&facts, &changed_inputs), (&extra_facts, &ordinary), (&ordinary, &facts)] {
            let cached = cache.proof(trusted, ordinary);
            let reference = super::ExecutionProof::new(trusted, ordinary);

            for goal in [&clause, &claim, &guard] {
                for incoming in (0..=ExecutionCondition::WORK_LIMIT).chain([1, 2]) {
                    let mut expected_budget = incoming;
                    let mut actual_budget = incoming;
                    let expected = reference.prove_mixed(goal, &mut expected_budget);

                    assert_eq!(cached.prove_mixed(goal, &mut actual_budget), expected);
                    assert_eq!(actual_budget, expected_budget, "shared raw queries retain work costs in each mixed scope");
                }
            }
        }

        let observations = proof.refined_observations(&clause);

        assert!(observations.contains(&claim));
        assert!(Arc::ptr_eq(&observations, &same.refined_observations(&clause)));

        let address = input(1);
        let owner = ExecutionPlace::argument(SymbolOrdinal::new(20));

        let guarantee = crate::execution_guarantees::ExecutionGuarantee {
            witness: true,
            addresses: [(address.clone(), BTreeSet::from([owner.clone()])),
                (input(3), BTreeSet::from([owner.clone()]))].into(),
        };

        let witness = Arc::new(super::ExecutionWitness {
            conditions: [(clause.clone(), guarantee)].into(),
            ..Default::default()
        });

        let refined = cache.refined_witness(&witness, &facts, &ordinary);

        assert!(refined[&claim].witness);
        assert_eq!(refined[&claim].addresses, [(address.clone(), BTreeSet::from([owner.clone()]))].into());
        assert!(Arc::ptr_eq(&refined, &cache.refined_witness(&witness, &copied, &ordinary)));

        let mut changed = Arc::clone(&witness);
        let metadata = Arc::make_mut(&mut changed).conditions.get_mut(&clause).expect("original carried clause");

        metadata.witness = false;
        metadata.addresses.clear();

        let changed_result = cache.refined_witness(&changed, &facts, &ordinary);

        assert!(!changed_result[&claim].witness);
        assert!(changed_result[&claim].addresses.is_empty());
        assert!(refined[&claim].witness, "changed witness metadata cannot mutate an earlier result");

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

        let direct = cache.ordinary_proof(&ordinary);

        for goal in [guard.clone(), clause.clone(), claim.clone(),
            ExecutionCondition::operation(Less, vec![input(11), input(13)])]
        {
            for incoming in (0..=ExecutionCondition::WORK_LIMIT).chain([1, 2, ExecutionCondition::WORK_LIMIT]) {
                let mut expected_budget = incoming;
                let mut actual_budget = incoming;

                let expected = goal.prove_mixed_with_implications(&BTreeSet::new(), &ordinary,
                    &mut expected_budget, None, None);

                assert_eq!(direct.prove_direct(&goal, &mut actual_budget), expected);
                assert_eq!(actual_budget, expected_budget, "direct ordinary inference retains its original cost");
            }
        }

        std::thread::scope(|threads| {
            for active in [false, true] {
                let cache = &cache;
                let facts = &facts;
                let claim = &claim;
                let guard = guard.clone();
                let clause = &clause;
                let witness = &witness;

                threads.spawn(move || {
                    let ordinary = super::ExecutionAssumptions::from([(guard, active)]);
                    let proof = cache.proof(facts, &ordinary);

                    assert_eq!(proof.prove_trusted(claim) == Some(true), active);
                    assert_eq!(proof.refined_observations(clause).contains(claim), active);
                    assert_eq!(cache.refined_witness(witness, facts, &ordinary).contains_key(claim), active);
                });
            }
        });

        let empty = super::ExecutionAssumptions::default();

        assert_ne!(cache.proof(&empty, &ordinary).prove_trusted(&claim), Some(true), "removing qualified evidence revokes authority");
        assert!(!cache.refined_witness(&witness, &empty, &ordinary).contains_key(&claim), "removed evidence cannot reuse a refinement");
        assert_ne!(cache.proof(&ordinary, &facts).prove_trusted(&claim), Some(true), "swapping evidence roles cannot qualify an ordinary occurrence");
        assert_ne!(cache.proof(&facts, &ordinary).prove_trusted(&ExecutionCondition::Trusted(Arc::new(input(2)))), Some(true), "another owner has no matching evidence");

        let other_claim = ExecutionCondition::Trusted(Arc::new(input(2)));

        let other_clause = ExecutionCondition::operation(LogicalOr, vec![
            ExecutionCondition::operation(LogicalNot, vec![guard.clone()]), other_claim.clone(),
        ]);

        let colliding = super::ExecutionAssumptions::from([(other_clause, true)]);
        let other_owner = cache.proof(&colliding, &ordinary);

        assert_eq!(other_owner.prove_trusted(&other_claim), Some(true));
        assert_ne!(other_owner.prove_trusted(&claim), Some(true), "same-shaped facts cannot merge owner evidence");
        assert!(!Arc::ptr_eq(prepared, other_owner.prepared.get().expect("another owner prepares distinct evidence")));

        let independent = ExecutionProofCache::default();
        let other = independent.proof(&facts, &ordinary);

        assert_eq!(other.prove_trusted(&claim), Some(true));
        assert!(!Arc::ptr_eq(prepared, other.prepared.get().expect("independent analysis prepares its own evidence")));
    }
}
