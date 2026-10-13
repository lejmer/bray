use super::Source;
use super::model::{Origin, Place, References, State, Target, Transfer};
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

        self.write_targets(&References::one(target), ty, origin);
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
        value: Option<Source>,
    ) -> MemoryOperationStatus {
        if self.invalidated(slot) {
            return MemoryOperationStatus::InvalidatedAllocation;
        }

        let references = self.references(slot);

        self.write_transferred(&references, ty, origin, value);

        MemoryOperationStatus::Valid
    }

    fn write_transferred(
        &mut self,
        references: &References,
        ty: TypeId,
        origin: BoundExpressionId,
        source: Option<Source>,
    ) {
        if references.unknown {
            // An unlocated write can affect any tracked cell. Collapse the source's
            // possible references instead of inventing a subtree at every possible address.
            let affected = self.affected_targets(references);

            let mut incoming = References {
                unknown: true,
                ..References::default()
            };

            if let (Some(source), Some(destination)) = (source.as_ref(), affected.first()) {
                for value in self
                    .capture_source(source, destination)
                    .values
                    .into_values()
                {
                    incoming.merge(&value);
                }
            }

            self.write_targets(references, ty, origin);

            for target in affected {
                self.values.entry(target).or_default().merge(&incoming);
            }

            return;
        }

        let destinations = &references.targets;

        let transfers = source
            .as_ref()
            .into_iter()
            .flat_map(|source| destinations.iter().map(move |target| (target, source)))
            .map(|(target, source)| (target.clone(), self.capture_source(source, target)))
            .collect::<Vec<_>>();

        self.write_targets(references, ty, origin);

        for (target, transfer) in transfers {
            if references.is_exact() {
                self.clear_value(&target);
                self.commit_value(transfer);
            } else {
                for (slot, mut value) in transfer.values {
                    value.unknown = true;
                    self.values.entry(slot).or_default().merge(&value);
                }

                for (slot, mut contents) in transfer.payloads {
                    contents.initialized.clear();
                    self.contents.entry(slot).or_default().merge(&contents);
                }
            }
        }
    }

    fn write_targets(&mut self, references: &References, ty: TypeId, origin: BoundExpressionId) {
        let exact = references.is_exact();

        for target in self.affected_targets(references) {
            let contents = self.contents.entry(target.clone()).or_default();

            contents.initialized.clear();

            if exact && references.targets.contains(&target) {
                contents.initialized.insert(ty);

                self.values.remove(&target);
            } else if let Some(stored) = self.values.get_mut(&target) {
                stored.unknown = true;
            }

            contents.pending.entry(ty).or_default().insert(origin);
        }
    }

    pub(in crate::analysis::storage_flow) fn write_value(
        &mut self,
        target: Target,
        ty: TypeId,
        origin: BoundExpressionId,
        source: Option<Source>,
    ) -> MemoryOperationStatus {
        self.write_transferred(&References::one(target), ty, origin, source);

        MemoryOperationStatus::Valid
    }

    pub(in crate::analysis::storage_flow) fn read_value(
        &mut self,
        target: Target,
        ty: TypeId,
        kind: MemoryReadKind,
        proven: Option<BoundExpressionId>,
        result: Option<Place>,
    ) -> MemoryOperationStatus {
        self.read_targets(References::one(target), ty, kind, proven, result)
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

        self.read_targets(self.references(slot), ty, kind, proven, result)
    }

    fn read_targets(
        &mut self,
        references: References,
        ty: TypeId,
        kind: MemoryReadKind,
        proven: Option<BoundExpressionId>,
        result: Option<Place>,
    ) -> MemoryOperationStatus {
        if proven.is_none() && !self.initialized_targets(&references, ty) {
            return MemoryOperationStatus::UninitializedRawStorage;
        }

        let transferred = result.as_ref().map(|destination| {
            let destination = Target::storage(destination.clone());
            let mut transfer: Option<Transfer> = None;

            for source in &references.targets {
                let incoming = self.capture_value(source, &destination);

                match &mut transfer {
                    Some(transfer) => {
                        transfer.merge(&incoming);
                    }
                    None => transfer = Some(incoming),
                }
            }

            let mut transfer = transfer.unwrap_or_default();

            transfer
                .values
                .entry(destination)
                .or_insert_with(|| References {
                    unknown: true,
                    ..References::default()
                });

            if references.unknown || references.displaced {
                for value in transfer.values.values_mut() {
                    value.unknown = true;
                }

                for contents in transfer.payloads.values_mut() {
                    contents.initialized.clear();
                }
            }

            transfer
        });

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
            self.consume_targets(&references, ty);
        }

        if let (Some(result), Some(transfer)) = (result, transferred) {
            self.clear(&result);
            self.commit_value(transfer);
        }

        MemoryOperationStatus::Valid
    }

    #[cfg(test)]
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
                if exact
                    && references.targets.iter().any(|source| {
                        source.contains(target)
                            && target.projections()[source.projections().len()..]
                                .contains(&bray_bound_tree::StorageProjection::UninitPayload)
                    })
                {
                    // The evaluated value owns these facts after its source loses initialization.
                    self.evaluated.insert(target.clone(), contents.clone());
                }

                contents.initialized.clear();

                // Consumption ends initialized ownership, not the evaluated value's identity.
                // Normal value transfer still needs its reference fields before slot retirement.

                if exact
                    && references
                        .targets
                        .iter()
                        .any(|source| source.contains(target))
                {
                    if references.targets.contains(target) {
                        contents.pending.remove(&ty);
                    } else {
                        // Whole-value consumption transfers nested protected ownership as well.
                        contents.pending.clear();
                    }
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
