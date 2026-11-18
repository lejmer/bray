use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::{ExecutionCallEvidence, ExecutionCondition};
use bray_bound_tree::BoundExecutionSite;

/// Stable analysis provenance for joining alternatives of one carried frame.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum DeferredProducer {
    Local(BoundExecutionSite),
    Summary {
        instantiation: bray_bound_tree::StorageMutation,
        callee: Option<Arc<bray_symbols::CapturedCallable>>,
        alternative: usize,
    },
}

impl DeferredProducer {
    pub(crate) fn local(&self) -> Option<BoundExecutionSite> {
        match self {
            Self::Local(site) => Some(*site),
            Self::Summary { .. } => None,
        }
    }
}

impl From<BoundExecutionSite> for DeferredProducer {
    fn from(site: BoundExecutionSite) -> Self { Self::Local(site) }
}

#[cfg(test)]
impl From<bray_bound_tree::BoundExpressionId> for DeferredProducer {
    fn from(expression: bray_bound_tree::BoundExpressionId) -> Self { Self::Local(expression.into()) }
}

type Selection = BTreeSet<(ExecutionCondition, bool)>;

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
    pub(crate) reports: BTreeMap<bray_bound_tree::CallReportOutput, crate::dependency::ReportCandidates>,
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
    pub(crate) producers: BTreeMap<DeferredProducer, ExecutionDeferredInvocation>,
    pub(crate) unknown: Option<ExecutionDeferredInvocation<()>>,
}

impl ExecutionDeferredValue {
    pub(crate) fn storage_references(&self, target: &bray_bound_tree::StorageTarget,
        retirement: bray_bound_tree::StorageValueRetirement,
    ) -> bool {
        self.producers.values().filter_map(|producer| producer.transition.as_ref())
            .chain(self.unknown.iter().filter_map(|producer| producer.transition.as_ref()))
            .any(|transition| transition.transfer.references(target, retirement))
    }

    pub(crate) fn retire_storage(&mut self, target: &bray_bound_tree::StorageTarget,
        retirement: bray_bound_tree::StorageValueRetirement,
    ) {
        for transition in self.producers.values_mut().filter_map(|producer| producer.transition.as_mut())
            .chain(self.unknown.iter_mut().filter_map(|producer| producer.transition.as_mut()))
        {
            if transition.transfer.references(target, retirement) { Arc::make_mut(transition).transfer.retire_dependencies(target, retirement); }
        }
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

impl ExecutionDeferredValue {
    /// Latent work of feasible alternatives of this frame. Captured child frames remain latent.
    pub(crate) fn transitions(&self, proof: &super::ExecutionProof<'_>)
        -> Option<Vec<Arc<bray_bound_tree::StorageDeferredTransition>>>
    {
        if self.unknown_possible(proof) { return None; }

        self.producers.values().filter(|producer| deferred_possible(&producer.selection, proof))
            .map(|producer| producer.transition.clone()).collect()
    }

    pub(crate) fn effects(&self, proof: &super::ExecutionProof<'_>)
        -> Option<bray_symbols::DependencyEffectOutcomes<bray_bound_tree::StorageEffect>>
    {
        if self.unknown_possible(proof) { return None; }

        let mut effects = bray_symbols::DependencyEffectOutcomes::new([]);

        for producer in self.producers.values().filter(|producer| deferred_possible(&producer.selection, proof)) {
            let transfer = &producer.transition.as_ref()?.transfer;
            let resolved = transfer.deferred_effects.as_ref()?;

            if let Some(rows) = &transfer.deferred_normal_rows
                && rows.keys().any(|index| !producer.entry.outputs.is_admitted(*index))
            {
                let selected = rows.iter().filter(|(index, _)| producer.entry.outputs.is_admitted(**index))
                    .flat_map(|(_, effects)| effects.iter().cloned());

                effects.merge(&resolved.clone().with_normal(selected));
            } else {
                effects.merge(resolved);
            }
        }

        Some(effects)
    }

    pub(crate) fn cleanup_effects(&self, discard_result: bool, proof: &super::ExecutionProof<'_>)
        -> Option<Vec<bray_bound_tree::StorageEffect>>
    {
        let mut effects = BTreeSet::new();

        for transition in self.transitions(proof)? {
            effects.extend(transition.transfer.captured_cleanup.as_ref()?.iter().cloned());

            if discard_result { effects.extend(transition.transfer.result_cleanup.as_ref()?.iter().cloned()); }
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
                self.reports.insert(*output, incoming.clone());
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
                        changed |= Arc::make_mut(&mut previous.entry).intersect(&candidate.entry, cache);
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

pub(crate) fn deferred_selection(facts: &Selection) -> Selection {
    facts
        .iter()
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

    pub(crate) fn known_sources(&self, proof: &crate::execution_guarantees::ExecutionProof<'_>) -> Option<BTreeSet<BoundExecutionSite>> {
        (!self.unknown_possible(proof)).then(|| {
            self.producers
                .iter()
                .filter(|(_, candidate)| deferred_possible(&candidate.selection, proof))
                .map(|(source, _)| source.local())
                .collect()
        }).flatten()
    }

    pub(crate) fn retire_selection(&mut self, stale: impl Fn(&ExecutionCondition) -> bool) -> bool {
        let mut changed = false;

        for guard in self.unknown.iter_mut().map(|candidate| &mut candidate.selection).chain(
            self.producers
                .values_mut()
                .map(|candidate| &mut candidate.selection),
        ) {
            let before = guard.len();

            guard.retain(|(condition, _)| !stale(condition));
            changed |= guard.len() != before;
        }

        changed
    }
}
