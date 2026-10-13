use super::super::model::{Origin, Place, References, State, Target};
use bray_bound_tree::{BorrowCapabilityId, StorageProjection};
use bray_symbols::TypeId;

impl Target {
    pub(in crate::analysis::storage_flow::memory) fn is_exact(&self) -> bool {
        !self.previous
            && match &self.origin {
                Origin::Entry {
                    reference,
                    projections,
                } => {
                    reference.is_exact()
                        && projections.iter().all(|projection| projection.is_exact())
                }
                Origin::Storage(place) | Origin::Allocation(place) => place.is_exact(),
            }
    }

    pub(in crate::analysis::storage_flow) fn observation_path(&self) -> Option<Place> {
        if self.previous {
            return None;
        }

        match &self.origin {
            Origin::Storage(place) => Some(place.clone()),
            Origin::Entry {
                reference,
                projections,
            } if reference.projections.is_empty() => Some(Place {
                root: reference.root,
                projections: projections.clone(),
            }),
            _ => None,
        }
    }

    pub(in crate::analysis::storage_flow) fn projections(&self) -> &[StorageProjection] {
        match &self.origin {
            Origin::Entry { projections, .. } => projections,
            Origin::Storage(place) | Origin::Allocation(place) => &place.projections,
        }
    }

    pub(in crate::analysis::storage_flow) fn relationship(
        &self,
        other: &Self,
    ) -> bray_bound_tree::StorageRelationship {
        use bray_bound_tree::StorageRelationship;

        if self.previous != other.previous {
            return StorageRelationship::Disjoint;
        }

        match (&self.origin, &other.origin) {
            (
                Origin::Entry {
                    reference: left,
                    projections: left_path,
                },
                Origin::Entry {
                    reference: right,
                    projections: right_path,
                },
            ) if left == right => StorageProjection::relationship(left_path, right_path),
            (Origin::Entry { .. }, _) | (_, Origin::Entry { .. }) => {
                StorageRelationship::PotentiallyOverlapping
            }
            (Origin::Storage(left), Origin::Storage(right))
            | (Origin::Allocation(left), Origin::Allocation(right)) => left.relationship(right),
            _ => StorageRelationship::Disjoint,
        }
    }

    pub(in crate::analysis::storage_flow) fn contains(&self, other: &Self) -> bool {
        if self.previous != other.previous {
            return false;
        }

        match (&self.origin, &other.origin) {
            (
                Origin::Entry {
                    reference: left,
                    projections: prefix,
                },
                Origin::Entry {
                    reference: right,
                    projections,
                },
            ) => left == right && projections.starts_with(prefix),
            (Origin::Storage(left), Origin::Storage(right))
            | (Origin::Allocation(left), Origin::Allocation(right)) => left.contains(right),
            _ => false,
        }
    }

    pub(in crate::analysis::storage_flow) fn project(
        &self,
        projections: &[StorageProjection],
    ) -> Self {
        let append =
            |prefix: &[StorageProjection]| prefix.iter().chain(projections).copied().collect();

        let path = |place: &Place| Place {
            root: place.root,
            projections: append(&place.projections),
        };

        let origin = match &self.origin {
            Origin::Entry {
                reference,
                projections: prefix,
            } => Origin::Entry {
                // The original reference identity survives later assignments to its source slot.
                reference: reference.clone(),
                projections: append(prefix),
            },
            Origin::Storage(place) => Origin::Storage(path(place)),
            Origin::Allocation(place) => Origin::Allocation(path(place)),
        };

        Self {
            origin,
            previous: self.previous,
        }
    }
}

impl State {
    pub(in crate::analysis::storage_flow) fn for_storage(
        storage: &bray_bound_tree::StoragePlan,
    ) -> Self {
        Self {
            owned_storage: storage
                .identity_entries()
                .filter_map(|(id, identity)| {
                    identity.is_invocation_owned(storage.kind()).then_some(id)
                })
                .collect(),
            ..Self::default()
        }
    }

    pub(in crate::analysis::storage_flow) fn target_relationship(
        &self,
        left: &Target,
        right: &Target,
    ) -> bray_bound_tree::StorageRelationship {
        let slot = match (&left.origin, &right.origin) {
            (Origin::Entry { .. }, Origin::Storage(slot))
            | (Origin::Storage(slot), Origin::Entry { .. }) => Some(slot),
            _ => None,
        };

        if slot.is_some_and(|slot| self.owned_storage.binary_search(&slot.root).is_ok()) {
            // An original input cannot address this invocation's value slots. A
            // reference subsequently formed to one is an ordinary Storage target.
            return bray_bound_tree::StorageRelationship::Disjoint;
        }

        left.relationship(right)
    }

    pub(super) fn addressed_target(&mut self, target: Target, ty: TypeId) -> References {
        self.contents
            .entry(target.clone())
            .or_default()
            .storage_type
            .get_or_insert(ty);

        References::one(target)
    }

    pub(in crate::analysis::storage_flow) fn value_targets(
        &self,
        location: &Target,
    ) -> Option<Vec<Target>> {
        let references = self.values.get(location)?;

        (!references.unknown && !references.displaced)
            .then(|| references.targets.iter().cloned().collect())
    }

    pub(in crate::analysis::storage_flow) fn borrow_targets(
        &self,
        capability: BorrowCapabilityId,
    ) -> Option<Vec<Target>> {
        let references = self.borrows.get(&capability)?;

        (!references.unknown && !references.displaced)
            .then(|| references.targets.iter().cloned().collect())
    }

    pub(in crate::analysis::storage_flow) fn entry_reference_at(
        &mut self,
        reference: Place,
        location: Target,
        ty: Option<TypeId>,
    ) {
        let target = Target {
            origin: Origin::entry(reference),
            previous: false,
        };

        self.contents
            .entry(target.clone())
            .or_default()
            .storage_type = ty;

        self.values.insert(location, References::one(target));
    }

    pub(in crate::analysis::storage_flow) fn has_value(&self, target: &Target) -> bool {
        self.values
            .range(target.clone()..)
            .next()
            .is_some_and(|(candidate, _)| target.contains(candidate))
            || self
                .contents
                .range(target.clone()..)
                .take_while(|(candidate, _)| target.contains(candidate))
                .any(|(candidate, _)| {
                    candidate.projections()[target.projections().len()..]
                        .contains(&StorageProjection::UninitPayload)
                })
    }

    pub(in crate::analysis::storage_flow) fn begin_entry_borrow(
        &mut self,
        capability: BorrowCapabilityId,
        reference: Place,
        ty: TypeId,
    ) {
        let target = Target {
            origin: Origin::entry(reference.clone()),
            previous: false,
        };

        self.contents
            .entry(target.clone())
            .or_default()
            .storage_type = Some(ty);

        self.values
            .insert(Target::storage(reference), References::one(target.clone()));

        self.borrows.insert(capability, References::one(target));
    }

    pub(in crate::analysis::storage_flow) fn borrow_target(
        &self,
        capability: BorrowCapabilityId,
        ty: TypeId,
    ) -> Option<Target> {
        self.typed_target(self.borrows.get(&capability)?, ty)
    }

    pub(in crate::analysis::storage_flow) fn value_target(
        &self,
        location: &Target,
        ty: TypeId,
    ) -> Option<Target> {
        self.typed_target(self.values.get(location)?, ty)
    }

    fn typed_target(&self, references: &References, ty: TypeId) -> Option<Target> {
        if !references.is_exact() {
            return None;
        }

        let target = references
            .targets
            .first()
            .expect("exact reference has one target");

        (self.contents.get(target)?.storage_type == Some(ty)).then(|| target.clone())
    }

    pub(in crate::analysis::storage_flow) fn borrow_target_storage(
        &mut self,
        capability: BorrowCapabilityId,
        target: Target,
        ty: TypeId,
    ) {
        self.contents
            .entry(target.clone())
            .or_default()
            .storage_type
            .get_or_insert(ty);

        self.borrows.insert(capability, References::one(target));
    }
}
