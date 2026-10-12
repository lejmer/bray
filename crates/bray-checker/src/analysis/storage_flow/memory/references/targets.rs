use super::super::model::{Origin, Place, References, State, Target};
use bray_bound_tree::{BorrowCapabilityId, BoundExpressionId, StorageProjection};
use bray_symbols::TypeId;
use std::collections::BTreeSet;

impl State {
    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn borrow(
        &mut self,
        source: Place,
        destination: Place,
        ty: TypeId,
    ) {
        let references = self.borrowed_reference(source, ty);

        self.slots.insert(destination, references);
    }

    pub(in crate::analysis::storage_flow) fn begin_borrow(
        &mut self,
        capability: BorrowCapabilityId,
        source: Place,
        ty: TypeId,
    ) {
        let references = self.borrowed_reference(source, ty);

        self.borrows.insert(capability, references);
    }

    pub(in crate::analysis::storage_flow) fn reborrow(
        &mut self,
        capability: BorrowCapabilityId,
        source: &Place,
        parent: Option<BorrowCapabilityId>,
    ) {
        let references = self
            .slots
            .get(source)
            .or_else(|| {
                source
                    .projections
                    .is_empty()
                    .then(|| parent.and_then(|parent| self.borrows.get(&parent)))
                    .flatten()
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
            self.slots.insert(destination, references.clone());
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

    pub(super) fn borrowed_reference(&mut self, source: Place, ty: TypeId) -> References {
        let exact = source.is_exact();

        let target = Target {
            origin: Origin::Storage(source),
            previous: false,
        };

        // Borrow formation establishes a reference, not raw initialization evidence.
        self.contents
            .entry(target.clone())
            .or_default()
            .storage_type
            .get_or_insert(ty);

        let mut references = References::one(target);

        references.unknown = !exact;

        references
    }

    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn referent(
        &self,
        place: &Place,
        ty: TypeId,
    ) -> Option<Place> {
        let references = self.references(&Place::root(place.root));

        self.project_referent(&references, place, ty)
    }

    pub(in crate::analysis::storage_flow) fn slot_referent(
        &self,
        slot: &Place,
        ty: TypeId,
    ) -> Option<Place> {
        self.project_referent(&self.references(slot), &Place::root(slot.root), ty)
    }

    pub(in crate::analysis::storage_flow) fn borrow_referent(
        &self,
        capability: BorrowCapabilityId,
        place: &Place,
        ty: TypeId,
    ) -> Option<Place> {
        self.project_referent(self.borrows.get(&capability)?, place, ty)
    }

    fn project_referent(
        &self,
        references: &References,
        place: &Place,
        ty: TypeId,
    ) -> Option<Place> {
        if !references.is_exact() {
            return None;
        }

        let target = references.targets.first()?;

        // A raw reinterpretation preserves its byte address, not a typed field path.
        if self.contents.get(target)?.storage_type != Some(ty) {
            return None;
        }

        let Origin::Storage(target) = &target.origin else {
            return None;
        };

        Some(Place {
            root: target.root,
            projections: target
                .projections
                .iter()
                .chain(place.projections.iter())
                .copied()
                .collect(),
        })
    }

    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn address(
        &mut self,
        source: Place,
        destination: Place,
        ty: TypeId,
        origin: BoundExpressionId,
    ) {
        let references = self.borrowed_reference(source, ty);

        self.slots.insert(destination.clone(), references);
        self.expose_address(&destination, ty, origin);
    }

    pub(in crate::analysis::storage_flow) fn expose_address(
        &mut self,
        slot: &Place,
        ty: TypeId,
        origin: BoundExpressionId,
    ) {
        let references = self.references(slot);
        let exact = references.is_exact();

        for target in &references.targets {
            let contents = self.contents.entry(target.clone()).or_default();

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
    pub(in crate::analysis::storage_flow::memory) fn references(&self, slot: &Place) -> References {
        // Transfers and mutations need an owned snapshot while changing their reached targets.
        let mut references = if slot.is_exact() {
            self.slots.get(slot).cloned().unwrap_or_default()
        } else {
            References::default()
        };

        references.unknown |= references.targets.is_empty();

        references
    }

    pub(in crate::analysis::storage_flow) fn invalidated(&self, slot: &Place) -> bool {
        self.slots.get(slot).is_some_and(|references| {
            references.targets.iter().any(|target| {
                self.contents
                    .get(target)
                    .is_some_and(|contents| !contents.invalidations.is_empty())
            })
        })
    }

    pub(in crate::analysis::storage_flow::memory) fn initialized(
        &self,
        slot: &Place,
        ty: TypeId,
    ) -> bool {
        self.slots.get(slot).is_some_and(|references| {
            !references.unknown
                && !references.displaced
                && !references.targets.is_empty()
                && references.targets.iter().all(|target| {
                    self.contents
                        .get(target)
                        .is_some_and(|contents| contents.initialized.contains(&ty))
                })
        })
    }

    pub(in crate::analysis::storage_flow::memory) fn invalidate_entry_slots(
        &mut self,
        references: &References,
    ) {
        if references.may_alias_anywhere() {
            for target in self.contents.keys() {
                let Origin::Entry(slot) = &target.origin else {
                    break;
                };

                self.slots.remove(slot);
            }
        }
    }

    pub(in crate::analysis::storage_flow::memory) fn affected_targets(
        &self,
        references: &References,
    ) -> BTreeSet<Target> {
        // Original input values establish identity, not separation from other storage.
        if references.may_alias_anywhere() {
            return self
                .contents
                .keys()
                .chain(references.targets.iter())
                .cloned()
                .collect();
        }

        // Target sets own shared immutable projection paths.
        let mut affected = references.targets.clone();

        // Every unresolved input target can alias an otherwise concrete mutation target.
        affected.extend(
            self.contents
                .keys()
                .take_while(|target| matches!(target.origin, Origin::Entry(_)))
                .cloned(),
        );

        for target in &references.targets {
            let Origin::Storage(place) = &target.origin else {
                continue;
            };

            let start = Target {
                origin: Origin::Storage(Place::root(place.root)),
                previous: false,
            };

            for (candidate, _) in self.contents.range(start..) {
                let Origin::Storage(other) = &candidate.origin else {
                    break;
                };

                if other.root != place.root {
                    break;
                }

                if candidate.previous == target.previous
                    && StorageProjection::relationship(&place.projections, &other.projections)
                        != bray_bound_tree::StorageRelationship::Disjoint
                {
                    affected.insert(candidate.clone());
                }
            }
        }

        affected
    }

    /// Current ordinary storage reached by this pointer. The boolean identifies a must-target.
    pub(in crate::analysis::storage_flow) fn reached_storage(
        &self,
        slot: &Place,
    ) -> Vec<(Place, Option<TypeId>, bool)> {
        let references = self.references(slot);
        let exact = references.is_exact();

        // Ownership paths already compare overlap. Only unknown pointers need the full set.
        let targets = if references.unknown {
            self.contents
                .keys()
                .chain(references.targets.iter())
                .collect::<BTreeSet<_>>()
        } else {
            references.targets.iter().collect()
        };

        targets
            .into_iter()
            .filter_map(|target| {
                let definite = exact && references.targets.contains(target);

                match &target.origin {
                    // Place clones share the immutable projection path.
                    Origin::Storage(place) if !target.previous => Some((
                        place.clone(),
                        self.contents
                            .get(target)
                            .and_then(|contents| contents.storage_type),
                        definite,
                    )),
                    _ => None,
                }
            })
            .collect()
    }

    /// Only fully located ordinary targets can narrow observation invalidation.
    pub(in crate::analysis::storage_flow) fn located_storage(
        &self,
        slot: &Place,
    ) -> Option<Vec<Place>> {
        let references = self.references(slot);

        located_storage(references)
    }

    pub(in crate::analysis::storage_flow) fn borrowed_storage(
        &self,
        capability: BorrowCapabilityId,
    ) -> Option<Vec<Place>> {
        located_storage(self.borrows.get(&capability)?.clone())
    }

    pub(in crate::analysis::storage_flow) fn allocation_origins(
        &self,
        slot: &Place,
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
        slot: &Place,
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
        slot: &Place,
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
