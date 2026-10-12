use super::model::{Origin, Place, References, State, Target};
use bray_bound_tree::{
    BoundExpressionId, MemoryOperationStatus, MemoryReadKind, StorageIdentityId,
};
use bray_symbols::TypeId;

impl State {
    pub(in crate::analysis::storage_flow) fn consume_place(&mut self, place: Place, ty: TypeId) {
        if !self.has_addressed_storage(place.root) {
            return;
        }

        self.consume_targets(
            &References::one(Target {
                origin: Origin::Storage(place),
                previous: false,
            }),
            ty,
        );
    }

    fn has_addressed_storage(&self, root: StorageIdentityId) -> bool {
        if self.contents.is_empty() {
            return false;
        }

        let start = Target {
            origin: Origin::Storage(Place::root(root)),
            previous: false,
        };

        self.contents.range(start..).next().is_some_and(
            |(target, _)| matches!(&target.origin, Origin::Storage(place) if place.root == root),
        )
    }

    pub(in crate::analysis::storage_flow) fn write_place(
        &mut self,
        place: Place,
        ty: TypeId,
        origin: BoundExpressionId,
    ) {
        // Ordinary accesses only need raw contents when an address of this storage exists.
        if !self.has_addressed_storage(place.root) {
            return;
        }

        let references = References::one(Target {
            origin: Origin::Storage(place),
            previous: false,
        });

        self.write_targets(&references, ty, origin, References::default());
    }

    pub(in crate::analysis::storage_flow) fn write(
        &mut self,
        slot: &Place,
        ty: TypeId,
        origin: BoundExpressionId,
        value: Option<&Place>,
    ) -> MemoryOperationStatus {
        if self.invalidated(slot) {
            return MemoryOperationStatus::InvalidatedAllocation;
        }

        let references = self.references(slot);

        let value = value
            .map(|value| self.references(value))
            .unwrap_or_default();

        self.write_targets(&references, ty, origin, value);

        MemoryOperationStatus::Valid
    }

    fn write_targets(
        &mut self,
        references: &References,
        ty: TypeId,
        origin: BoundExpressionId,
        value: References,
    ) {
        self.invalidate_entry_slots(references);

        let exact = references.is_exact();

        for target in self.affected_targets(references) {
            let contents = self.contents.entry(target.clone()).or_default();

            contents.initialized.clear();

            if exact && references.targets.contains(&target) {
                contents.initialized.insert(ty);

                // The stored reference value is immutable input to this write.
                contents.references = value.clone();

                if let Origin::Storage(place) = &target.origin {
                    self.slots.insert(place.clone(), value.clone());
                }
            } else {
                contents.references.merge(&value);
                contents.references.unknown = true;

                if let Origin::Storage(place) = &target.origin
                    && let Some(stored) = self.slots.get_mut(place)
                {
                    stored.merge(&value);
                    stored.unknown = true;
                }
            }

            contents.pending.entry(ty).or_default().insert(origin);
        }
    }

    pub(in crate::analysis::storage_flow) fn read(
        &mut self,
        slot: &Place,
        ty: TypeId,
        kind: MemoryReadKind,
        proven: Option<BoundExpressionId>,
        result: Option<Place>,
    ) -> MemoryOperationStatus {
        if self.invalidated(slot) {
            return MemoryOperationStatus::InvalidatedAllocation;
        }

        if proven.is_none() && !self.initialized(slot, ty) {
            return MemoryOperationStatus::UninitializedRawStorage;
        }

        let references = self.references(slot);

        let mut value = References {
            unknown: references.unknown || references.displaced,
            ..References::default()
        };

        for target in &references.targets {
            match &target.origin {
                Origin::Storage(place) if !target.previous => {
                    value.merge(&self.references(place));
                }
                _ => {
                    if let Some(contents) = self.contents.get(target) {
                        value.merge(&contents.references);
                    }
                }
            }
        }

        if let Some(origin) = proven {
            for target in &self.affected_targets(&references) {
                let contents = self.contents.entry(target.clone()).or_default();

                if references.is_exact() && references.targets.contains(target) {
                    contents.initialized.insert(ty);
                }

                contents.pending.entry(ty).or_default().insert(origin);
            }
        }

        if kind == MemoryReadKind::Move {
            self.consume(slot, ty);
        }

        if let Some(result) = result {
            self.slots.insert(result, value);
        }

        MemoryOperationStatus::Valid
    }

    pub(in crate::analysis::storage_flow) fn consume(
        &mut self,
        slot: &Place,
        ty: TypeId,
    ) -> MemoryOperationStatus {
        if self.invalidated(slot) {
            return MemoryOperationStatus::InvalidatedAllocation;
        }

        self.consume_targets(&self.references(slot), ty);

        MemoryOperationStatus::Valid
    }

    fn consume_targets(&mut self, references: &References, ty: TypeId) {
        self.invalidate_entry_slots(references);

        let exact = references.is_exact();

        for target in &self.affected_targets(references) {
            if let Some(contents) = self.contents.get_mut(target) {
                contents.initialized.clear();
                contents.references = References::default();

                if exact && references.targets.contains(target) {
                    contents.pending.remove(&ty);
                }
            }
        }
    }

    pub(in crate::analysis::storage_flow) fn copy(
        &mut self,
        source: &Place,
        destination: &Place,
        ty: TypeId,
        origin: BoundExpressionId,
        proven: Option<bool>,
    ) -> MemoryOperationStatus {
        if self.invalidated(source) || self.invalidated(destination) {
            return MemoryOperationStatus::InvalidatedAllocation;
        }

        // An empty range has no scalar effect.
        if proven == Some(true) {
            return MemoryOperationStatus::Valid;
        }

        if proven.is_none() && !self.initialized(source, ty) {
            return MemoryOperationStatus::UninitializedRawStorage;
        }

        self.write(destination, ty, origin, None)
    }

    pub(in crate::analysis::storage_flow) fn deallocate(
        &mut self,
        slot: &Place,
        origin: BoundExpressionId,
    ) -> MemoryOperationStatus {
        if self.invalidated(slot) {
            return MemoryOperationStatus::InvalidatedAllocation;
        }

        let references = self.references(slot);
        let targets = self.affected_targets(&references);

        if targets.iter().any(|target| {
            self.contents
                .get(target)
                .is_some_and(|contents| !contents.pending.is_empty())
        }) {
            return MemoryOperationStatus::OutstandingObligations;
        }

        for target in targets {
            let contents = self.contents.entry(target).or_default();

            contents.initialized.clear();
            contents.invalidations.insert(origin);
        }

        MemoryOperationStatus::Valid
    }
}
