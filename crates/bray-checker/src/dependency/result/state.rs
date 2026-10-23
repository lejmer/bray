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
    reports: BTreeMap<Option<BoundExpressionId>, BTreeSet<DependencyRequirement>>,
    report_candidates: BTreeMap<
        (
            bray_bound_tree::BoundExecutionSite,
            bray_bound_tree::CallReportOutput,
        ),
        BTreeSet<DependencyRequirement>,
    >,
    completed_reports: Option<BTreeSet<DependencyRequirement>>,
    abnormal_reports: Option<BTreeSet<DependencyRequirement>>,
}

impl State {
    pub(crate) fn merge(&mut self, incoming: &Self) -> bool {
        let mut changed = merge_map(&mut self.values, &incoming.values);

        changed |= merge_map(&mut self.locals, &incoming.locals);
        changed |= merge_map(&mut self.local_sources, &incoming.local_sources);
        changed |= merge_map(&mut self.sources, &incoming.sources);
        changed |= merge_map(&mut self.call_definitions, &incoming.call_definitions);
        changed |= merge_map(&mut self.reports, &incoming.reports);
        changed |= merge_map(&mut self.report_candidates, &incoming.report_candidates);
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
        merge_reports(&mut self.completed_reports, &incoming.reports);
    }

    pub(crate) fn merge_abnormal(&mut self, incoming: &Self) {
        merge_map(&mut self.call_definitions, &incoming.call_definitions);
        merge_reports(&mut self.abnormal_reports, &incoming.reports);
        self.abnormal_reports.get_or_insert_default();
    }

    pub(crate) fn prepare_reports(
        &mut self,
        occurrence: bray_bound_tree::BoundExecutionSite,
        output: bray_bound_tree::CallReportOutput,
        requirements: BTreeSet<DependencyRequirement>,
    ) {
        self.report_candidates
            .insert((occurrence, output), requirements);
    }

    pub(crate) fn report_candidate(
        &self,
        occurrence: bray_bound_tree::BoundExecutionSite,
        output: bray_bound_tree::CallReportOutput,
    ) -> Option<&BTreeSet<DependencyRequirement>> {
        self.report_candidates.get(&(occurrence, output))
    }

    pub(crate) fn owner_requirements(
        &self,
        storage: &bray_bound_tree::StoragePlan,
        root: bray_bound_tree::StorageIdentityId,
        formal: Option<bray_symbols::DependencySubjectRoot>,
    ) -> BTreeSet<DependencyRequirement> {
        if let Some(formal) = formal {
            return BTreeSet::from([DependencyRequirement::direct(
                bray_symbols::DependencySubject::root(formal),
                bray_symbols::DependencyRequirementKind::ValueDependencies,
            )]);
        }

        let mut values = BTreeSet::new();

        for binding in storage.binding_targets(bray_bound_tree::StorageBinding::Identity(root)) {
            if let bray_bound_tree::StorageBindingTarget::Local(binding) = binding {
                values.extend(self.locals.get(&binding).into_iter().flatten().cloned());
            }
        }

        match storage.identity(root) {
            Some(
                bray_bound_tree::StorageIdentity::Temporary(expression)
                | bray_bound_tree::StorageIdentity::Result(
                    bray_bound_tree::AnyBoundNodeId::Expression(expression),
                )
                | bray_bound_tree::StorageIdentity::ScopedCapability { expression, .. },
            ) => {
                values.extend(self.value_requirements(expression).cloned());
            }
            _ => {}
        }

        values
    }

    pub(crate) fn value_requirements(
        &self,
        expression: BoundExpressionId,
    ) -> impl Iterator<Item = &DependencyRequirement> {
        self.values.get(&expression).into_iter().flatten()
    }

    pub(crate) fn commit_reports(
        &mut self,
        occurrence: bray_bound_tree::BoundExecutionSite,
        output: bray_bound_tree::CallReportOutput,
        boundary: Option<BoundExpressionId>,
    ) {
        let requirements = self
            .report_candidates
            .remove(&(occurrence, output))
            .expect("a checked report output has its entry dependency snapshot");

        self.discard_report_candidates(occurrence);
        self.retain_reports(boundary, requirements);
    }

    pub(crate) fn discard_report_candidates(
        &mut self,
        occurrence: bray_bound_tree::BoundExecutionSite,
    ) {
        for output in [
            bray_bound_tree::CallReportOutput::Completed,
            bray_bound_tree::CallReportOutput::Abnormal,
        ] {
            self.report_candidates.remove(&(occurrence, output));
        }
    }

    pub(crate) fn retain_reports(
        &mut self,
        boundary: Option<BoundExpressionId>,
        requirements: impl IntoIterator<Item = DependencyRequirement>,
    ) {
        self.reports
            .entry(boundary)
            .or_default()
            .extend(requirements);
    }

    pub(crate) fn transfer_report_boundary(
        &mut self,
        boundary: BoundExpressionId,
        parent: Option<BoundExpressionId>,
        caught: bool,
    ) {
        if let Some(requirements) = self.reports.remove(&Some(boundary)) {
            if caught {
                assign(&mut self.values, boundary, requirements);
            } else {
                self.retain_reports(parent, requirements);
            }
        }
    }

    pub(crate) fn summary(mut self) -> bray_symbols::DependencyContractTemplateData {
        let returned = std::mem::take(&mut self.returned);

        let mut summary =
            bray_symbols::DependencyContractTemplateData::new(self.closed_requirements(returned));

        if let Some(requirements) = self.completed_reports.take() {
            summary = summary.with_completion_reports(self.closed_requirements(requirements));
        }

        if let Some(requirements) = self.abnormal_reports.take() {
            summary = summary.with_abnormal_reports(self.closed_requirements(requirements));
        }

        summary
    }

    fn closed_requirements(
        &self,
        requirements: BTreeSet<DependencyRequirement>,
    ) -> Vec<DependencyRequirement> {
        let requirements = requirements.into_iter().collect::<Vec<_>>();

        if !crate::dependency::equations::has_variables(&requirements, false) {
            return requirements;
        }

        let maximum = self
            .call_definitions
            .keys()
            .next_back()
            .copied()
            .unwrap_or(0);

        let definitions = (0..=maximum).map(|ordinal| {
            // Each output owns its equation group while immutable subjects remain shared.
            self.call_definitions
                .get(&ordinal)
                .cloned()
                .unwrap_or_default()
        });

        vec![DependencyRequirement::fixed_point(
            definitions,
            requirements,
        )]
    }
}

fn merge_reports(
    output: &mut Option<BTreeSet<DependencyRequirement>>,
    reports: &BTreeMap<Option<BoundExpressionId>, BTreeSet<DependencyRequirement>>,
) {
    if !reports.is_empty() {
        output
            .get_or_insert_default()
            .extend(reports.values().flatten().cloned());
    }
}

fn merge_map<K: Ord + Clone, V: Ord + Clone>(
    target: &mut BTreeMap<K, BTreeSet<V>>,
    incoming: &BTreeMap<K, BTreeSet<V>>,
) -> bool {
    let mut changed = false;

    for (key, values) in incoming {
        match target.entry(key.clone()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                // Each CFG entry owns its requirement set. Presence can denote an independent report.
                entry.insert(values.clone());
                changed = true;
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                changed |= super::core::extend(entry.get_mut(), values.iter().cloned());
            }
        }
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
