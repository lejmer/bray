use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::{ExecutionCallEvidence, ExecutionCondition};
use bray_bound_tree::BoundExecutionSite;

type Selection = BTreeSet<(ExecutionCondition, bool)>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct DeferredInvocation {
    pub(super) entry: Arc<ExecutionCallEvidence>,
    pub(super) selection: Selection,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct DeferredValue {
    pub(super) producers: BTreeMap<BoundExecutionSite, DeferredInvocation>,
    pub(super) unknown: Option<Selection>,
}

pub(super) fn selection(facts: &Selection) -> Selection {
    facts
        .iter()
        .filter(|(condition, _)| {
            !condition.contains_trusted() && !condition.observes_borrowed(None)
        })
        .cloned()
        .collect()
}

pub(super) fn possible(guard: &Selection, facts: &Selection) -> bool {
    !guard
        .iter()
        .any(|(condition, holds)| condition.prove_trusted(&BTreeSet::new(), facts) == Some(!holds))
}

impl DeferredValue {
    pub(super) fn unknown(facts: &Selection) -> Self {
        Self {
            unknown: Some(selection(facts)),
            ..Self::default()
        }
    }

    pub(super) fn unknown_possible(&self, facts: &Selection) -> bool {
        self.unknown
            .as_ref()
            .is_some_and(|guard| possible(guard, facts))
    }

    pub(super) fn known_sources(&self, facts: &Selection) -> Option<BTreeSet<BoundExecutionSite>> {
        (!self.unknown_possible(facts)).then(|| {
            self.producers
                .iter()
                .filter(|(_, candidate)| possible(&candidate.selection, facts))
                .map(|(source, _)| *source)
                .collect()
        })
    }

    pub(super) fn merge(&mut self, incoming: &Self) -> bool {
        let mut changed = false;

        for (source, candidate) in &incoming.producers {
            if let Some(previous) = self.producers.get_mut(source) {
                if previous != candidate {
                    changed |= Arc::make_mut(&mut previous.entry).intersect(&candidate.entry);

                    let count = previous.selection.len();

                    previous
                        .selection
                        .retain(|fact| candidate.selection.contains(fact));

                    changed |= count != previous.selection.len();
                }
            } else {
                // Producers share captured evidence while each flow branch owns its selection facts.
                self.producers.insert(*source, candidate.clone());
                changed = true;
            }
        }

        if let Some(incoming) = &incoming.unknown {
            if let Some(guard) = &mut self.unknown {
                let count = guard.len();

                guard.retain(|fact| incoming.contains(fact));
                changed |= count != guard.len();
            } else {
                self.unknown = Some(incoming.clone());
                changed = true;
            }
        }

        changed
    }

    pub(super) fn join_values(
        &mut self,
        replacements: &BTreeMap<ExecutionCondition, ExecutionCondition>,
        identities: &BTreeMap<ExecutionCondition, ExecutionCondition>,
        context: Option<&Selection>,
    ) {
        for guard in self.unknown.iter_mut().chain(
            self.producers
                .values_mut()
                .map(|candidate| &mut candidate.selection),
        ) {
            if let Some(context) = context {
                guard.extend(selection(context));
            }

            *guard = guard
                .iter()
                .flat_map(|(condition, holds)| {
                    let mut facts = BTreeSet::new();

                    condition
                        .with_joined_values(replacements, identities)
                        .assume(*holds, &mut facts);

                    facts
                })
                .collect();
        }

        for candidate in self.producers.values_mut() {
            Arc::make_mut(&mut candidate.entry).join_values(replacements, identities);
        }
    }

    pub(super) fn retire_selection(&mut self, stale: impl Fn(&ExecutionCondition) -> bool) {
        for guard in self.unknown.iter_mut().chain(
            self.producers
                .values_mut()
                .map(|candidate| &mut candidate.selection),
        ) {
            guard.retain(|(condition, _)| !stale(condition));
        }
    }
}
