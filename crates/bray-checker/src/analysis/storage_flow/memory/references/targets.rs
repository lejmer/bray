use super::super::model::{Origin, Place, References, State, Target};
use bray_bound_tree::{BorrowCapabilityId, BoundExpressionId};
use bray_symbols::TypeId;
use std::collections::BTreeSet;

impl State {
    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn begin_borrow(
        &mut self,
        capability: BorrowCapabilityId,
        source: Place,
        ty: TypeId,
    ) {
        self.borrow_target_storage(capability, Target::storage(source), ty);
    }

    pub(in crate::analysis::storage_flow) fn reborrow_value(
        &mut self,
        capability: BorrowCapabilityId,
        source: &Target,
        parent: Option<BorrowCapabilityId>,
    ) {
        let references = self
            .values
            .get(source)
            .or_else(|| {
                source
                    .storage_path()
                    .filter(|path| path.projections.is_empty())
                    .and_then(|_| parent.and_then(|parent| self.borrows.get(&parent)))
            })
            .cloned()
            .unwrap_or(References {
                unknown: true,
                ..References::default()
            });

        self.borrows.insert(capability, references);
    }

    pub(in crate::analysis::storage_flow) fn retain_borrow(
        &mut self,
        capability: BorrowCapabilityId,
        destination: Place,
    ) {
        self.clear(&destination);

        if let Some(references) = self.borrows.get(&capability) {
            // The destination owns a reference value after the expression's capability expires.
            self.values
                .insert(Target::storage(destination), references.clone());
        }
    }

    pub(in crate::analysis::storage_flow) fn has_borrow(
        &self,
        capability: BorrowCapabilityId,
    ) -> bool {
        self.borrows.contains_key(&capability)
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
        let references = self.addressed_target(Target::storage(source), ty, bray_symbols::DependencyReferenceKind::Address);

        self.values
            .insert(Target::storage(destination.clone()), references);

        self.expose_address(&destination, ty, origin);
    }

    pub(in crate::analysis::storage_flow) fn expose_address(
        &mut self,
        slot: &Place,
        ty: TypeId,
        origin: BoundExpressionId,
    ) {
        let references = self.references(&Target::storage(slot.clone()));
        let exact = references.is_exact();

        for target in &references.targets {
            let contents = self.contents.entry(target.clone()).or_default();

            contents.exposed = true;

            if let Some(previous) = contents.storage_type {
                assert_eq!(
                    previous, ty,
                    "address formation preserves the declared storage type"
                );
            }

            contents.storage_type = Some(ty);

            if exact {
                contents.initialized.insert(ty);
            }

            contents.pending.entry(ty).or_default().insert(origin);
        }
    }
    pub(in crate::analysis::storage_flow::memory) fn references(
        &self,
        slot: &Target,
    ) -> References {
        // Transfers and mutations need an owned snapshot while changing their reached targets.
        let mut references = if slot.is_exact() {
            self.values.get(slot).cloned().unwrap_or_default()
        } else {
            References::default()
        };

        references.unknown |= references.targets.is_empty();

        references
    }

    pub(in crate::analysis::storage_flow) fn invalidated(&self, slot: &Target) -> bool {
        self.values.get(slot).is_some_and(|references| {
            references.targets.iter().any(|target| {
                self.contents
                    .get(target)
                    .is_some_and(|contents| !contents.invalidations.is_empty())
            })
        })
    }

    pub(in crate::analysis::storage_flow::memory) fn initialized(
        &self,
        slot: &Target,
        ty: TypeId,
    ) -> bool {
        self.initialized_targets(&self.references(slot), ty)
    }

    pub(in crate::analysis::storage_flow::memory) fn initialized_targets(
        &self,
        references: &References,
        ty: TypeId,
    ) -> bool {
        !references.unknown
            && !references.displaced
            && !references.targets.is_empty()
            && references.targets.iter().all(|target| {
                target.is_exact()
                    && self
                        .contents
                        .get(target)
                        .is_some_and(|contents| contents.initialized.contains(&ty))
            })
    }

    pub(in crate::analysis::storage_flow::memory) fn affected_targets(
        &self,
        references: &References,
    ) -> BTreeSet<Target> {
        let regions = references.regions();

        self.contents
            .keys()
            .chain(self.values.keys())
            .chain(references.targets.iter())
            .filter(|candidate| {
                regions.as_ref().is_none_or(|regions| {
                    regions.iter().any(|target| {
                        self.target_relationship(target, candidate)
                            != bray_bound_tree::StorageRelationship::Disjoint
                    })
                })
            })
            // Targets share immutable paths while each transition owns its affected set.
            .cloned()
            .collect()
    }

    /// Current ordinary storage reached by this pointer. The boolean identifies a must-target.
    pub(in crate::analysis::storage_flow) fn reached_storage(
        &self,
        slot: &Target,
    ) -> Vec<(Place, Option<TypeId>, bool)> {
        let references = self.references(slot);
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
        located_storage(self.borrows.get(&capability)?.clone())
    }

    pub(in crate::analysis::storage_flow) fn allocation_origins(
        &self,
        slot: &Target,
    ) -> impl Iterator<Item = BoundExpressionId> + '_ {
        self.references(slot)
            .targets
            .into_iter()
            .flat_map(|target| {
                self.contents
                    .get(&target)
                    .into_iter()
                    .flat_map(|contents| contents.allocations.iter().copied())
            })
    }

    pub(in crate::analysis::storage_flow) fn invalidation_origins(
        &self,
        slot: &Target,
    ) -> impl Iterator<Item = BoundExpressionId> + '_ {
        self.references(slot)
            .targets
            .into_iter()
            .flat_map(|target| {
                self.contents
                    .get(&target)
                    .into_iter()
                    .flat_map(|contents| contents.invalidations.iter().copied())
            })
    }

    pub(in crate::analysis::storage_flow) fn initialization_origins(
        &self,
        slot: &Target,
    ) -> impl Iterator<Item = BoundExpressionId> + '_ {
        self.references(slot)
            .targets
            .into_iter()
            .flat_map(|target| {
                self.contents
                    .get(&target)
                    .into_iter()
                    .flat_map(|contents| contents.pending.values().flatten().copied())
            })
    }
}

#[cfg(test)]
fn located_storage(references: References) -> Option<Vec<Place>> {
    if references.unknown || references.displaced {
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
