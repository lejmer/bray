use super::model::{Contents, Origin, Place, References, State, Target};
use bray_bound_tree::{BoundExpressionId, StorageIdentityId, StorageProjection};
use bray_symbols::TypeId;
use std::collections::BTreeSet;

impl State {
    fn slots_in<'a>(
        &'a self,
        place: &'a Place,
    ) -> impl Iterator<Item = (&'a Place, &'a References)> {
        // Place clones share the immutable projection path used as the ordered range boundary.
        self.slots
            .range(place.clone()..)
            .take_while(move |(slot, _)| place.contains(slot))
    }

    pub(in crate::analysis::storage_flow) fn has_reference(&self, slot: &Place) -> bool {
        slot.is_exact() && self.slots_in(slot).next().is_some()
    }

    pub(in crate::analysis::storage_flow) fn transfer(
        &mut self,
        source: &Place,
        destination: Place,
    ) {
        if !destination.is_exact() {
            self.clear(&Place::root(destination.root));
            return;
        }

        if !source.is_exact() {
            self.clear(&destination);
            return;
        }

        if *source == destination {
            return;
        }

        // Snapshot only the transferred subtree before replacing the destination subtree.
        let values = self
            .slots_in(source)
            .map(|(place, references)| {
                let projections = destination
                    .projections
                    .iter()
                    .chain(place.projections[source.projections.len()..].iter())
                    .copied()
                    .collect();

                (
                    Place {
                        root: destination.root,
                        projections,
                    },
                    references.clone(),
                )
            })
            .collect::<Vec<_>>();

        self.clear(&destination);
        self.slots.extend(values);
    }

    pub(in crate::analysis::storage_flow) fn clear(&mut self, place: &Place) {
        // Only the replaced subtree is visited, even when the function has many live slots.
        let removed = self
            .slots_in(place)
            .map(|(slot, _)| slot)
            .cloned()
            .collect::<Vec<_>>();

        for slot in removed {
            self.slots.remove(&slot);
        }
    }

    pub(in crate::analysis::storage_flow) fn offset(&mut self, source: &Place, destination: Place) {
        let mut references = self.references(source);

        references.displaced = true;
        self.slots.insert(destination, references);
    }

    pub(in crate::analysis::storage_flow) fn forget(&mut self, identity: StorageIdentityId) {
        self.clear(&Place::root(identity));
    }

    pub(in crate::analysis::storage_flow) fn retain_live(
        &mut self,
        live: &BTreeSet<StorageIdentityId>,
    ) {
        self.slots.retain(|place, _| live.contains(&place.root));
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
            for references in self.slots.values_mut().chain(
                self.contents
                    .values_mut()
                    .map(|contents| &mut contents.references),
            ) {
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

    pub(in crate::analysis::storage_flow) fn borrow(
        &mut self,
        source: Place,
        destination: Place,
        ty: TypeId,
    ) {
        let exact = source.is_exact();

        let target = Target {
            origin: Origin::Storage(source),
            previous: false,
        };

        // Borrow formation establishes a reference, not raw initialization evidence.
        self.contents
            .entry(target.clone())
            .or_default()
            .storage_type = Some(ty);

        let mut references = References::one(target);

        references.unknown = !exact;
        self.slots.insert(destination, references);
    }

    pub(in crate::analysis::storage_flow) fn referent(
        &self,
        place: &Place,
        ty: TypeId,
    ) -> Option<Place> {
        let references = self.references(&Place::root(place.root));

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

    pub(in crate::analysis::storage_flow) fn address(
        &mut self,
        source: Place,
        destination: Place,
        ty: TypeId,
        origin: BoundExpressionId,
    ) {
        let exact = source.is_exact();

        let target = Target {
            origin: Origin::Storage(source),
            previous: false,
        };

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

        let mut references = References::one(target);

        references.unknown = !exact;
        self.slots.insert(destination, references);
    }

    pub(super) fn references(&self, slot: &Place) -> References {
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

    pub(super) fn initialized(&self, slot: &Place, ty: TypeId) -> bool {
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

    pub(super) fn affected_targets(&self, references: &References) -> BTreeSet<Target> {
        // Unknown addresses may alias any tracked target. Exact references avoid this scan.
        if references.unknown {
            return self
                .contents
                .keys()
                .chain(references.targets.iter())
                .cloned()
                .collect();
        }

        // Target sets own shared immutable projection paths.
        let mut affected = references.targets.clone();

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
