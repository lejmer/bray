use super::super::model::{Contents, Origin, Place, References, State, Target};
use super::slots::take_subtree;
use bray_bound_tree::{BorrowCapabilityId, BoundExpressionId, StorageIdentityId};
use bray_symbols::TypeId;

impl State {
    pub(in crate::analysis::storage_flow) fn begin_transition(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
    ) {
        self.pending.insert(call, Default::default());
    }

    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn stage_slot(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        source: &Place,
        destination: Place,
    ) {
        self.stage_value(
            call,
            &Target::storage(source.clone()),
            Target::storage(destination),
        );
    }

    pub(in crate::analysis::storage_flow) fn stage_value(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        source: &Target,
        destination: Target,
    ) {
        let mut staged = self.capture_value(source, &destination);

        staged
            .values
            .entry(destination.clone())
            .or_insert_with(|| References {
                unknown: true,
                ..References::default()
            });

        let pending = self.pending.entry(call).or_default();

        if pending.values.contains_key(&destination) {
            // Alternative sources for one output must agree on its initialized payloads.
            let mut previous = super::super::model::Transfer {
                values: take_subtree(&mut pending.values, &destination).collect(),
                payloads: take_subtree(&mut pending.payloads, &destination).collect(),
            };

            previous.merge(&staged);
            staged = previous;
        }

        pending.values.extend(staged.values);
        pending.payloads.extend(staged.payloads);
    }

    pub(in crate::analysis::storage_flow) fn stage_unknown(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        destination: Target,
    ) {
        let pending = self.pending.entry(call).or_default();

        pending
            .values
            .entry(destination.clone())
            .or_default()
            .unknown = true;

        for (_, contents) in pending
            .payloads
            .range_mut(destination.clone()..)
            .take_while(|(target, _)| destination.contains(target))
        {
            contents.initialized.clear();
        }
    }

    pub(in crate::analysis::storage_flow) fn stage_borrow(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        source: BorrowCapabilityId,
        destination: Target,
    ) {
        let Some(references) = self.borrows.get(&source) else {
            return;
        };

        self.pending
            .entry(call)
            .or_default()
            .values
            .entry(destination)
            .or_default()
            .merge(references);
    }

    pub(in crate::analysis::storage_flow) fn stage_address(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        source: Target,
        destination: Target,
        ty: TypeId,
    ) {
        let references = self.addressed_target(source, ty);

        self.pending
            .entry(call)
            .or_default()
            .values
            .entry(destination)
            .or_default()
            .merge(&references);
    }

    pub(in crate::analysis::storage_flow) fn complete_transition(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
    ) {
        if let Some(transfer) = self.pending.remove(&call) {
            for slot in transfer.values.keys() {
                self.clear_value(slot);
            }

            self.commit_value(transfer);
        }
    }

    pub(in crate::analysis::storage_flow) fn invalidate_target(
        &mut self,
        target: &Target,
        kind: bray_symbols::DependencyMutationKind,
    ) {
        let references = References::one(target.clone());

        for affected in self.affected_targets(&references) {
            self.values.remove(&affected);

            if let Some(contents) = self.contents.get_mut(&affected) {
                contents.initialized.retain(|ty| {
                    kind == bray_symbols::DependencyMutationKind::Write
                        && contents.storage_type == Some(*ty)
                });
            }
        }
    }

    pub(in crate::analysis::storage_flow) fn invalidate(
        &mut self,
        invalidation: &bray_bound_tree::StorageInvalidation,
        storage: &bray_bound_tree::StoragePlan,
        kind: bray_symbols::DependencyMutationKind,
    ) {
        self.values.retain(|target, _| match target.storage_path() {
            Some(slot) => !invalidation.invalidates_path(slot, storage),
            None => !matches!(invalidation, bray_bound_tree::StorageInvalidation::All),
        });

        for (target, contents) in &mut self.contents {
            let affected = match &target.origin {
                Origin::Storage(path) => invalidation.invalidates_path(path, storage),
                Origin::Entry { .. } | Origin::Allocation(_) => {
                    matches!(invalidation, bray_bound_tree::StorageInvalidation::All)
                }
            };

            if affected {
                // A checked typed write preserves valid contents of the declared type.
                // Reinterpreted views and consumed contents have no such guarantee.
                contents.initialized.retain(|ty| {
                    kind == bray_symbols::DependencyMutationKind::Write
                        && contents.storage_type == Some(*ty)
                });
            }
        }
    }

    pub(in crate::analysis::storage_flow) fn fresh(
        &mut self,
        slot: Place,
        allocation: BoundExpressionId,
    ) {
        // A new execution cannot equate old aliases with this incarnation.
        let current = Target {
            origin: Origin::Allocation(slot.clone()),
            previous: false,
        };

        self.retire(&current);

        let contents = Contents {
            allocations: [allocation].into_iter().collect(),
            ..Contents::default()
        };

        self.contents.insert(current.clone(), contents);

        self.values
            .insert(Target::storage(slot), References::one(current));
    }

    pub(in crate::analysis::storage_flow) fn uninit(&mut self, target: Target, element: TypeId) {
        self.retire(&target);

        self.contents.insert(
            target,
            Contents {
                storage_type: Some(element),
                ..Contents::default()
            },
        );
    }

    pub(in crate::analysis::storage_flow) fn point_at(
        &mut self,
        target: Target,
        destination: Place,
        ty: TypeId,
    ) {
        let references = self.addressed_target(target, ty);

        self.clear(&destination);
        self.values.insert(Target::storage(destination), references);
    }

    pub(super) fn retire(&mut self, current: &Target) {
        if !self
            .contents
            .range(current.clone()..)
            .next()
            .is_some_and(|(target, _)| current.contains(target))
        {
            return;
        }

        for references in self
            .values
            .values_mut()
            .chain(self.borrows.values_mut())
            .chain(
                self.pending
                    .values_mut()
                    .flat_map(|transfer| transfer.values.values_mut()),
            )
        {
            references.retire(current);
        }

        let values = take_subtree(&mut self.values, current).collect::<Vec<_>>();

        for (target, mut value) in values {
            value.unknown = true;

            self.values
                .entry(Target {
                    previous: true,
                    ..target
                })
                .or_default()
                .merge(&value);
        }

        let contents = take_subtree(&mut self.contents, current).collect::<Vec<_>>();

        for (target, mut contents) in contents {
            // Older instances share a summary target, so no definite contents survive.
            contents.initialized.clear();

            self.contents
                .entry(Target {
                    previous: true,
                    ..target
                })
                .or_default()
                .merge(&contents);
        }
    }

    pub(in crate::analysis::storage_flow) fn begin(&mut self, identity: StorageIdentityId) {
        // Retire a storage incarnation once, including all its addressed fields and payloads.
        self.retire(&Target::storage(Place::root(identity)));
    }
}
