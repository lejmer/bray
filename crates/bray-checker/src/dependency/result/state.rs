use bray_bound_tree::BoundExpressionId;
use bray_symbols::{DependencyRequirement, LocalBindingSymbolId};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// Scope obligations and portable dependencies captured by the same report owner.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct ReportDependencies {
    pub(crate) subjects: BTreeSet<bray_bound_tree::BoundDependencySubject>,
    pub(crate) requirements: BTreeSet<DependencyRequirement>,
}

impl ReportDependencies {
    pub(crate) fn merge(self: &mut Arc<Self>, incoming: &Self) -> bool {
        if incoming.subjects.is_subset(&self.subjects)
            && incoming.requirements.is_subset(&self.requirements)
        {
            return false;
        }

        let retained = Arc::make_mut(self);

        retained.subjects.extend(incoming.subjects.iter().copied());
        retained.requirements.extend(incoming.requirements.iter().cloned());

        true
    }
}

/// Snapshots retain the outcome they came from through joins and frame capture.
/// An unindexed snapshot represents the abnormal output or an opaque producer.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct ReportCandidates(BTreeMap<Option<usize>, Arc<ReportDependencies>>);

impl ReportCandidates {
    pub(crate) fn insert(&mut self, row: Option<usize>, report: Arc<ReportDependencies>) {
        self.0.insert(row, report);
    }

    pub(crate) fn merge(&mut self, incoming: &Self) -> bool {
        let mut changed = false;

        for (row, report) in &incoming.0 {
            if let Some(prior) = self.0.get_mut(row) {
                changed |= prior.merge(report);
            } else {
                self.0.insert(*row, Arc::clone(report));
                changed = true;
            }
        }

        changed
    }

    pub(crate) fn admitted(&self, outputs: Option<&crate::execution_guarantees::ExecutionOutputs>) -> Option<Arc<ReportDependencies>> {
        let mut selected = self.0.iter().filter_map(|(row, report)| {
            row.is_none_or(|row| outputs.is_none_or(|outputs| outputs.is_admitted(row))).then_some(report)
        });

        let mut report = Arc::clone(selected.next()?);

        for other in selected {
            report.merge(other);
        }

        Some(report)
    }
}

/// Possible retained dependencies travel with the same control-flow state as storage.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct State {
    pub(super) values:
        BTreeMap<bray_bound_tree::BoundExecutionSite, BTreeSet<DependencyRequirement>>,
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
        ReportCandidates,
    >,
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

        for (output, dependencies) in &incoming.report_candidates {
            if let Some(previous) = self.report_candidates.get_mut(output) {
                changed |= previous.merge(dependencies);
            } else {
                self.report_candidates.insert(*output, dependencies.clone());
                changed = true;
            }
        }

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

    pub(crate) fn completed_report(&self) -> Option<Arc<[DependencyRequirement]>> {
        (!self.reports.is_empty()).then(|| self.closed_requirements(self.reports.values().flatten().cloned().collect()).into())
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
        row: Option<usize>,
        dependencies: Arc<ReportDependencies>,
    ) {
        self.report_candidates.entry((occurrence, output)).or_default().insert(row, dependencies);
    }

    pub(crate) fn take_report_candidates(
        &mut self,
        occurrence: bray_bound_tree::BoundExecutionSite,
        output: bray_bound_tree::CallReportOutput,
    ) -> ReportCandidates {
        use bray_bound_tree::CallReportOutput;

        let selected = self.report_candidates.remove(&(occurrence, output)).unwrap_or_default();

        self.report_candidates.remove(&(occurrence, CallReportOutput::Completed));
        self.report_candidates.remove(&(occurrence, CallReportOutput::Abnormal));

        selected
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
            Some(bray_bound_tree::StorageIdentity::InvocationInput {
                expression,
                ordinal,
            }) => {
                values.extend(
                    self.value_requirements(bray_bound_tree::BoundExecutionSite::CallInput {
                        expression,
                        ordinal,
                    })
                    .cloned(),
                );
            }
            Some(
                bray_bound_tree::StorageIdentity::Temporary(expression)
                | bray_bound_tree::StorageIdentity::Evaluated(expression)
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
        expression: impl Into<bray_bound_tree::BoundExecutionSite>,
    ) -> impl Iterator<Item = &DependencyRequirement> {
        self.values.get(&expression.into()).into_iter().flatten()
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
                assign(&mut self.values, boundary.into(), requirements);
            } else {
                self.retain_reports(parent, requirements);
            }
        }
    }

    pub(crate) fn summary(mut self) -> bray_symbols::DependencyContractTemplateData {
        let returned = std::mem::take(&mut self.returned);

        let mut summary =
            bray_symbols::DependencyContractTemplateData::new(self.closed_requirements(returned));

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
