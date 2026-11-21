use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::{ExecutionCallEvidence, ExecutionCondition, ExecutionInput, ExecutionObservationKind, ExecutionProof};
use crate::{ExecutionConditionKind as Kind, ExecutionPlace};
use bray_bound_tree::{BoundExecutionSite, StorageDeferredProducer};

type Selection = BTreeSet<(ExecutionCondition, bool)>;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum FrameReportOwner {
    Boundary(bray_bound_tree::CallReportOutput),
    Payload(bray_symbols::DependencySubject),
}

/// An invocation's captured environment and selection facts, retained by its actual frame value.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExecutionDeferredInvocation<Entry = Arc<ExecutionCallEvidence>> {
    /// Selected callable and witnesses. Its operand positions describe the captured environment.
    pub(crate) callee: Option<Arc<bray_symbols::CapturedCallable>>,
    /// The operation's already resolved storage transition, captured once at formation.
    pub(crate) transition: Option<Arc<bray_bound_tree::StorageDeferredTransition>>,
    pub(crate) entry: Entry,
    /// Result dependencies bound when this frame captures its evaluated inputs.
    pub(crate) completed: Option<Arc<BTreeSet<bray_symbols::DependencyRequirement>>>,
    pub(crate) reports: BTreeMap<FrameReportOwner, crate::dependency::ReportCandidates>,
    pub(crate) selection: BTreeSet<(ExecutionCondition, bool)>,
}

impl<Entry> ExecutionDeferredInvocation<Entry> {
    pub(crate) fn has_receiver(&self) -> bool {
        self.callee.as_ref().is_some_and(|callee| callee.receiver.is_some())
    }
}

/// Feasible latent producers carried directly by one future or task value.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExecutionDeferredValue {
    pub(crate) producers: BTreeMap<StorageDeferredProducer, ExecutionDeferredInvocation>,
    pub(crate) unknown: Option<ExecutionDeferredInvocation<()>>,
}

impl ExecutionDeferredValue {
    /// Retains captured inputs and their evidence under the reached frame owner.
    pub(crate) fn rebase_inputs(self: &Arc<Self>, holder: ExecutionPlace, proof: Option<&ExecutionProof<'_>>) -> Arc<Self> {
        let holder = Arc::new(holder);
        let mut updated = Arc::clone(self);

        for (source, producer) in &self.producers {
            if let Some(entry) = rebased_frame_entry(&producer.entry, &holder, proof) {
                Arc::make_mut(&mut updated).producers.get_mut(source)
                    .expect("rebasing retains the actual frame producer").entry = Arc::new(entry);
            }
        }

        updated
    }

    pub(crate) fn retiring_storage(
        self: &Arc<Self>,
        retirements: &[(bray_bound_tree::StorageTarget, bray_bound_tree::StorageValueRetirement)],
    ) -> Arc<Self> {
        let mut updated = Arc::clone(self);

        for (source, producer) in &self.producers {
            if let Some(transition) = producer.transition.as_ref().and_then(|transition| transition.retired(retirements)) {
                Arc::make_mut(&mut updated).producers.get_mut(source)
                    .expect("retirement keeps every frame producer").transition = Some(Arc::new(transition));
            }
        }

        if let Some(transition) = self.unknown.as_ref().and_then(|producer| producer.transition.as_ref())
            .and_then(|transition| transition.retired(retirements))
        {
            Arc::make_mut(&mut updated).unknown.as_mut()
                .expect("retirement keeps the opaque producer").transition = Some(Arc::new(transition));
        }

        updated
    }

    pub(crate) fn conditions(&self) -> impl Iterator<Item = &ExecutionCondition> {
        self.producers.values().flat_map(|producer| producer.entry.conditions()
            .chain(producer.selection.iter().map(|(condition, _)| condition)))
            .chain(self.unknown.iter().flat_map(|unknown| unknown.selection.iter().map(|(condition, _)| condition)))
    }

    pub(crate) fn try_map_conditions<E>(&self, mut map: impl FnMut(&ExecutionCondition) -> Result<ExecutionCondition, E>) -> Result<Self, E> {
        let mut mapped = self.clone();

        for (key, original) in &self.producers {
            let producer = mapped.producers.get_mut(key).expect("mapping retains every frame alternative");

            producer.entry = Arc::new(original.entry.try_map_conditions(&mut map)?);

            producer.selection = original.selection.iter().map(|(condition, holds)| Ok((map(condition)?, *holds)))
                .collect::<Result<_, E>>()?;
        }

        if let Some(original) = &self.unknown {
            let unknown = mapped.unknown.as_mut().expect("mapping retains the opaque alternative");

            unknown.selection = original.selection.iter().map(|(condition, holds)| Ok((map(condition)?, *holds)))
                .collect::<Result<_, E>>()?;
        }

        Ok(mapped)
    }
}

fn rebased_frame_entry(
    entry: &ExecutionCallEvidence, holder: &Arc<ExecutionPlace>, proof: Option<&ExecutionProof<'_>>,
) -> Option<ExecutionCallEvidence> {
    let owners = frame_input_identities(entry, holder);
    let copies = frame_input_copies(entry, holder, proof.is_some());
    let values = owners.iter().chain(copies.iter()).map(|(old, reached)| (old.clone(), reached.clone())).collect();
    let substitution = super::ExecutionSubstitution::new(&values, &owners);
    let mut rebased = entry.joined_values(&substitution)?;

    if let Some(proof) = proof && !copies.is_empty() {
        // Import only premises about the immutable data being detached here.
        // An already retained input keeps the facts in its own capture.
        let captured = |value: &ExecutionCondition| value.selector_place().is_some() && copies.contains_key(value);

        for (condition, holds) in proof.ordinary_premises() {
            if !condition.contains_trusted() && condition.observes_only(&captured)
                && condition.observations().any(&captured)
            {
                substitution.value(condition).assume(*holds, &mut rebased.assumptions);
            }
        }
    }

    Some(rebased)
}

fn frame_input_identities(entry: &ExecutionCallEvidence, holder: &Arc<ExecutionPlace>) -> BTreeMap<ExecutionCondition, ExecutionCondition> {
    let mut owners = BTreeMap::new();

    for (argument, value) in entry.positional_arguments() {
        let Some((observation, _)) = value.storage_contents() else { continue; };
        let source = frame_input_source(observation).map(Arc::clone).or_else(|| {
            matches!(observation.kind(), Kind::PostState(_, _) | Kind::Expression(_))
                .then(|| Arc::new(observation.clone()))
        });
        let Some(source) = source else { continue; };

        // A captured copy no longer observes the staging slot for the next invocation.
        owners.insert(observation.clone(), frame_input(holder, argument, source));
    }

    for argument in entry.owned_inputs.iter().copied() {
        let Some(value) = entry.arguments.get(&ExecutionPlace::argument(argument)) else { continue; };
        let mut pending = vec![value];
        let mut visited = std::collections::HashSet::new();

        while let Some(value) = pending.pop() {
            if !visited.insert(std::ptr::from_ref(value.kind())) { continue; }

            let ownership = value.ownership_identity();

            if matches!(ownership, Some((ExecutionObservationKind::Reference, _))) { continue; }

            pending.extend(value.owned_components());

            let Some((ExecutionObservationKind::Owned, owner)) = ownership else { continue; };
            let Some((root, _)) = owner.observation_path() else { continue; };
            let source = frame_input_source(&root).map(Arc::clone)
                .or_else(|| matches!(root.kind(), Kind::Expression(_)).then(|| Arc::new(root.clone())));
            let Some(source) = source else { continue; };
            let owned = frame_input(holder, argument, source);

            owners.entry(root).and_modify(|prior| {
                if *prior != owned { *prior = ExecutionCondition::unknown(); }
            }).or_insert(owned);
        }
    }

    owners
}

fn frame_input_copies(entry: &ExecutionCallEvidence, holder: &Arc<ExecutionPlace>, reached: bool) -> BTreeMap<ExecutionCondition, ExecutionCondition> {
    let mut copies = BTreeMap::new();

    for (argument, value) in entry.positional_arguments().filter(|(_, value)| !value.is_reference()) {
        for observed in value.computed_value().observations() {
            let Kind::ScalarValue(origin) = observed.kind() else { continue; };
            let source = frame_input_source(origin).map(Arc::clone)
                .or_else(|| (reached && origin.selector_place().is_some()).then(|| Arc::clone(origin)));
            let Some(source) = source else { continue; };

            copies.entry(observed.clone()).or_insert_with(|| {
                // Rebind the datum itself. It can already sit inside a scalar observation.
                ExecutionCondition::new(Kind::ScalarValue(Arc::new(frame_input(holder, argument, source))))
            });
        }
    }

    copies
}

fn frame_input(holder: &Arc<ExecutionPlace>, argument: bray_symbols::SymbolOrdinal, source: Arc<ExecutionCondition>) -> ExecutionCondition {
    ExecutionCondition::new(Kind::Input(ExecutionPlace {
        root: ExecutionInput::FrameInput { holder: Arc::clone(holder), argument, source },
        projections: Arc::new([]),
    }))
}

fn frame_input_source(value: &ExecutionCondition) -> Option<&Arc<ExecutionCondition>> {
    match value.kind() {
        Kind::Input(ExecutionPlace { root: ExecutionInput::FrameInput { source, .. }, .. }) => Some(source),
        _ => None,
    }
}

impl ExecutionDeferredValue {
    pub(crate) fn effects(&self, proof: &super::ExecutionProof<'_>)
        -> Option<bray_symbols::DependencyEffectOutcomes<bray_bound_tree::StorageEffect>>
    {
        use bray_bound_tree::StorageEffectRow;

        if self.unknown_possible(proof) { return None; }

        let mut effects = bray_symbols::DependencyEffectOutcomes::new([]);

        for producer in self.producers.values().filter(|producer| deferred_possible(&producer.selection, proof)) {
            let transfer = producer.transition.as_ref()?;
            let resolved = transfer.effects.as_ref()?;

            let normal_rows = transfer.effect_rows.iter().flat_map(|rows| {
                rows.range(StorageEffectRow::Normal(0)..=StorageEffectRow::Normal(usize::MAX))
            }).map(|(row, effects)| {
                let StorageEffectRow::Normal(index) = row else { unreachable!("normal effect range retains only normal rows"); };

                (*index, effects)
            });

            if normal_rows.clone().any(|(index, _)| !producer.entry.outputs.is_admitted(index)) {
                let selected = normal_rows.filter(|(index, _)| producer.entry.outputs.is_admitted(*index))
                    .flat_map(|(_, effects)| effects.iter().cloned());

                effects.merge(&resolved.clone().with_normal(selected));
            } else {
                effects.merge(resolved);
            }
        }

        Some(effects)
    }

    /// Reads the performed prefix from its activated body, preserving an opaque kind envelope.
    pub(crate) fn failure_effects(
        &self,
        proof: &super::ExecutionProof<'_>,
        kind: bray_symbols::DependencyFailureKind,
        row: Option<usize>,
    ) -> Option<Vec<bray_bound_tree::StorageEffect>> {
        if self.unknown_possible(proof) { return None; }

        let mut effects = BTreeSet::new();

        for producer in self.producers.values().filter(|producer| deferred_possible(&producer.selection, proof)) {
            let transfer = producer.transition.as_ref()?;

            if let Some(failures) = producer.entry.failure_rows.as_deref() {
                let feasible = failures.iter().filter(|failure| {
                    failure.covers(kind) && row.is_none_or(|row| row == failure.index)
                        && failure.guard.as_ref().is_none_or(|guard| proof.can_assume(guard, true))
                });

                let resolved = feasible.map(|failure| {
                    let prefix = transfer.effect_rows.as_ref()?
                        .get(&bray_bound_tree::StorageEffectRow::Abnormal(kind, failure.index))?;

                    effects.extend(prefix.iter().cloned());

                    Some(())
                }).collect::<Option<()>>();

                if resolved.is_some() {
                    continue;
                }
            }

            effects.extend(transfer.effects.as_ref()?.failure(kind).iter().cloned());
        }

        Some(effects.into_iter().collect())
    }

    pub(crate) fn cell_changes(
        &self,
        proof: &super::ExecutionProof<'_>,
        failure: Option<(bray_symbols::DependencyFailureKind, Option<usize>)>,
    ) -> Option<Arc<[bray_bound_tree::StorageEffect]>> {
        use bray_bound_tree::StorageEffectRow;

        if self.unknown_possible(proof) {
            return None;
        }

        let mut possible = Vec::new();

        for producer in self.producers.values().filter(|producer| deferred_possible(&producer.selection, proof)) {
            let rows = producer.transition.as_ref()?.effect_rows.as_ref()?;

            if let Some((kind, selected)) = failure {
                let failures = producer.entry.failure_rows.as_deref()?;

                possible.extend(failures.iter().filter(|row| {
                    row.covers(kind) && selected.is_none_or(|selected| row.index == selected)
                        && row.guard.as_ref().is_none_or(|guard| proof.can_assume(guard, true))
                }).map(|row| rows.get(&StorageEffectRow::AbnormalCells(kind, row.index))));
            } else {
                let start = possible.len();

                possible.extend(rows.range(StorageEffectRow::Normal(0)..=StorageEffectRow::Normal(usize::MAX))
                    .filter_map(|(row, _)| {
                        let StorageEffectRow::Normal(index) = *row else {
                            unreachable!("normal effect range retains only normal rows");
                        };

                        producer.entry.outputs.is_admitted(index).then(|| rows.get(&StorageEffectRow::NormalCells(index)))
                    }));

                if possible.len() == start {
                    return None;
                }
            }
        }

        bray_symbols::common_cell_changes(possible.into_iter())
    }

    pub(crate) fn cleanup_effects(&self, discard_result: bool, proof: &super::ExecutionProof<'_>)
        -> Option<Vec<bray_bound_tree::StorageEffect>>
    {
        if self.unknown_possible(proof) { return None; }

        let mut effects = BTreeSet::new();

        for producer in self.producers.values().filter(|producer| deferred_possible(&producer.selection, proof)) {
            let retained = producer.transition.as_ref()?;

            effects.extend(retained.captured_cleanup.as_ref()?.iter().cloned());

            if discard_result {
                effects.extend(retained.result_cleanup.as_ref()?.iter().cloned());
            }
        }

        Some(effects.into_iter().collect())
    }
}

impl<Entry> ExecutionDeferredInvocation<Entry> {
    pub(crate) fn merge_capture(&mut self, incoming: &Self) -> bool {
        assert_eq!(self.callee, incoming.callee,
            "one selected deferred producer retains its exact callable and witness choices");

        let before = self.selection.len();

        self.selection.retain(|fact| incoming.selection.contains(fact));

        let mut changed = before != self.selection.len();

        changed |= bray_bound_tree::StorageDeferredTransition::merge_alternative(&mut self.transition, &incoming.transition);

        match (&mut self.completed, &incoming.completed) {
            (Some(previous), Some(incoming)) if !incoming.is_subset(previous) => {
                Arc::make_mut(previous).extend(incoming.iter().cloned());
                changed = true;
            }
            (Some(_), None) => {
                self.completed = None;
                changed = true;
            }
            _ => {}
        }

        for (output, incoming) in &incoming.reports {
            if let Some(reports) = self.reports.get_mut(output) {
                changed |= reports.merge(incoming);
            } else {
                self.reports.insert(output.clone(), incoming.clone());
                changed = true;
            }
        }

        changed
    }
}

impl ExecutionDeferredValue {
    pub(crate) fn merge(&mut self, incoming: &Self, cache: &super::ExecutionProofCache) -> bool {
        let mut changed = false;

        for (source, candidate) in &incoming.producers {
            if let Some(previous) = self.producers.get_mut(source) {
                if previous != candidate {
                    if previous.entry != candidate.entry {
                        if std::env::var_os("BRA_694_TRACE_FRAME").is_some()
                            && source.local().and_then(BoundExecutionSite::expression).is_some_and(|expression| matches!(expression.ordinal(), 14 | 21))
                        {
                            let describe = |entry: &ExecutionCallEvidence| entry.positional_arguments()
                                .map(|(ordinal, value)| (ordinal, format!("{:?}", value.computed_value().kind()).chars().take(650).collect::<String>()))
                                .collect::<Vec<_>>();

                            eprintln!("frame join {source:?} left={:?} right={:?} ordinary={:?}/{:?} selected={:?}/{:?}",
                                describe(&previous.entry), describe(&candidate.entry), previous.entry.assumptions, candidate.entry.assumptions,
                                previous.selection, candidate.selection);
                        }

                        changed |= Arc::make_mut(&mut previous.entry).intersect_guarded(
                            &candidate.entry, cache, Some([&previous.selection, &candidate.selection]),
                        );
                    }

                    changed |= previous.merge_capture(candidate);
                }
            } else {
                // Producers share captured evidence while each flow branch owns its selection facts.
                self.producers.insert(source.clone(), candidate.clone());
                changed = true;
            }
        }

        if let Some(incoming) = &incoming.unknown {
            if let Some(previous) = &mut self.unknown {
                changed |= previous.merge_capture(incoming);
            } else {
                self.unknown = Some(incoming.clone());
                changed = true;
            }
        }

        changed
    }

}

pub(crate) fn deferred_selection<'a>(facts: impl IntoIterator<Item = &'a (ExecutionCondition, bool)>) -> Selection {
    facts
        .into_iter()
        .filter(|(condition, _)| {
            !condition.contains_trusted() && !condition.observes_live(None)
        })
        .cloned()
        .collect()
}

pub(crate) fn deferred_possible(guard: &Selection, proof: &crate::execution_guarantees::ExecutionProof<'_>) -> bool {
    !guard
        .iter()
        .any(|(condition, holds)| proof.prove_trusted(condition) == Some(!holds))
}

impl ExecutionDeferredValue {
    pub(crate) fn unknown(facts: &Selection) -> Self {
        Self {
            unknown: Some(ExecutionDeferredInvocation { callee: None, transition: None,
                entry: (), selection: deferred_selection(facts), completed: None, reports: BTreeMap::new(),
            }),
            ..Self::default()
        }
    }

    /// Retains the incoming branch's ordinary selection on the actual frame alternatives.
    pub(crate) fn constrain_selection(&mut self, facts: &Selection) {
        let facts = deferred_selection(facts);

        for guard in self.producers.values_mut().map(|candidate| &mut candidate.selection)
            .chain(self.unknown.iter_mut().map(|candidate| &mut candidate.selection))
        {
            guard.extend(facts.iter().cloned());
        }
    }

    pub(crate) fn unknown_possible(&self, proof: &crate::execution_guarantees::ExecutionProof<'_>) -> bool {
        self.unknown
            .as_ref()
            .is_some_and(|candidate| deferred_possible(&candidate.selection, proof))
    }

    pub(crate) fn retire_selection(self: &Arc<Self>, stale: impl Fn(&ExecutionCondition) -> bool) -> Arc<Self> {
        let affected = self.unknown.iter().map(|candidate| &candidate.selection)
            .chain(self.producers.values().map(|candidate| &candidate.selection))
            .flatten().any(|(condition, _)| stale(condition));

        if !affected { return Arc::clone(self); }

        let mut updated = self.as_ref().clone();

        for guard in updated.unknown.iter_mut().map(|candidate| &mut candidate.selection).chain(
            updated.producers
                .values_mut()
                .map(|candidate| &mut candidate.selection),
        ) {
            guard.retain(|(condition, _)| !stale(condition));
        }

        Arc::new(updated)
    }

    /// Expires the reused evaluation in the actual frame's private admissions.
    pub(crate) fn retire_evaluation(
        self: &Arc<Self>,
        mutation: bray_bound_tree::StorageMutation,
        stale: &impl Fn(&ExecutionCondition) -> bool,
    ) -> Arc<Self> {
        let mut updated = self.retire_selection(stale);

        if updated.producers.values().any(|producer| producer.entry.observes_retiring_evaluation(stale)) {
            let frame = Arc::make_mut(&mut updated);

            for producer in frame.producers.values_mut().filter(|producer| producer.entry.observes_retiring_evaluation(stale)) {
                Arc::make_mut(&mut producer.entry).retire_evaluation(mutation, stale);
            }
        }

        updated
    }
}
