use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::BoundExpressionId;

use crate::ExecutionCondition;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ExecutionState {
    pub(super) trust_boundaries: BTreeSet<BoundExpressionId>,
    pub(super) assumptions: BTreeSet<(ExecutionCondition, bool)>,
    pub(super) witness_carriers: BTreeSet<ExecutionCondition>,
    pub(super) witness_dependencies: BTreeMap<ExecutionCondition, BTreeSet<crate::ExecutionPlace>>,
    pub(super) trusted_assumptions: BTreeSet<(ExecutionCondition, bool)>,
    pub(super) current: BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
    pub(super) expressions: BTreeMap<BoundExpressionId, ExecutionCondition>,
    pub(super) pending_results: BTreeMap<BoundExpressionId, ExecutionCondition>,
    pub(super) result: ExecutionCondition,
    pub(super) entries: BTreeMap<BoundExpressionId, crate::ExecutionCallEvidence>,
    pub(super) completion_dependencies: BTreeSet<crate::ExecutionCompletionDependency>,
}

impl Default for ExecutionState {
    fn default() -> Self {
        Self {
            trust_boundaries: BTreeSet::new(),
            assumptions: BTreeSet::new(),
            witness_carriers: BTreeSet::new(),
            witness_dependencies: BTreeMap::new(),
            trusted_assumptions: BTreeSet::new(),
            current: BTreeMap::new(),
            expressions: BTreeMap::new(),
            pending_results: BTreeMap::new(),
            result: ExecutionCondition::Unknown,
            entries: BTreeMap::new(),
            completion_dependencies: BTreeSet::new(),
        }
    }
}

impl ExecutionState {
    pub(super) fn is_witness(&self, value: &ExecutionCondition) -> bool {
        self.witness_carriers.iter().any(|carrier| value.contains_value(carrier)
            && self.trusted_assumptions.iter().any(|(condition, holds)| *holds && condition.observes(carrier)
                && condition.prove(&self.assumptions, &mut { ExecutionCondition::WORK_LIMIT }) != Some(true)))
    }

    pub(super) fn invalidate_cleanup(&mut self) {
        self.witness_carriers.clear();
        self.witness_dependencies.clear();
        self.trusted_assumptions.clear();

        for entry in self.entries.values_mut().filter(|entry| entry.pending_execution) {
            entry.assumptions.clear();
            entry.trusted_assumptions.clear();
            entry.trusted_boundary = false;
        }

        // Cleanup can mutate observations through owned capabilities, just like an opaque call.
        for (place, value) in &mut self.current {
            if !place.fields.is_empty() {
                *value = ExecutionCondition::Unknown;
            }
        }
    }

    pub(super) fn assign(&mut self, place: crate::ExecutionPlace, value: ExecutionCondition) {
        self.current.retain(|observed, _| !place.contains(observed));
        self.current.insert(place, value);
    }

    pub(super) fn invalidate_trusted(&mut self, value: &ExecutionCondition) {
        let mut invalidated = BTreeSet::from([value.clone()]);

        // A returned address or capability can retain an owner through several checked wrappers.
        loop {
            let before = invalidated.len();

            let components = invalidated.iter().filter_map(|value| match value {
                ExecutionCondition::Constructed(_, fields) => Some(fields.iter().map(|(_, value)| value.clone())),
                _ => None,
            }).flatten().collect::<Vec<_>>();

            invalidated.extend(components);

            for (carrier, dependencies) in &self.witness_dependencies {
                if dependencies.iter().filter_map(|place| place.value_in(&self.current)).any(|dependency|
                    invalidated.iter().any(|value| dependency.observes(value))) {
                    invalidated.insert(carrier.clone());
                }
            }

            if invalidated.len() == before { break; }
        }

        self.witness_carriers.retain(|carrier| !invalidated.iter().any(|value| carrier.observes(value)));

        self.trusted_assumptions.retain(|(condition, _)|
            !invalidated.iter().any(|value| condition.observes(value)));

        self.witness_dependencies.retain(|carrier, _|
            !invalidated.iter().any(|value| carrier.observes(value)));

        for entry in self.entries.values_mut().filter(|entry| entry.pending_execution) {
            entry.assumptions.retain(|(condition, _)| !invalidated.iter().any(|value| condition.observes(value)));
            entry.trusted_assumptions.retain(|(condition, _)| !invalidated.iter().any(|value| condition.observes(value)));
        }
    }

    pub(super) fn invalidate_trusted_place(&mut self, place: &crate::ExecutionPlace) {
        let dependent = self.witness_dependencies.iter().filter(|(_, owners)|
            owners.iter().any(|owner| owner.overlaps(place))).map(|(carrier, _)| carrier.clone()).collect::<Vec<_>>();

        for carrier in dependent {
            self.invalidate_trusted(&carrier);
        }
    }
}
