use super::StorageFlowInput;
use bray_bound_tree::{
    BorrowCapabilityId, BoundExpressionId, StorageAccessId, StorageIdentityId, StoragePlan,
};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};
use std::sync::Arc;

use bray_bound_tree::{ReportDependencies, StorageAbsence};
use crate::analysis::outcomes::{CleanupProgress, OutcomeState};

pub(in crate::analysis::storage_flow) type EffectPrefix = Arc<BTreeSet<bray_symbols::DependencyStorageEffect>>;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(in crate::analysis) struct StorageFlowState {
    pub(in crate::analysis::storage_flow) reachable: bool,
    pub(in crate::analysis::storage_flow) execution:
        Option<crate::analysis::guarantee::ExecutionState>,
    pub(in crate::analysis::storage_flow) refinements: crate::analysis::refinement::RefinementSet,
    pub(in crate::analysis) live: BTreeSet<StorageIdentityId>,
    pub(in crate::analysis) initialized: BTreeSet<StorageIdentityId>,
    pub(in crate::analysis::storage_flow) observed_pattern_bindings: BTreeSet<StorageIdentityId>,
    pub(in crate::analysis) moved:
        BTreeMap<bray_bound_tree::StoragePath, BTreeMap<BoundExpressionId, bray_symbols::TypeId>>,
    pub(in crate::analysis) definitely_absent:
        Arc<BTreeMap<bray_bound_tree::StoragePath, StorageAbsence>>,
    // Every path that created this storage has transferred its whole value.
    pub(in crate::analysis) fully_moved: BTreeSet<StorageIdentityId>,
    pub(in crate::analysis::storage_flow) active_borrows: BTreeSet<BorrowCapabilityId>,
    pub(in crate::analysis::storage_flow) definitely_active_borrows: BTreeSet<BorrowCapabilityId>,
    // Committed writes into entry referents along every path to this state.
    pub(in crate::analysis::storage_flow) written_inputs: BTreeSet<bray_bound_tree::StoragePath>,
    pub(in crate::analysis::storage_flow) effect_prefix: EffectPrefix,
    // Admitted inputs shared by both successors before applying the normal outcome.
    pub(in crate::analysis::storage_flow) failure_entry: Option<AdmittedFailure>,
    pub(in crate::analysis::storage_flow) cleanup: Option<CleanupProgress>,
    pub(in crate::analysis::storage_flow) result_dependencies: crate::dependency::ResultState,
    pub(in crate::analysis::storage_flow) memory: crate::analysis::storage_flow::memory::State,
    reports: BTreeMap<Option<BoundExpressionId>, RetainedReports>,
    pub(in crate::analysis) recovered: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::analysis::storage_flow) struct AdmittedFailure {
    pub(in crate::analysis::storage_flow) occurrence: bray_bound_tree::BoundExecutionSite,
    pub(in crate::analysis::storage_flow) state: Arc<StorageFlowState>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RetainedReports {
    dependencies: Arc<ReportDependencies>,
    pending: bool,
}

impl OutcomeState for StorageFlowState {
    fn cleanup_progress(&self) -> Option<&CleanupProgress> {
        self.cleanup.as_ref()
    }
}

impl RetainedReports {
    fn merge(&mut self, incoming: &Self) -> bool {
        let changed = self.dependencies.merge(&incoming.dependencies);

        let pending = self.pending;

        self.pending |= incoming.pending;

        changed || self.pending != pending
    }
}

impl StorageFlowState {
    pub(in crate::analysis::storage_flow) fn failure_at(&self, occurrence: bray_bound_tree::BoundExecutionSite) -> Option<&AdmittedFailure> {
        self.failure_entry.as_ref().filter(|entry| entry.occurrence == occurrence)
    }

    pub(in crate::analysis::storage_flow) fn retain_reports(
        &mut self,
        boundary: Option<BoundExpressionId>,
        dependencies: Arc<ReportDependencies>,
        pending: bool,
    ) {
        self.merge_retained_reports(boundary, Cow::Owned(RetainedReports { dependencies, pending }));
    }

    fn merge_retained_reports(&mut self, boundary: Option<BoundExpressionId>, reports: Cow<'_, RetainedReports>) -> bool {
        match self.reports.entry(boundary) {
            Entry::Vacant(entry) => {
                entry.insert(reports.into_owned());

                true
            }
            Entry::Occupied(mut entry) => entry.get_mut().merge(&reports),
        }
    }

    pub(in crate::analysis::storage_flow) fn completed_report(&self) -> Option<Arc<[bray_symbols::DependencyRequirement]>> {
        if self.reports.is_empty() { return None; }

        let requirements = self.reports.values().flat_map(|report| report.dependencies.requirements.iter().cloned()).collect();

        Some(self.result_dependencies.closed_requirements(requirements).into())
    }

    pub(in crate::analysis::storage_flow) fn pending_report_boundaries(
        &self,
    ) -> impl Iterator<Item = Option<BoundExpressionId>> + '_ {
        self.reports
            .iter()
            .filter_map(|(boundary, reports)| reports.pending.then_some(*boundary))
    }

    pub(in crate::analysis::storage_flow) fn record_move(
        &mut self,
        path: bray_bound_tree::StoragePath,
        origin: BoundExpressionId,
        ty: bray_symbols::TypeId,
        definite: bool,
    ) {
        if definite && path.is_exact() {
            if path.projections.is_empty() {
                self.move_complete_storage(path.root);
            }

            // Both indexes own their physical path, sharing its immutable projections.
            Arc::make_mut(&mut self.definitely_absent)
                .insert(path.clone(), StorageAbsence::Moved);
        }

        self.moved.entry(path).or_default().insert(origin, ty);
    }

    pub(in crate::analysis::storage_flow) fn initialize_path(
        &mut self,
        path: &bray_bound_tree::StoragePath,
    ) {
        if path.is_exact() {
            self.fully_moved.remove(&path.root);

            if path.projections.is_empty() {
                self.live.insert(path.root);
                self.initialized.insert(path.root);
            }

            self.moved.retain(|moved, _| !path.contains(moved));
        }

        // An uncertain write cannot prove initialization, but can make prior absence uncertain.
        if !self.definitely_absent.is_empty() {
            Arc::make_mut(&mut self.definitely_absent).retain(|absent, _| {
                path.root != absent.root
                    || bray_bound_tree::StorageProjection::relationship(
                        &path.projections,
                        &absent.projections,
                    ) == bray_bound_tree::StorageRelationship::Disjoint
            });
        }
    }

    pub(in crate::analysis::storage_flow) fn end_storage_lifetime(
        &mut self,
        identity: StorageIdentityId,
        storage: &StoragePlan,
        input: &StorageFlowInput,
    ) {
        let slot = bray_bound_tree::StorageTarget::storage(bray_bound_tree::StoragePath::root(identity));

        if bray_bound_tree::storage_identity_transfers_at_unit_exit(storage, identity)
            || self.retained_report_value_requires(bray_bound_tree::BoundDependencySubject::Storage(identity))
            || self.recovered
            || self.active_borrows.iter().any(|borrow| {
                if self.memory.borrow_address(*borrow).is_some_and(|address| {
                    !address.is_unknown() && !address.is_displaced() && !address.targets.is_empty()
                }) {
                    self.memory.borrow_relationship(*borrow, std::iter::once(&slot), true)
                        != bray_bound_tree::StorageRelationship::Disjoint
                } else {
                    storage.borrow_capability(*borrow)
                        .is_some_and(|borrow| storage.root_identity(borrow.access()) == Some(identity))
                }
            })
        {
            return;
        }

        self.forget_storage(identity, storage, input);
    }

    fn retain_unheld_borrows(
        &self, ended: &mut BTreeSet<BorrowCapabilityId>, input: &StorageFlowInput,
    ) {
        use bray_bound_tree::BoundDependencySubject::BorrowCapability;

        let loans = self.memory.surviving_value_loans(
            |path| self.value_is_live(path), |path| Self::owns_cleanup(input, path.root),
        );
        let reports = self.reachable_reports(|_| true)
            .flat_map(|report| report.subjects.iter().copied())
            .filter_map(|subject| match subject {
                BorrowCapability(borrow) => Some(borrow),
                _ => None,
            });

        for loan in loans.chain(reports) {
            ended.remove(&loan);

            if ended.is_empty() { break; }
        }
    }

    pub(in crate::analysis::storage_flow) fn reachable_reports<'state>(&'state self,
        owner_remains: impl Fn(bray_bound_tree::StorageIdentityId) -> bool + 'state,
    ) -> impl Iterator<Item = &'state ReportDependencies> + 'state {
        let carried = self.memory.carried_reports(move |target| {
            target.storage_path().is_none_or(|path| {
                self.value_is_live(path) && owner_remains(path.root)
            })
        });

        self.reports.values().map(|report| report.dependencies.as_ref()).chain(carried)
    }

    pub(super) fn retained_report_value_requires(&self, subject: bray_bound_tree::BoundDependencySubject) -> bool {
        self.reachable_reports(|_| true).any(|report| report.subjects.contains(&subject))
    }

    pub(in crate::analysis::storage_flow) fn retained_dependencies<'a>(
        &'a self,
        input: &'a StorageFlowInput,
    ) -> impl Iterator<Item = bray_bound_tree::BoundDependencySubject> + 'a {
        self.live.iter().copied().filter(|owner| self.live_cleanup_owner(input, *owner))
            .map(bray_bound_tree::BoundDependencySubject::Storage)
            .chain(self.carried_cleanup_subjects(input))
            .chain(self.reachable_reports(|_| true).flat_map(|report| report.subjects.iter().copied()))
    }

    fn carried_cleanup_subjects<'a>(&'a self, input: &'a StorageFlowInput)
        -> impl Iterator<Item = bray_bound_tree::BoundDependencySubject> + 'a {
        self.memory.carried_value_subjects(move |path| self.value_is_live(path)
            && Self::owns_cleanup(input, path.root))
    }

    fn value_is_live(&self, path: &bray_bound_tree::StoragePath) -> bool {
        self.live.contains(&path.root) && !self.fully_moved.contains(&path.root)
            && !self.definitely_absent.keys().any(|absent| absent.contains(path))
    }

    /// A committed replacement ends displaced loans whose support has no surviving owner.
    pub(in crate::analysis::storage_flow) fn release_displaced_loans(
        &mut self, mut displaced: BTreeSet<BorrowCapabilityId>, storage: &StoragePlan, input: &StorageFlowInput,
    ) {
        if displaced.is_empty() || !self.reachable || self.recovered { return; }

        displaced.retain(|loan| self.active_borrows.contains(loan)
            && storage.borrow_capability(*loan).is_some_and(|borrow| borrow.entry_binding().is_none()));

        if displaced.is_empty() { return; }

        self.end_borrows(displaced, storage, input);
    }

    pub(in crate::analysis::storage_flow) fn transfer_report_boundary(
        &mut self,
        boundary: BoundExpressionId,
        parent: Option<BoundExpressionId>,
        caught: bool,
    ) -> Option<Arc<ReportDependencies>> {
        let report = self.reports.remove(&Some(boundary))?;

        if caught {
            Some(report.dependencies)
        } else {
            self.merge_retained_reports(parent, Cow::Owned(report));

            None
        }
    }

    fn live_cleanup_owner(&self, input: &StorageFlowInput, owner: StorageIdentityId) -> bool {
        self.live.contains(&owner) && !self.fully_moved.contains(&owner) && Self::owns_cleanup(input, owner)
    }

    fn owns_cleanup(input: &StorageFlowInput, owner: StorageIdentityId) -> bool {
        input.cleanup_requirements.get(owner).is_some_and(|requirement| {
            !requirement.transfers() && matches!(requirement.cleanup(), bray_bound_tree::AsyncStorageCleanupRequirement::Cleanup(_))
        })
    }

    pub(in crate::analysis::storage_flow) fn end_borrows(
        &mut self,
        mut ended: BTreeSet<BorrowCapabilityId>,
        storage: &StoragePlan,
        input: &StorageFlowInput,
    ) {
        if ended.is_empty() { return; }

        self.retain_unheld_borrows(&mut ended, input);

        if ended.is_empty() { return; }

        let mut supported = BTreeSet::new();

        for &borrow in self.active_borrows.difference(&ended) {
            if let Some(reached) = self.memory.borrow_ancestors(borrow) {
                supported.extend(reached.iter().copied().filter(|ancestor| ended.contains(ancestor)));
            } else if let Some(chain) = super::super::memory::borrow_chain_from(storage, Some(&self.memory), Some(borrow)) {
                supported.extend(chain.into_iter().filter(|ancestor| ended.contains(ancestor)));
            }
        }

        ended.retain(|borrow| !supported.contains(borrow));

        if ended.is_empty() { return; }

        self.memory.end_borrows(&ended);
        self.active_borrows.retain(|borrow| !ended.contains(borrow));

        self.definitely_active_borrows
            .retain(|borrow| !ended.contains(borrow));

        self.moved.retain(|access, _| {
            access
                .access_id(storage)
                .and_then(|access| storage.access(access))
                .is_none_or(|access| {
                    access
                        .root()
                        .borrow_capability()
                        .is_none_or(|borrow| !ended.contains(&borrow))
                })
        });

        self.retain_absent_storage();
    }

    pub(in crate::analysis::storage_flow) fn retain_absent_storage(&mut self) {
        if !self.definitely_absent.is_empty() {
            Arc::make_mut(&mut self.definitely_absent).retain(|path, _| self.live.contains(&path.root));
        }
    }

    pub(in crate::analysis::storage_flow) fn mark_absent(
        &mut self,
        access: StorageAccessId,
        reason: StorageAbsence,
        storage: &StoragePlan,
    ) {
        if let Some(path) = storage.logical_path(access).cloned() {
            Arc::make_mut(&mut self.definitely_absent)
                .entry(path)
                .and_modify(|current| {
                    if reason == StorageAbsence::Moved {
                        *current = reason;
                    }
                })
                .or_insert(reason);
        }
    }

    pub(in crate::analysis::storage_flow) fn invalidate_observed_absence(
        &mut self,
        invalidation: &bray_bound_tree::StorageInvalidation,
        discriminants: &BTreeMap<StorageIdentityId, BTreeSet<bray_bound_tree::StoragePath>>,
    ) {
        if self.definitely_absent.is_empty() {
            return;
        }

        Arc::make_mut(&mut self.definitely_absent).retain(|access, reason| {
            if *reason == StorageAbsence::Moved {
                return true;
            }

            let Some((_, parent)) = access.projections.split_last() else {
                return !invalidation.invalidates_path(access);
            };

            let descriptor = bray_bound_tree::StoragePath {
                root: access.root,
                projections: parent.into(),
            };

            let observed = discriminants.get(&descriptor.root).is_some_and(|paths| paths.contains(&descriptor));

            !invalidation.invalidates_discriminant(&descriptor, observed)
        });
    }

    pub(in crate::analysis::storage_flow) fn entry(
        values: &bray_symbols::SemanticValueStore,
        storage: &StoragePlan,
        input: &StorageFlowInput,
    ) -> Self {
        let initialized = storage
            .identity_entries()
            .filter_map(|(id, identity)| {
                (identity.is_initialized_at_entry() && !input.unused_entry_storage.contains(&id))
                    .then_some(id)
            })
            .collect::<BTreeSet<_>>();

        let active_borrows = storage
            .borrow_capability_entries()
            .filter_map(|(id, capability)| capability.entry_binding().is_some().then_some(id))
            .collect::<BTreeSet<_>>();

        let mut memory = crate::analysis::storage_flow::memory::State::for_storage(
            storage,
            &input.cleanup_requirements,
            &input.footprint.complete_reference_types,
        );

        for capability in &active_borrows {
            let borrow = storage
                .borrow_capability(*capability)
                .expect("entry capability is planned");

            if let Some(path) = storage.logical_path(borrow.access()) {
                let ty = storage
                    .access(borrow.access())
                    .expect("entry borrow has a planned target")
                    .reached_type();

                memory.begin_entry_borrow(*capability, path.clone(), ty);
            }
        }

        input.initialize_entry_references(values, &mut memory, &initialized);

        Self {
            reachable: true,
            execution: None,
            refinements: Default::default(),
            live: initialized.clone(),
            initialized,
            observed_pattern_bindings: BTreeSet::new(),
            moved: BTreeMap::new(),
            definitely_absent: Default::default(),
            fully_moved: BTreeSet::new(),
            definitely_active_borrows: active_borrows.clone(),
            active_borrows,
            memory,
            written_inputs: BTreeSet::new(),
            effect_prefix: EffectPrefix::default(),
            failure_entry: None,
            cleanup: None,
            result_dependencies: Default::default(),
            reports: BTreeMap::new(),
            recovered: false,
        }
    }

    pub(in crate::analysis::storage_flow) fn entry_addresses(&self)
        -> BTreeMap<bray_bound_tree::StoragePath, bray_bound_tree::StorageObservedTarget>
    {
        self.memory.typed_storage().filter_map(|(target, pointee)| {
            let bray_bound_tree::StorageOrigin::Entry {
                reference, kind: bray_symbols::DependencyReferenceKind::Address, ..
            } = &target.origin else { return None; };

            Some((reference.clone(), bray_bound_tree::StorageObservedTarget {
                target: target.clone(), pointee: Some(pointee),
            }))
        }).collect()
    }

    pub(in crate::analysis::storage_flow) fn merge<'join>(
        &mut self,
        incoming: &Self,
        join: impl Into<crate::analysis::guarantee::ExecutionJoin<'join>>,
    ) -> bool {
        if !incoming.reachable {
            return false;
        }

        if !self.reachable {
            // Each block owns its task-local state after propagation.
            *self = incoming.clone();

            return true;
        }

        let join = join.into();

        let execution_changed = crate::analysis::guarantee::merge_execution(
            &mut self.execution,
            &incoming.execution,
            join,
        );

        let failure_changed = if let Some(branch) = &incoming.failure_entry {
            if let Some(previous) = &mut self.failure_entry {
                assert_eq!(branch.occurrence, previous.occurrence, "merged attempts retain their admitted invocation");

                !Arc::ptr_eq(&previous.state, &branch.state)
                    && Arc::make_mut(&mut previous.state).merge(&branch.state, join)
            } else {
                self.failure_entry = incoming.failure_entry.clone();

                true
            }
        } else {
            false
        };

        let effects_changed = !Arc::ptr_eq(&self.effect_prefix, &incoming.effect_prefix)
            && !incoming.effect_prefix.is_subset(&self.effect_prefix);

        if effects_changed {
            // Both predecessor effect sets retain the immutable portable descriptions.
            Arc::make_mut(&mut self.effect_prefix).extend(incoming.effect_prefix.iter().cloned());
        }

        let writes_changed = if self.written_inputs == incoming.written_inputs {
            false
        } else {
            let writes = self.written_inputs.iter().filter(|path|
                incoming.written_inputs.iter().any(|other| other.contains(path)))
                .chain(incoming.written_inputs.iter().filter(|path|
                    self.written_inputs.iter().any(|other| other.contains(path))))
                .cloned().collect::<BTreeSet<_>>();

            let changed = self.written_inputs != writes;

            self.written_inputs = writes;

            changed
        };

        let dependencies_changed = self
            .result_dependencies
            .merge(&incoming.result_dependencies);

        let mut reports_changed = false;

        for (boundary, reports) in &incoming.reports {
            reports_changed |= self.merge_retained_reports(*boundary, Cow::Borrowed(reports));
        }

        let refinements_changed = self.refinements.intersect(&incoming.refinements);
        let live_count = self.live.len();
        let initialized_count = self.initialized.len();
        let observed_count = self.observed_pattern_bindings.len();
        let moved_count = self.moved.len();
        let mut absence_changed = false;
        let borrow_count = self.active_borrows.len();
        let definite_borrow_count = self.definitely_active_borrows.len();
        let was_recovered = self.recovered;

        let fully_moved_changed = merge_fully_moved(
            &self.live,
            &mut self.fully_moved,
            &incoming.live,
            &incoming.fully_moved,
        );

        self.live.extend(incoming.live.iter().copied());

        self.initialized
            .retain(|storage| incoming.initialized.contains(storage));

        self.observed_pattern_bindings
            .retain(|storage| incoming.observed_pattern_bindings.contains(storage));

        let mut moved_changed = false;

        for (path, origins) in &incoming.moved {
            // Each joined state owns its cause set and shares the immutable path.
            let current = self.moved.entry(path.clone()).or_default();
            let count = current.len();

            current.extend(origins.iter().map(|(origin, ty)| (*origin, *ty)));
            moved_changed |= current.len() != count;
        }

        let memory_changed = self.memory.merge(
            &incoming.memory,
            &self.definitely_absent,
            &incoming.definitely_absent,
        );

        if !self.definitely_absent.is_empty() && !Arc::ptr_eq(&self.definitely_absent, &incoming.definitely_absent) {
            Arc::make_mut(&mut self.definitely_absent).retain(|access, reason| {
                let Some(incoming) = incoming.definitely_absent.get(access) else {
                    absence_changed = true;

                    return false;
                };

                if *reason == StorageAbsence::Moved && *incoming == StorageAbsence::NoPayload {
                    *reason = StorageAbsence::NoPayload;
                    absence_changed = true;
                }

                true
            });
        }

        self.active_borrows
            .extend(incoming.active_borrows.iter().copied());

        self.definitely_active_borrows
            .retain(|borrow| incoming.definitely_active_borrows.contains(borrow));

        self.recovered |= incoming.recovered;

        effects_changed
            || failure_changed
            || writes_changed
            || dependencies_changed
            || reports_changed
            || execution_changed
            || refinements_changed
            || self.live.len() != live_count
            || self.initialized.len() != initialized_count
            || self.observed_pattern_bindings.len() != observed_count
            || self.moved.len() != moved_count
            || moved_changed
            || absence_changed
            || fully_moved_changed
            || self.active_borrows.len() != borrow_count
            || self.definitely_active_borrows.len() != definite_borrow_count
            || memory_changed
            || self.recovered != was_recovered
    }

    pub(in crate::analysis::storage_flow) fn forget_storage(
        &mut self,
        identity: StorageIdentityId,
        storage: &StoragePlan,
        input: &StorageFlowInput,
    ) {
        let displaced = if self.active_borrows.is_empty() {
            BTreeSet::new()
        } else {
            let slot = bray_bound_tree::StorageTarget::storage(bray_bound_tree::StoragePath::root(identity));

            self.memory.value_loans(&slot).collect()
        };

        self.live.remove(&identity);
        self.initialized.remove(&identity);
        self.fully_moved.remove(&identity);
        self.observed_pattern_bindings.remove(&identity);
        self.memory.forget(identity);

        self.moved.retain(|access, _| access.root != identity);

        self.retain_absent_storage();
        self.release_displaced_loans(displaced, storage, input);
    }

    pub(in crate::analysis::storage_flow) fn retain_live_storage(&mut self) {
        self.initialized
            .retain(|identity| self.live.contains(identity));

        self.observed_pattern_bindings
            .retain(|identity| self.live.contains(identity));

        self.fully_moved
            .retain(|identity| self.live.contains(identity));

        self.memory.retain_live(&self.live);

        self.moved.retain(|path, _| self.live.contains(&path.root));

        self.retain_absent_storage();
    }

    pub(in crate::analysis::storage_flow) fn move_complete_storage(
        &mut self,
        storage: StorageIdentityId,
    ) {
        self.fully_moved.insert(storage);
        self.initialized.remove(&storage);
    }
}

// Join move evidence only over paths that created the storage. Read both original
// live sets before their union, without copying either set.
pub(in crate::analysis::storage_flow) fn merge_fully_moved(
    live: &BTreeSet<StorageIdentityId>,
    moved: &mut BTreeSet<StorageIdentityId>,
    incoming_live: &BTreeSet<StorageIdentityId>,
    incoming_moved: &BTreeSet<StorageIdentityId>,
) -> bool {
    let count = moved.len();

    moved.retain(|storage| incoming_moved.contains(storage) || !incoming_live.contains(storage));

    let mut changed = moved.len() != count;

    for storage in incoming_moved {
        if !live.contains(storage) {
            changed |= moved.insert(*storage);
        }
    }

    changed
}
