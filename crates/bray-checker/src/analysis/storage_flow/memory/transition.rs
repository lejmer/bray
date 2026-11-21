use super::Source;
use super::references::storage_subtree;
use super::model::{Place, AddressValue, State, Target, Transfer};
use bray_bound_tree::{
    BoundExpressionId, MemoryOperationStatus, MemoryReadKind,
};
use bray_symbols::{StorageDischarge, TypeId};

impl State {
    pub(in crate::analysis::storage_flow) fn observe_effect_types(
        &mut self,
        effects: &[super::Effect],
    ) {
        for effect in effects {
            if let super::Effect::Mutation { address: Some(target), kind: bray_symbols::DependencyMutationKind::Replace(ty), .. } = effect
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
        self.consume_locations(&AddressValue::one(target), ty);
    }

    pub(in crate::analysis::storage_flow) fn consume_locations(&mut self, address: &AddressValue, ty: TypeId) {
        if self.tracks_locations(address) {
            self.consume_references(address, ty, true);
        }
    }

    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn write_target(
        &mut self, target: Target, ty: TypeId, origin: BoundExpressionId,
        invalidation: Option<&bray_bound_tree::StorageInvalidation>,
    ) {
        self.write_locations(&AddressValue::one(target), ty, origin, invalidation);
    }

    pub(in crate::analysis::storage_flow) fn write_locations(
        &mut self, address: &AddressValue, ty: TypeId, origin: BoundExpressionId,
        invalidation: Option<&bray_bound_tree::StorageInvalidation>,
    ) {
        if self.tracks_locations(address) {
            self.write_references(address, ty, origin, invalidation);
        }
    }

    fn tracks_locations(&self, address: &AddressValue) -> bool {
        // Logical-only admission must retain already prepared reference values. Keep
        // every alternative when physical tracking is needed, so weak writes stay weak.
        address.targets.iter().any(|target| target.storage_path().is_none()
            || self.has_addressed_storage(target))
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

    #[cfg(test)]
    pub(super) fn write(
        &mut self,
        slot: &Target,
        ty: TypeId,
        origin: BoundExpressionId,
        value: Option<Source>,
    ) -> MemoryOperationStatus {
        if self.invalidated(slot) {
            return MemoryOperationStatus::InvalidatedAllocation;
        }

        let pointer = slot.is_exact().then(|| self.values.get(slot)).flatten().cloned();

        self.write_captured(pointer.as_ref(), ty, origin, value)
    }

    pub(in crate::analysis::storage_flow) fn write_captured(
        &mut self,
        pointer: Option<&super::model::ValueBinding>,
        ty: TypeId,
        origin: BoundExpressionId,
        value: Option<Source>,
    ) -> MemoryOperationStatus {
        if pointer.is_some_and(|pointer| self.invalidated_capture(pointer)) {
            return MemoryOperationStatus::InvalidatedAllocation;
        }

        self.write_transferred(&captured_address(pointer), ty, origin, value);

        MemoryOperationStatus::Valid
    }

    fn write_transferred(
        &mut self,
        references: &AddressValue,
        ty: TypeId,
        origin: BoundExpressionId,
        source: Option<Source>,
    ) {
        if references.is_unknown() {
            // An unlocated write can affect any tracked cell. Collapse the source's
            // possible references instead of inventing a subtree at every possible address.
            let affected = self.affected_targets(references);

            let mut incoming = super::model::ValueBinding::from(AddressValue::unknown());

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

                value.completeness.lose_completeness();
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
            for references in value.references_mut() {
                references.forget_value();
            }

            value.completeness.lose_completeness();

            self.values.entry(slot).and_modify(|previous| { previous.merge(&value); }).or_insert(value);
        }

        for (slot, mut contents) in transfer.payloads {
            contents.clear_initialization();

            self.contents
                .entry(slot.clone())
                .or_default()
                .merge(&contents, &slot);
        }

        for (reference, bounds) in transfer.returned_inputs {
            self.returned_inputs
                .entry((reference, false))
                .or_default()
                .merge(&bounds);
        }
    }

    fn write_references(
        &mut self,
        references: &AddressValue,
        ty: TypeId,
        origin: BoundExpressionId,
        invalidation: Option<&bray_bound_tree::StorageInvalidation>,
    ) {
        let exact = references.is_exact();

        for target in self.affected_targets(references) {
            let selected = references.targets.contains(&target);
            let contents = self.contents.entry(target.clone()).or_default();

            contents.clear_initialization();

            if exact && selected {
                contents.record_write(ty);

                self.values.remove(&target);
            } else if !invalidation
                .is_some_and(|invalidation| Self::stable_reference_slot(&target, invalidation))
                && let Some(stored) = self.values.get_mut(&target)
            {
                for references in stored.references_mut() {
                    references.forget_value();
                }

                stored.completeness.lose_completeness();
            }

            if !exact || selected {
                // The duty follows the addressed cell. Its possible aliases
                // lose facts, while their independently recorded duties remain.
                contents.pending.entry(ty).or_default().insert(origin);
            }
        }
    }

    pub(in crate::analysis::storage_flow) fn write_value(
        &mut self,
        target: Target,
        ty: TypeId,
        origin: BoundExpressionId,
        source: Option<Source>,
    ) -> MemoryOperationStatus {
        self.write_transferred(&AddressValue::one(target), ty, origin, source);

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
        self.read_targets(AddressValue::one(target), ty, kind, proven, result)
    }

    #[cfg(test)]
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

    pub(in crate::analysis::storage_flow) fn read_captured(
        &mut self,
        source: Option<&super::model::ValueBinding>,
        ty: TypeId,
        kind: MemoryReadKind,
        proven: Option<BoundExpressionId>,
        result: Option<Place>,
    ) -> MemoryOperationStatus {
        if source.is_some_and(|source| self.invalidated_capture(source)) {
            return MemoryOperationStatus::InvalidatedAllocation;
        }

        let references = captured_address(source);

        self.read_targets(references, ty, kind, proven, result)
    }

    fn read_targets(
        &mut self,
        references: AddressValue,
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

            if !references.has_values()
                || references.targets.iter().any(|target| !target.is_exact())
            {
                for value in transfer.values.values_mut() {
                    for references in value.references_mut() {
                        references.forget_value();
                    }

                    value.completeness.lose_completeness();
                }

                for contents in transfer.payloads.values_mut() {
                    contents.clear_initialization();
                }
            }

            transfer
        });

        if kind == MemoryReadKind::Move {
            // A checked move transfers the witnessed initialized value at this
            // boundary. Keep its exact discharge without creating duties at aliases.
            if proven.is_some() && references.is_exact() {
                for target in &references.targets {
                    self.contents.entry(target.clone()).or_default();
                }
            }

            self.consume_references(&references, ty, true);
        } else if let Some(origin) = proven {
            let exact = references.is_exact();
            let possible = (!exact).then(|| self.affected_targets(&references));

            for target in possible.as_ref().unwrap_or(&references.targets) {
                let contents = self.contents.entry(target.clone()).or_default();

                if exact {
                    contents.observe_initialized(ty);
                }

                contents.pending.entry(ty).or_default().insert(origin);
            }
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

        self.consume_references(&self.references(slot), ty, true);

        MemoryOperationStatus::Valid
    }

    pub(in crate::analysis::storage_flow) fn apply_cell_changes(
        &mut self,
        changes: impl IntoIterator<Item = (Target, bray_symbols::DependencyMutationKind)>,
        occurrence: bray_bound_tree::BoundExecutionSite,
    ) {
        use bray_symbols::DependencyMutationKind as Mutation;

        for (target, change) in changes {
            let ty = match change {
                Mutation::Replace(ty) | Mutation::Discharge(ty) => ty,
                _ => unreachable!("published cell changes retain a typed update"),
            };

            if self.reached_dependency_storage(&target, ty).is_none() {
                continue;
            }

            let contents = self.contents.entry(target.clone()).or_default();

            match change {
                Mutation::Replace(ty) => {
                    contents.record_write(ty);
                    contents.pending.entry(ty).or_default().extend(occurrence.expression());
                }
                Mutation::Discharge(ty) => self.consume_references(&AddressValue::one(target), ty, false),
                _ => unreachable!("published cell changes retain a typed update"),
            }
        }
    }

    fn consume_references(&mut self, references: &AddressValue, ty: TypeId, preserve_evaluated: bool) {
        let exact = references.is_exact();

        for target in &self.affected_targets(references) {
            if let Some(contents) = self.contents.get_mut(target) {
                if preserve_evaluated && exact
                    && references.targets.iter().any(|source| {
                        source.contains_storage(target)
                            && target.projections()[source.projections().len()..]
                                .contains(&bray_bound_tree::StorageProjection::UninitPayload)
                    })
                {
                    // The evaluated value owns these facts after its source loses initialization.
                    self.evaluated.insert(target.clone(), contents.clone());
                }

                contents.clear_initialization();

                // Consumption ends initialized ownership, not the evaluated value's identity.
                // Normal value transfer still needs its reference fields before slot retirement.

                if exact
                    && references
                        .targets
                        .iter()
                        .any(|source| source.contains_storage(target))
                {
                    if references.targets.contains(target) {
                        contents.record_consumption(ty);
                    } else {
                        // Whole-value consumption transfers nested protected ownership as well.
                        contents.pending.clear();
                    }
                }
            }
        }
    }

    #[cfg(test)]
    pub(super) fn copy(
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

        let source = source.is_exact().then(|| self.values.get(source)).flatten().cloned();
        let destination = destination.is_exact().then(|| self.values.get(destination)).flatten().cloned();

        self.copy_captured(source.as_ref(), destination.as_ref(), ty, origin, proven)
    }

    pub(in crate::analysis::storage_flow) fn copy_captured(
        &mut self,
        source: Option<&super::model::ValueBinding>,
        destination: Option<&super::model::ValueBinding>,
        ty: TypeId,
        origin: BoundExpressionId,
        proven: Option<bool>,
    ) -> MemoryOperationStatus {
        if source.into_iter().chain(destination).any(|pointer| self.invalidated_capture(pointer)) {
            return MemoryOperationStatus::InvalidatedAllocation;
        }

        // An empty range has no scalar effect.
        if proven == Some(true) {
            return MemoryOperationStatus::Valid;
        }

        if proven.is_none() && !self.initialized_targets(&captured_address(source), ty) {
            return MemoryOperationStatus::UninitializedRawStorage;
        }

        self.write_captured(destination, ty, origin, None)
    }

    pub(in crate::analysis::storage_flow) fn deallocate(
        &mut self,
        pointer: Option<&super::model::ValueBinding>,
        origin: BoundExpressionId,
    ) -> MemoryOperationStatus {
        if pointer.is_some_and(|pointer| self.invalidated_capture(pointer)) {
            return MemoryOperationStatus::InvalidatedAllocation;
        }

        let references = captured_address(pointer);
        let targets = self.affected_targets(&references);

        if self.outstanding_storage(&targets).next().is_some() {
            return MemoryOperationStatus::OutstandingObligations;
        }

        for target in targets {
            self.retire_backing(target, origin.into());
        }

        MemoryOperationStatus::Valid
    }

    pub(in crate::analysis::storage_flow::memory) fn retire_backing(&mut self, target: Target,
        origin: bray_bound_tree::BoundExecutionSite,
    ) {
        self.contents.entry(target.clone()).or_default().retire(origin);
        self.retirements.insert((target, bray_bound_tree::StorageValueRetirement::Backing));
    }

    #[cfg(test)]
    pub(super) fn deallocate_slot(&mut self, slot: &Target, origin: BoundExpressionId) -> MemoryOperationStatus {
        let pointer = self.values.get(slot).cloned();

        self.deallocate(pointer.as_ref(), origin)
    }

    pub(in crate::analysis::storage_flow) fn deallocation_duty_origins(
        &self,
        pointer: Option<&super::model::ValueBinding>,
    ) -> Vec<BoundExpressionId> {
        let references = captured_address(pointer);

        self.outstanding_storage(&self.affected_targets(&references))
            .flat_map(|contents| contents.pending.values().flatten().copied())
            .collect()
    }

    fn outstanding_storage<'a>(
        &'a self,
        targets: &'a std::collections::BTreeSet<Target>,
    ) -> impl Iterator<Item = &'a super::model::Contents> + 'a {
        targets.iter().filter_map(|target| self.contents.get(target))
            .filter(|contents| !contents.pending.is_empty())
    }

    pub(in crate::analysis::storage_flow) fn retirement_discharges<'a>(
        &'a self,
        target: &'a Target,
    ) -> impl Iterator<Item = StorageDischarge<bray_bound_tree::StorageProjection>> + 'a {
        storage_subtree(&self.contents, target).filter(|(cell, _)| cell.is_exact())
            .flat_map(move |(cell, contents)| {
                contents.consumed_types().map(move |ty| StorageDischarge {
                    projections: cell.projections()[target.projections().len()..].into(),
                    ty,
                })
            })
    }

    /// Retains the actual duties not transferred before this reached retirement.
    pub(in crate::analysis::storage_flow) fn retirement_obligations(
        &self,
        target: &Target,
        discharged: &[StorageDischarge<bray_bound_tree::StorageProjection>],
    ) -> Option<Vec<BoundExpressionId>> {
        let discharged = discharged.iter().map(|duty| {
            let cell = target.project(&duty.projections);

            let complete_value = self.contents.get(&cell).is_some_and(|contents| {
                contents.storage_type == Some(duty.ty) || contents.is_initialized(duty.ty)
            });

            (cell, duty.ty, complete_value)
        }).collect::<Vec<_>>();

        let discharged = &discharged;

        let mut outstanding = self.contents.iter().filter(|(candidate, contents)| {
            !contents.pending.is_empty()
                && self.target_relationship(target, candidate) != bray_bound_tree::StorageRelationship::Disjoint
        }).flat_map(|(candidate, contents)| {
            contents.pending.iter().filter(move |(ty, _)| {
                !(candidate.is_exact() && discharged.iter().any(|(cell, read, complete_value)| {
                    if cell == candidate {
                        read == *ty
                    } else {
                        *complete_value && cell.contains_storage(candidate)
                    }
                }))
            }).map(|(_, origins)| origins)
        }).peekable();

        outstanding.peek()?;

        Some(outstanding.flatten().copied().collect())
    }
}

fn captured_address(pointer: Option<&super::model::ValueBinding>) -> AddressValue {
    pointer.and_then(super::model::ValueBinding::captured_reference)
        .unwrap_or_else(AddressValue::unknown)
}
