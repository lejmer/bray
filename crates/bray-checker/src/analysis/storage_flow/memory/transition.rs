use super::model::{Origin, Place, References, State, Target};
use bray_bound_tree::{
    BoundExpressionId, MemoryOperationStatus, MemoryReadKind, StorageIdentityId,
};
use bray_symbols::TypeId;

impl State {
    pub(in crate::analysis::storage_flow) fn consume_target(&mut self, target: Target, ty: TypeId) {
        if target
            .storage_path()
            .is_some_and(|place| !self.has_addressed_storage(place.root))
        {
            return;
        }

        self.consume_targets(&References::one(target), ty);
    }

    pub(in crate::analysis::storage_flow) fn write_target(
        &mut self,
        target: Target,
        ty: TypeId,
        origin: BoundExpressionId,
    ) {
        if target
            .storage_path()
            .is_some_and(|place| !self.has_addressed_storage(place.root))
        {
            return;
        }

        self.write_targets(&References::one(target), ty, origin, References::default());
    }

    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn consume_place(&mut self, place: Place, ty: TypeId) {
        self.consume_target(Target::storage(place), ty);
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

    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn write_place(
        &mut self,
        place: Place,
        ty: TypeId,
        origin: BoundExpressionId,
    ) {
        self.write_target(Target::storage(place), ty, origin);
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
        let exact = references.is_exact();

        for target in self.affected_targets(references) {
            let contents = self.contents.entry(target.clone()).or_default();

            contents.initialized.clear();

            if exact && references.targets.contains(&target) {
                contents.initialized.insert(ty);

                // Ordinary slots and raw storage share the same stored reference value.
                if value.targets.is_empty() {
                    self.values.remove(&target);
                } else {
                    self.values.insert(target.clone(), value.clone());
                }
            } else {
                if let Some(stored) = self.values.get_mut(&target) {
                    stored.merge(&value);
                    stored.unknown = true;
                } else if !value.targets.is_empty() {
                    let mut stored = value.clone();

                    stored.unknown = true;
                    self.values.insert(target.clone(), stored);
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
            if let Some(stored) = self.values.get(target) {
                value.merge(stored);
            } else {
                value.unknown = true;
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
            self.values.insert(Target::storage(result), value);
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
        let exact = references.is_exact();

        for target in &self.affected_targets(references) {
            if let Some(contents) = self.contents.get_mut(target) {
                contents.initialized.clear();

                // Consumption ends initialized ownership, not the evaluated value's identity.
                // Normal value transfer still needs its reference fields before slot retirement.

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
