use super::super::model::{Origin, Place, AddressValue, ReachedAccess, State, Target, ValueBinding};
use bray_bound_tree::{BorrowCapabilityId, BoundExpressionId};
use bray_symbols::TypeId;
use std::collections::BTreeSet;

pub(in crate::analysis::storage_flow) fn borrow_chain_from(
    storage: &bray_bound_tree::StoragePlan,
    memory: Option<&State>,
    mut capability: Option<BorrowCapabilityId>,
) -> Option<Vec<BorrowCapabilityId>> {
    let mut chain = Vec::new();

    while let Some(current) = capability {
        if chain.contains(&current) {
            return None;
        }

        let borrow = storage.borrow_capability(current)?;

        chain.push(current);

        if let Some(ancestors) = memory.and_then(|memory| memory.borrow_ancestors(current)) {
            for &ancestor in ancestors {
                if !chain.contains(&ancestor) {
                    chain.push(ancestor);
                }
            }

            break;
        }

        capability = borrow.parent();
    }

    Some(chain)
}

impl State {
    pub(in crate::analysis::storage_flow) fn reached_dependency_storage(
        &self,
        target: &Target,
        ty: TypeId,
    ) -> Option<(bool, bool)> {
        let contents = self.contents.get(target);
        let allocation = target.allocation_region().and_then(|root| self.contents.get(&root));

        let invalidated = contents.into_iter().chain(allocation)
            .any(|contents| !contents.invalidations.is_empty());

        if invalidated {
            return None;
        }

        let initialized = contents.is_some_and(|contents| contents.is_initialized(ty));
        let allocated = contents.into_iter().chain(allocation).any(|contents| !contents.allocations.is_empty());

        Some((allocated || initialized, initialized))
    }

    pub(in crate::analysis::storage_flow) fn entry_cell_changes(
        &self,
    ) -> impl Iterator<Item = (&Target, bray_symbols::DependencyMutationKind)> {
        self.contents.iter().filter(|(target, contents)| {
            matches!(target.origin, Origin::Entry { kind: bray_symbols::DependencyReferenceKind::Address, .. })
                && target.is_exact() && contents.invalidations.is_empty()
        }).flat_map(|(target, contents)| contents.cell_changes().map(move |kind| (target, kind)))
    }

    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn begin_borrow(
        &mut self,
        capability: BorrowCapabilityId,
        source: Place,
        ty: TypeId,
    ) {
        self.borrow_target_storage(crate::test_support::semantic_values(), capability, [Target::storage(source)], ty);
    }

    pub(in crate::analysis::storage_flow) fn reborrow_access(&self, source: ReachedAccess) -> ReachedAccess {
        if !source.address.has_values() {
            return source;
        }

        let mut references = AddressValue::default();
        let mut common_loan = None;

        for slot in &source.address.targets {
            let value = self.values.get(slot);
            let loan = value.and_then(super::super::model::ValueBinding::borrow_capability).or(source.loan);

            let incoming = value.and_then(|value| value.address.as_ref())
                .or_else(|| {
                    slot.storage_path()
                        .filter(|path| path.projections.is_empty())
                        .and_then(|_| loan.and_then(|parent| self.borrow_address(parent)))
                });

            if let Some(incoming) = incoming {
                references.merge(incoming);
            } else {
                references.forget_value();
            }

            common_loan = Some(match common_loan {
                None => loan,
                Some(previous) if previous == loan => previous,
                _ => None,
            });
        }

        let loan = if references.has_values() && !references.targets.is_empty()
            && references.targets.iter().all(|target| !target.previous) {
            common_loan.flatten()
        } else {
            None
        };

        ReachedAccess { address: references, loan }
    }

    pub(in crate::analysis::storage_flow) fn retain_borrow(
        &mut self,
        capability: BorrowCapabilityId,
        destination: Place,
    ) {
        self.clear(&destination);

        if let Some(references) = self.borrow_address(capability).cloned() {
            // The destination owns a reference value after the expression's capability expires.
            self.values
                .insert(Target::storage(destination), super::super::model::ValueBinding::borrowed(references, capability));
        }
    }

    pub(in crate::analysis::storage_flow) fn has_borrow(
        &self,
        capability: BorrowCapabilityId,
    ) -> bool {
        self.borrows.contains_key(&capability)
    }

    pub(in crate::analysis::storage_flow) fn reached_borrow_capability(
        &self,
        target: &Target,
    ) -> Option<BorrowCapabilityId> {
        if !target.is_exact() {
            return None;
        }

        self.values.get(target)?.borrow_capability()
    }

    pub(in crate::analysis::storage_flow) fn end_borrows(
        &mut self,
        ended: &BTreeSet<BorrowCapabilityId>,
    ) {
        self.borrows
            .retain(|capability, _| !ended.contains(capability));
    }

    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn referent(
        &self,
        place: &Place,
        ty: TypeId,
    ) -> Option<Place> {
        self.value_target(&Target::storage(place.clone()), ty)?
            .storage_path()
            .cloned()
    }

    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn address(
        &mut self,
        source: Place,
        destination: Place,
        ty: TypeId,
        origin: BoundExpressionId,
    ) {
        let references = self.addressed_target(
            Target::storage(source),
            ty,
            bray_symbols::DependencyReferenceKind::Address,
        );

        self.publish_exposed_address(&destination, ty, origin, &references.into());
    }

    pub(in crate::analysis::storage_flow) fn publish_exposed_address(
        &mut self,
        slot: &Place,
        ty: TypeId,
        origin: BoundExpressionId,
        source: &ValueBinding,
    ) {
        let value = source.exposed_address();

        self.observe_address_exposure(&value, ty, origin);
        self.clear(slot);
        self.values.insert(Target::storage(slot.clone()), value);
    }

    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn expose_address(&mut self, slot: &Place, ty: TypeId, origin: BoundExpressionId) {
        if let Some(value) = self.values.get(&Target::storage(slot.clone())).cloned() {
            self.observe_address_exposure(&value, ty, origin);
        }
    }

    fn observe_address_exposure(
        &mut self,
        value: &ValueBinding,
        ty: TypeId,
        origin: BoundExpressionId,
    ) {
        let exact = value.address.as_ref().is_some_and(AddressValue::is_exact);

        for references in value.possible_references() {
            for target in &references.targets {
                let contents = self.contents.entry(target.clone()).or_default();

                contents.exposed = true;

                if exact {
                    if let Some(previous) = contents.storage_type {
                        assert_eq!(
                            previous, ty,
                            "exact address formation preserves the declared storage type: {target:?}"
                        );
                    }

                    contents.storage_type = Some(ty);
                    contents.observe_initialized(ty);
                }

                contents.pending.entry(ty).or_default().insert(origin);
            }
        }
    }

    pub(in crate::analysis::storage_flow) fn borrow_view(&mut self, slot: &Place, ty: TypeId) {
        let Some(references) = self.values.get_mut(&Target::storage(slot.clone())).and_then(|value| value.address.as_mut()) else {
            return;
        };

        if references.is_exact() {
            let target = references
                .targets
                .first()
                .expect("exact reference has one target");

            // A checked borrow may interpret a differently typed region. Keep
            // its provenance without making the view that region's declaration.
            if self
                .contents
                .get(target)
                .and_then(|contents| contents.storage_type)
                .is_some_and(|declared| declared != ty) {
                references.bound();
            }
        }
    }
    pub(in crate::analysis::storage_flow::memory) fn references(
        &self,
        slot: &Target,
    ) -> AddressValue {
        // Transfers and mutations need an owned snapshot while changing their reached targets.
        if slot.is_exact() {
            self.values.get(slot).and_then(|value| value.captured_reference()).unwrap_or_else(AddressValue::unknown)
        } else {
            AddressValue::unknown()
        }
    }

    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn invalidated(&self, slot: &Target) -> bool {
        self.values.get(slot).is_some_and(|value| self.invalidated_capture(value))
    }

    pub(in crate::analysis::storage_flow::memory) fn invalidated_capture(&self, value: &super::super::model::ValueBinding) -> bool {
        self.captured_referent_contents(value)
            .any(|contents| !contents.invalidations.is_empty())
    }

    pub(in crate::analysis::storage_flow) fn captured_referent_contents<'a>(
        &'a self,
        value: &'a super::super::model::ValueBinding,
    ) -> impl Iterator<Item = &'a bray_bound_tree::StorageContents> + 'a {
        value.possible_references()
            .flat_map(|references| &references.targets)
            .filter_map(|target| self.contents.get(target))
    }

    #[cfg(test)]
    pub(in crate::analysis::storage_flow::memory) fn initialized(
        &self,
        slot: &Target,
        ty: TypeId,
    ) -> bool {
        self.initialized_targets(&self.references(slot), ty)
    }

    pub(in crate::analysis::storage_flow::memory) fn initialized_targets(
        &self,
        references: &AddressValue,
        ty: TypeId,
    ) -> bool {
        references.has_values()
            && !references.targets.is_empty()
            && references.targets.iter().all(|target| {
                target.is_exact()
                    && self
                        .contents
                        .get(target)
                        .is_some_and(|contents| contents.is_initialized(ty))
            })
    }

    pub(in crate::analysis::storage_flow::memory) fn affected_targets(
        &self,
        references: &AddressValue,
    ) -> BTreeSet<Target> {
        let regions = references.regions();

        self.contents
            .keys()
            .chain(self.values.keys())
            .chain(references.targets.iter())
            .filter(|candidate| {
                let reaches = |target: &Target| {
                    self.target_relationship(target, candidate)
                        != bray_bound_tree::StorageRelationship::Disjoint
                };

                match &regions {
                    Some(regions) => regions.iter().any(reaches),
                    // An unresolved pointer can still retain explicit possible
                    // targets. Its unknown part cannot address private inline slots.
                    None => {
                        self.unknown_reference_may_reach(candidate)
                            || references.targets.iter().any(reaches)
                    }
                }
            })
            // Targets share immutable paths while each transition owns its affected set.
            .cloned()
            .collect()
    }

    /// Current ordinary storage reached by this pointer. The boolean identifies a must-target.
    #[cfg(test)]
    pub(in crate::analysis::storage_flow::memory) fn reached_storage(
        &self,
        slot: &Target,
    ) -> Vec<(Place, Option<TypeId>, bool)> {
        self.reached_reference_storage(&self.references(slot))
    }

    pub(in crate::analysis::storage_flow) fn reached_captured_storage(
        &self,
        binding: &super::super::model::ValueBinding,
    ) -> Vec<(Place, Option<TypeId>, bool)> {
        self.reached_reference_storage(&binding.captured_reference().unwrap_or_else(AddressValue::unknown))
    }

    fn reached_reference_storage(&self, references: &AddressValue) -> Vec<(Place, Option<TypeId>, bool)> {
        let exact = references.is_exact();

        // Exact paths already compare overlap. Unknown regions may reach any exposed storage.
        let regions = references.regions();

        let targets = regions.as_ref().map_or_else(
            || {
                self.contents
                    .keys()
                    .chain(references.targets.iter())
                    .collect::<BTreeSet<_>>()
            },
            |regions| regions.iter().collect(),
        );

        targets
            .into_iter()
            .filter_map(|target| {
                let definite = exact && references.targets.contains(target);

                match &target.origin {
                    // Place clones share the immutable projection path.
                    Origin::Storage(place)
                        if !target.previous
                            && !place.projections.iter().any(|projection| {
                                matches!(
                                    projection,
                                    bray_bound_tree::StorageProjection::UninitPayload
                                        | bray_bound_tree::StorageProjection::OwnedTarget
                                )
                            }) =>
                    {
                        Some((
                            place.clone(),
                            self.contents
                                .get(target)
                                .and_then(|contents| contents.storage_type),
                            definite,
                        ))
                    }
                    _ => None,
                }
            })
            .collect()
    }

    /// Only fully located ordinary targets can narrow observation invalidation.
    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn located_storage(
        &self,
        slot: &Target,
    ) -> Option<Vec<Place>> {
        let references = self.references(slot);

        located_storage(references)
    }

    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn borrowed_storage(
        &self,
        capability: BorrowCapabilityId,
    ) -> Option<Vec<Place>> {
        located_storage(self.borrow_address(capability)?.clone())
    }

    #[cfg(test)]
    fn referent_contents<'a>(
        &'a self,
        slot: &'a Target,
    ) -> impl Iterator<Item = &'a bray_bound_tree::StorageContents> + 'a {
        self.values.get(slot).filter(|_| slot.is_exact())
            .into_iter().flat_map(|value| self.captured_referent_contents(value))
    }

    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn allocation_origins<'a>(
        &'a self,
        slot: &'a Target,
    ) -> impl Iterator<Item = BoundExpressionId> + 'a {
        self.referent_contents(slot).flat_map(|contents| contents.allocations.iter().copied())
    }

    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn invalidation_origins<'a>(
        &'a self,
        slot: &'a Target,
    ) -> impl Iterator<Item = bray_bound_tree::BoundExecutionSite> + 'a {
        self.referent_contents(slot).flat_map(|contents| contents.invalidations.iter().copied())
    }

    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn initialization_origins<'a>(
        &'a self,
        slot: &'a Target,
    ) -> impl Iterator<Item = BoundExpressionId> + 'a {
        self.referent_contents(slot).flat_map(|contents| contents.pending.values().flatten().copied())
    }
}

#[cfg(test)]
fn located_storage(references: AddressValue) -> Option<Vec<Place>> {
    if !references.has_values() {
        return None;
    }

    references
        .targets
        .into_iter()
        .map(|target| match target.origin {
            Origin::Storage(place) if !target.previous => Some(place),
            _ => None,
        })
        .collect()
}
