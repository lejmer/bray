use super::super::model::{Contents, Origin, Place, References, State, Target};
use bray_bound_tree::{BorrowCapabilityId, BoundExpressionId, StorageIdentityId};
use bray_symbols::TypeId;

impl State {
    pub(in crate::analysis::storage_flow) fn begin_transition(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
    ) {
        self.pending.insert(call, Default::default());
    }

    pub(in crate::analysis::storage_flow) fn stage_slot(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        source: &Place,
        destination: Place,
    ) {
        let mut staged = self
            .transferred_slots(source, &destination)
            .collect::<std::collections::BTreeMap<_, _>>();

        if !staged.contains_key(&destination) {
            staged.insert(
                destination,
                References {
                    unknown: true,
                    ..References::default()
                },
            );
        }

        let pending = self.pending.entry(call).or_default();

        for (slot, references) in staged {
            pending.entry(slot).or_default().merge(&references);
        }
    }

    pub(in crate::analysis::storage_flow) fn stage_unknown(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        destination: Place,
    ) {
        self.pending
            .entry(call)
            .or_default()
            .entry(destination)
            .or_default()
            .unknown = true;
    }

    pub(in crate::analysis::storage_flow) fn stage_borrow(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        source: BorrowCapabilityId,
        destination: Place,
    ) {
        let Some(references) = self.borrows.get(&source) else {
            return;
        };

        self.pending
            .entry(call)
            .or_default()
            .entry(destination)
            .or_default()
            .merge(references);
    }

    pub(in crate::analysis::storage_flow) fn stage_address(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
        source: Place,
        destination: Place,
        ty: TypeId,
    ) {
        let references = self.borrowed_reference(source, ty);

        self.pending
            .entry(call)
            .or_default()
            .entry(destination)
            .or_default()
            .merge(&references);
    }

    pub(in crate::analysis::storage_flow) fn complete_transition(
        &mut self,
        call: bray_bound_tree::BoundExecutionSite,
    ) {
        if let Some(values) = self.pending.remove(&call) {
            for (slot, references) in values {
                self.clear(&slot);
                self.slots.insert(slot, references);
            }
        }
    }

    pub(in crate::analysis::storage_flow) fn invalidate(
        &mut self,
        invalidation: &bray_bound_tree::StorageInvalidation,
        storage: &bray_bound_tree::StoragePlan,
        kind: bray_symbols::DependencyMutationKind,
    ) {
        self.slots
            .retain(|slot, _| !invalidation.invalidates_path(slot, storage));

        for (target, contents) in &mut self.contents {
            let affected = match &target.origin {
                Origin::Storage(path) => invalidation.invalidates_path(path, storage),
                Origin::Entry(_) | Origin::Allocation(_) | Origin::Uninit(_) => {
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

                contents.references.unknown = true;
            }
        }
    }

    pub(in crate::analysis::storage_flow) fn fresh(
        &mut self,
        slot: Place,
        allocation: Option<BoundExpressionId>,
    ) {
        // A new execution cannot equate old aliases with this incarnation.
        let origin = if allocation.is_some() {
            Origin::Allocation(slot.clone())
        } else {
            Origin::Uninit(slot.clone())
        };

        let current = Target {
            origin,
            previous: false,
        };

        self.retire(&current);

        let contents = Contents {
            allocations: allocation.into_iter().collect(),
            ..Contents::default()
        };

        self.contents.insert(current.clone(), contents);
        self.slots.insert(slot, References::one(current));
    }

    fn retire(&mut self, current: &Target) {
        let previous = Target {
            previous: true,
            ..current.clone()
        };

        if self.contents.contains_key(current) {
            for references in self
                .slots
                .values_mut()
                .chain(self.borrows.values_mut())
                .chain(
                    self.pending
                        .values_mut()
                        .flat_map(|values| values.values_mut()),
                )
                .chain(
                    self.contents
                        .values_mut()
                        .map(|contents| &mut contents.references),
                )
            {
                if references.targets.remove(current) {
                    // Targets share immutable projection paths.
                    references.targets.insert(previous.clone());
                }
            }
        }

        if let Some(mut contents) = self.contents.remove(current) {
            // Older instances share a summary target, so no definite contents survive.
            contents.initialized.clear();
            self.contents.entry(previous).or_default().merge(&contents);
        }
    }

    pub(in crate::analysis::storage_flow) fn begin(&mut self, identity: StorageIdentityId) {
        let start = Target {
            origin: Origin::Storage(Place::root(identity)),
            previous: false,
        };

        // Only addressed places have physical slot contents. Ordinary locals take the empty range.
        let targets = self
            .contents
            .range(start..)
            .map(|(target, _)| target)
            .take_while(
                |target| matches!(&target.origin, Origin::Storage(place) if place.root == identity),
            )
            .filter(|target| !target.previous)
            .cloned()
            .collect::<Vec<_>>();

        for target in targets {
            self.retire(&target);
        }
    }
}
