use super::Source;
use super::references::storage_subtree;
use super::model::{Place, References, State, Target, Transfer};
use bray_bound_tree::{
    BoundExpressionId, MemoryOperationStatus, MemoryReadKind,
};
use bray_symbols::TypeId;

impl State {
    pub(in crate::analysis::storage_flow) fn observe_effect_types(
        &mut self,
        effects: &[super::Effect],
    ) {
        for effect in effects {
            if let super::Effect::Mutation(target, bray_symbols::DependencyMutationKind::Replace(ty), _) = effect
                && target
                    .projections()
                    .contains(&bray_bound_tree::StorageProjection::OwnedTarget)
            {
                // A checked owning-region write retains its backing element type,
                // without establishing initialized contents or retyping known storage.
                self.contents
                    .entry(target.clone())
                    .or_default()
                    .storage_type
                    .get_or_insert(*ty);
            }
        }
    }

    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn consume_target(&mut self, target: Target, ty: TypeId) {
        self.consume_targets([target], ty);
    }

    pub(in crate::analysis::storage_flow) fn consume_targets(&mut self, targets: impl IntoIterator<Item = Target>, ty: TypeId) {
        if let Some(references) = self.transition_references(targets) {
            self.consume_references(&references, ty);
        }
    }

    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn write_target(
        &mut self, target: Target, ty: TypeId, origin: BoundExpressionId,
        invalidation: Option<&bray_bound_tree::StorageInvalidation>,
    ) {
        self.write_targets([target], ty, origin, invalidation);
    }

    pub(in crate::analysis::storage_flow) fn write_targets(
        &mut self, targets: impl IntoIterator<Item = Target>, ty: TypeId, origin: BoundExpressionId,
        invalidation: Option<&bray_bound_tree::StorageInvalidation>,
    ) {
        if let Some(references) = self.transition_references(targets) {
            self.write_references(&references, ty, origin, invalidation);
        }
    }

    fn transition_references(&self, targets: impl IntoIterator<Item = Target>) -> Option<References> {
        let references = References { targets: targets.into_iter().collect(), ..Default::default() };

        // Logical-only admission must retain already prepared reference values. Keep
        // every alternative when physical tracking is needed, so weak writes stay weak.
        references.targets.iter().any(|target| target.storage_path().is_none()
            || self.has_addressed_storage(target)).then_some(references)
    }

    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn consume_place(&mut self, place: Place, ty: TypeId) {
        self.consume_target(Target::storage(place), ty);
    }

    pub(in crate::analysis::storage_flow::memory) fn has_addressed_storage(&self, target: &Target) -> bool {
        let region = super::references::exposure_region(target);

        storage_subtree(&self.contents, &region).next().is_some()
    }

    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn write_place(
        &mut self,
        place: Place,
        ty: TypeId,
        origin: BoundExpressionId,
    ) {
        self.write_target(Target::storage(place), ty, origin, None);
    }

    pub(in crate::analysis::storage_flow) fn write(
        &mut self,
        slot: &Target,
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

            let mut incoming = super::model::StoredValue::from(References {
                unknown: true,
                ..References::default()
            });

            if let (Some(source), Some(destination)) = (source.as_ref(), affected.first()) {
                for value in self
                    .capture_source(source, destination)
                    .values
                    .into_values()
                {
                    incoming.merge(&value);
                }
            }

            self.write_references(references, ty, origin, None);

            for target in affected {
                let value = self.values.entry(target).or_default();

                value.merge(&incoming);

                value.complete = false;
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

        self.write_references(references, ty, origin, None);

        for (target, transfer) in transfers {
            if references.is_exact() {
                self.clear_value(&target);
                self.commit_value(transfer);
            } else {
                self.commit_weak_value(transfer);
            }
        }
    }

    pub(in crate::analysis::storage_flow::memory) fn commit_weak_value(&mut self, transfer: Transfer) {
        for (slot, mut value) in transfer.values {
            if let Some(references) = &mut value.references {
                references.unknown = true;
            }

            value.complete = false;

            self.values.entry(slot).and_modify(|previous| { previous.merge(&value); }).or_insert(value);
        }

        for (slot, mut contents) in transfer.payloads {
            contents.initialized.clear();

            self.contents
                .entry(slot.clone())
                .or_default()
                .merge(&contents, &slot);
        }

        super::references::merge_deferred(&mut self.deferred, &transfer.deferred);

        for (reference, bounds) in transfer.returned_inputs {
            self.returned_inputs
                .entry((reference, false))
                .or_default()
                .merge(&bounds);
        }
    }


    fn write_references(
        &mut self,
        references: &References,
        ty: TypeId,
        origin: BoundExpressionId,
        invalidation: Option<&bray_bound_tree::StorageInvalidation>,
    ) {
        let exact = references.is_exact();

        for target in self.affected_targets(references) {
            let contents = self.contents.entry(target.clone()).or_default();

            contents.initialized.clear();

            if exact && references.targets.contains(&target) {
                contents.initialized.insert(ty);

                self.values.remove(&target);
            } else if !invalidation
                .is_some_and(|invalidation| Self::stable_reference_slot(&target, invalidation))
                && let Some(stored) = self.values.get_mut(&target)
            {
                if let Some(references) = &mut stored.references {
                    references.unknown = true;
                }

                stored.complete = false;
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
        slot: &Target,
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

            // Reading a whole value does not turn its aggregate root into a pointer.
            // Actual pointer slots retain their captured reference uncertainty below.
            transfer.values.entry(destination).or_default();

            if references.unknown
                || references.displaced
                || references.inexact
                || references.targets.iter().any(|target| !target.is_exact())
            {
                for value in transfer.values.values_mut() {
                    if let Some(references) = &mut value.references {
                        references.unknown = true;
                    }

                    value.complete = false;
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
            self.consume_references(&references, ty);
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
        slot: &Target,
        ty: TypeId,
    ) -> MemoryOperationStatus {
        if self.invalidated(slot) {
            return MemoryOperationStatus::InvalidatedAllocation;
        }

        self.consume_references(&self.references(slot), ty);

        MemoryOperationStatus::Valid
    }

    fn consume_references(&mut self, references: &References, ty: TypeId) {
        let exact = references.is_exact();

        for target in &self.affected_targets(references) {
            if let Some(contents) = self.contents.get_mut(target) {
                if exact
                    && references.targets.iter().any(|source| {
                        source.contains_storage(target)
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
                        .any(|source| source.contains_storage(target))
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
        source: &Target,
        destination: &Target,
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
        slot: &Target,
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
