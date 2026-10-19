use super::StorageFlowInput;
use bray_bound_tree::{
    BorrowCapabilityId, BoundExpressionId, StorageAccessId, StorageIdentityId, StoragePlan,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::analysis) enum StorageAbsence {
    Moved,
    NoPayload,
}

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
        BTreeMap<bray_bound_tree::StoragePath, StorageAbsence>,
    pub(in crate::analysis) fully_moved: BTreeSet<StorageIdentityId>,
    pub(in crate::analysis::storage_flow) active_borrows: BTreeSet<BorrowCapabilityId>,
    pub(in crate::analysis::storage_flow) definitely_active_borrows: BTreeSet<BorrowCapabilityId>,
    pub(in crate::analysis::storage_flow) result_dependencies: crate::dependency::ResultState,
    pub(in crate::analysis::storage_flow) memory: crate::analysis::storage_flow::memory::State,
    reports: BTreeMap<Option<BoundExpressionId>, RetainedReports>,
    pub(in crate::analysis::storage_flow) recovered: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct RetainedReports {
    dependencies: BTreeSet<bray_bound_tree::BoundDependencySubject>,
    pending: bool,
}

impl RetainedReports {
    fn merge(&mut self, incoming: &Self) -> bool {
        let previous = self.dependencies.len();
        let pending = self.pending;

        self.dependencies.extend(&incoming.dependencies);
        self.pending |= incoming.pending;

        self.dependencies.len() != previous || self.pending != pending
    }
}

impl StorageFlowState {
    pub(in crate::analysis::storage_flow) fn retain_reports(
        &mut self,
        boundary: Option<BoundExpressionId>,
        dependencies: impl IntoIterator<Item = bray_bound_tree::BoundDependencySubject>,
        pending: bool,
    ) {
        let reports = self.reports.entry(boundary).or_default();

        reports.dependencies.extend(dependencies);
        reports.pending |= pending;
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
            self.definitely_absent
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
        self.definitely_absent.retain(|absent, _| {
            path.root != absent.root
                || bray_bound_tree::StorageProjection::relationship(
                    &path.projections,
                    &absent.projections,
                ) == bray_bound_tree::StorageRelationship::Disjoint
        });
    }

    pub(in crate::analysis::storage_flow) fn end_storage_lifetime(
        &mut self,
        identity: StorageIdentityId,
        storage: &StoragePlan,
    ) {
        if bray_bound_tree::storage_identity_transfers_at_unit_exit(storage, identity)
            || self.report_retains(bray_bound_tree::BoundDependencySubject::Storage(identity))
            || self.recovered
            || self.active_borrows.iter().any(|borrow| {
                storage
                    .borrow_capability(*borrow)
                    .is_some_and(|borrow| storage.root_identity(borrow.access()) == Some(identity))
            })
        {
            return;
        }

        self.forget_storage(identity);
    }

    pub(in crate::analysis::storage_flow) fn retains_borrow(
        &self,
        borrow: BorrowCapabilityId,
        input: &StorageFlowInput,
    ) -> bool {
        self.report_retains(bray_bound_tree::BoundDependencySubject::BorrowCapability(
            borrow,
        )) || input
            .cleanup_dependency_owners
            .get(&bray_bound_tree::BoundDependencySubject::BorrowCapability(
                borrow,
            ))
            .is_some_and(|owners| self.has_live_cleanup_owner(owners))
    }

    pub(in crate::analysis::storage_flow) fn retained_dependencies<'a>(
        &'a self,
        input: &'a StorageFlowInput,
    ) -> impl Iterator<Item = bray_bound_tree::BoundDependencySubject> + 'a {
        input
            .cleanup_dependency_owners
            .iter()
            .filter_map(|(subject, owners)| self.has_live_cleanup_owner(owners).then_some(*subject))
            .chain(
                self.reports
                    .values()
                    .flat_map(|reports| reports.dependencies.iter().copied()),
            )
    }

    pub(in crate::analysis::storage_flow) fn report_retains(
        &self,
        subject: bray_bound_tree::BoundDependencySubject,
    ) -> bool {
        self.reports
            .values()
            .any(|reports| reports.dependencies.contains(&subject))
    }

    pub(in crate::analysis::storage_flow) fn transfer_report_boundary(
        &mut self,
        boundary: BoundExpressionId,
        parent: Option<BoundExpressionId>,
        caught: bool,
    ) {
        if let Some(subjects) = self.reports.remove(&Some(boundary)) {
            if !caught {
                self.reports.entry(parent).or_default().merge(&subjects);
            }
        }

        self.result_dependencies
            .transfer_report_boundary(boundary, parent, caught);
    }

    fn has_live_cleanup_owner(&self, owners: &BTreeSet<StorageIdentityId>) -> bool {
        owners
            .iter()
            .any(|owner| self.live.contains(owner) && !self.fully_moved.contains(owner))
    }

    pub(in crate::analysis::storage_flow) fn end_borrows(
        &mut self,
        ended: &BTreeSet<BorrowCapabilityId>,
        storage: &StoragePlan,
    ) {
        self.memory.end_borrows(ended);
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
        self.definitely_absent
            .retain(|path, _| self.live.contains(&path.root));
    }

    pub(in crate::analysis::storage_flow) fn mark_absent(
        &mut self,
        access: StorageAccessId,
        reason: StorageAbsence,
        storage: &StoragePlan,
    ) {
        if let Some(path) = storage.logical_path(access).cloned() {
            self.definitely_absent
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
        storage: &StoragePlan,
        discriminants: &BTreeMap<StorageIdentityId, Vec<bray_bound_tree::StoragePath>>,
    ) {
        self.definitely_absent.retain(|access, reason| {
            if *reason == StorageAbsence::Moved {
                return true;
            }

            let Some((_, parent)) = access.projections.split_last() else {
                return !invalidation.invalidates_path(access, storage);
            };

            let descriptor = bray_bound_tree::StoragePath {
                root: access.root,
                projections: parent.into(),
            };

            !crate::analysis::storage_invalidation::discriminant_invalidated(
                invalidation,
                &descriptor,
                storage,
                discriminants,
            )
        });
    }

    pub(super) fn entry(storage: &StoragePlan, input: &StorageFlowInput) -> Self {
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
            &input.cleanup_free_storage,
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

        for ((slot, layer), (path, ty)) in &input.entry_references {
            if let Some(target) =
                StorageFlowInput::reference_location(&memory, slot.root, path, None)
            {
                memory.entry_reference_at(slot.clone(), *layer, target, *ty);
            }
        }

        Self {
            reachable: true,
            execution: None,
            refinements: Default::default(),
            live: initialized.clone(),
            initialized,
            observed_pattern_bindings: BTreeSet::new(),
            moved: BTreeMap::new(),
            definitely_absent: BTreeMap::new(),
            fully_moved: BTreeSet::new(),
            definitely_active_borrows: active_borrows.clone(),
            active_borrows,
            memory,
            result_dependencies: Default::default(),
            reports: BTreeMap::new(),
            recovered: false,
        }
    }

    pub(in crate::analysis::storage_flow) fn merge(
        &mut self,
        incoming: &Self,
        block: Option<crate::analysis::id::AnalysisBlockId>,
    ) -> bool {
        if !incoming.reachable {
            return false;
        }

        if !self.reachable {
            // Each block owns its task-local state after propagation.
            *self = incoming.clone();

            return true;
        }

        let execution_changed = crate::analysis::guarantee::merge_execution(
            &mut self.execution,
            &incoming.execution,
            block,
        );

        let dependencies_changed = self
            .result_dependencies
            .merge(&incoming.result_dependencies);

        let mut reports_changed = false;

        for (boundary, subjects) in &incoming.reports {
            match self.reports.entry(*boundary) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    // Joined paths own independent sets of retained semantic identities.
                    entry.insert(subjects.clone());
                    reports_changed = true;
                }
                std::collections::btree_map::Entry::Occupied(mut entry) => {
                    reports_changed |= entry.get_mut().merge(subjects);
                }
            }
        }

        let refinements_changed = self.refinements.intersect(&incoming.refinements);
        let live_count = self.live.len();
        let initialized_count = self.initialized.len();
        let observed_count = self.observed_pattern_bindings.len();
        let moved_count = self.moved.len();
        let mut absence_changed = false;
        let fully_moved_count = self.fully_moved.len();
        let borrow_count = self.active_borrows.len();
        let definite_borrow_count = self.definitely_active_borrows.len();
        let was_recovered = self.recovered;

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

        self.definitely_absent.retain(|access, reason| {
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

        self.fully_moved
            .retain(|storage| incoming.fully_moved.contains(storage));

        self.active_borrows
            .extend(incoming.active_borrows.iter().copied());

        self.definitely_active_borrows
            .retain(|borrow| incoming.definitely_active_borrows.contains(borrow));

        self.recovered |= incoming.recovered;

        dependencies_changed
            || reports_changed
            || execution_changed
            || refinements_changed
            || self.live.len() != live_count
            || self.initialized.len() != initialized_count
            || self.observed_pattern_bindings.len() != observed_count
            || self.moved.len() != moved_count
            || moved_changed
            || absence_changed
            || self.fully_moved.len() != fully_moved_count
            || self.active_borrows.len() != borrow_count
            || self.definitely_active_borrows.len() != definite_borrow_count
            || memory_changed
            || self.recovered != was_recovered
    }

    pub(in crate::analysis::storage_flow) fn forget_storage(
        &mut self,
        identity: StorageIdentityId,
    ) {
        self.live.remove(&identity);
        self.initialized.remove(&identity);
        self.fully_moved.remove(&identity);
        self.observed_pattern_bindings.remove(&identity);
        self.memory.forget(identity);

        self.moved.retain(|access, _| access.root != identity);

        self.retain_absent_storage();
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
