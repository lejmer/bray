use bray_bound_tree::{BoundExpressionId, ReportDependencies};
use bray_symbols::{DependencyRequirement, LocalBindingSymbolId};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// Snapshots retain the outcome they came from through joins and frame capture.
/// An unindexed snapshot represents an opaque producer.
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

    pub(crate) fn admitted(&self, accepts: impl Fn(usize) -> bool) -> Option<Arc<ReportDependencies>> {
        let mut selected = self.0.iter().filter_map(|(row, report)| {
            row.is_none_or(&accepts).then_some(report)
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
    report_candidates: BTreeMap<
        (
            bray_bound_tree::BoundExecutionSite,
            bray_bound_tree::CallReportOutput,
        ),
        ReportCandidates,
    >,
}

impl State {
    pub(crate) fn merge(&mut self, incoming: &Self) -> bool {
        let mut changed = merge_map(&mut self.values, &incoming.values);

        changed |= merge_map(&mut self.locals, &incoming.locals);
        changed |= merge_map(&mut self.local_sources, &incoming.local_sources);
        changed |= merge_map(&mut self.sources, &incoming.sources);
        changed |= merge_map(&mut self.call_definitions, &incoming.call_definitions);

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

        let mut selected = self.report_candidates.remove(&(occurrence, output)).unwrap_or_default();

        if let Some(opaque) = output.opaque()
            && let Some(opaque) = self.report_candidates.remove(&(occurrence, opaque))
        {
            selected.merge(&opaque);
        }

        for failure in bray_symbols::DependencyFailureKind::ALL.map(Some).into_iter().chain([None]) {
            self.report_candidates.remove(&(occurrence, CallReportOutput::Abnormal(failure)));
        }

        self.report_candidates.remove(&(occurrence, CallReportOutput::Completed));

        selected
    }

    pub(crate) fn owner_requirements(
        &self,
        storage: &bray_bound_tree::StoragePlan,
        root: bray_bound_tree::StorageIdentityId,
        formal: Option<bray_symbols::DependencySubjectRoot>,
    ) -> BTreeSet<DependencyRequirement> {
        if formal == Some(bray_symbols::DependencySubjectRoot::Result) {
            return self.returned.clone();
        }

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
            Some(bray_bound_tree::StorageIdentity::InvocationResult(invocation)) => {
                values.extend(self.value_requirements(invocation).cloned());
            }
            Some(bray_bound_tree::StorageIdentity::ScopedCapability { expression, .. }) => {
                values.extend(self.value_requirements(bray_bound_tree::BoundExecutionSite::ScopedEnter(expression)).cloned());
            }
            Some(bray_bound_tree::StorageIdentity::ScopedResultReplacement(expression)) => {
                values.extend(self.value_requirements(bray_bound_tree::BoundExecutionSite::Cleanup(expression.into())).cloned());
            }
            Some(
                bray_bound_tree::StorageIdentity::Temporary(expression)
                | bray_bound_tree::StorageIdentity::Evaluated(expression)
                | bray_bound_tree::StorageIdentity::Result(
                    bray_bound_tree::AnyBoundNodeId::Expression(expression),
                ),
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

    pub(crate) fn take_value(&mut self, site: bray_bound_tree::BoundExecutionSite) -> BTreeSet<DependencyRequirement> {
        self.values.remove(&site).unwrap_or_default()
    }

    pub(crate) fn abandon_transfer(
        &mut self,
        storage: &bray_bound_tree::StoragePlan,
        value: bray_bound_tree::StorageIdentityId,
    ) {
        if storage.binding(bray_bound_tree::StorageBindingTarget::Result) == Some(bray_bound_tree::StorageBinding::Identity(value)) {
            self.returned.clear();
        } else if let Some(bray_bound_tree::AnyBoundNodeId::Expression(expression)) =
            storage.identity(value).and_then(|identity| identity.definition_node())
            && storage.value_slot(expression) == Some(value)
        {
            self.values.remove(&expression.into());
            self.sources.remove(&expression);
        }
    }

    pub(crate) fn select_scoped_region(&mut self, transfer: crate::analysis::ScopedResultTransfer) {
        let crate::analysis::ScopedResultDestination::Region { displaced, .. } = transfer.destination else {
            return;
        };

        let expression = transfer.invocation.expression().expect("scoped region retains its source expression");
        let site = expression.into();
        let requirements = if transfer.source.is_some() { self.take_value(transfer.invocation) } else { BTreeSet::new() };

        if displaced.is_some() {
            let previous = self.take_value(site);

            assign(&mut self.values, bray_bound_tree::BoundExecutionSite::Cleanup(expression.into()), previous);
        }

        assign(&mut self.values, site, requirements);
    }

    pub(crate) fn publish_catch_value(
        &mut self,
        boundary: BoundExpressionId,
        requirements: BTreeSet<DependencyRequirement>,
    ) {
        assign(&mut self.values, boundary.into(), requirements);
    }

    pub(crate) fn summary(mut self) -> bray_symbols::DependencyContractTemplateData {
        let returned = std::mem::take(&mut self.returned);

        bray_symbols::DependencyContractTemplateData::new(self.closed_requirements(returned))
    }

    pub(crate) fn closed_requirements(
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
