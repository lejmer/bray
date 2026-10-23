use crate::{StoragePath, StorageProjection};

/// The established origin of a storage referent, distinct from a slot holding its address.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageOrigin {
    /// A referent supplied at entry through an exact reference slot.
    Entry {
        /// The slot whose entry value supplies this reference.
        reference: StoragePath,
        /// Successive borrow layers identify distinct entry snapshots at the same path.
        layer: usize,
        /// Components reached within that original referent.
        projections: std::sync::Arc<[crate::StorageProjection]>,
    },
    /// A returned referent whose identity is independent of its lifetime anchor.
    Returned {
        /// The slot that received the reference.
        reference: StoragePath,
        /// Components reached within the returned referent.
        projections: std::sync::Arc<[crate::StorageProjection]>,
    },
    /// A value slot directly belonging to this semantic unit.
    Storage(StoragePath),
    /// An allocation created by one checked operation.
    Allocation(StoragePath),
}

/// `previous` summarizes older executions of one creation site. It is never a must-alias.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageTarget {
    /// Earlier executions of this creation site cannot establish current exact identity.
    /// Epoch precedes the path in ordering so ranges cannot cross an older cell.
    pub previous: bool,
    /// The storage referent established by the owning analysis.
    pub origin: StorageOrigin,
}

impl StorageOrigin {
    /// Creates the first referent supplied through this entry reference.
    pub fn entry(reference: StoragePath) -> Self {
        Self::Entry {
            reference,
            layer: 1,
            projections: std::sync::Arc::from([]),
        }
    }
}

impl StorageTarget {
    pub(crate) fn is_valid_for(&self, unit: crate::BoundUnitId) -> bool {
        match &self.origin {
            StorageOrigin::Storage(path) | StorageOrigin::Allocation(path) => {
                path.is_valid_for(unit)
            }
            StorageOrigin::Entry {
                reference,
                projections,
                ..
            }
            | StorageOrigin::Returned {
                reference,
                projections,
            } => {
                reference.is_valid_for(unit)
                    && projections
                        .iter()
                        .all(|projection| projection.is_valid_for(unit))
            }
        }
    }

    /// Classifies the checked write boundary without re-resolving aliases.
    pub fn mutation_scope(
        &self,
        storage: &crate::StoragePlan,
        values: &bray_symbols::SemanticValueStore,
    ) -> crate::StorageMutationScope {
        use crate::StorageMutationScope;

        if self.previous {
            return StorageMutationScope::Unbounded;
        }

        match &self.origin {
            StorageOrigin::Storage(path)
                if path.is_inline()
                    && storage
                        .identity(path.root)
                        .is_some_and(|identity| identity.is_invocation_owned(storage.kind())) =>
            {
                StorageMutationScope::Invocation
            }
            StorageOrigin::Entry {
                reference,
                layer: 1,
                projections,
            } if reference.projections.is_empty()
                && StoragePath::inline_projections(projections)
                && matches!(
                    storage.identity(reference.root),
                    Some(
                        crate::StorageIdentity::Parameter(_)
                            | crate::StorageIdentity::Receiver(_)
                            | crate::StorageIdentity::AnonymousParameter(_)
                    )
                )
                && storage.storage_type(reference.root).is_some_and(|ty| {
                    matches!(
                        values.type_data(ty).as_ref(),
                        bray_symbols::TypeData::Borrow {
                            kind: bray_symbols::BorrowKind::Mutable,
                            ..
                        }
                    )
                }) =>
            {
                StorageMutationScope::Inputs
            }
            _ => StorageMutationScope::Unbounded,
        }
    }

    /// Returns the inline value path represented by an invocation's immutable state frame.
    /// The caller establishes confinement before using an entry referent as a frame.
    pub fn inline_state_path(&self) -> Option<StoragePath> {
        if self.previous {
            return None;
        }

        match &self.origin {
            StorageOrigin::Storage(path) if path.is_inline() => Some(path.clone()),
            StorageOrigin::Entry {
                reference,
                layer: 1,
                projections,
            } if reference.projections.is_empty()
                && StoragePath::inline_projections(projections) =>
            {
                Some(StoragePath {
                    root: reference.root,
                    projections: projections.clone(),
                })
            }
            _ => None,
        }
    }

    /// Identifies one current value slot.
    pub fn storage(place: StoragePath) -> Self {
        Self {
            origin: StorageOrigin::Storage(place),
            previous: false,
        }
    }

    /// Returns a current direct slot, excluding references and older executions.
    pub fn storage_path(&self) -> Option<&StoragePath> {
        match &self.origin {
            StorageOrigin::Storage(place) if !self.previous => Some(place),
            _ => None,
        }
    }
    /// Whether this target identifies one current referent with exact projections.
    pub fn is_exact(&self) -> bool {
        !self.previous
            && match &self.origin {
                StorageOrigin::Entry {
                    reference,
                    projections,
                    ..
                } => {
                    reference.is_exact()
                        && projections.iter().all(|projection| projection.is_exact())
                }
                StorageOrigin::Returned {
                    reference,
                    projections,
                } => {
                    reference.is_exact()
                        && projections.iter().all(|projection| projection.is_exact())
                }
                StorageOrigin::Storage(place) | StorageOrigin::Allocation(place) => {
                    place.is_exact()
                }
            }
    }

    /// Returns the value observation supplied by a direct slot or one entry borrow layer.
    pub fn observation_path(&self) -> Option<StoragePath> {
        if self.previous || self.projections().contains(&StorageProjection::OwnedTarget) {
            return None;
        }

        match &self.origin {
            StorageOrigin::Storage(place) => Some(place.clone()),
            StorageOrigin::Entry {
                reference,
                projections,
                layer: 1,
            } if reference.projections.is_empty() => Some(StoragePath {
                root: reference.root,
                projections: projections.clone(),
            }),
            _ => None,
        }
    }

    /// Returns components within the established referent.
    pub fn projections(&self) -> &[StorageProjection] {
        match &self.origin {
            StorageOrigin::Entry { projections, .. }
            | StorageOrigin::Returned { projections, .. } => projections,
            StorageOrigin::Storage(place) | StorageOrigin::Allocation(place) => &place.projections,
        }
    }

    /// Returns the enclosing allocation region when its boundary is established.
    pub fn allocation_region(&self) -> Option<Self> {
        let projections = self.projections();

        let boundary = projections
            .iter()
            .rposition(|projection| *projection == StorageProjection::OwnedTarget)
            .map(|index| index + 1);

        let mut region = self.clone();

        match &mut region.origin {
            StorageOrigin::Returned {
                projections: path, ..
            } => *path = projections[..boundary.unwrap_or(0)].into(),
            StorageOrigin::Allocation(place) => {
                place.projections = projections[..boundary.unwrap_or(0)].into()
            }
            StorageOrigin::Entry {
                projections: path, ..
            } => *path = projections[..boundary?].into(),
            StorageOrigin::Storage(place) => place.projections = projections[..boundary?].into(),
        }

        Some(region)
    }

    /// Compares referents without equating independent entry or returned references.
    pub fn relationship(&self, other: &Self) -> crate::StorageRelationship {
        use crate::StorageRelationship;

        if self.previous || other.previous {
            if matches!(self.origin, StorageOrigin::Returned { .. })
                || matches!(other.origin, StorageOrigin::Returned { .. })
            {
                // Repeated calls can return the same object. Older evaluations lose
                // exact identity, but never establish disjoint allocations.
                return StorageRelationship::PotentiallyOverlapping;
            }

            if self.previous != other.previous {
                return StorageRelationship::Disjoint;
            }
        }

        match (&self.origin, &other.origin) {
            (
                StorageOrigin::Entry {
                    reference: left,
                    layer: left_layer,
                    projections: left_path,
                },
                StorageOrigin::Entry {
                    reference: right,
                    layer: right_layer,
                    projections: right_path,
                },
            ) if left == right && left_layer == right_layer => {
                region_relationship(left_path, right_path)
            }
            (
                StorageOrigin::Returned {
                    reference: left,
                    projections: left_path,
                },
                StorageOrigin::Returned {
                    reference: right,
                    projections: right_path,
                },
            ) if left == right => region_relationship(left_path, right_path),
            (StorageOrigin::Returned { .. }, _) | (_, StorageOrigin::Returned { .. }) => {
                StorageRelationship::PotentiallyOverlapping
            }
            (StorageOrigin::Entry { .. }, _) | (_, StorageOrigin::Entry { .. }) => {
                StorageRelationship::PotentiallyOverlapping
            }
            (StorageOrigin::Storage(left), StorageOrigin::Storage(right))
            | (StorageOrigin::Allocation(left), StorageOrigin::Allocation(right)) => {
                if left.root != right.root {
                    StorageRelationship::Disjoint
                } else {
                    region_relationship(&left.projections, &right.projections)
                }
            }
            _ => StorageRelationship::Disjoint,
        }
    }

    /// Whether this target contains the other target in the same origin and epoch.
    pub fn contains(&self, other: &Self) -> bool {
        if self.previous != other.previous {
            return false;
        }

        match (&self.origin, &other.origin) {
            (
                StorageOrigin::Entry {
                    reference: left,
                    layer: left_layer,
                    projections: prefix,
                },
                StorageOrigin::Entry {
                    reference: right,
                    layer: right_layer,
                    projections,
                },
            ) => left == right && left_layer == right_layer && projections.starts_with(prefix),
            (
                StorageOrigin::Returned {
                    reference: left,
                    projections: prefix,
                },
                StorageOrigin::Returned {
                    reference: right,
                    projections,
                },
            ) => left == right && projections.starts_with(prefix),
            (StorageOrigin::Storage(left), StorageOrigin::Storage(right))
            | (StorageOrigin::Allocation(left), StorageOrigin::Allocation(right)) => {
                left.contains(right)
            }
            _ => false,
        }
    }

    /// Derives components while preserving the original reference identity and epoch.
    pub fn project(&self, projections: &[StorageProjection]) -> Self {
        let append =
            |prefix: &[StorageProjection]| prefix.iter().chain(projections).copied().collect();

        let path = |place: &StoragePath| StoragePath {
            root: place.root,
            projections: append(&place.projections),
        };

        let origin = match &self.origin {
            StorageOrigin::Entry {
                reference,
                layer,
                projections: prefix,
            } => StorageOrigin::Entry {
                // The original reference identity survives later assignments to its source slot.
                reference: reference.clone(),
                layer: *layer,
                projections: append(prefix),
            },
            StorageOrigin::Returned {
                reference,
                projections: prefix,
            } => StorageOrigin::Returned {
                reference: reference.clone(),
                projections: append(prefix),
            },
            StorageOrigin::Storage(place) => StorageOrigin::Storage(path(place)),
            StorageOrigin::Allocation(place) => StorageOrigin::Allocation(path(place)),
        };

        Self {
            origin,
            previous: self.previous,
        }
    }
}

fn region_relationship(
    left: &[StorageProjection],
    right: &[StorageProjection],
) -> crate::StorageRelationship {
    // An owned target is another allocation. Its bytes do not overlap the owner header.
    let boundary = |path: &[StorageProjection]| {
        path.iter()
            .rposition(|projection| *projection == StorageProjection::OwnedTarget)
    };

    if boundary(left) != boundary(right) {
        return crate::StorageRelationship::Disjoint;
    }

    StorageProjection::relationship(left, right)
}

#[cfg(test)]
mod tests {
    use super::{StorageOrigin, StorageTarget};
    use crate::{
        BoundUnitId, StorageIdentityId, StoragePath, StorageProjection, StorageRelationship,
    };
    use bray_symbols::SymbolOrdinal;

    fn slot(index: u32) -> StoragePath {
        StoragePath::root(StorageIdentityId::from_slot(BoundUnitId::new(1), index))
    }

    #[test]
    fn independent_reference_slots_do_not_prove_disjoint_referents() {
        let left = StorageTarget {
            previous: false,
            origin: StorageOrigin::entry(slot(0)),
        };

        let right = StorageTarget {
            previous: false,
            origin: StorageOrigin::entry(slot(1)),
        };

        assert!(left.is_exact());
        assert!(left.storage_path().is_none());

        assert_eq!(
            left.relationship(&right),
            StorageRelationship::PotentiallyOverlapping
        );

        assert!(!left.contains(&right));
    }

    #[test]
    fn projected_entry_snapshots_keep_their_epoch_and_borrow_layer() {
        let current = StorageTarget {
            previous: false,
            origin: StorageOrigin::entry(slot(0)),
        };

        let projected = current.project(&[StorageProjection::TupleElement(SymbolOrdinal::new(1))]);

        let previous = StorageTarget {
            previous: true,
            ..current.clone()
        };

        let second_layer = StorageTarget {
            previous: false,
            origin: StorageOrigin::Entry {
                reference: slot(0),
                layer: 2,
                projections: [].into(),
            },
        };

        assert!(current.contains(&projected));
        assert!(projected.is_exact());

        assert_eq!(
            projected.observation_path().unwrap().projections.as_ref(),
            &[StorageProjection::TupleElement(SymbolOrdinal::new(1))]
        );

        assert!(!previous.is_exact());
        assert!(previous.observation_path().is_none());
        assert!(!previous.contains(&current));
        assert!(!current.contains(&second_layer));
        assert!(second_layer.observation_path().is_none());
    }

    #[test]
    fn returned_referents_are_not_their_receiving_slots() {
        let returned = StorageTarget {
            previous: false,
            origin: StorageOrigin::Returned {
                reference: slot(0),
                projections: [].into(),
            },
        };

        assert!(returned.is_exact());
        assert!(returned.storage_path().is_none());
        assert!(returned.observation_path().is_none());

        assert_eq!(
            returned.relationship(&StorageTarget::storage(slot(0))),
            StorageRelationship::PotentiallyOverlapping
        );
    }

    #[test]
    fn allocation_regions_preserve_owned_boundaries_for_every_origin() {
        let path = slot(0);

        let origins = [
            StorageOrigin::Storage(path.clone()),
            StorageOrigin::Allocation(path.clone()),
            StorageOrigin::entry(path.clone()),
            StorageOrigin::Returned {
                reference: path,
                projections: [].into(),
            },
        ];

        for origin in origins {
            for previous in [false, true] {
                let header = StorageTarget {
                    origin: origin.clone(),
                    previous,
                };

                let payload = header.project(&[
                    StorageProjection::OwnedTarget,
                    StorageProjection::TupleElement(SymbolOrdinal::new(1)),
                ]);

                let region = payload.allocation_region().unwrap();

                assert_eq!(region, header.project(&[StorageProjection::OwnedTarget]));
                assert_eq!(region.previous, previous);

                assert_eq!(
                    region.relationship(&header),
                    if previous && matches!(region.origin, StorageOrigin::Returned { .. }) {
                        StorageRelationship::PotentiallyOverlapping
                    } else {
                        StorageRelationship::Disjoint
                    }
                );
            }
        }
    }
}
