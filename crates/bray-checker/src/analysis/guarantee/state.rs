use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::BoundExpressionId;

use crate::ExecutionCondition;

pub(super) type DeferredInvocations = BTreeMap<ExecutionCondition, super::deferred::DeferredValue>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ExecutionState {
    pub(super) trust_boundaries: BTreeSet<BoundExpressionId>,
    pub(super) assumptions: BTreeSet<(ExecutionCondition, bool)>,
    pub(super) witnesses:
        BTreeMap<ExecutionCondition, std::sync::Arc<crate::execution_guarantees::ExecutionWitness>>,
    pub(super) trusted_assumptions: BTreeSet<(ExecutionCondition, bool)>,
    pub(super) allocation_owners: BTreeMap<ExecutionCondition, ExecutionCondition>,
    pub(super) current: super::current::CurrentValues,
    pub(super) expressions: BTreeMap<BoundExpressionId, ExecutionCondition>,
    pub(super) pending_results: BTreeMap<BoundExpressionId, ExecutionCondition>,
    pub(super) owned_results: BTreeMap<
        ExecutionCondition,
        bray_symbols::DependencyOwnedResult<bray_bound_tree::StorageProjection, ExecutionCondition>,
    >,
    pub(super) entries: BTreeMap<bray_bound_tree::BoundExecutionSite, crate::ExecutionCallEvidence>,
    pub(super) deferred_invocations: DeferredInvocations,
    pub(super) completion_dependencies: BTreeSet<crate::ExecutionCompletionDependency>,
}

impl Default for ExecutionState {
    fn default() -> Self {
        Self {
            trust_boundaries: BTreeSet::new(),
            assumptions: BTreeSet::new(),
            witnesses: BTreeMap::new(),
            trusted_assumptions: BTreeSet::new(),
            allocation_owners: BTreeMap::new(),
            current: super::current::CurrentValues::default(),
            expressions: BTreeMap::new(),
            pending_results: BTreeMap::new(),
            owned_results: BTreeMap::new(),
            entries: BTreeMap::new(),
            deferred_invocations: BTreeMap::new(),
            completion_dependencies: BTreeSet::new(),
        }
    }
}

pub(super) fn update_parent_observations(
    observations: &mut impl crate::execution_guarantees::ExecutionValues,
    place: &crate::ExecutionPlace,
    value: &ExecutionCondition,
    storage: Option<&[bray_bound_tree::StorageProjection]>,
) {
    for length in 0..place.projections.len() {
        let mut parent = place.clone();

        parent.projections = place.projections[..length].into();

        if let Some(snapshot) = observations.get_value_mut(&parent) {
            *snapshot =
                snapshot.with_component(&place.projections[length..], value.clone(), storage);
        }
    }
}

impl ExecutionState {
    pub(super) fn entry_witness_inputs(
        &self,
        invocation: bray_bound_tree::BoundExecutionSite,
    ) -> BTreeSet<ExecutionCondition> {
        let Some(entry) = self.entries.get(&invocation) else { return BTreeSet::new(); };

        let owners = entry.owned_arguments().collect::<BTreeSet<_>>();

        entry.witness_inputs.iter().filter(|carrier| {
            crate::execution_guarantees::ObservationSet::retains_owner(&owners, carrier)
        }).cloned().collect()
    }

    pub(super) fn private_witness_inputs(
        &self,
        invocation: Option<bray_bound_tree::BoundExecutionSite>,
    ) -> BTreeSet<ExecutionCondition> {
        let Some(invocation) = invocation.filter(|invocation| self.entries.get(invocation)
            .is_some_and(|entry| !entry.pending_execution)) else { return BTreeSet::new(); };

        self.entry_witness_inputs(invocation).into_iter().filter(|carrier| {
            self.witnesses.get(carrier).is_some_and(|witness| !witness.available)
        }).collect()
    }

    pub(super) fn private_allocation_conditions(
        &self,
        invocation: Option<bray_bound_tree::BoundExecutionSite>,
        carriers: &BTreeSet<ExecutionCondition>,
    ) -> BTreeSet<ExecutionCondition> {
        let owners = invocation.and_then(|invocation| self.entries.get(&invocation))
            .into_iter().flat_map(|entry| entry.owned_arguments()).collect::<BTreeSet<_>>();

        let conditions = carriers.iter().filter_map(|carrier| self.witnesses.get(carrier))
            .flat_map(|witness| witness.conditions.keys()).collect::<BTreeSet<_>>();

        self.allocation_owners.iter().filter(|(condition, owner)| {
            conditions.contains(condition)
                && crate::execution_guarantees::ObservationSet::retains_owner(&owners, owner)
        }).map(|(condition, _)| condition.clone()).collect()
    }

    /// The existing ledger retains private input clauses while public authority is suspended.
    pub(super) fn suspend_owned_witnesses(&mut self, carriers: &BTreeSet<ExecutionCondition>) {
        let mut retired = BTreeSet::new();

        for carrier in carriers {
            let Some(witness) = self.witnesses.get_mut(carrier) else { continue; };

            for condition in witness.conditions.keys() {
                condition.clone().assume(true, &mut retired);
            }

            if witness.available {
                std::sync::Arc::make_mut(witness).available = false;
            }
        }

        self.exclude_live_witnesses(&mut retired);
        self.trusted_assumptions.retain(|fact| !retired.contains(fact));
    }

    /// Mutate pending entry snapshots only when their captured context changes.
    pub(super) fn pending_entries_mut<'a>(
        &'a mut self,
        changed: &'a impl Fn(&crate::ExecutionCallEvidence) -> bool,
    ) -> impl Iterator<Item = &'a mut crate::ExecutionCallEvidence> {
        self.entries
            .values_mut()
            .filter(move |entry| entry.pending_execution && changed(entry))
            .chain(
                self.deferred_invocations
                    .values_mut()
                    .flat_map(|entries| entries.producers.values_mut())
                    .filter(move |candidate| {
                        candidate.entry.pending_execution && changed(&candidate.entry)
                    })
                    .map(|candidate| std::sync::Arc::make_mut(&mut candidate.entry)),
            )
    }

    pub(super) fn resume_owned_witnesses(
        &mut self,
        invocation: bray_bound_tree::BoundExecutionSite,
    ) {
        let entry = &self.entries[&invocation];

        let mut assumptions = entry.assumptions.clone();

        assumptions.extend(self.assumptions.iter().cloned());

        for owner in &entry.witness_inputs {
            let Some(witness) = self.witnesses.get_mut(owner) else {
                continue;
            };

            let mut resumed = false;

            for condition in witness.conditions.keys() {
                if condition.prove_trusted(&entry.trusted_assumptions, &assumptions) == Some(true) {
                    resumed = true;

                    condition
                        .clone()
                        .assume(true, &mut self.trusted_assumptions);
                }
            }

            if resumed && !witness.available {
                std::sync::Arc::make_mut(witness).available = true;
            }
        }
    }

    pub(super) fn deferred_entries(
        &self,
        value: &ExecutionCondition,
    ) -> Option<&super::deferred::DeferredValue> {
        self.deferred_invocations.get(value.owner_identity())
    }

    /// Visits initialized owned components and the checked captures of pending futures.
    pub(super) fn owned_values<'a>(
        &'a self,
        value: &ExecutionCondition,
    ) -> impl Iterator<Item = ExecutionCondition> + 'a + use<'a> {
        // Snapshots share immutable components. Only the traversal's small worklist is owned.
        let mut pending = vec![value.clone()];

        let mut deferred = BTreeSet::new();

        std::iter::from_fn(move || {
            let value = pending.pop()?;

            pending.extend(value.owned_components().cloned());

            if let Some((crate::execution_guarantees::ExecutionObservationKind::Owned, owner)) = value.ownership_identity()
                && let Some(entries) = self.deferred_invocations.get(owner)
            {
                for (invocation, candidate) in &entries.producers {
                    let entry = &candidate.entry;

                    if deferred.insert((owner.clone(), *invocation)) {
                        pending.extend(entry.owned_arguments());
                    }
                }
            }

            Some(value)
        })
    }

    /// Finds actual witness carriers contained by owned values and their deferred captures.
    pub(super) fn owned_witnesses(
        &self,
        values: impl IntoIterator<Item = ExecutionCondition>,
    ) -> impl Iterator<
        Item = (
            &ExecutionCondition,
            &std::sync::Arc<crate::execution_guarantees::ExecutionWitness>,
        ),
    > {
        let mut owned = BTreeSet::new();

        if !self.witnesses.is_empty() {
            for value in values {
                owned.extend(self.owned_values(&value));
            }
        }

        let count = if owned.is_empty() {
            0
        } else {
            self.witnesses.len()
        };

        self.witnesses
            .iter()
            .take(count)
            .filter(move |(carrier, _)| {
                crate::execution_guarantees::ObservationSet::retains_owner(&owned, carrier)
            })
    }

    pub(super) fn call_trusted_assumptions(
        &self,
        transferred: impl IntoIterator<Item = ExecutionCondition>,
    ) -> BTreeSet<(ExecutionCondition, bool)> {
        let mut transferred = transferred.into_iter().collect::<Vec<_>>();
        let mut owners = BTreeSet::new();

        while let Some(value) = transferred.pop() {
            if value == ExecutionCondition::Unknown {
                continue;
            }

            transferred.extend(value.owned_components().cloned());

            owners.insert(value);
        }

        self.trusted_assumptions
            .iter()
            .filter(|(condition, _)| {
                let mut pending = vec![condition];

                while let Some(condition) = pending.pop() {
                    if let Some(owner) = self.allocation_owners.get(condition)
                        && !owners.contains(owner)
                    {
                        return false;
                    }

                    match condition {
                        ExecutionCondition::Trusted(condition) => pending.push(condition),
                        ExecutionCondition::Operation(_, operands) => {
                            pending.extend(operands.iter())
                        }
                        _ => {}
                    }
                }

                true
            })
            .cloned()
            .collect()
    }

    pub(super) fn carry_guarantee(
        &mut self,
        carrier: ExecutionCondition,
        condition: ExecutionCondition,
    ) {
        let witness = self
            .witnesses
            .entry(carrier.owner_identity().clone())
            .or_default();

        witness.carry(&condition, true, &Default::default());
    }

    /// Snapshot access bounds independently of the owning carrier's later consumption.
    pub(super) fn address_dependencies(
        &self,
        value: &ExecutionCondition,
    ) -> crate::execution_guarantees::AddressDependencies {
        let mut addresses = crate::execution_guarantees::AddressDependencies::new();

        for value in self.owned_values(value) {
            let mut carrier = value.owner_identity().clone();

            loop {
                if let Some(witness) = self.witnesses.get(&carrier) {
                    for (condition, guarantee) in &witness.conditions {
                        if condition.prove_trusted(&self.trusted_assumptions, &self.assumptions)
                            != Some(true)
                        {
                            continue;
                        }

                        for (address, owners) in &guarantee.addresses {
                            if address.is_owned_projection_of(&value) {
                                addresses
                                    .entry(address.clone())
                                    .or_default()
                                    .extend(owners.iter().cloned());
                            }
                        }
                    }
                }

                carrier = match carrier {
                    ExecutionCondition::Projection(_, parent) => parent.owner_identity().clone(),
                    ExecutionCondition::Input(mut place) if !place.projections.is_empty() => {
                        place.projections = place.projections[..place.projections.len() - 1].into();

                        ExecutionCondition::Input(place)
                    }
                    _ => break,
                };
            }
        }

        addresses
    }

    pub(super) fn directly_carries_witness(&self, value: &ExecutionCondition) -> bool {
        let Some((crate::execution_guarantees::ExecutionObservationKind::Owned, identity)) = value.ownership_identity() else {
            return false;
        };

        self.witnesses
            .get(identity)
            .is_some_and(|witness| {
                witness.available && witness.is_owning(&self.trusted_assumptions, &self.assumptions)
            })
    }

    pub(super) fn owns_witness(&self, value: &ExecutionCondition) -> bool {
        self.owned_witnesses([value.clone()])
            .any(|(carrier, _)| self.directly_carries_witness(carrier))
    }

    /// Reusing a finite evaluation identity must not revive a previous iteration's evidence.
    pub(super) fn retire_evaluation(&mut self, mutation: bray_bound_tree::StorageMutation) {
        let stale = |value: &ExecutionCondition| value.depends_on(mutation);

        self.current.retire_availability(mutation);

        self.owned_results.retain(|value, _| !stale(value));

        self.witnesses.retain(|value, witness| {
            if stale(value) {
                return false;
            }

            witness.retain_conditions(|condition, guarantee| {
                !stale(condition) && !guarantee.addresses.keys().any(stale)
            });

            !witness.conditions.is_empty() || !witness.dependencies.is_empty()
        });

        self.deferred_invocations.retain(|value, entries| {
            entries.retire_selection(|condition| condition.depends_on(mutation));

            !stale(value)
        });

        self.trusted_assumptions
            .retain(|(condition, _)| !stale(condition));

        self.assumptions.retain(|(condition, _)| !stale(condition));

        self.allocation_owners
            .retain(|condition, owner| !stale(condition) && !stale(owner));

        for entry in self.pending_entries_mut(&|entry| {
            entry
                .arguments
                .values()
                .chain(entry.copy_sources.values())
                .any(stale)
                || entry
                    .assumptions
                    .iter()
                    .chain(&entry.trusted_assumptions)
                    .any(|(condition, _)| stale(condition))
        }) {
            entry.arguments.retain(|_, value| !stale(value));
            entry.copy_sources.retain(|_, value| !stale(value));
            entry.assumptions.retain(|(condition, _)| !stale(condition));

            entry
                .trusted_assumptions
                .retain(|(condition, _)| !stale(condition));
        }

        for value in self
            .current
            .values_mut()
            .chain(self.expressions.values_mut())
            .chain(self.pending_results.values_mut())
        {
            *value = value.without_evaluation(mutation, &mut { ExecutionCondition::WORK_LIMIT });
        }
    }

    pub(super) fn invalidate_cleanup(
        &mut self,
        stable_values: &crate::execution_guarantees::ObservationSet,
        stable_owners: &BTreeSet<ExecutionCondition>,
        admitted: Option<bray_bound_tree::BoundExecutionSite>,
    ) {
        self.refine_witnesses();

        let private = self.private_witness_inputs(admitted);
        let private_allocations = self.private_allocation_conditions(admitted, &private);

        self.assumptions.retain(|(condition, _)| {
            !condition.observes_live(None) || condition.observes_only(|value| stable_values.preserves(value))
        });

        let mut retained = BTreeSet::new();

        self.witnesses.retain(|carrier, witness| {
            if private.contains(carrier) {
                return true;
            }

            if !crate::execution_guarantees::ObservationSet::retains_owner(stable_owners, carrier) {
                return false;
            }

            witness.retain_conditions(|condition, _| condition.observes_only(|value| stable_values.preserves(value)));

            // Availability says the carrier can be used, not that every possible
            // clause was proved on this path. Preserve only established truth.
            for condition in witness.conditions.keys().filter(|condition| {
                witness.available && condition.prove_trusted(&self.trusted_assumptions, &self.assumptions) == Some(true)
            }) {
                condition.clone().assume(true, &mut retained);
            }

            !witness.conditions.is_empty()
        });

        // Opaque predicates cannot observe mutable state absent from their arguments.
        // Preserve established facts with no observations and unchanged ordinary
        // guards. A predicate observing a value still needs its complete witness.
        let preserves = |fact: &(ExecutionCondition, bool)| {
            retained.contains(fact) || fact.0.observes_only(|_| false)
                || (fact.0.observes_only(|value| stable_values.preserves(value))
                    && fact.0.observations().all(|value| {
                        !matches!(value, ExecutionCondition::Predicate(_, _, _) | ExecutionCondition::Call(_, _, _, _))
                            || value.observes_only(|_| false)
                    }))
        };

        // Private moved inputs retain historical entry facts while child execution
        // owns their authority. Completion still requires their surviving live witness.
        let preserves_owned = |fact: &(ExecutionCondition, bool), owners: &BTreeSet<ExecutionCondition>| {
            preserves(fact)
                || (fact.0.observes_only(|value| stable_values.preserves(value))
                    && fact.0.observations().any(|value| {
                        crate::execution_guarantees::ObservationSet::retains_owner(owners, value)
                    }))
        };

        self.trusted_assumptions.retain(preserves);

        self.allocation_owners.retain(|condition, owner| {
            private_allocations.contains(condition)
                || (condition.observes_only(|value| stable_values.preserves(value))
                    && crate::execution_guarantees::ObservationSet::retains_owner(stable_owners, owner))
        });

        for entry in self.pending_entries_mut(&|entry| {
            let owners = entry.owned_arguments().collect::<BTreeSet<_>>();

            entry.trusted_boundary
                || entry
                    .copy_sources
                    .values()
                    .any(|value| !value.observes_only(|value| stable_values.preserves(value)))
                || entry
                    .assumptions
                    .iter()
                    .any(|(condition, _)| !condition.observes_only(|value| stable_values.preserves(value)))
                || entry
                    .trusted_assumptions
                    .iter()
                    .any(|fact| !preserves_owned(fact, &owners))
        }) {
            let owners = entry.owned_arguments().collect::<BTreeSet<_>>();

            entry
                .copy_sources
                .retain(|_, value| value.observes_only(|value| stable_values.preserves(value)));

            entry
                .assumptions
                .retain(|(condition, _)| condition.observes_only(|value| stable_values.preserves(value)));

            entry
                .trusted_assumptions
                .retain(|fact| preserves_owned(fact, &owners));

            entry.trusted_boundary = false;
        }
    }

    pub(super) fn join_values(
        &mut self,
        replacements: &BTreeMap<ExecutionCondition, ExecutionCondition>,
        identities: &BTreeMap<ExecutionCondition, ExecutionCondition>,
    ) {
        if replacements.is_empty() && identities.is_empty() {
            return;
        }

        let rename =
            |value: &ExecutionCondition| value.with_joined_values(replacements, identities);

        let rename_identity =
            |value: &ExecutionCondition| value.with_joined_owners(replacements, identities);

        for facts in [&mut self.assumptions, &mut self.trusted_assumptions] {
            let mut renamed = BTreeSet::new();

            for (condition, value) in facts.iter() {
                rename(condition).assume(*value, &mut renamed);
            }

            *facts = renamed;
        }

        crate::execution_guarantees::rename_witnesses(
            &mut self.witnesses,
            &rename,
            &rename_identity,
        );

        self.allocation_owners = self
            .allocation_owners
            .iter()
            .map(|(condition, owner)| (rename(condition), rename_identity(owner)))
            .filter(|(condition, _)| *condition != ExecutionCondition::Unknown)
            .collect();

        let mut ownership = BTreeMap::new();

        for (value, result) in std::mem::take(&mut self.owned_results) {
            let value = rename(&value);

            if value != ExecutionCondition::Unknown {
                ownership
                    .entry(value)
                    .and_modify(
                        |previous: &mut bray_symbols::DependencyOwnedResult<
                            bray_bound_tree::StorageProjection,
                            ExecutionCondition,
                        >| {
                            *previous = bray_symbols::DependencyOwnedResult::choice([
                                previous.clone(),
                                result.clone(),
                            ]);
                        },
                    )
                    .or_insert(result);
            }
        }

        self.owned_results = ownership;

        let mut computations = DeferredInvocations::new();

        for (owner, mut entries) in std::mem::take(&mut self.deferred_invocations) {
            let value = rename_identity(&owner);

            if value == ExecutionCondition::Unknown {
                continue;
            }

            entries.join_values(
                replacements,
                identities,
                (value != owner).then_some(&self.assumptions),
            );

            match computations.entry(value) {
                std::collections::btree_map::Entry::Vacant(slot) => {
                    slot.insert(entries);
                }
                std::collections::btree_map::Entry::Occupied(mut slot) => {
                    slot.get_mut().merge(&entries);
                }
            }
        }

        self.deferred_invocations = computations;

        for value in self.current.values_mut() {
            *value = rename(value);
        }
    }

    pub(super) fn assign(
        &mut self,
        place: crate::ExecutionPlace,
        value: ExecutionCondition,
        storage: Option<&[bray_bound_tree::StorageProjection]>,
    ) {
        self.current.replace(place, super::current::CurrentValue {
            value, availability: super::current::ValueAvailability::Present,
        }, storage);
    }

    pub(super) fn observe(
        &mut self,
        place: crate::ExecutionPlace,
        value: ExecutionCondition,
        storage: Option<&[bray_bound_tree::StorageProjection]>,
    ) {
        let availability = self.current.availability(&place);

        self.current.write(place, super::current::CurrentValue { value, availability }, storage);
    }

    pub(super) fn vacate(&mut self, place: crate::ExecutionPlace) {
        self.current.replace(place, super::current::CurrentValue {
            value: ExecutionCondition::Unknown, availability: super::current::ValueAvailability::Absent,
        }, None);
    }

}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use bray_symbols::{ConstantProjectionKind, SymbolOrdinal};

    use super::ExecutionState;
    use crate::{ExecutionCondition, ExecutionPlace};

    #[test]
    fn retirement_respects_the_retained_loan_context() {
        let owner = input(0);
        let loan = owner.clone().borrowed();
        let mut state = ExecutionState::default();

        state.assumptions.insert((loan.clone(), true));
        state.retire_trusted_values([owner.clone()], &BTreeSet::from([loan.clone()]));

        assert!(state.assumptions.contains(&(loan.clone(), true)));

        state.retire_trusted_values([owner], &BTreeSet::new());

        assert!(!state.assumptions.contains(&(loan, true)));
    }

    #[test]
    fn known_value_writes_revoke_only_borrowed_authority_on_the_changed_storage() {
        let symbol = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>(
            "ValidRead",
        );

        let condition = |value| {
            ExecutionCondition::Trusted(Arc::new(ExecutionCondition::predicate(
                symbol.into(),
                crate::test_support::empty_substitution(symbol.into()),
                vec![value],
            )))
        };

        for scalar in [false, true] {
        let storage = |owner| {
            if scalar {
                return ExecutionCondition::scalar_observation(input(owner), input(4)).borrowed();
            }

            ExecutionCondition::owned_value(input(owner), input(4)).borrowed()
        };

        let observed = condition(storage(0));
        let other = condition(storage(1));
        let copied_value = condition(input(0));
        let mut state = ExecutionState::default();

        for (owner, condition) in [
            (input(0), &observed),
            (input(1), &other),
            (input(2), &copied_value),
        ] {
            state.carry_guarantee(owner, condition.clone());
            state.trusted_assumptions.insert((condition.clone(), true));
        }

        state.invalidate_contents(
            &[ExecutionPlace::argument(SymbolOrdinal::new(0))].into(),
            [ExecutionCondition::Boolean(true)],
            &BTreeSet::new(),
            &crate::execution_guarantees::ObservationSet::default(),
            None,
        );

        assert!(!state.trusted_assumptions.contains(&(observed, true)));
        assert!(state.trusted_assumptions.contains(&(other, true)));
        assert!(state.trusted_assumptions.contains(&(copied_value, true)));
        assert!(!state.witnesses.contains_key(&input(0)));
        assert!(state.witnesses.contains_key(&input(1)));
        assert!(state.witnesses.contains_key(&input(2)));
        }
    }

    #[test]
    fn a_transferred_tag_preserves_guards_without_preserving_mutable_payloads() {
        let owner = input(0);
        let observed = owner.clone().borrowed();

        let absent = ExecutionCondition::Literal(Arc::new(bray_symbols::ConstantValueData::new(
            crate::test_support::error_type(), bray_symbols::ConstantValueKind::NullableAbsent,
        )));

        let guard = ExecutionCondition::operation(bray_bound_tree::BoundOperator::NotEqual, vec![observed.clone(), absent]);
        let payload = observed.clone().component(ConstantProjectionKind::NullableValue).borrowed();
        let mut state = ExecutionState::default();

        state.assumptions.extend([(guard.clone(), true), (payload.clone(), true)]);
        state.trusted_assumptions.extend([(guard.clone(), true), (payload.clone(), true)]);
        state.carry_guarantee(owner.clone(), guard.clone());
        state.carry_guarantee(owner.clone(), payload.clone());

        let mut retained = crate::execution_guarantees::ObservationSet::default();

        retained.insert_classified(observed, bray_bound_tree::StorageObservationKind::Discriminant);
        state.invalidate_contents(&BTreeSet::new(), [owner.clone()], &BTreeSet::new(), &retained, None);
        assert!(state.assumptions.contains(&(guard.clone(), true)));
        assert_eq!(guard.prove_trusted(&state.trusted_assumptions, &state.assumptions), Some(true));
        assert!(!state.assumptions.contains(&(payload.clone(), true)));
        assert!(!state.trusted_assumptions.contains(&(payload, true)));
    }

    #[test]
    fn cleanup_keeps_observation_free_guarantees_with_unchanged_guards() {
        let symbol = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>(
            "ValidWrite",
        );

        let substitution = crate::test_support::empty_substitution(symbol.into());

        let predicate = |arguments: Vec<_>| ExecutionCondition::Trusted(Arc::new(
            ExecutionCondition::Predicate(symbol.into(), substitution, arguments.into()),
        ));

        let guard = input(0);
        let independent = predicate(Vec::new());

        let guarded = ExecutionCondition::operation(bray_bound_tree::BoundOperator::LogicalOr,
            vec![guard.clone(), independent.clone()]);

        let observed = ExecutionCondition::operation(bray_bound_tree::BoundOperator::LogicalOr,
            vec![guard.clone(), predicate(vec![input(1).borrowed()])]);

        for unchanged in [false, true] {
            let mut state = ExecutionState::default();

            state.trusted_assumptions.extend([
                (independent.clone(), true), (guarded.clone(), true), (observed.clone(), true),
            ]);

            let stable = if unchanged {
                BTreeSet::from([guard.clone(), input(1).borrowed()])
            } else { BTreeSet::new() };

            state.invalidate_cleanup(
                &stable.iter().cloned().collect(), &BTreeSet::new(), None);

            assert!(state.trusted_assumptions.contains(&(independent.clone(), true)));
            assert_eq!(state.trusted_assumptions.contains(&(guarded.clone(), true)), unchanged);
            assert!(!state.trusted_assumptions.contains(&(observed.clone(), true)));
        }
    }

    fn input(ordinal: u32) -> ExecutionCondition {
        ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(ordinal)))
    }

    #[test]
    fn cleanup_requires_the_complete_carrier_to_survive() {
        let owner = input(0);
        let marker = owner.clone().component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0)));
        let symbol = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>("ValidRead");

        let condition = ExecutionCondition::Trusted(Arc::new(ExecutionCondition::Predicate(
            symbol.into(),
            crate::test_support::empty_substitution(symbol.into()),
            [marker.clone().borrowed()].into(),
        )));

        for complete in [false, true] {
            let mut state = ExecutionState::default();

            state.carry_guarantee(owner.clone(), condition.clone());
            state.trusted_assumptions.insert((condition.clone(), true));

            state.invalidate_cleanup(
                &[marker.clone(), marker.clone().borrowed()].into_iter().collect(),
                &BTreeSet::from([if complete { owner.clone() } else { marker.clone() }]),
                None,
            );

            assert_eq!(state.witnesses.contains_key(&owner), complete);
            assert_eq!(state.trusted_assumptions.contains(&(condition.clone(), true)), complete);

            state.invalidate_trusted(&owner);

            assert!(state.witnesses.is_empty());
            assert!(!state.trusted_assumptions.contains(&(condition.clone(), true)));
        }
    }

    #[test]
    fn cleanup_and_witness_selection_use_actual_overridden_children() {
        let parent = input(0);
        let left = parent.clone().component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0)));
        let right = parent.clone().component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1)));
        let parent_claim = permission(20);
        let left_claim = permission(21);
        let right_claim = permission(22);
        let stable = [parent_claim.clone(), left_claim.clone(), right_claim.clone()].into_iter().collect();

        for replacement in [
            input(1),
            ExecutionCondition::scalar_observation(left.clone(), left.clone()),
            ExecutionCondition::reference_value(left.clone(), left.clone()),
        ] {
            let actual = ExecutionCondition::constructed(parent.clone(), [(
                bray_bound_tree::StorageProjection::TupleElement(SymbolOrdinal::new(0)), replacement,
            )].into());

            let mut state = ExecutionState::default();

            state.carry_guarantee(left.clone(), left_claim.clone());
            left_claim.clone().assume(true, &mut state.trusted_assumptions);

            assert!(state.owns_witness(&parent));
            assert!(!state.owns_witness(&actual));
            assert!(state.owned_witnesses([actual.clone()]).next().is_none());

            for (owner, condition) in [(&parent, &parent_claim), (&right, &right_claim)] {
                state.carry_guarantee(owner.clone(), condition.clone());
                condition.clone().assume(true, &mut state.trusted_assumptions);
            }

            for (owner, condition) in [(&parent, &parent_claim), (&left, &left_claim), (&right, &right_claim)] {
                state.allocation_owners.insert(condition.clone(), owner.clone());
            }

            assert_eq!(state.owned_witnesses([actual.clone()]).map(|(owner, _)| owner.clone()).collect::<BTreeSet<_>>(),
                BTreeSet::from([parent.clone(), right.clone()]));

            state.invalidate_cleanup(&stable, &BTreeSet::from([actual]), None);

            assert!(state.directly_carries_witness(&parent));
            assert!(state.directly_carries_witness(&right));
            assert!(!state.directly_carries_witness(&left));
            assert!(!state.trusted_assumptions.contains(&(left_claim.clone(), true)));
            assert_eq!(state.allocation_owners.values().cloned().collect::<BTreeSet<_>>(), BTreeSet::from([parent.clone(), right.clone()]));
        }
    }

    fn permission(ordinal: u32) -> ExecutionCondition {
        let symbol = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>(
            "ValidRead",
        );

        ExecutionCondition::Trusted(Arc::new(ExecutionCondition::Predicate(
            symbol.into(),
            crate::test_support::empty_substitution(symbol.into()),
            [input(ordinal), ExecutionCondition::Boolean(true)].into(),
        )))
    }

    #[test]
    fn only_recorded_admitted_owners_keep_unavailable_entry_authority() {
        use bray_bound_tree::{BoundErrorExpression, BoundExpression, BoundUnitId};
        use crate::test_support::{error_type, expression_unit, push_expression};

        let (_, expressions) = expression_unit(BoundUnitId::new(41), |tree, origin| vec![
            push_expression(tree, BoundExpression::Error(BoundErrorExpression::new(origin, error_type()))),
        ]);

        let invocation = expressions[0].into();
        let owner = input(0);
        let condition = permission(2);

        for (owned, recorded, own_context) in [(true, true, true), (false, true, true), (true, false, true), (true, true, false)] {
            let accepted = owned && recorded && own_context;
            let mut state = ExecutionState::default();

            state.carry_guarantee(owner.clone(), condition.clone());
            condition.clone().assume(true, &mut state.trusted_assumptions);
            state.allocation_owners.insert(condition.clone(), owner.clone());

            state.entries.insert(invocation, crate::ExecutionCallEvidence {
                arguments: [(ExecutionPlace::argument(SymbolOrdinal::new(0)), owner.clone())].into(),
                owned_inputs: if owned { [SymbolOrdinal::new(0)].into() } else { [].into() },
                witness_inputs: if recorded { [owner.clone()].into() } else { BTreeSet::new() },
                trusted_assumptions: state.trusted_assumptions.clone(),
                ..crate::ExecutionCallEvidence::default()
            });

            state.suspend_owned_witnesses(&BTreeSet::from([owner.clone()]));

            assert!(!state.trusted_assumptions.contains(&(condition.clone(), true)));

            state.invalidate_cleanup(&Default::default(), &BTreeSet::new(), own_context.then_some(invocation));

            assert_eq!(state.witnesses.contains_key(&owner), accepted);
            assert_eq!(state.allocation_owners.contains_key(&condition), accepted);
            assert!(!state.trusted_assumptions.contains(&(condition.clone(), true)));

            state.resume_owned_witnesses(invocation);

            assert_eq!(condition.prove_trusted(&state.trusted_assumptions, &state.assumptions) == Some(true), accepted);
        }
    }

    #[test]
    fn call_join_retains_possible_address_bounds_without_preserving_unproven_facts() {
        let argument = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let address = input(2);
        let condition = permission(2);
        let mut left = crate::ExecutionCallEvidence::default();
        let mut right = crate::ExecutionCallEvidence::default();

        for (entry, owner) in [(&mut left, 30), (&mut right, 31)] {
            entry.addresses.entry(argument.clone()).or_default().insert(
                address.clone(),
                BTreeSet::from([ExecutionPlace::argument(SymbolOrdinal::new(owner))]),
            );
        }

        condition
            .clone()
            .assume(true, &mut left.trusted_assumptions);

        assert!(left.intersect(&right));
        assert_eq!(left.addresses[&argument][&address].len(), 2);
        assert!(!left.prove_trusted_requirements(&[condition]).is_ok());
        assert!(!left.intersect(&right));
    }

    #[test]
    fn address_bounds_lookup_uses_only_live_facts_on_the_complete_carrier() {
        let owner = ExecutionPlace::argument(SymbolOrdinal::new(30));
        let carrier = input(2);

        let address = ExecutionCondition::Projection(
            ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0)),
            Arc::new(carrier.clone()),
        );

        let access = ExecutionCondition::Trusted(Arc::new(address.clone()));
        let mut state = ExecutionState::default();

        std::sync::Arc::make_mut(state.witnesses.entry(carrier).or_default())
            .conditions
            .entry(access.clone())
            .or_default()
            .addresses
            .insert(address.clone(), BTreeSet::from([owner.clone()]));

        assert!(state.address_dependencies(&address).is_empty());
        access.clone().assume(true, &mut state.trusted_assumptions);

        assert_eq!(
            state.address_dependencies(&address),
            std::collections::BTreeMap::from([(address.clone(), BTreeSet::from([owner]))])
        );

        assert!(state.address_dependencies(&input(3)).is_empty());
        state.trusted_assumptions.clear();
        assert!(state.address_dependencies(&address).is_empty());
    }

    #[test]
    fn copyable_access_facts_expire_without_creating_ownership_witnesses() {
        let pointer = input(2);
        let access = permission(2);
        let owner = ExecutionPlace::argument(SymbolOrdinal::new(30));
        let mut state = ExecutionState::default();

        state.current.insert(owner.clone(), input(30));

        std::sync::Arc::make_mut(state.witnesses.entry(pointer.clone()).or_default())
            .conditions
            .entry(access.clone())
            .or_default()
            .addresses
            .insert(pointer.clone(), BTreeSet::from([owner.clone()]));

        access.clone().assume(true, &mut state.trusted_assumptions);
        assert!(!state.directly_carries_witness(&pointer));
        assert!(state.trusted_assumptions.contains(&(access.clone(), true)));
        state.invalidate_trusted_roots(&BTreeSet::from([owner.root]));
        assert!(!state.trusted_assumptions.contains(&(access, true)));
        assert!(!state.directly_carries_witness(&pointer));
    }

    #[test]
    fn address_expiry_retires_only_bound_clauses_of_a_complete_carrier() {
        for (whole_lifetime, by_root) in [(false, false), (false, true), (true, true)] {
            let carrier = input(20);
            let marker = permission(1);
            let access = permission(2);
            let owner = ExecutionPlace::argument(SymbolOrdinal::new(30));
            let mut state = ExecutionState::default();

            state.current.insert(owner.clone(), input(30));

            let compound = ExecutionCondition::operation(
                bray_bound_tree::BoundOperator::LogicalAnd,
                vec![marker.clone(), access.clone()],
            );

            state.carry_guarantee(carrier.clone(), compound.clone());
            compound.assume(true, &mut state.trusted_assumptions);

            let witness = state
                .witnesses
                .get_mut(&carrier)
                .expect("the complete carrier owns its clauses");

            std::sync::Arc::make_mut(witness)
                .conditions
                .get_mut(&access)
                .expect("the access conjunct is retained")
                .addresses
                .insert(input(2), BTreeSet::from([owner.clone()]));

            if whole_lifetime {
                std::sync::Arc::make_mut(witness)
                    .dependencies
                    .insert(owner.clone());
            }

            if by_root {
                state.invalidate_trusted_roots(&BTreeSet::from([owner.root]));
            } else {
                state.invalidate_trusted(&input(30));
            }

            assert!(!state.trusted_assumptions.contains(&(access, true)));

            assert_eq!(
                state.trusted_assumptions.contains(&(marker.clone(), true)),
                !whole_lifetime
            );

            if !whole_lifetime {
                assert!(state.directly_carries_witness(&carrier));
                state.invalidate_trusted(&carrier);
                assert!(!state.trusted_assumptions.contains(&(marker, true)));
            }
        }
    }

    #[test]
    fn equal_payload_values_do_not_merge_distinct_address_storage_lifetimes() {
        let carrier = input(20);
        let mut state = ExecutionState::default();

        for ordinal in [30, 31] {
            let owner = ExecutionPlace::argument(SymbolOrdinal::new(ordinal));
            let access = permission(ordinal);

            state
                .current
                .insert(owner.clone(), ExecutionCondition::Boolean(false));

            state.carry_guarantee(carrier.clone(), access.clone());
            access.clone().assume(true, &mut state.trusted_assumptions);

            std::sync::Arc::make_mut(
                state
                    .witnesses
                    .get_mut(&carrier)
                    .expect("one carrier retains both addresses"),
            )
            .conditions
            .get_mut(&access)
            .expect("each access has its own clause")
            .addresses
            .insert(input(ordinal), BTreeSet::from([owner]));
        }

        state.invalidate_trusted_roots(&BTreeSet::from([ExecutionPlace::argument(
            SymbolOrdinal::new(30),
        )
        .root]));

        state.invalidate_trusted(&ExecutionCondition::Boolean(false));
        assert!(!state.trusted_assumptions.contains(&(permission(30), true)));
        assert!(state.trusted_assumptions.contains(&(permission(31), true)));
    }

    #[test]
    fn ordinary_nullable_guards_do_not_share_sibling_address_bounds() {
        let root = input(0);

        let payload = root
            .clone()
            .component(ConstantProjectionKind::NullableValue);

        let marker = payload
            .clone()
            .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0)));

        let address =
            payload.component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1)));

        let absent = ExecutionCondition::Literal(Arc::new(bray_symbols::ConstantValueData::new(
            crate::test_support::error_type(),
            bray_symbols::ConstantValueKind::NullableAbsent,
        )));

        let guard = ExecutionCondition::operation(
            bray_bound_tree::BoundOperator::Equal,
            vec![root, absent],
        );

        let guarded = |claim| {
            ExecutionCondition::operation(
                bray_bound_tree::BoundOperator::LogicalOr,
                vec![guard.clone(), claim],
            )
        };

        let marker = guarded(permission(1).with_equalities(&[(input(1), marker)].into()));
        let access = guarded(permission(2).with_equalities(&[(input(2), address.clone())].into()));

        let condition = ExecutionCondition::operation(
            bray_bound_tree::BoundOperator::LogicalAnd,
            vec![marker.clone(), access.clone()],
        );

        let owners = BTreeSet::from([ExecutionPlace::argument(SymbolOrdinal::new(30))]);

        let mut witness =
            std::sync::Arc::new(crate::execution_guarantees::ExecutionWitness::default());

        witness.carry(
            &condition,
            true,
            &[(address.clone(), owners.clone())].into(),
        );

        assert!(witness.conditions[&marker].addresses.is_empty());

        assert_eq!(
            witness.conditions[&access].addresses.get(&address),
            Some(&owners)
        );
    }

    #[test]
    fn referent_writes_preserve_nullable_owner_guards_and_retire_access_claims() {
        for stable_descriptor in [false, true] {
        let carrier = input(0);
        let payload = carrier.clone().component(ConstantProjectionKind::NullableValue);
        let marker = payload.clone().component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0)));
        let pointer = payload.component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1)));

        let absent = ExecutionCondition::Literal(Arc::new(bray_symbols::ConstantValueData::new(
            crate::test_support::error_type(),
            bray_symbols::ConstantValueKind::NullableAbsent,
        )));

        let guard = ExecutionCondition::operation(
            bray_bound_tree::BoundOperator::Equal,
            vec![carrier.clone(), absent],
        );

        let guarded = |subject| ExecutionCondition::operation(
            bray_bound_tree::BoundOperator::LogicalOr,
            vec![guard.clone(), permission(1).with_equalities(&[(input(1), subject)].into())],
        );

        let ownership = guarded(ExecutionCondition::Borrowed(Arc::new(marker)));
        let access = guarded(pointer.clone());
        let mut state = ExecutionState::default();

        for condition in [&ownership, &access] {
            condition.clone().assume(true, &mut state.trusted_assumptions);
            state.carry_guarantee(carrier.clone(), condition.clone());
        }

        let addresses = if stable_descriptor { BTreeSet::from([pointer.owner_identity().clone()]) } else { BTreeSet::new() };
        let stable_values = if stable_descriptor { [pointer.clone()].into_iter().collect() } else { Default::default() };
        state.invalidate_referents(
            &[pointer],
            &addresses,
            &BTreeSet::new(),
            &BTreeSet::new(),
            &stable_values,
            None,
        );

        assert!(state.trusted_assumptions.contains(&(ownership.clone(), true)));
        assert!(state.witnesses[&carrier].conditions.contains_key(&ownership));
        assert!(!state.trusted_assumptions.contains(&(access.clone(), true)));
        assert!(!state.witnesses[&carrier].conditions.contains_key(&access));

        state.invalidate_contents(&BTreeSet::new(), [carrier], &BTreeSet::new(),
                &Default::default(), None);

        assert!(!state.trusted_assumptions.contains(&(ownership, true)));
        }
    }

    #[test]
    fn guarded_refinement_preserves_only_the_derived_clauses_address_dependencies() {
        for select_marker in [false, true] {
            let carrier = input(20);
            let marker = permission(1);
            let access = permission(2);
            let flag = input(10);
            let owner = ExecutionPlace::argument(SymbolOrdinal::new(30));

            let guarded = ExecutionCondition::operation(
                bray_bound_tree::BoundOperator::LogicalOr,
                vec![
                    ExecutionCondition::operation(
                        bray_bound_tree::BoundOperator::LogicalAnd,
                        vec![flag.clone(), marker.clone()],
                    ),
                    ExecutionCondition::operation(
                        bray_bound_tree::BoundOperator::LogicalAnd,
                        vec![
                            ExecutionCondition::operation(
                                bray_bound_tree::BoundOperator::LogicalNot,
                                vec![flag.clone()],
                            ),
                            access.clone(),
                        ],
                    ),
                ],
            );

            let mut state = ExecutionState::default();

            state.carry_guarantee(carrier.clone(), guarded.clone());
            guarded.clone().assume(true, &mut state.trusted_assumptions);

            std::sync::Arc::make_mut(
                state
                    .witnesses
                    .get_mut(&carrier)
                    .expect("the conditional has one complete carrier"),
            )
            .conditions
            .get_mut(&guarded)
            .expect("the guarded clause is retained")
            .addresses
            .insert(input(2), BTreeSet::from([owner.clone()]));

            state.assumptions.insert((flag, select_marker));
            state.refine_witnesses();

            assert_eq!(
                state.trusted_assumptions.contains(&(marker.clone(), true)),
                select_marker
            );

            assert_eq!(
                state.trusted_assumptions.contains(&(access.clone(), true)),
                !select_marker
            );

            state.invalidate_trusted_roots(&BTreeSet::from([owner.root]));

            assert_eq!(
                state.trusted_assumptions.contains(&(marker, true)),
                select_marker
            );

            assert!(!state.trusted_assumptions.contains(&(access, true)));
        }
    }

    #[test]
    fn guarded_refinement_never_promotes_unproved_carrier_obligations() {
        for guard_holds in [false, true] {
            let predicate = permission(2);
            let guard = input(3);

            let conditional = ExecutionCondition::operation(
                bray_bound_tree::BoundOperator::LogicalOr,
                vec![guard.clone(), predicate.clone()],
            );

            let mut state = ExecutionState::default();

            state.carry_guarantee(input(0), predicate.clone());
            state.carry_guarantee(input(0), conditional.clone());
            conditional.assume(true, &mut state.trusted_assumptions);
            state.assumptions.insert((guard, guard_holds));
            state.refine_witnesses();

            assert_eq!(
                state.trusted_assumptions.contains(&(predicate, true)),
                !guard_holds
            );
        }
    }

    #[test]
    fn discharging_a_guard_never_borrows_authority_from_another_carrier() {
        let symbol = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>(
            "ValidWrite",
        );

        let substitution = crate::test_support::empty_substitution(symbol.into());

        let predicate = ExecutionCondition::Trusted(Arc::new(ExecutionCondition::Predicate(
            symbol.into(),
            substitution,
            [input(2).borrowed(), ExecutionCondition::Boolean(true)].into(),
        )));

        let guard = input(3);

        let conditional = ExecutionCondition::operation(
            bray_bound_tree::BoundOperator::LogicalOr,
            vec![guard.clone(), predicate.clone()],
        );

        let mut state = ExecutionState::default();

        state.carry_guarantee(input(0), conditional.clone());
        state.carry_guarantee(input(1), predicate.clone());
        conditional.assume(true, &mut state.trusted_assumptions);

        predicate
            .clone()
            .assume(true, &mut state.trusted_assumptions);

        state.assumptions.insert((guard.clone(), true));

        state.refine_witnesses();

        assert!(
            !state.witnesses[&input(0)]
                .conditions
                .contains_key(&predicate)
        );

        assert!(
            state.witnesses[&input(1)]
                .conditions
                .contains_key(&predicate)
        );

        state.assumptions = [(guard, false)].into();
        state.refine_witnesses();

        assert!(
            state.witnesses[&input(0)]
                .conditions
                .contains_key(&predicate)
        );
    }

    #[test]
    fn repeated_cleanup_does_not_revive_old_evidence_or_retire_its_sibling() {
        use crate::test_support::{error_type, expression_unit, push_block, push_expression};

        use bray_bound_tree::{
            BoundErrorExpression, BoundExpression, BoundUnitId, BoundUnitKind, StorageAccess,
            StorageAccessRoot, StorageIdentity, StorageMutation, StoragePlanBuilder,
        };

        use bray_symbols::TypeAssociatedLifecycleSlot;

        let unit = BoundUnitId::new(1);
        let mut boundary = None;

        let (bound, expressions) = expression_unit(unit, |tree, origin| {
            boundary = Some(push_block(tree, origin, []));

            vec![push_expression(
                tree,
                BoundExpression::Error(BoundErrorExpression::new(origin, error_type())),
            )]
        });

        let occurrence =
            bray_bound_tree::BoundExecutionSite::Node(boundary.expect("test boundary").into());

        let mut storage = StoragePlanBuilder::new(unit, BoundUnitKind::CallableBody);

        let root = storage
            .push_identity(StorageIdentity::Temporary(expressions[0]))
            .expect("fixture identity must fit its identity space");

        let source = bound
            .tree()
            .expression(expressions[0])
            .expect("fixture expression must exist in its bound unit")
            .origin()
            .source_anchor();

        let access = storage
            .push_access(StorageAccess::new(
                StorageAccessRoot::Storage(root),
                [],
                error_type(),
                source,
                false,
            ))
            .expect("fixture access must fit its identity space");

        let mutation = |part| StorageMutation::Lifecycle {
            occurrence,
            access,
            part,
            slot: TypeAssociatedLifecycleSlot::Destructor,
        };

        let place = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let old = ExecutionCondition::PostState(mutation(0), place.clone());
        let sibling = ExecutionCondition::PostState(mutation(1), place.clone());
        let mut state = ExecutionState::default();

        state.carry_guarantee(old.clone(), old.clone());
        state.carry_guarantee(sibling.clone(), sibling.clone());

        state
            .trusted_assumptions
            .extend([(old.clone(), true), (sibling.clone(), true)]);

        let marker = bray_bound_tree::StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let payload = bray_bound_tree::StorageProjection::TupleElement(SymbolOrdinal::new(1));
        let unchanged = input(1);

        state.current.insert(
            place.clone(),
            ExecutionCondition::constructed(old.clone(),
                [(marker, unchanged.clone()), (payload, old.clone())].into(),
            ),
        );

        state.current.insert(
            place.clone().project(&[marker]).expect("tuple projection"),
            unchanged.clone(),
        );

        state.current.insert(
            place.clone().project(&[payload]).expect("tuple projection"),
            old.clone(),
        );

        state.entries.insert(
            occurrence,
            crate::ExecutionCallEvidence {
                implementation_witnesses: Default::default(),
                witness_inputs: Default::default(),
                copy_sources: Default::default(),
                owned_inputs: Default::default(),
                addresses: Default::default(),
                anonymous_target: None,
                trusted_boundary: true,
                pending_execution: true,
                arguments: [(place.clone(), old.clone())].into(),
                assumptions: [(old.clone(), true)].into(),
                trusted_assumptions: [(old.clone(), true)].into(),
            },
        );

        state.retire_evaluation(mutation(0));

        let current = place
            .value_in(&state.current)
            .expect("retired value retains known components");

        assert_eq!(current.clone().project(marker), unchanged);
        assert_eq!(current.project(payload), ExecutionCondition::Unknown);

        state.assign(place, old.clone(), None);

        assert!(!state.owns_witness(&old));
        assert!(!state.trusted_assumptions.contains(&(old, true)));
        assert!(state.owns_witness(&sibling));
        assert!(state.trusted_assumptions.contains(&(sibling, true)));
        assert!(state.entries[&occurrence].arguments.is_empty());
        assert!(state.entries[&occurrence].trusted_assumptions.is_empty());
    }

    #[test]
    fn content_changes_keep_unchanged_component_guarantees_until_the_owner_ends() {
        let owner = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let carrier = input(0);

        let marker = carrier
            .clone()
            .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0)));

        let payload = carrier
            .clone()
            .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1)));

        let preserved = marker.clone().borrowed();

        let conditions = [
            preserved.clone(),
            payload.borrowed(),
            carrier.clone().borrowed(),
        ];

        let mut state = ExecutionState::default();

        state.current.insert(owner.clone(), carrier.clone());

        for condition in conditions {
            state.carry_guarantee(carrier.clone(), condition.clone());
            state.trusted_assumptions.insert((condition, true));
        }

        state.invalidate_contents(
            &BTreeSet::from([owner]),
            [carrier.clone()],
            &BTreeSet::new(),
                &[marker].into_iter().collect(),
            None,
        );

        assert_eq!(
            state.trusted_assumptions,
            BTreeSet::from([(preserved.clone(), true)])
        );

        assert!(
            state.witnesses[&carrier]
                .conditions
                .contains_key(&preserved)
        );

        state.retire_trusted_values([carrier], &BTreeSet::new());
        assert!(state.trusted_assumptions.is_empty());
    }

    #[test]
    fn content_changes_keep_spatial_evidence_but_never_extend_its_carrier_or_storage() {
        let symbol = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>(
            "ValidWrite",
        );

        let substitution = crate::test_support::empty_substitution(symbol.into());
        let owner = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let carrier = input(1);

        let spatial = ExecutionCondition::Trusted(Arc::new(ExecutionCondition::Predicate(
            symbol.into(),
            substitution,
            [carrier.clone(), ExecutionCondition::Boolean(true)].into(),
        )));

        let observed = input(0).borrowed();
        let mut state = ExecutionState::default();

        state.current.insert(owner.clone(), input(0));

        std::sync::Arc::make_mut(state.witnesses.entry(carrier.clone()).or_default())
            .dependencies = BTreeSet::from([owner.clone()]);

        state.carry_guarantee(carrier.clone(), spatial.clone());
        state.carry_guarantee(carrier.clone(), observed.clone());

        state
            .trusted_assumptions
            .extend([(spatial.clone(), true), (observed, true)]);

        let stable = BTreeSet::from([symbol.into()]);

        state.invalidate_contents(
            &BTreeSet::from([owner.clone()]),
            [input(0)],
            &stable,
                &Default::default(),
            None,
        );

        assert_eq!(
            state.trusted_assumptions,
            BTreeSet::from([(spatial.clone(), true)])
        );

        assert!(!state.witnesses[&carrier].dependencies.is_empty());

        let mut ended = state.clone();

        ended.invalidate_trusted_roots(&BTreeSet::from([owner.root]));
        assert!(ended.trusted_assumptions.is_empty());

        assert!(
            ended
                .witnesses
                .values()
                .all(|witness| witness.dependencies.is_empty())
        );

        // A carrier can establish a condition about a separate pointer. Overwriting
        // that carrier must revoke the condition even though the pointer is unchanged.
        let external = input(2);

        state.witnesses.clear();
        state.carry_guarantee(external.clone(), spatial.clone());

        state.invalidate_contents(&BTreeSet::new(), [external], &stable,
                &Default::default(), None);

        assert!(state.trusted_assumptions.is_empty());

        assert!(
            state
                .witnesses
                .values()
                .all(|witness| witness.conditions.is_empty())
        );

        let mut overwritten = ExecutionState::default();

        overwritten.carry_guarantee(carrier.clone(), spatial.clone());
        overwritten.trusted_assumptions.insert((spatial, true));

        overwritten.invalidate_contents(&BTreeSet::new(), [carrier], &stable,
                &Default::default(), None);

        assert!(overwritten.trusted_assumptions.is_empty());
    }

    #[test]
    fn content_changes_retain_intermediate_lifetime_dependencies_without_old_guarantees() {
        let symbol = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>(
            "ValidWrite",
        );

        let substitution = crate::test_support::empty_substitution(symbol.into());
        let owner = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let intermediate = ExecutionPlace::argument(SymbolOrdinal::new(1));
        let carrier = input(2);

        let spatial = ExecutionCondition::Trusted(Arc::new(ExecutionCondition::Predicate(
            symbol.into(),
            substitution,
            [carrier.clone(), ExecutionCondition::Boolean(true)].into(),
        )));

        let mut state = ExecutionState::default();

        state.current.insert(owner.clone(), input(0));
        state.current.insert(intermediate.clone(), input(1));

        std::sync::Arc::make_mut(state.witnesses.entry(input(1)).or_default()).dependencies =
            BTreeSet::from([owner.clone()]);

        std::sync::Arc::make_mut(state.witnesses.entry(carrier.clone()).or_default())
            .dependencies = BTreeSet::from([intermediate]);

        state.carry_guarantee(carrier, spatial.clone());
        state.trusted_assumptions.insert((spatial.clone(), true));

        state.invalidate_contents(
            &BTreeSet::from([owner.clone()]),
            [input(0)],
            &BTreeSet::from([symbol.into()]),
                &Default::default(),
            None,
        );

        assert_eq!(state.trusted_assumptions, BTreeSet::from([(spatial, true)]));
        assert!(!state.witnesses[&input(1)].dependencies.is_empty());

        state.invalidate_trusted_roots(&BTreeSet::from([owner.root]));
        assert!(state.trusted_assumptions.is_empty());

        assert!(
            state
                .witnesses
                .values()
                .all(|witness| witness.dependencies.is_empty())
        );
    }
    #[test]
    fn retiring_a_container_preserves_only_its_transferred_child_value() {
        let root = ExecutionPlace::argument(SymbolOrdinal::new(0));

        let left = root
            .clone()
            .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0)));

        let right = root
            .clone()
            .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1)));

        let child = ExecutionCondition::Input(left);
        let mut state = ExecutionState::default();

        for value in [
            ExecutionCondition::Input(root.clone()),
            child.clone(),
            ExecutionCondition::Input(right),
        ] {
            state.carry_guarantee(value.clone(), value.clone());
            state.trusted_assumptions.insert((value, true));
        }

        let retained = BTreeSet::from([child.clone()]);
        let mut moved = state.clone();

        moved.retire_trusted_values([ExecutionCondition::Input(root.clone())], &retained);

        assert_eq!(
            moved
                .witnesses
                .iter()
                .filter(|(_, witness)| !witness.conditions.is_empty())
                .map(|(carrier, _)| carrier)
                .cloned()
                .collect::<BTreeSet<_>>(),
            retained
        );

        assert_eq!(
            moved.trusted_assumptions,
            BTreeSet::from([(child.clone(), true)])
        );

        // Retaining a value cannot extend the storage required by its witness.
        std::sync::Arc::make_mut(state.witnesses.entry(child).or_default()).dependencies =
            BTreeSet::from([root.clone()]);

        state.invalidate_trusted_roots(&BTreeSet::from([root.root.clone()]));
        state.retire_trusted_values([ExecutionCondition::Input(root)], &retained);

        assert!(
            state
                .witnesses
                .values()
                .all(|witness| witness.conditions.is_empty())
        );

        assert!(state.trusted_assumptions.is_empty());
    }

    #[test]
    fn invalidation_follows_long_chains_and_cycles_without_retiring_independent_witnesses() {
        let mut state = ExecutionState::default();

        for ordinal in 1..=512 {
            let dependency = ExecutionPlace::argument(SymbolOrdinal::new(ordinal));
            let carrier = input(ordinal + 1);

            state.current.insert(dependency.clone(), input(ordinal));

            std::sync::Arc::make_mut(state.witnesses.entry(carrier.clone()).or_default())
                .dependencies = BTreeSet::from([dependency]);

            state.carry_guarantee(carrier.clone(), carrier.clone());
            state.trusted_assumptions.insert((carrier, true));
        }

        let cycle = ExecutionPlace::argument(SymbolOrdinal::new(514));
        let independent = input(1000);

        state.current.insert(cycle.clone(), input(513));

        std::sync::Arc::make_mut(state.witnesses.entry(input(1)).or_default()).dependencies =
            BTreeSet::from([cycle]);

        state.carry_guarantee(independent.clone(), independent.clone());

        state
            .trusted_assumptions
            .insert((independent.clone(), true));

        for batch in 0..3 {
            let mut state = state.clone();

            if batch == 1 {
                state.invalidate_trusted_roots(&BTreeSet::from([
                    ExecutionPlace::argument(SymbolOrdinal::new(1)).root,
                    ExecutionPlace::argument(SymbolOrdinal::new(256)).root,
                ]));
            } else if batch == 2 {
                state.invalidate_contents(
                    &BTreeSet::from([
                        ExecutionPlace::argument(SymbolOrdinal::new(1)),
                        ExecutionPlace::argument(SymbolOrdinal::new(256)),
                    ]),
                    [input(1), input(256)],
                    &BTreeSet::new(),
                &Default::default(),
                    None,
                );

                assert!(state.trusted_assumptions.contains(&(input(2), true)));
                assert!(!state.trusted_assumptions.contains(&(input(256), true)));

                assert_eq!(
                    state
                        .witnesses
                        .values()
                        .filter(|witness| !witness.dependencies.is_empty())
                        .count(),
                    513
                );

                state.invalidate_trusted_roots(&BTreeSet::from([ExecutionPlace::argument(
                    SymbolOrdinal::new(1),
                )
                .root]));
            } else {
                state.invalidate_trusted(&input(1));
            }

            assert_eq!(
                state
                    .witnesses
                    .iter()
                    .filter(|(_, witness)| !witness.conditions.is_empty())
                    .map(|(carrier, _)| carrier)
                    .cloned()
                    .collect::<BTreeSet<_>>(),
                BTreeSet::from([independent.clone()])
            );

            assert_eq!(
                state.trusted_assumptions,
                BTreeSet::from([(independent.clone(), true)])
            );

            assert!(
                state
                    .witnesses
                    .values()
                    .all(|witness| witness.dependencies.is_empty())
            );
        }
    }

    #[test]
    fn invalidation_overlaps_parent_and_child_paths_but_preserves_siblings_and_snapshots() {
        let root = ExecutionPlace::argument(SymbolOrdinal::new(0));

        let left = root
            .clone()
            .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0)));

        let child = left
            .clone()
            .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0)));

        let right = root
            .clone()
            .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1)));

        let changed = ExecutionCondition::Input(left);

        let captured = ExecutionCondition::Entry {
            condition: Arc::new(changed.clone()),
            captured: true,
        };

        let live = ExecutionCondition::Entry {
            condition: Arc::new(changed.clone()),
            captured: false,
        };

        let mut state = ExecutionState::default();

        for (place, ordinal) in [root, child, right].into_iter().zip(0u32..) {
            let dependency = ExecutionPlace::argument(SymbolOrdinal::new(ordinal + 10));
            let carrier = input(ordinal + 20);

            state
                .current
                .insert(dependency.clone(), ExecutionCondition::Input(place));

            std::sync::Arc::make_mut(state.witnesses.entry(carrier.clone()).or_default())
                .dependencies = BTreeSet::from([dependency]);

            state.carry_guarantee(carrier.clone(), carrier.clone());
            state.trusted_assumptions.insert((carrier, true));
        }

        state.assumptions.insert((changed.clone(), true));
        state.assumptions.insert((changed.clone().borrowed(), true));
        state.trusted_assumptions.insert((captured.clone(), true));
        state.trusted_assumptions.insert((live, true));
        state.invalidate_trusted(&changed);

        assert_eq!(
            state
                .witnesses
                .iter()
                .filter(|(_, witness)| !witness.conditions.is_empty())
                .map(|(carrier, _)| carrier)
                .cloned()
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([input(22)])
        );

        assert_eq!(
            state.trusted_assumptions,
            BTreeSet::from([(captured, true), (input(22), true)])
        );

        assert_eq!(state.assumptions, BTreeSet::from([(changed, true)]));

        assert_eq!(
            state
                .witnesses
                .values()
                .filter(|witness| !witness.dependencies.is_empty())
                .count(),
            1
        );
    }

    #[test]
    fn witness_lookup_preserves_component_overlap_and_entry_snapshots() {
        let root = ExecutionPlace::argument(SymbolOrdinal::new(0));

        let child = ExecutionCondition::Input(
            root.clone()
                .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0))),
        );

        let sibling = ExecutionCondition::Input(
            root.clone()
                .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1))),
        );

        let parent = ExecutionCondition::Input(root);
        let mut state = ExecutionState::default();

        for value in (1..=512).map(input) {
            state.carry_guarantee(value.clone(), value);
        }

        state.carry_guarantee(child.clone(), parent.clone());
        state.trusted_assumptions.insert((parent.clone(), true));

        assert!(state.owns_witness(&child));
        assert!(!state.owns_witness(&sibling));

        state.assumptions.insert((parent.clone(), true));

        assert!(!state.owns_witness(&child));

        state.assumptions.clear();

        state.trusted_assumptions = BTreeSet::from([(
            ExecutionCondition::Entry {
                condition: Arc::new(parent.clone()),
                captured: true,
            },
            true,
        )]);

        assert!(!state.owns_witness(&child));

        let live = ExecutionCondition::Entry {
            condition: Arc::new(parent),
            captured: false,
        };

        for witness in state.witnesses.values_mut() {
            std::sync::Arc::make_mut(witness).conditions.clear();
        }

        state.carry_guarantee(child.clone(), live.clone());
        state.trusted_assumptions = BTreeSet::from([(live, true)]);

        assert!(state.owns_witness(&child));
    }
    #[test]
    fn retiring_a_carrier_revokes_its_unobserved_guarantee_but_keeps_moved_children() {
        let child = input(1);

        let owner = ExecutionCondition::constructed(input(0),
            [(
                bray_bound_tree::StorageProjection::TupleElement(SymbolOrdinal::new(0)),
                child.clone(),
            )]
            .into(),
        );

        let owner_condition = input(20);
        let child_condition = input(21);
        let mut state = ExecutionState::default();

        state.carry_guarantee(owner.clone(), owner_condition.clone());
        state.carry_guarantee(child.clone(), child_condition.clone());

        state.trusted_assumptions.extend([
            (owner_condition.clone(), true),
            (child_condition.clone(), true),
        ]);

        state.retire_trusted_values([owner.clone()], &BTreeSet::from([child.clone()]));

        assert!(
            state
                .witnesses
                .get(&owner)
                .is_none_or(|witness| witness.conditions.is_empty())
        );

        assert!(state.owns_witness(&child));
        assert!(!state.trusted_assumptions.contains(&(owner_condition, true)));
        assert!(state.trusted_assumptions.contains(&(child_condition, true)));
    }
    #[test]
    fn retiring_a_temporary_loan_preserves_only_unchanged_referent_facts() {
        let owner = input(0);
        let field = bray_bound_tree::StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let marker = owner.clone().project(field);
        let view = |contents| ExecutionCondition::reference_value(owner.clone().borrowed(),
            ExecutionCondition::constructed(input(2), [(field, contents)].into()));
        let before = view(marker.clone().borrowed());
        let temporary = ExecutionCondition::constructed(input(1), [(field, before)].into());
        let symbol = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>("ValidRead");
        let condition = ExecutionCondition::Trusted(Arc::new(ExecutionCondition::Predicate(
            symbol.into(), crate::test_support::empty_substitution(symbol.into()), [marker.clone().borrowed()].into(),
        )));

        for noise in [0, 256] {
            for unchanged in [true, false] {
                let mut retained = BTreeSet::from([view(if unchanged { marker.clone().borrowed() } else { input(3).borrowed() })]);
                retained.extend((100..100 + noise).map(|ordinal| {
                    let owner = input(ordinal);

                    ExecutionCondition::reference_value(owner.clone().borrowed(), owner)
                }));
                let mut state = ExecutionState::default();

                state.carry_guarantee(owner.clone().borrowed(), condition.clone());
                state.trusted_assumptions.insert((condition.clone(), true));
                state.retire_trusted_values([temporary.clone()], &retained);

                assert_eq!(state.trusted_assumptions.contains(&(condition.clone(), true)), unchanged);
                assert!(!crate::execution_guarantees::ObservationSet::retains_owner(&retained, &marker));
            }
        }
    }

}
