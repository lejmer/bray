use bray_bound_tree::BoundExpressionId;
use bray_symbols::{DependencyRequirement, LocalBindingSymbolId};
use std::collections::{BTreeMap, BTreeSet};

/// Possible retained dependencies travel with the same control-flow state as storage.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct State {
    pub(super) values: BTreeMap<BoundExpressionId, BTreeSet<DependencyRequirement>>,
    pub(super) locals: BTreeMap<LocalBindingSymbolId, BTreeSet<DependencyRequirement>>,
    pub(super) local_sources: BTreeMap<LocalBindingSymbolId, BTreeSet<super::sources::SourcePath>>,
    pub(super) borrowed_bindings: BTreeMap<LocalBindingSymbolId, BoundExpressionId>,
    pub(super) sources: BTreeMap<BoundExpressionId, BTreeSet<super::sources::SourcePath>>,
    pub(super) call_definitions: BTreeMap<u32, BTreeSet<DependencyRequirement>>,
    pub(super) returned: BTreeSet<DependencyRequirement>,
}

impl State {
    pub(crate) fn merge(&mut self, incoming: &Self) -> bool {
        let mut changed = merge_map(&mut self.values, &incoming.values);

        changed |= merge_map(&mut self.locals, &incoming.locals);
        changed |= merge_map(&mut self.local_sources, &incoming.local_sources);
        changed |= merge_map(&mut self.sources, &incoming.sources);
        changed |= merge_map(&mut self.call_definitions, &incoming.call_definitions);
        changed |= super::core::extend(&mut self.returned, incoming.returned.iter().cloned());

        for (binding, expression) in &incoming.borrowed_bindings {
            if let Some(previous) = self.borrowed_bindings.insert(*binding, *expression) {
                assert_eq!(
                    previous, *expression,
                    "one pattern binding has one source expression"
                );
            } else {
                changed = true;
            }
        }

        changed
    }

    pub(crate) fn merge_returns(&mut self, incoming: &Self) {
        merge_map(&mut self.call_definitions, &incoming.call_definitions);
        self.returned.extend(incoming.returned.iter().cloned());
    }

    pub(crate) fn requirements(mut self) -> Vec<DependencyRequirement> {
        let requirements = self.returned.into_iter().collect::<Vec<_>>();

        if !crate::dependency::equations::has_variables(&requirements, false) {
            return requirements;
        }

        let maximum = self
            .call_definitions
            .keys()
            .next_back()
            .copied()
            .unwrap_or(0);

        let definitions =
            (0..=maximum).map(|ordinal| self.call_definitions.remove(&ordinal).unwrap_or_default());

        vec![DependencyRequirement::fixed_point(
            definitions,
            requirements,
        )]
    }
}

fn merge_map<K: Ord + Clone, V: Ord + Clone>(
    target: &mut BTreeMap<K, BTreeSet<V>>,
    incoming: &BTreeMap<K, BTreeSet<V>>,
) -> bool {
    let mut changed = false;

    for (key, values) in incoming {
        // Each CFG entry owns its possible requirements independently of its predecessors.
        changed |= super::core::extend(
            target.entry(key.clone()).or_default(),
            values.iter().cloned(),
        );
    }

    changed
}

/// Empty dependency sets are implicit so independent scalar code adds no flow-state entries.
pub(super) fn assign<K: Ord, V>(map: &mut BTreeMap<K, BTreeSet<V>>, key: K, values: BTreeSet<V>) {
    if values.is_empty() {
        map.remove(&key);
    } else {
        map.insert(key, values);
    }
}
