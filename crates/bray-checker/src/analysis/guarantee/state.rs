use crate::execution_guarantees::{ExecutionAssumptions, ExecutionSubstitution};
use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::BoundExpressionId;

use crate::ExecutionCondition;
use crate::execution_guarantees::ExecutionProof;
use crate::analysis::outcomes::{CleanupProgress, OutcomeState};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ExecutionState {
    pub(super) proofs: std::sync::Arc<crate::execution_guarantees::ExecutionProofCache>,
    pub(super) trust_boundaries: BTreeSet<BoundExpressionId>,
    pub(super) assumptions: ExecutionAssumptions,
    pub(super) witnesses:
        BTreeMap<ExecutionCondition, std::sync::Arc<crate::execution_guarantees::ExecutionWitness>>,
    pub(super) trusted_assumptions: ExecutionAssumptions,
    pub(super) allocation_owners: BTreeMap<ExecutionCondition, ExecutionCondition>,
    pub(super) current: super::current::CurrentValues,
    pub(super) expressions: BTreeMap<BoundExpressionId, ExecutionCondition>,
    pub(super) pending_results: BTreeMap<BoundExpressionId, ExecutionCondition>,
    pub(super) owned_results: super::ownership::OwnedResults,
    pub(super) entries: BTreeMap<bray_bound_tree::BoundExecutionSite, std::sync::Arc<crate::ExecutionCallEvidence>>,
    // The input operation's evaluated check, consumed by its admission edges.
    pub(super) entry_check: Option<(bray_bound_tree::BoundExecutionSite, ExecutionCondition)>,
    pub(super) cleanup: Option<CleanupProgress>,
    // Replay retains the already-admitted attempt through the suspension edge.
    pub(super) admitted_failure: Option<(bray_bound_tree::BoundExecutionSite, std::sync::Arc<Self>)>,
    pub(super) completion_dependencies: BTreeSet<crate::ExecutionCompletionDependency>,
}

impl Default for ExecutionState {
    fn default() -> Self {
        Self {
            proofs: Default::default(),
            trust_boundaries: BTreeSet::new(),
            assumptions: Default::default(),
            witnesses: BTreeMap::new(),
            trusted_assumptions: Default::default(),
            allocation_owners: BTreeMap::new(),
            current: super::current::CurrentValues::default(),
            expressions: BTreeMap::new(),
            pending_results: BTreeMap::new(),
            owned_results: super::ownership::OwnedResults::default(),
            entries: BTreeMap::new(),
            entry_check: None,
            cleanup: None,
            admitted_failure: None,
            completion_dependencies: BTreeSet::new(),
        }
    }
}

impl OutcomeState for ExecutionState {
    fn cleanup_progress(&self) -> Option<&CleanupProgress> {
        self.cleanup.as_ref()
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
    pub(in crate::analysis) fn trace_cycle_delta(&self, next: &Self) {
        let size = |facts: &ExecutionAssumptions| {
            (facts.len(), facts.iter().map(|(condition, _)| condition.observations().take(4096).count()).max().unwrap_or(0))
        };

        eprintln!("cycle proof delta: current={} expressions={} results={} entries={} ownership={} witnesses={} ordinary={:?}->{:?} trusted={:?}->{:?}",
            self.current != next.current, self.expressions != next.expressions,
            self.pending_results != next.pending_results, self.entries != next.entries,
            self.owned_results != next.owned_results, self.witnesses != next.witnesses,
            size(&self.assumptions), size(&next.assumptions), size(&self.trusted_assumptions), size(&next.trusted_assumptions));
    }

    pub(in crate::analysis) fn ordinary_proof(&self) -> ExecutionProof<'_> {
        self.proofs.ordinary_proof(&self.assumptions)
    }

    /// The admitted invocation shared with the physical transition producer.
    pub(in crate::analysis) fn admitted_receiver(&self, invocation: bray_bound_tree::BoundExecutionSite) -> ExecutionCondition {
        self.entries.get(&invocation)
            .and_then(|entry| crate::ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0)).value_in(&entry.arguments))
            .unwrap_or_else(ExecutionCondition::unknown)
    }

    pub(in crate::analysis) fn admitted_call(&self, invocation: bray_bound_tree::BoundExecutionSite)
        -> Option<&crate::ExecutionCallEvidence>
    {
        self.entries.get(&invocation).map(std::sync::Arc::as_ref)
    }

    pub(super) fn admitted_call_mut(&mut self, invocation: bray_bound_tree::BoundExecutionSite)
        -> Option<&mut crate::ExecutionCallEvidence>
    {
        self.entries.get_mut(&invocation).map(std::sync::Arc::make_mut)
    }

    pub(super) fn extracted_input_snapshots(&self, mutation: bray_bound_tree::StorageMutation,
        frame: Option<&crate::ExecutionCallEvidence>,
    )
        -> crate::execution_guarantees::RetainedSnapshots
    {
        self.admitted_call(mutation.occurrence()).into_iter().chain(frame)
            .flat_map(|entry| entry.extracted_values.values().cloned()).collect()
    }

    pub(super) fn admitted_extraction(
        &self,
        invocation: bray_bound_tree::BoundExecutionSite,
        producer: Option<&bray_bound_tree::StorageDeferredProducer>,
        term: bray_symbols::ConstantTermId,
    ) -> Option<&ExecutionCondition> {
        let entry = self.admitted_call(invocation)?;

        let entry = if let Some(producer) = producer {
            let frame = entry.arguments.get(&crate::ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0)))?
                .deferred_value()?;

            &frame.producers.get(producer)?.entry
        } else { entry };

        entry.extracted_values.get(&term)
    }

    /// Admission reads the current referent of the retained frame descriptor.
    pub(super) fn capture_frame_extractions(
        &self,
        entry: &mut crate::ExecutionCallEvidence,
        values: BTreeMap<bray_symbols::ConstantTermId, ExecutionCondition>,
    ) {
        if !entry.extracted_values.is_empty() {
            let owned = self.witness_snapshots(entry.owned_arguments());

            entry.witness_inputs.retain(|owner, _| owned.retains_owner(owner));
            entry.witness_allocations.retain(|_, owner| entry.witness_inputs.contains_key(owner));
        }

        entry.extracted_values = values;

        let reached = entry.extracted_values.values().cloned();
        let witnesses = self.owned_witnesses(reached.clone()).filter(|(_, witness)| witness.available)
            .map(|(owner, witness)| (owner.clone(), std::sync::Arc::clone(witness))).collect::<BTreeMap<_, _>>();

        entry.witness_allocations.extend(self.allocation_owners.iter().filter(|(_, owner)| witnesses.contains_key(owner))
            .map(|(condition, owner)| (condition.clone(), owner.clone())));
        entry.witness_inputs.extend(witnesses);
        entry.trusted_assumptions.extend(self.call_trusted_assumptions(reached).iter().cloned());
    }

    pub(super) fn proof(&self) -> ExecutionProof<'_> {
        self.proofs.proof(&self.trusted_assumptions, &self.assumptions)
    }

    pub(super) fn suspend_admitted_witnesses(
        &mut self,
        invocation: bray_bound_tree::BoundExecutionSite,
    ) {
        let Some(entry) = self.entries.get(&invocation).filter(|entry| !entry.witness_inputs.is_empty()) else {
            return;
        };

        let carriers = entry.witness_inputs.keys().cloned().collect();

        self.suspend_owned_witnesses(&carriers);
    }

    fn admitted_private_witnesses(
        &self,
        invocation: Option<bray_bound_tree::BoundExecutionSite>,
    ) -> Vec<std::sync::Arc<crate::execution_guarantees::ExecutionWitness>> {
        let Some(invocation) = invocation.filter(|invocation| self.entries.get(invocation)
            .is_some_and(|entry| !entry.pending_execution)) else { return Vec::new(); };

        let entry = &self.entries[&invocation];
        let proof = self.ordinary_proof();
        let mut witnesses = entry.witness_inputs.values().cloned().collect::<Vec<_>>();
        let mut seen = std::collections::HashSet::new();

        for value in entry.witness_sources().flat_map(|value| self.witness_values(&value)) {
            if !matches!(value.ownership_identity(), Some((crate::execution_guarantees::ExecutionObservationKind::Owned, _))) {
                continue;
            }

            if let Some(frame) = value.deferred_value()
                && seen.insert(std::sync::Arc::as_ptr(frame))
            {
                witnesses.extend(frame.producers.values()
                    .filter(|candidate| super::deferred::possible(&candidate.selection, &proof))
                    .flat_map(|candidate| candidate.entry.witness_inputs.values().cloned()));
            }
        }

        witnesses
    }

    /// Admitted execution takes support out of the public ledger.
    pub(super) fn suspend_owned_witnesses(&mut self, carriers: &BTreeSet<ExecutionCondition>) {
        if carriers.is_empty() { return; }

        let mut retired = BTreeSet::new();

        for carrier in carriers {
            let Some(witness) = self.witnesses.remove(carrier) else { continue; };

            for condition in witness.conditions.keys() {
                condition.clone().assume(true, &mut retired);
            }
        }

        self.allocation_owners.retain(|_, owner| !carriers.contains(owner));
        self.exclude_live_witnesses(&mut retired);

        if !retired.is_empty() {
            self.trusted_assumptions.retain(|fact| !retired.contains(fact));
        }
    }

    /// Applies one shared rewrite to current values, private results and admitted evidence.
    pub(super) fn update_stored_values(&mut self, mut update: impl FnMut(&ExecutionCondition) -> ExecutionCondition) {
        self.current.update_values(&mut update);

        Self::update_evaluated_values(
            self.expressions.values_mut().chain(self.pending_results.values_mut()), &mut self.entries, update,
        );
    }

    fn update_evaluated_values<'a>(
        values: impl Iterator<Item = &'a mut ExecutionCondition>,
        entries: &mut BTreeMap<bray_bound_tree::BoundExecutionSite, std::sync::Arc<crate::ExecutionCallEvidence>>,
        mut update: impl FnMut(&ExecutionCondition) -> ExecutionCondition,
    ) {
        for value in values {
            *value = update(value);
        }

        for entry in entries.values_mut() {
            if entry.conditions().any(|value| !std::ptr::eq(update(value).kind(), value.kind())) {
                *entry = std::sync::Arc::new(entry.try_map_conditions(|value| Ok::<_, std::convert::Infallible>(update(value)))
                    .expect("updating carried frame records is infallible"));
            }
        }
    }

    /// Replace the actual shared frame record, without searching by a reconstructed owner key.
    pub(in crate::analysis) fn replace_deferred_value(&mut self, frame: &std::sync::Arc<crate::execution_guarantees::ExecutionDeferredValue>,
        replacement: Option<std::sync::Arc<crate::execution_guarantees::ExecutionDeferredValue>>,
    ) {
        self.update_stored_values(ExecutionCondition::mapping_frames(|_| true, |original, mapped| {
            if std::sync::Arc::ptr_eq(original, frame) { replacement.clone() }
            else { Some(std::sync::Arc::clone(mapped)) }
        }));
    }

    /// Update admitted snapshots only when the transition invalidates their captured context.
    #[track_caller]
    pub(super) fn update_entry_snapshots(&mut self,
        changed: &impl Fn(&crate::ExecutionCallEvidence) -> bool,
        change: &mut impl FnMut(&mut crate::ExecutionCallEvidence),
    ) {
        for entry in self.entries.values_mut().filter(|entry| changed(entry)) { change(std::sync::Arc::make_mut(entry)); }

        let caller = std::panic::Location::caller();
        let trace = std::env::var_os("BRA_694_TRACE_FRAME").is_some();

        self.update_stored_values(ExecutionCondition::mapping_frames(|_| true, |_, frame| {
            if !frame.producers.values().any(|producer| changed(&producer.entry)) { return Some(std::sync::Arc::clone(frame)); }

            // Only affected immutable frame records detach from their predecessor's shared snapshot.
            let mut updated = frame.as_ref().clone();

            for (source, producer) in updated.producers.iter_mut().filter(|(_, producer)| changed(&producer.entry)) {
                let entry = std::sync::Arc::make_mut(&mut producer.entry);
                let before = entry.trusted_assumptions.len();

                let trace = trace && source.local().and_then(bray_bound_tree::BoundExecutionSite::expression)
                    .is_some_and(|expression| expression.unit().raw() == 18981 && expression.ordinal() == 21);

                let facts = trace.then(|| entry.trusted_assumptions.clone());

                change(entry);

                if before != entry.trusted_assumptions.len() && let Some(facts) = facts
                {
                    let lost = facts.difference(&entry.trusted_assumptions).map(|(value, holds)| (*holds,
                        format!("{:?}", value.kind()).chars().take(270).collect::<String>())).collect::<Vec<_>>();

                    let carriers = entry.witness_inputs.iter().map(|(carrier, witness)| {
                        let record = witness.conditions.keys().map(|condition| {
                            format!("{:?}", condition.kind()).chars().take(260).collect::<String>()
                        }).collect::<Vec<_>>();

                        (format!("{:?}", carrier.kind()).chars().take(130).collect::<String>(), record)
                    }).collect::<Vec<_>>();

                    eprintln!("frame entry changed {source:?} by {caller}: lost={lost:?} carriers={carriers:?}");
                }
            }

            Some(std::sync::Arc::new(updated))
        }));
    }

    /// Carries admitted proof into a nested frame only through its complete owned captures.
    pub(super) fn transfer_frame_admission(
        &self, parent: &crate::ExecutionCallEvidence, child: &mut crate::ExecutionCallEvidence,
    ) {
        child.borrow_owners = child.positional_arguments().filter_map(|(ordinal, argument)| {
            if !argument.is_reference() { return None; }

            let mut inherited = parent.positional_arguments().filter_map(|(source, captured)| {
                std::ptr::eq(argument.kind(), captured.kind()).then(|| parent.borrow_owners.get(&source)).flatten()
            });

            let first = inherited.next()?;

            let owners = if let Some(second) = inherited.next() {
                bray_base::sorted_unique_shared_slice([first, second].into_iter().chain(inherited).flat_map(|owners| owners.iter().cloned()))
            } else {
                std::sync::Arc::clone(first)
            };

            Some((ordinal, owners))
        }).collect();

        if !parent.addresses.is_empty() {
            child.addresses = child.positional_arguments().filter_map(|(ordinal, argument)| {
                let observed = self.reached_address_data(argument);
                let mut bounds = crate::execution_guarantees::AddressDependencies::new();

                for (address, places) in parent.addresses.values().flatten() {
                    if observes_address_data(&observed, address) {
                        bounds.entry(address.clone()).or_default().extend(places.iter().cloned());
                    }
                }

                (!bounds.is_empty()).then(|| (crate::ExecutionPlace::argument(ordinal), bounds))
            }).collect();
        }

        let owners = self.witness_snapshots(child.owned_arguments());

        child.witness_inputs = parent.witness_inputs.iter().filter(|(owner, _)| owners.retains_owner(owner))
            .map(|(owner, witness)| (owner.clone(), std::sync::Arc::clone(witness))).collect();

        child.witness_allocations = parent.witness_allocations.iter()
            .filter(|(_, owner)| child.witness_inputs.contains_key(*owner))
            .map(|(condition, owner)| (condition.clone(), owner.clone())).collect();

        child.trusted_assumptions = private_admission_facts(parent, child.witness_inputs.values(), &self.proofs).into();
    }

    pub(super) fn resume_owned_witnesses(
        &mut self,
        entry: &crate::ExecutionCallEvidence,
        returned: &crate::execution_guarantees::RetainedSnapshots,
    ) {
        if entry.witness_inputs.is_empty() || returned.is_empty() { return; }

        let assumptions = entry.assumptions.joined_with(&self.assumptions);
        let proof = self.proofs.proof(&entry.trusted_assumptions, &assumptions);
        let mut resumed_owners = BTreeSet::new();

        for (owner, witness) in &entry.witness_inputs {
            if !returned.retains_owner(owner) { continue; }

            let mut resumed = false;

            for condition in witness.conditions.keys() {
                if proof.prove_trusted(condition) == Some(true) {
                    resumed = true;

                    condition
                        .clone()
                        .assume(true, &mut self.trusted_assumptions);
                }
            }

            if resumed {
                resumed_owners.insert(owner);

                let active = self.witnesses.entry(owner.clone()).or_insert_with(|| std::sync::Arc::clone(witness));

                if !std::sync::Arc::ptr_eq(active, witness) {
                    active.merge(witness);
                }

                if !active.available {
                    std::sync::Arc::make_mut(active).available = true;
                }
            }
        }

        for (condition, owner) in &entry.witness_allocations {
            if resumed_owners.contains(owner) && proof.prove_trusted(condition) == Some(true) {
                self.allocation_owners.insert(condition.clone(), owner.clone());
            }
        }
    }

    /// Visits initialized owned components and the checked captures of pending futures.
    pub(super) fn owned_values<'a>(
        &'a self,
        value: &ExecutionCondition,
    ) -> impl Iterator<Item = ExecutionCondition> + 'a + use<'a> {
        self.value_contexts(value, false)
    }

    /// A future retains extraction evidence through its loan without owning the pointee.
    pub(super) fn witness_values<'a>(
        &'a self,
        value: &ExecutionCondition,
    ) -> impl Iterator<Item = ExecutionCondition> + 'a + use<'a> {
        self.value_contexts(value, true)
    }

    fn value_contexts<'a>(
        &'a self,
        value: &ExecutionCondition,
        extraction_evidence: bool,
    ) -> impl Iterator<Item = ExecutionCondition> + 'a + use<'a> {
        // Snapshots share immutable components. Only the traversal's small worklist is owned.
        let mut pending = vec![value.clone()];

        let mut deferred = BTreeSet::new();

        std::iter::from_fn(move || {
            let value = pending.pop()?;

            pending.extend(value.owned_components().cloned());

            if extraction_evidence
                && let Some((crate::execution_guarantees::ExecutionObservationKind::Owned, owner)) = value.ownership_identity()
                && let Some(entries) = value.deferred_value()
            {
                for (invocation, candidate) in &entries.producers {
                    if deferred.insert((owner.clone(), invocation.clone())) {
                        pending.extend(candidate.entry.extracted_values.values().cloned());
                    }
                }
            }

            Some(value)
        })
    }

    /// Finds actual witness carriers retained by values and their captured proof sources.
    pub(super) fn owned_witnesses(
        &self,
        values: impl IntoIterator<Item = ExecutionCondition>,
    ) -> impl Iterator<
        Item = (
            &ExecutionCondition,
            &std::sync::Arc<crate::execution_guarantees::ExecutionWitness>,
        ),
    > {
        let owned = if self.witnesses.is_empty() {
            crate::execution_guarantees::RetainedSnapshots::default()
        } else {
            self.witness_snapshots(values)
        };

        let count = if owned.is_empty() { 0 } else { self.witnesses.len() };

        self.witnesses.iter().take(count).filter(move |(carrier, _)| owned.retains_owner(carrier))
    }

    pub(super) fn witness_snapshots(
        &self,
        values: impl IntoIterator<Item = ExecutionCondition>,
    ) -> crate::execution_guarantees::RetainedSnapshots {
        values.into_iter().flat_map(|value| self.witness_values(&value)).collect()
    }

    pub(super) fn call_trusted_assumptions(
        &self,
        transferred: impl IntoIterator<Item = ExecutionCondition>,
    ) -> ExecutionAssumptions {
        let mut transferred = transferred.into_iter().collect::<Vec<_>>();
        let mut owners = BTreeSet::new();

        while let Some(value) = transferred.pop() {
            if value == ExecutionCondition::unknown() {
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

                    match condition.kind() {
                        crate::ExecutionConditionKind::Trusted(condition) => pending.push(condition),
                        crate::ExecutionConditionKind::Operation(_, operands) => {
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
        let observed = std::cell::OnceCell::new();
        let proof = self.proof();

        for witness in self.witnesses.values() {
            for (condition, guarantee) in &witness.conditions {
                let mut established = None;

                for (address, owners) in &guarantee.addresses {
                    let observed = observed.get_or_init(|| self.reached_address_data(value));

                    if !observes_address_data(observed, address) { continue; }

                    // Certify the actual observed clause once, after finding an address it supports.
                    if !*established.get_or_insert_with(|| proof.prove_trusted(condition) == Some(true)) {
                        break;
                    }

                    addresses.entry(address.clone()).or_default().extend(owners.iter().cloned());
                }
            }
        }

        addresses
    }

    fn reached_address_data(&self, value: &ExecutionCondition) -> BTreeSet<ExecutionCondition> {
        let mut pending = self.owned_values(value).collect::<Vec<_>>();
        let mut observed = BTreeSet::new();

        while let Some(value) = pending.pop() {
            // A loan observes its current data without owning its contents.
            let data = value.computed_value();

            if observed.insert(data.clone()) {
                pending.extend(data.owned_components().cloned());
            }
        }

        observed
    }

    pub(super) fn directly_carries_witness(&self, value: &ExecutionCondition) -> bool {
        let Some((crate::execution_guarantees::ExecutionObservationKind::Owned, identity)) = value.ownership_identity() else {
            return false;
        };

        self.witnesses
            .get(identity)
            .is_some_and(|witness| {
                witness.available && witness.owning_conditions(self.proof()).next().is_some()
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

        self.owned_results.retain(|value| !stale(value));

        self.witnesses.retain(|value, witness| {
            if stale(value) {
                return false;
            }

            witness.retain_conditions(|condition, guarantee| {
                !stale(condition) && !guarantee.addresses.keys().any(stale)
            });

            !witness.conditions.is_empty() || !witness.dependencies.is_empty()
        });

        self.trusted_assumptions
            .retain(|(condition, _)| !stale(condition));

        self.assumptions.retain(|(condition, _)| !stale(condition));

        self.allocation_owners
            .retain(|condition, owner| !stale(condition) && !stale(owner));

        for entry in self.entries.values_mut().filter(|entry| entry.observes_retiring_evaluation(&stale)) {
            std::sync::Arc::make_mut(entry).retire_evaluation(mutation, &stale);
        }

        self.update_stored_values(ExecutionCondition::mapping_frames(|_| true, |_, frame| {
            Some(frame.retire_evaluation(mutation, &stale))
        }));

        self.current.update_values(|value| value.without_evaluation(mutation));

        for value in self.expressions.values_mut().chain(self.pending_results.values_mut()) {
            *value = value.without_evaluation(mutation);
        }
    }

    pub(super) fn invalidate_cleanup(
        &mut self,
        stable_values: &crate::execution_guarantees::ObservationSet,
        stable_owners: &crate::execution_guarantees::RetainedSnapshots,
        admitted: Option<bray_bound_tree::BoundExecutionSite>,
    ) {
        self.refine_witnesses();

        let retained_owner = |value: &ExecutionCondition| stable_owners.retains_owner(value);

        let retains_contents = crate::execution_guarantees::memoize_condition(|condition| {
            condition.observes_only(|value| stable_values.preserves(value))
        });

        let retains_clause = |owner: &ExecutionCondition, condition: &ExecutionCondition, support: &crate::execution_guarantees::ExecutionGuarantee| {
            retains_contents(condition) && (retained_owner(owner)
                || support.addresses.is_empty() && stable_owners.retains_witness_carrier(owner)
                    && observes_carried_referent(condition, owner))
        };

        let admitted_witnesses = self.admitted_private_witnesses(admitted);
        let retained_witnesses = stable_private_frame_witnesses(stable_values, stable_owners);

        let is_admitted = |witness: &std::sync::Arc<crate::execution_guarantees::ExecutionWitness>| {
            admitted_witnesses.iter().any(|admitted| std::sync::Arc::ptr_eq(admitted, witness))
        };

        let keeps_private_support = |witness: &std::sync::Arc<crate::execution_guarantees::ExecutionWitness>| {
            is_admitted(witness) || retained_witnesses.contains(&std::sync::Arc::as_ptr(witness))
        };

        self.assumptions.retain(|(condition, _)| {
            !condition.observes_live(None) || retains_contents(condition)
        });

        let mut retained = BTreeSet::new();
        let proof = self.proofs.proof(&self.trusted_assumptions, &self.assumptions);

        self.witnesses.retain(|carrier, witness| {
            witness.retain_conditions(|condition, support| retains_clause(carrier, condition, support));

            // Availability says the carrier can be used, not that every possible
            // clause was proved on this path. Preserve only established truth.
            for condition in witness.conditions.keys().filter(|condition| {
                witness.available && proof.prove_trusted(condition) == Some(true)
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
                || (retains_contents(&fact.0)
                    && fact.0.observations().all(|value| {
                        !matches!(value.kind(), crate::ExecutionConditionKind::Predicate(_, _, _) | crate::ExecutionConditionKind::Call(_, _, _, _, _))
                            || value.observes_only(|_| false)
                    }))
        };

        // Private moved inputs retain historical entry facts while child execution
        // owns their authority. Completion still requires their surviving live witness.
        let preserves_owned = |fact: &(ExecutionCondition, bool), owners: &crate::execution_guarantees::RetainedSnapshots, admission: &BTreeSet<(ExecutionCondition, bool)>| {
            preserves(fact) || admission.contains(fact)
                || (retains_contents(&fact.0)
                    && fact.0.observations().any(|value| {
                        owners.retains_owner(value)
                    }))
        };

        self.trusted_assumptions.retain(preserves);

        self.allocation_owners.retain(|condition, owner| {
            retains_contents(condition) && retained_owner(owner)
        });

        // Entry mutation keeps the analysis cache shared without borrowing the mutable state.
        let proofs = std::sync::Arc::clone(&self.proofs);

        let support_changes = |entry: &crate::ExecutionCallEvidence| entry.witness_inputs.iter().any(|(owner, witness)| {
            !keeps_private_support(witness) && (witness.conditions.is_empty() && !retained_owner(owner)
                || witness.conditions.iter().any(|(condition, support)| !retains_clause(owner, condition, support)))
        });

        self.update_entry_snapshots(&|entry| support_changes(entry) || entry.pending_execution && {
            entry.trusted_boundary
                || entry
                    .assumptions
                    .iter()
                    .any(|(condition, _)| !retains_contents(condition))
                || {
                    let mut affected = entry.trusted_assumptions.iter().filter(|fact| !preserves(fact)).peekable();

                    affected.peek().is_some() && {
                        let owners = entry.owned_arguments().collect::<crate::execution_guarantees::RetainedSnapshots>();
                        let admission = private_admission_facts(entry, entry.witness_inputs.values().filter(|witness| keeps_private_support(witness)), &proofs);

                        affected.any(|fact| !preserves_owned(fact, &owners, &admission))
                    }
                }
        }, &mut |entry| {
            if entry.pending_execution {
                let owners = entry.owned_arguments().collect::<crate::execution_guarantees::RetainedSnapshots>();
                let admission = private_admission_facts(entry, entry.witness_inputs.values().filter(|witness| keeps_private_support(witness)), &proofs);

                entry.assumptions.retain(|(condition, _)| retains_contents(condition));
                entry.trusted_assumptions.retain(|fact| preserves_owned(fact, &owners, &admission));
                entry.trusted_boundary = false;
            }

            entry.witness_inputs.retain(|owner, witness| {
                if keeps_private_support(witness) { return true; }

                witness.retain_conditions(|condition, support| retains_clause(owner, condition, support));

                !witness.conditions.is_empty() || !witness.dependencies.is_empty() && retained_owner(owner)
            });

            entry.witness_allocations.retain(|condition, owner| {
                entry.witness_inputs.get(owner).is_some_and(|witness| is_admitted(witness) || retains_contents(condition))
            });
        });
    }

    pub(super) fn join_values(
        &mut self,
        substitution: &ExecutionSubstitution<'_>,
    ) {
        if substitution.is_empty() {
            return;
        }

        self.join_frame_values(substitution);
        self.rename_values(substitution);
    }

    pub(super) fn rename_values(&mut self, substitution: &ExecutionSubstitution<'_>) {
        if !substitution.has_changes() {
            return;
        }

        let rename = |value: &ExecutionCondition| substitution.value(value);
        let rename_identity = |value: &ExecutionCondition| substitution.owner(value);

        for facts in [&mut self.assumptions, &mut self.trusted_assumptions] {
            let mut renamed = ExecutionAssumptions::default();

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
            .filter(|(condition, _)| *condition != ExecutionCondition::unknown())
            .collect();

        let mut ownership = super::ownership::OwnedResults::default();

        for (value, result) in std::mem::take(&mut self.owned_results) {
            let value = rename(&value);

            if value != ExecutionCondition::unknown() {
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

        self.current.update_values(rename);
    }

    fn join_frame_values(&mut self, substitution: &ExecutionSubstitution<'_>) {
        // Every frame reads the same predecessor values, including when its own slot changes.
        let changed = {
            let current = &self.current;

            let current_fact = crate::execution_guarantees::memoize_condition(|condition| {
                !condition.observations().any(|observed| matches!(observed.kind(), crate::ExecutionConditionKind::ScalarValue(_))
                    && observed.selector_place().is_some_and(|place| current.live_value(&place)
                        .is_none_or(|value| value.copied_value() != observed)))
            });

            let context = std::cell::LazyCell::new(|| {
                super::deferred::selection(self.assumptions.iter().filter(|(condition, _)| current_fact(condition)))
            });

            let mut update = ExecutionCondition::mapping_frames(|_| true, |_, frame| {
                let mut joined = frame.as_ref().clone();

                joined.join_values(substitution, &current_fact, &context);

                Some(if joined == **frame { std::sync::Arc::clone(frame) } else { std::sync::Arc::new(joined) })
            });

            let changed = current.iter().filter_map(|(place, value)| {
                let updated = update(value);

                (!std::ptr::eq(updated.kind(), value.kind())).then(|| (place.clone(), updated))
            }).collect::<Vec<_>>();

            Self::update_evaluated_values(
                self.expressions.values_mut().chain(self.pending_results.values_mut()), &mut self.entries, update,
            );

            changed
        };

        self.current.install_rewritten_values(changed);
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
            value: ExecutionCondition::unknown(), availability: super::current::ValueAvailability::Absent,
        }, None);
    }

}

fn observes_carried_referent(condition: &ExecutionCondition, carrier: &ExecutionCondition) -> bool {
    let referent = carrier.clone().into_referent();
    let mut trusted = condition.observations().filter(|value| {
        matches!(value.kind(), crate::ExecutionConditionKind::Trusted(_))
    }).peekable();

    trusted.peek().is_some() && trusted.all(|clause| {
        clause.observes_live_where(|value| value.is_owned_projection_of(&referent), |_| false)
    })
}

/// Retain proved admission from the same private complete carriers, without publishing it.
fn stable_private_frame_witnesses(
    stable: &crate::execution_guarantees::ObservationSet,
    owners: &crate::execution_guarantees::RetainedSnapshots,
) -> std::collections::HashSet<*const crate::execution_guarantees::ExecutionWitness> {
    let mut retained = std::collections::HashSet::new();
    let mut seen = std::collections::HashSet::new();

    for frame in owners.owned_deferred_frames().filter(|frame| seen.insert(std::sync::Arc::as_ptr(frame))) {
        for candidate in frame.producers.values() {
            let entry = &candidate.entry;

            if !entry.pending_execution || entry.witness_inputs.is_empty() { continue; }

            // Retained inactive frames keep their actual owned captures. A capture
            // with reachable storage still needs the operation's checked frame.
            let private = entry.owned_arguments().filter(|value| !value.is_reference()
                && value.inline_captured_storage().is_some_and(|capture| {
                    use bray_bound_tree::StorageObservationKind::{Marker, Mutable};

                    capture.observations().any(|(_, kind)| kind.is_immutable_inline())
                        && capture.observations().all(|(path, kind)| {
                            kind.is_ordinary_inline() || kind == Mutable && capture.observations().any(|(other, kind)| {
                                path == other && matches!(kind, Marker(_))
                            })
                        })
                        && capture.complete_carried_regions().is_some_and(|regions| regions.is_empty())
                })).collect::<crate::execution_guarantees::RetainedSnapshots>();

            for (owner, witness) in &entry.witness_inputs {
                if private.retains_owner(owner) && witness.dependencies.is_empty()
                    && !witness.conditions.is_empty()
                    && witness.conditions.iter().all(|(condition, support)| {
                        support.addresses.is_empty() && condition.observes_only(|value| {
                            private.retains_contents(value) || stable.preserves(value)
                        })
                    })
                {
                    retained.insert(std::sync::Arc::as_ptr(witness));
                }
            }
        }
    }

    retained
}

fn observes_address_data(observed: &BTreeSet<ExecutionCondition>, address: &ExecutionCondition) -> bool {
    let Some((root, path)) = address.observation_path() else { return false; };

    observed.iter().any(|data| {
        let Some((source, prefix)) = data.observation_path() else { return false; };

        if source != root || !path.starts_with(&prefix) { return false; }

        let actual = path[prefix.len()..].iter().fold(data.clone(), |value, field| value.component(*field))
            .into_referent();

        actual.descriptor_identity().observation_path() == Some((root.clone(), path.clone()))
    })
}

fn private_admission_facts<'a>(
    entry: &crate::ExecutionCallEvidence,
    private: impl IntoIterator<Item = &'a std::sync::Arc<crate::execution_guarantees::ExecutionWitness>>,
    proofs: &crate::execution_guarantees::ExecutionProofCache,
) -> BTreeSet<(ExecutionCondition, bool)> {
    let mut retained = BTreeSet::new();

    for witness in private {
        for condition in witness.owning_conditions(proofs.proof(&entry.trusted_assumptions, &entry.assumptions)) {
            condition.clone().assume(true, &mut retained);
        }
    }

    retained
}

#[cfg(test)]
mod tests {
    use crate::execution_guarantees::ExecutionAssumptions;
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use bray_symbols::{ConstantProjectionKind, SymbolOrdinal};

    use super::ExecutionState;
    use crate::{ExecutionCondition, ExecutionPlace};

    #[test]
    fn rewriting_shared_frames_retains_all_holders_and_distinct_equal_frames() {
        let (_, expressions) = crate::test_support::integer_expression_unit(bray_bound_tree::BoundUnitId::new(0), 3);

        let site: bray_bound_tree::BoundExecutionSite = expressions[0].into();

        let frame = Arc::new(crate::ExecutionDeferredValue {
            producers: [(site.into(), crate::ExecutionDeferredInvocation {
                callee: None,
                transition: None,
                entry: Arc::new(crate::ExecutionCallEvidence::default()),
                completed: None,
                reports: Default::default(),
                selection: [(input(3), true)].into(),
            })].into(),
            unknown: None,
        });

        let independent = Arc::new(frame.as_ref().clone());
        let mut state = ExecutionState::default();

        for (index, owner) in [(0, &frame), (1, &frame), (2, &independent)] {
            state.expressions.insert(expressions[index], input(index as u32).with_deferred_value(Arc::clone(owner)));
        }

        Arc::make_mut(state.entries.entry(site).or_default()).arguments.insert(
            ExecutionPlace::argument(SymbolOrdinal::new(0)), input(1).with_deferred_value(Arc::clone(&frame)));

        state.update_entry_snapshots(&|entry| !entry.pending_execution, &mut |entry| entry.pending_execution = true);

        let updated = state.expressions[&expressions[0]].deferred_value().unwrap().clone();

        assert!(!Arc::ptr_eq(&updated, &frame));
        assert!(Arc::ptr_eq(&updated, state.expressions[&expressions[1]].deferred_value().unwrap()));
        assert_eq!(updated.as_ref(), state.expressions[&expressions[2]].deferred_value().unwrap().as_ref());

        state.replace_deferred_value(&updated, None);

        assert!(state.expressions[&expressions[0]].deferred_value().is_none());
        assert!(state.expressions[&expressions[1]].deferred_value().is_none());
        assert!(state.expressions[&expressions[2]].deferred_value().is_some());
        assert!(state.entries[&site].arguments.values().all(|value| value.deferred_value().is_none()));
    }

    #[test]
    fn private_admission_keeps_only_proved_owning_clauses() {
        let owner = input(0);
        let guard = input(3);
        let held = permission(2);

        let condition = ExecutionCondition::operation(bray_bound_tree::BoundOperator::LogicalOr, vec![
            ExecutionCondition::operation(bray_bound_tree::BoundOperator::LogicalNot, vec![guard.clone()]),
            held.clone(),
        ]);

        for (owning, proved, selected) in [(true, true, None), (true, true, Some(true)),
            (false, true, None), (true, false, None), (true, true, Some(false))] {
            let mut witness = Arc::new(crate::execution_guarantees::ExecutionWitness::default());

            witness.carry(&condition, owning, &Default::default());

            let mut entry = crate::ExecutionCallEvidence::default();

            entry.witness_inputs.insert(owner.clone(), witness);
            let slot = ExecutionPlace::argument(SymbolOrdinal::new(0));
            let bounds = [ExecutionPlace::argument(SymbolOrdinal::new(4))].into();

            entry.addresses.insert(slot.clone(), [(owner.clone(), bounds)].into());
            if proved { condition.clone().assume(true, &mut entry.trusted_assumptions); }
            if let Some(selected) = selected { guard.clone().assume(selected, &mut entry.assumptions); }

            let state = ExecutionState::default();

            for (argument, forwarded) in [(owner.clone(), true), (owner.clone().borrowed(), false), (input(1), false)] {
                let observes_address = argument.computed_value() == &owner;
                let mut child = crate::ExecutionCallEvidence {
                    arguments: [(slot.clone(), argument)].into(),
                    owned_inputs: [SymbolOrdinal::new(0)].into(),
                    ..Default::default()
                };

                state.transfer_frame_admission(&entry, &mut child);

                assert_eq!(child.witness_inputs.contains_key(&owner), forwarded);
                assert_eq!(child.addresses.get(&slot), observes_address.then(|| &entry.addresses[&slot]));

                assert_eq!(child.trusted_assumptions.contains(&(condition.clone(), true)),
                    forwarded && owning && proved && selected != Some(false));

                if selected.is_none() {
                    assert!(!child.trusted_assumptions.contains(&(held.clone(), true)),
                        "an unknown guard cannot grant its owning consequent");
                }
            }
        }
    }

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
            ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(ExecutionCondition::predicate(
                symbol.into(),
                crate::test_support::empty_substitution(symbol.into()),
                vec![value],
            ))))
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
            [ExecutionCondition::boolean(true)],
            &BTreeSet::new(),
            &crate::execution_guarantees::ObservationSet::default(),
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

        let absent = ExecutionCondition::new(crate::ExecutionConditionKind::Literal(Arc::new(bray_symbols::ConstantValueData::new(
            crate::test_support::error_type(), bray_symbols::ConstantValueKind::NullableAbsent,
        ))));

        let guard = ExecutionCondition::operation(bray_bound_tree::BoundOperator::NotEqual, vec![observed.clone(), absent.clone()]);
        let payload = observed.clone().component(ConstantProjectionKind::NullableValue).borrowed();

        for (snapshot, preserved) in [
            (observed, true),
            (ExecutionCondition::owned_value(owner.clone(), ExecutionCondition::constructed(owner.discriminant(),
                [(bray_bound_tree::StorageProjection::NullableValue, input(1))].into())), true),
            (ExecutionCondition::owned_value(owner.clone(), ExecutionCondition::constructed(input(2).discriminant(),
                [(bray_bound_tree::StorageProjection::NullableValue, input(1))].into())), false),
        ] {
        let mut state = ExecutionState::default();

        state.assumptions.extend([(guard.clone(), true), (payload.clone(), true)]);
        state.trusted_assumptions.extend([(guard.clone(), true), (payload.clone(), true)]);
        state.carry_guarantee(owner.clone(), guard.clone());
        state.carry_guarantee(owner.clone(), payload.clone());

        let mut retained = crate::execution_guarantees::ObservationSet::default();

        let current_guard = ExecutionCondition::operation(bray_bound_tree::BoundOperator::NotEqual,
            vec![snapshot.clone(), absent.clone()]);

        retained.insert(input(99));
        assert!(!retained.preserves(&guard));
        retained.insert_classified(snapshot, bray_bound_tree::StorageObservationKind::Discriminant);
        assert_eq!(retained.preserves(&guard), preserved, "owner identity alone cannot preserve a different tag");
        state.invalidate_contents(&BTreeSet::new(), [owner.clone()], &BTreeSet::new(), &retained);
        if preserved { assert!(state.assumptions.contains(&(guard.clone(), true))); }

        assert_eq!(current_guard.prove_trusted(&state.trusted_assumptions, &state.assumptions) == Some(true), preserved,
            "historical tag facts cannot prove a replacement value's current tag");

        assert!(!state.assumptions.contains(&(payload.clone(), true)));
        assert!(!state.trusted_assumptions.contains(&(payload.clone(), true)));
        }
    }

    #[test]
    fn cleanup_keeps_observation_free_guarantees_with_unchanged_guards() {
        let symbol = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>(
            "ValidWrite",
        );

        let substitution = crate::test_support::empty_substitution(symbol.into());

        let predicate = |arguments: Vec<_>| ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(
            ExecutionCondition::new(crate::ExecutionConditionKind::Predicate(symbol.into(), substitution, arguments.into())),
        )));

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
                &stable.iter().cloned().collect(), &Default::default(), None);

            assert!(state.trusted_assumptions.contains(&(independent.clone(), true)));
            assert_eq!(state.trusted_assumptions.contains(&(guarded.clone(), true)), unchanged);
            assert!(!state.trusted_assumptions.contains(&(observed.clone(), true)));
        }
    }

    fn input(ordinal: u32) -> ExecutionCondition {
        ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(ordinal))))
    }

    #[test]
    fn cleanup_requires_the_complete_carrier_to_survive() {
        let owner = input(0);
        let marker = owner.clone().component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0)));
        let symbol = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>("ValidRead");

        let condition = ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(ExecutionCondition::new(crate::ExecutionConditionKind::Predicate(
            symbol.into(),
            crate::test_support::empty_substitution(symbol.into()),
            [marker.clone().borrowed()].into(),
        )))));

        for complete in [false, true] {
            let mut state = ExecutionState::default();

            state.carry_guarantee(owner.clone(), condition.clone());
            state.trusted_assumptions.insert((condition.clone(), true));

            state.invalidate_cleanup(
                &[marker.clone(), marker.clone().borrowed()].into_iter().collect(),
                &[if complete { owner.clone() } else { marker.clone() }].into_iter().collect(),
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

            state.invalidate_cleanup(&stable, &[actual].into_iter().collect(), None);

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

        ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(ExecutionCondition::new(crate::ExecutionConditionKind::Predicate(
            symbol.into(),
            crate::test_support::empty_substitution(symbol.into()),
            [input(ordinal), ExecutionCondition::boolean(true)].into(),
        )))))
    }

    #[test]
    fn only_recorded_admitted_owners_keep_unavailable_entry_authority() {
        use bray_bound_tree::{BoundErrorExpression, BoundExpression, BoundUnitId};
        use crate::test_support::{error_type, expression_unit, push_expression};

        let (_, expressions) = expression_unit(BoundUnitId::new(41), |tree, origin| vec![
            push_expression(tree, BoundExpression::Error(BoundErrorExpression::new(origin, error_type()))),
            push_expression(tree, BoundExpression::Error(BoundErrorExpression::new(origin, error_type()))),
        ]);

        let invocation = expressions[0].into();
        let pending = expressions[1].into();
        let owner = input(0);
        let condition = permission(2);

        let term = crate::test_support::semantic_values()
            .intern_constant_term(bray_symbols::ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .expect("the extraction's formal input");

        for (owned, recorded, own_context, extracted) in [
            (true, true, true, false), (false, true, true, false),
            (true, false, true, false), (true, true, false, false),
            (false, true, true, true), (false, false, true, true), (false, true, false, true),
        ] {
            let captured = (owned || extracted) && recorded;
            let accepted = captured && own_context;
            let mut state = ExecutionState::default();

            state.carry_guarantee(owner.clone(), condition.clone());
            condition.clone().assume(true, &mut state.trusted_assumptions);
            state.allocation_owners.insert(condition.clone(), owner.clone());

            state.entries.insert(invocation, crate::ExecutionCallEvidence {
                outputs: Default::default(),
                arguments: [(ExecutionPlace::argument(SymbolOrdinal::new(0)), owner.clone())].into(),
                owned_inputs: if owned { [SymbolOrdinal::new(0)].into() } else { [].into() },
                witness_inputs: if captured { [(owner.clone(), Arc::clone(&state.witnesses[&owner]))].into() } else { Default::default() },
                witness_allocations: if captured { [(condition.clone(), owner.clone())].into() } else { Default::default() },
                extracted_values: if extracted { [(term, owner.clone())].into() } else { Default::default() },
                trusted_assumptions: state.trusted_assumptions.clone(),
                ..crate::ExecutionCallEvidence::default()
            }.into());

            let mut captured = state.entries[&invocation].clone();

            Arc::make_mut(&mut captured).pending_execution = true;
            state.entries.insert(pending, captured);
            state.suspend_owned_witnesses(&BTreeSet::from([owner.clone()]));

            assert!(!state.trusted_assumptions.contains(&(condition.clone(), true)));

            state.invalidate_cleanup(&Default::default(), &Default::default(), own_context.then_some(invocation));

            assert!(!state.witnesses.contains_key(&owner));
            assert!(!state.allocation_owners.contains_key(&condition));
            assert_eq!(state.entries[&pending].witness_inputs.contains_key(&owner), accepted);
            assert_eq!(state.entries[&pending].witness_allocations.contains_key(&condition), accepted);
            assert!(!state.trusted_assumptions.contains(&(condition.clone(), true)));

            assert_eq!(state.entries[&pending].trusted_assumptions.contains(&(condition.clone(), true)), accepted,
                "pending admission must agree with its exact private owning carrier");

            let returned = [owner.clone()].into_iter().collect();

            if accepted {
                let mut changed = state.clone();

                changed.invalidate_referents(&[input(2)], &BTreeSet::new(), &BTreeSet::new(),
                    &BTreeSet::new(), &Default::default());

                let after_write = changed.entries[&pending].clone();

                assert!(!after_write.trusted_assumptions.contains(&(condition.clone(), true)),
                    "actual external observation mutation revokes historical admission");

                changed.resume_owned_witnesses(&after_write, &returned);
                assert!(!changed.trusted_assumptions.contains(&(condition.clone(), true)));

                let mut dropped = state.clone();

                dropped.retire_trusted_values([owner.clone()], &BTreeSet::new());

                let dropped_entry = Arc::clone(&dropped.entries[&pending]);

                dropped.resume_owned_witnesses(&dropped_entry, &returned);

                assert!(!dropped.trusted_assumptions.contains(&(condition.clone(), true)),
                    "discarded authority cannot resume from a saved entry");
            }

            let entry = state.entries[&pending].clone();

            state.resume_owned_witnesses(&entry, &returned);

            assert_eq!(condition.prove_trusted(&state.trusted_assumptions, &state.assumptions) == Some(true), accepted);
            assert_eq!(state.allocation_owners.contains_key(&condition), accepted);
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

        assert!(left.intersect(&right, &Default::default()));
        assert_eq!(left.addresses[&argument][&address].len(), 2);
        assert!(!left.prove_trusted_requirements(&[condition]).is_ok());
        assert!(!left.intersect(&right, &Default::default()));
    }

    #[test]
    fn address_bounds_lookup_uses_only_live_facts_on_the_complete_carrier() {
        let owner = ExecutionPlace::argument(SymbolOrdinal::new(30));
        let carrier = input(2);

        let address = ExecutionCondition::new(crate::ExecutionConditionKind::Projection(
            ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0)),
            Arc::new(carrier.clone()),
        ));

        let access = ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(address.clone())));
        let mut state = ExecutionState::default();

        Arc::make_mut(state.witnesses.entry(carrier).or_default())
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

        let field = bray_bound_tree::StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let object = ExecutionCondition::constructed(input(2), [(field, address.clone())].into());
        let borrowed = object.clone().borrowed();

        assert_eq!(state.address_dependencies(&borrowed), state.address_dependencies(&address));
        assert!(state.owned_result(&borrowed).is_empty(), "an observed address bound grants no ownership");

        let cleared = object.with_storage_component(&[field], ExecutionCondition::boolean(false)).borrowed();

        assert!(state.address_dependencies(&cleared).is_empty(), "the old field snapshot is no longer current");
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

        Arc::make_mut(state.witnesses.entry(pointer.clone()).or_default())
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

            Arc::make_mut(witness)
                .conditions
                .get_mut(&access)
                .expect("the access conjunct is retained")
                .addresses
                .insert(input(2), BTreeSet::from([owner.clone()]));

            if whole_lifetime {
                Arc::make_mut(witness)
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
                .insert(owner.clone(), ExecutionCondition::boolean(false));

            state.carry_guarantee(carrier.clone(), access.clone());
            access.clone().assume(true, &mut state.trusted_assumptions);

            Arc::make_mut(
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

        state.invalidate_trusted(&ExecutionCondition::boolean(false));
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

        let absent = ExecutionCondition::new(crate::ExecutionConditionKind::Literal(Arc::new(bray_symbols::ConstantValueData::new(
            crate::test_support::error_type(),
            bray_symbols::ConstantValueKind::NullableAbsent,
        ))));

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
            Arc::new(crate::execution_guarantees::ExecutionWitness::default());

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

        let absent = ExecutionCondition::new(crate::ExecutionConditionKind::Literal(Arc::new(bray_symbols::ConstantValueData::new(
            crate::test_support::error_type(),
            bray_symbols::ConstantValueKind::NullableAbsent,
        ))));

        let guard = ExecutionCondition::operation(
            bray_bound_tree::BoundOperator::Equal,
            vec![carrier.clone(), absent],
        );

        let guarded = |subject| ExecutionCondition::operation(
            bray_bound_tree::BoundOperator::LogicalOr,
            vec![guard.clone(), permission(1).with_equalities(&[(input(1), subject)].into())],
        );

        let ownership = guarded(ExecutionCondition::new(crate::ExecutionConditionKind::Borrowed(Arc::new(marker))));
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
        );

        assert!(state.trusted_assumptions.contains(&(ownership.clone(), true)));
        assert!(state.witnesses[&carrier].conditions.contains_key(&ownership));
        assert!(!state.trusted_assumptions.contains(&(access.clone(), true)));
        assert!(!state.witnesses[&carrier].conditions.contains_key(&access));

        state.invalidate_contents(&BTreeSet::new(), [carrier], &BTreeSet::new(),
                &Default::default());

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

            Arc::make_mut(
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

        let predicate = ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(ExecutionCondition::new(crate::ExecutionConditionKind::Predicate(
            symbol.into(),
            substitution,
            [input(2).borrowed(), ExecutionCondition::boolean(true)].into(),
        )))));

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

        let mutation = |part| StorageMutation::Cleanup(bray_bound_tree::StorageCleanupAction {
            occurrence,
            operation: bray_bound_tree::AnalysisOperationId::new(access.unit(), 0),
            access,
            part,
            kind: bray_bound_tree::StorageCleanupActionKind::Lifecycle(TypeAssociatedLifecycleSlot::Destructor),
        });

        let place = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let old = ExecutionCondition::post_state(mutation(0), ExecutionCondition::new(crate::ExecutionConditionKind::Input(place.clone())));
        let sibling = ExecutionCondition::post_state(mutation(1), ExecutionCondition::new(crate::ExecutionConditionKind::Input(place.clone())));
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
                failure_rows: None,
                outputs: Default::default(),
                implementation_witnesses: Default::default(),
                witness_inputs: Default::default(),
                witness_allocations: Default::default(),
                extracted_values: Default::default(),
                owned_inputs: Default::default(),
                borrow_owners: Default::default(),
                addresses: Default::default(),
                anonymous_target: None,
                trusted_boundary: true,
                pending_execution: true,
                memory_access: None,
                arguments: [(place.clone(), old.clone())].into(),
                assumptions: [(old.clone(), true)].into(),
                trusted_assumptions: [(old.clone(), true)].into(),
            }.into(),
        );

        state.retire_evaluation(mutation(0));

        let current = place
            .value_in(&state.current)
            .expect("retired value retains known components");

        assert_eq!(current.clone().project(marker), unchanged);
        assert_eq!(current.project(payload), ExecutionCondition::unknown());

        state.assign(place.clone(), old.clone(), None);

        assert!(!state.owns_witness(&old));
        assert!(!state.trusted_assumptions.contains(&(old, true)));
        assert!(state.owns_witness(&sibling));
        assert!(state.trusted_assumptions.contains(&(sibling, true)));
        assert_eq!(state.entries[&occurrence].arguments[&place], ExecutionCondition::unknown());
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
        );

        assert_eq!(
            state.trusted_assumptions,
            ExecutionAssumptions::from([(preserved.clone(), true)])
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

        let spatial = ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(ExecutionCondition::new(crate::ExecutionConditionKind::Predicate(
            symbol.into(),
            substitution,
            [carrier.clone(), ExecutionCondition::boolean(true)].into(),
        )))));

        let observed = input(0).borrowed();
        let mut state = ExecutionState::default();

        state.current.insert(owner.clone(), input(0));

        Arc::make_mut(state.witnesses.entry(carrier.clone()).or_default())
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
        );

        assert_eq!(
            state.trusted_assumptions,
            ExecutionAssumptions::from([(spatial.clone(), true)])
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
                &Default::default());

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
                &Default::default());

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

        let spatial = ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(ExecutionCondition::new(crate::ExecutionConditionKind::Predicate(
            symbol.into(),
            substitution,
            [carrier.clone(), ExecutionCondition::boolean(true)].into(),
        )))));

        let mut state = ExecutionState::default();

        state.current.insert(owner.clone(), input(0));
        state.current.insert(intermediate.clone(), input(1));

        Arc::make_mut(state.witnesses.entry(input(1)).or_default()).dependencies =
            BTreeSet::from([owner.clone()]);

        Arc::make_mut(state.witnesses.entry(carrier.clone()).or_default())
            .dependencies = BTreeSet::from([intermediate]);

        state.carry_guarantee(carrier, spatial.clone());
        state.trusted_assumptions.insert((spatial.clone(), true));

        state.invalidate_contents(
            &BTreeSet::from([owner.clone()]),
            [input(0)],
            &BTreeSet::from([symbol.into()]),
                &Default::default(),
        );

        assert_eq!(state.trusted_assumptions, ExecutionAssumptions::from([(spatial, true)]));
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

        let child = ExecutionCondition::new(crate::ExecutionConditionKind::Input(left));
        let mut state = ExecutionState::default();

        for value in [
            ExecutionCondition::new(crate::ExecutionConditionKind::Input(root.clone())),
            child.clone(),
            ExecutionCondition::new(crate::ExecutionConditionKind::Input(right)),
        ] {
            state.carry_guarantee(value.clone(), value.clone());
            state.trusted_assumptions.insert((value, true));
        }

        let retained = BTreeSet::from([child.clone()]);
        let mut moved = state.clone();

        moved.retire_trusted_values([ExecutionCondition::new(crate::ExecutionConditionKind::Input(root.clone()))], &retained);

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
            ExecutionAssumptions::from([(child.clone(), true)])
        );

        // Retaining a value cannot extend the storage required by its witness.
        Arc::make_mut(state.witnesses.entry(child).or_default()).dependencies =
            BTreeSet::from([root.clone()]);

        state.invalidate_trusted_roots(&BTreeSet::from([root.root.clone()]));
        state.retire_trusted_values([ExecutionCondition::new(crate::ExecutionConditionKind::Input(root))], &retained);

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

            Arc::make_mut(state.witnesses.entry(carrier.clone()).or_default())
                .dependencies = BTreeSet::from([dependency]);

            state.carry_guarantee(carrier.clone(), carrier.clone());
            state.trusted_assumptions.insert((carrier, true));
        }

        let cycle = ExecutionPlace::argument(SymbolOrdinal::new(514));
        let independent = input(1000);

        state.current.insert(cycle.clone(), input(513));

        Arc::make_mut(state.witnesses.entry(input(1)).or_default()).dependencies =
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
                ExecutionAssumptions::from([(independent.clone(), true)])
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

        let changed = ExecutionCondition::new(crate::ExecutionConditionKind::Input(left));

        let captured = ExecutionCondition::new(crate::ExecutionConditionKind::Entry {
            condition: Arc::new(changed.clone()),
            captured: true,
        });

        let live = ExecutionCondition::new(crate::ExecutionConditionKind::Entry {
            condition: Arc::new(changed.clone()),
            captured: false,
        });

        let mut state = ExecutionState::default();

        for (place, ordinal) in [root, child, right].into_iter().zip(0u32..) {
            let dependency = ExecutionPlace::argument(SymbolOrdinal::new(ordinal + 10));
            let carrier = input(ordinal + 20);

            state
                .current
                .insert(dependency.clone(), ExecutionCondition::new(crate::ExecutionConditionKind::Input(place)));

            Arc::make_mut(state.witnesses.entry(carrier.clone()).or_default())
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
            ExecutionAssumptions::from([(captured, true), (input(22), true)])
        );

        assert_eq!(state.assumptions, ExecutionAssumptions::from([(changed, true)]));

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

        let child = ExecutionCondition::new(crate::ExecutionConditionKind::Input(
            root.clone()
                .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0))),
        ));

        let sibling = ExecutionCondition::new(crate::ExecutionConditionKind::Input(
            root.clone()
                .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1))),
        ));

        let parent = ExecutionCondition::new(crate::ExecutionConditionKind::Input(root));
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

        state.trusted_assumptions = ExecutionAssumptions::from([(
            ExecutionCondition::new(crate::ExecutionConditionKind::Entry {
                condition: Arc::new(parent.clone()),
                captured: true,
            }),
            true,
        )]);

        assert!(!state.owns_witness(&child));

        let live = ExecutionCondition::new(crate::ExecutionConditionKind::Entry {
            condition: Arc::new(parent),
            captured: false,
        });

        for witness in state.witnesses.values_mut() {
            std::sync::Arc::make_mut(witness).conditions.clear();
        }

        state.carry_guarantee(child.clone(), live.clone());
        state.trusted_assumptions = ExecutionAssumptions::from([(live, true)]);

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

        let condition = ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(ExecutionCondition::new(crate::ExecutionConditionKind::Predicate(
            symbol.into(), crate::test_support::empty_substitution(symbol.into()), [marker.clone().borrowed()].into(),
        )))));

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
                assert!(!crate::execution_guarantees::RetainedSnapshots::from(&retained).retains_owner(&marker));
            }
        }

        let (_, expressions) = crate::test_support::integer_expression_unit(bray_bound_tree::BoundUnitId::new(0), 2);

        let epoch = |expression: bray_bound_tree::BoundExpressionId| ExecutionCondition::post_state(
            bray_bound_tree::StorageMutation::Operation(expression.into()), owner.clone());

        let previous = epoch(expressions[0]);
        let marker = previous.clone().project(field);

        let claim = ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(
            ExecutionCondition::predicate(symbol.into(), crate::test_support::empty_substitution(symbol.into()),
                vec![marker.clone().borrowed()]))));

        for unchanged in [true, false] {
            let current = epoch(expressions[1]);

            let contents = if unchanged {
                marker.clone()
            } else {
                current.clone().project(field)
            };

            let current = ExecutionCondition::owned_value(owner.clone(),
                ExecutionCondition::constructed(current, [(field, contents)].into()));

            let retained = BTreeSet::from([current.clone()]);
            let snapshots = crate::execution_guarantees::RetainedSnapshots::from(&retained);
            let mut state = ExecutionState::default();

            assert_eq!(snapshots.retains_contents(&marker.clone().borrowed()), unchanged);
            assert!(!snapshots.retains_owner(&marker.clone().borrowed()));
            state.carry_guarantee(marker.clone(), claim.clone());
            state.trusted_assumptions.insert((claim.clone(), true));
            state.retire_trusted_values([previous.clone().borrowed()], &retained);
            assert_eq!(state.trusted_assumptions.contains(&(claim.clone(), true)), unchanged);
            state.invalidate_trusted(&current);
            assert!(!state.trusted_assumptions.contains(&(claim.clone(), true)));
        }

        let before = ExecutionCondition::owned_value(owner.clone(), ExecutionCondition::boolean(false));
        let current = ExecutionCondition::owned_value(owner.clone(), ExecutionCondition::boolean(true));
        let other = ExecutionCondition::owned_value(input(4), ExecutionCondition::boolean(true));
        let temporary = ExecutionCondition::constructed(input(1), [(field, before.clone().borrowed())].into());
        let retained = BTreeSet::from([current.clone()]);
        let borrowed_slot = ExecutionCondition::reference_value(input(5).borrowed(), current.clone().borrowed());

        assert!(!crate::execution_guarantees::RetainedSnapshots::from(&retained).retains_contents(&borrowed_slot));
        assert!(!crate::execution_guarantees::RetainedSnapshots::from(&retained).retains_owner(&current.clone().borrowed()));

        for (observed, retained, valid) in [
            (current.clone(), current.clone(), true),
            (before, current.clone(), false),
            (current, other, false),
        ] {
            let condition = ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(ExecutionCondition::new(crate::ExecutionConditionKind::Predicate(
                symbol.into(), crate::test_support::empty_substitution(symbol.into()), [observed.clone().borrowed()].into(),
            )))));

            let mut state = ExecutionState::default();

            state.carry_guarantee(observed, condition.clone());
            state.trusted_assumptions.insert((condition.clone(), true));
            state.retire_trusted_values([temporary.clone()], &BTreeSet::from([retained]));

            assert_eq!(state.trusted_assumptions.contains(&(condition, true)), valid, "retirement must compare the established fact with the surviving owner");
        }
    }
}
