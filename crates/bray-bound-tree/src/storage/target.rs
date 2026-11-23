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
        /// The declared reference category at this exact incoming slot.
        kind: bray_symbols::DependencyReferenceKind,
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
    /// Possible storage in separately owned backing with no reached address.
    BackingRegion,
    /// An external selected static or declaration-wide footprint.
    Static {
        /// The exact selection or declaration region and its ownership domain.
        root: bray_symbols::DependencySubjectRoot,
        /// The checked specialization, available for selected storage.
        selection: Option<std::sync::Arc<bray_symbols::StaticReferenceSelection>>,
        /// Components reached within the static.
        projections: std::sync::Arc<[crate::StorageProjection]>,
    },
    /// A value slot directly belonging to this semantic unit.
    Storage(StoragePath),
    /// An allocation created by one checked operation.
    Allocation {
        /// The checked producer slot identifying this allocation.
        allocation: StoragePath,
        /// Components within the allocation, independently of its producer slot.
        projections: std::sync::Arc<[StorageProjection]>,
    },
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
    /// Whether this origin identifies a backing region without a reached cell.
    pub const fn is_backing_region(&self) -> bool {
        matches!(self, Self::BackingRegion)
    }

    /// Creates the first referent supplied through this entry reference.
    pub fn entry(reference: StoragePath, kind: bray_symbols::DependencyReferenceKind) -> Self {
        Self::Entry {
            reference,
            layer: 1,
            kind,
            projections: std::sync::Arc::from([]),
        }
    }
}

impl StorageTarget {
    pub(crate) fn is_valid_for(&self, unit: crate::BoundUnitId) -> bool {
        match &self.origin {
            StorageOrigin::BackingRegion => true,
            StorageOrigin::Static { root, projections, selection } => {
                !self.previous
                    && root.is_static()
                    && (selection.is_some() == matches!(root, bray_symbols::DependencySubjectRoot::StaticInstance(..)))
                    && projections
                        .iter()
                        .all(|projection| projection.is_valid_for(unit))
            }
            StorageOrigin::Storage(path) => {
                path.is_valid_for(unit)
            }
            StorageOrigin::Allocation { allocation, projections } => allocation.is_valid_for(unit)
                && projections.iter().all(|projection| projection.is_valid_for(unit)),
            StorageOrigin::Entry {
                reference,
                projections,
                ..
            }
            | StorageOrigin::Returned {
                reference,
                projections,
                ..
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
                            ..
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
                            ..
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

    /// Identifies the source dependency retained for an exact reached target.
    pub fn dependency_subject(
        &self,
        values: &bray_symbols::SemanticValueStore,
        storage: &crate::StoragePlan,
    ) -> Option<crate::BoundDependencySubject> {
        if !self.is_exact() {
            return None;
        }

        if let StorageOrigin::Static { root, .. } = self.origin {
            return crate::BoundDependencySubject::from_static_root(values, root);
        }

        let path = self.inline_state_path()?;

        Some(path.access_id(storage).map_or(
            crate::BoundDependencySubject::Storage(path.root),
            crate::BoundDependencySubject::StorageAccess,
        ))
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
                StorageOrigin::BackingRegion => false,
                StorageOrigin::Storage(place) => {
                    place.is_exact()
                }
                StorageOrigin::Allocation { allocation, projections } => allocation.is_exact()
                    && projections.iter().all(|projection| projection.is_exact()),
                StorageOrigin::Static { root, projections, .. } => matches!(root, bray_symbols::DependencySubjectRoot::StaticInstance(..))
                    && projections.iter().all(|projection| projection.is_exact()),
            }
    }

    /// Returns a direct slot or first incoming-reference layer structurally.
    /// Interpreting an entry as a logical value also requires its declared storage type.
    /// A raw pointer's pointee is distinct from the pointer descriptor.
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
                            ..
            } if reference.projections.is_empty() => Some(StoragePath {
                root: reference.root,
                projections: projections.clone(),
            }),
            _ => None,
        }
    }

    /// Returns the logical value observed through an incoming borrow.
    /// Nested entry snapshots cannot be rebound through a descriptor's current slot.
    /// Raw addresses retain their distinct pointee identity.
    pub fn value_observation_path(&self) -> Option<StoragePath> {
        if self.previous || self.projections().contains(&StorageProjection::OwnedTarget) {
            return None;
        }

        match &self.origin {
            StorageOrigin::Storage(path) => Some(path.clone()),
            StorageOrigin::Entry { reference, layer: 1, kind: bray_symbols::DependencyReferenceKind::Borrow, projections }
                if reference.projections.is_empty() => Some(StoragePath {
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
            | StorageOrigin::Returned { projections, .. }
            | StorageOrigin::Static { projections, .. }
            | StorageOrigin::Allocation { projections, .. } => projections,
            StorageOrigin::Storage(place) => &place.projections,
            StorageOrigin::BackingRegion => &[],
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
            StorageOrigin::BackingRegion => return None,
            StorageOrigin::Returned {
                projections: path, ..
            } | StorageOrigin::Allocation { projections: path, .. } => *path = projections[..boundary.unwrap_or(0)].into(),
            StorageOrigin::Entry {
                projections: path, ..
            }
            | StorageOrigin::Static {
                projections: path, ..
            } => *path = projections[..boundary?].into(),
            StorageOrigin::Storage(place) => place.projections = projections[..boundary?].into(),
        }

        Some(region)
    }

    /// Compares referents without equating independent entry or returned references.
    pub fn relationship(&self, other: &Self) -> crate::StorageRelationship {
        self.relationship_in_slots(other, &[])
    }

    fn structural_relationship(&self, other: &Self) -> crate::StorageRelationship {
        use crate::StorageRelationship;

        if self.origin.is_backing_region() || other.origin.is_backing_region() {
            return StorageRelationship::PotentiallyOverlapping;
        }

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
                                    ..
                },
                StorageOrigin::Entry {
                    reference: right,
                    layer: right_layer,
                    projections: right_path,
                                    ..
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
            (StorageOrigin::Entry { .. }, StorageOrigin::Allocation { .. })
            | (StorageOrigin::Allocation { .. }, StorageOrigin::Entry { .. }) => {
                // Entry referents predate allocations created by this unit. Later
                // stores retain explicit aliases to the allocation's own identity.
                StorageRelationship::Disjoint
            }
            (StorageOrigin::Entry { .. }, _) | (_, StorageOrigin::Entry { .. }) => {
                StorageRelationship::PotentiallyOverlapping
            }
            (
                StorageOrigin::Static {
                    root: left,
                    projections: left_path,
                    ..
                },
                StorageOrigin::Static {
                    root: right,
                    projections: right_path,
                    ..
                },
            ) if left == right => region_relationship(left_path, right_path),
            (StorageOrigin::Allocation { allocation: left, projections: left_path },
                StorageOrigin::Allocation { allocation: right, projections: right_path }) => {
                if left == right { region_relationship(left_path, right_path) }
                else { StorageRelationship::Disjoint }
            }
            (StorageOrigin::Storage(left), StorageOrigin::Storage(right)) => {
                if left.root != right.root {
                    StorageRelationship::Disjoint
                } else {
                    region_relationship(&left.projections, &right.projections)
                }
            }
            _ => StorageRelationship::Disjoint,
        }
    }

    /// Compares targets with the unit's selected static slots in scope.
    pub fn relationship_in_slots(&self, other: &Self, slots: &[crate::StorageIdentity]) -> crate::StorageRelationship {
        use crate::StorageRelationship;

        let ordinary = self.structural_relationship(other);

        if ordinary != StorageRelationship::Disjoint || self.previous || other.previous {
            return ordinary;
        }

        let left = self.static_selection(slots);
        let right = other.static_selection(slots);

        let overlapping = match (left, right) {
            (Some(left), Some(right)) => left.may_share_instance(right),
            (Some(left), None) => other.bounds_static(left),
            (None, Some(right)) => self.bounds_static(right),
            (None, None) => false,
        };

        if overlapping && region_relationship(self.projections(), other.projections()) != StorageRelationship::Disjoint {
            StorageRelationship::PotentiallyOverlapping
        } else {
            StorageRelationship::Disjoint
        }
    }

    fn static_selection<'a>(&'a self, slots: &'a [crate::StorageIdentity]) -> Option<&'a bray_symbols::StaticReferenceSelection> {
        match &self.origin {
            StorageOrigin::Static { selection, .. } => selection.as_deref(),
            StorageOrigin::Storage(path) => {
                let crate::StorageIdentity::Static(selection) = slots.get(usize::try_from(path.root.ordinal()).ok()?)? else { return None; };

                Some(selection.as_ref())
            }
            _ => None,
        }
    }

    fn bounds_static(&self, selection: &bray_symbols::StaticReferenceSelection) -> bool {
        match &self.origin {
            StorageOrigin::Static { root: bray_symbols::DependencySubjectRoot::ProductStatic(declaration)
                | bray_symbols::DependencySubjectRoot::ExactThreadStatic(declaration), .. } =>
                selection.template().declaration() == *declaration,
            _ => false,
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
                                    ..
                },
                StorageOrigin::Entry {
                    reference: right,
                    layer: right_layer,
                    projections,
                                    ..
                },
            ) => left == right && left_layer == right_layer && projections.starts_with(prefix),
            (
                StorageOrigin::Returned { reference: left, projections: prefix },
                StorageOrigin::Returned { reference: right, projections },
            ) => left == right && projections.starts_with(prefix),
            (StorageOrigin::BackingRegion, StorageOrigin::BackingRegion) => true,
            (StorageOrigin::Allocation { allocation: left, projections: prefix },
                StorageOrigin::Allocation { allocation: right, projections }) => left == right && projections.starts_with(prefix),
            (StorageOrigin::Storage(left), StorageOrigin::Storage(right)) => {
                left.contains(right)
            }
            (
                StorageOrigin::Static {
                    root: left,
                    projections: prefix,
                    ..
                },
                StorageOrigin::Static {
                    root: right,
                    projections,
                    ..
                },
            ) => left == right && projections.starts_with(prefix),
            _ => false,
        }
    }

    /// Whether this target contains physical storage in the same allocation and epoch.
    pub fn contains_storage(&self, other: &Self) -> bool {
        self.contains(other)
            && StorageProjection::remains_in_allocation(&other.projections()[self.projections().len()..])
    }

    /// The same origin and epoch at an enclosing projection prefix.
    pub fn prefix(&self, length: usize) -> Self {
        let mut prefix = self.clone();

        match &mut prefix.origin {
            StorageOrigin::BackingRegion => {},
            StorageOrigin::Storage(path) => path.projections = path.projections[..length].into(),
            StorageOrigin::Entry { projections, .. } | StorageOrigin::Returned { projections, .. }
            | StorageOrigin::Allocation { projections, .. } | StorageOrigin::Static { projections, .. } => {
                *projections = projections[..length].into();
            }
        }

        prefix
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
                kind,
                projections: prefix,
            } => StorageOrigin::Entry {
                // The original reference identity survives later assignments to its source slot.
                reference: reference.clone(),
                layer: *layer,
                kind: *kind,
                projections: append(prefix),
            },
            StorageOrigin::Returned {
                reference,
                projections: prefix,
            } => StorageOrigin::Returned {
                reference: reference.clone(),
                projections: append(prefix),
            },
            StorageOrigin::BackingRegion => StorageOrigin::BackingRegion,
            StorageOrigin::Storage(place) => StorageOrigin::Storage(path(place)),
            StorageOrigin::Allocation { allocation, projections: prefix } => StorageOrigin::Allocation {
                allocation: allocation.clone(), projections: append(prefix),
            },
            StorageOrigin::Static {
                root,
                projections: prefix,
                selection,
            } => StorageOrigin::Static {
                root: *root,
                projections: append(prefix),
                selection: selection.clone(),
            },
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
    fn entry_reference_kind_separates_values_from_raw_addresses() {
        let reference = slot(0);
        let component = StorageProjection::TupleElement(SymbolOrdinal::new(1));

        let borrowed = StorageTarget {
            previous: false,
            origin: StorageOrigin::entry(reference.clone(), bray_symbols::DependencyReferenceKind::Borrow),
        }.project(&[component]);

        let address = StorageTarget {
            previous: false,
            origin: StorageOrigin::entry(reference.clone(), bray_symbols::DependencyReferenceKind::Address),
        }.project(&[component]);

        assert_eq!(borrowed.value_observation_path(), Some(StoragePath {
            root: reference.root,
            projections: [component].into(),
        }));

        assert!(address.value_observation_path().is_none());
        assert_eq!(borrowed.relationship(&address), StorageRelationship::Identical);
        assert!(StorageTarget { previous: true, ..borrowed }.value_observation_path().is_none());

        let nested = StorageTarget {
            previous: false,
            origin: StorageOrigin::entry(StoragePath {
                root: reference.root,
                projections: [StorageProjection::NullableValue].into(),
            }, bray_symbols::DependencyReferenceKind::Borrow),
        };

        // The descriptor's current slot can be overwritten after entry. It
        // cannot stand in for the captured referent of this incoming borrow.
        assert!(nested.value_observation_path().is_none());
    }

    #[test]
    fn independent_reference_slots_do_not_prove_disjoint_referents() {
        let left = StorageTarget {
            previous: false,
            origin: StorageOrigin::entry(slot(0), bray_symbols::DependencyReferenceKind::Address),
        };

        let right = StorageTarget {
            previous: false,
            origin: StorageOrigin::entry(slot(1), bray_symbols::DependencyReferenceKind::Address),
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
            origin: StorageOrigin::entry(slot(0), bray_symbols::DependencyReferenceKind::Address),
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
                kind: bray_symbols::DependencyReferenceKind::Address,
                projections: [].into(),
            },
        };

        assert!(current.contains(&projected));
        assert!(projected.is_exact());

        assert_eq!(
            projected
                .observation_path()
                .expect("fixture `observation_path` call must succeed")
                .projections
                .as_ref(),
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
    fn static_declaration_footprints_do_not_supply_selected_addresses() {
        use bray_symbols::{DependencySubjectRoot, StaticSymbolId, SymbolId};

        let declaration = StaticSymbolId::from_symbol_id(SymbolId::new(1));

        let root = StorageTarget {
            previous: false,
            origin: StorageOrigin::Static {
                root: DependencySubjectRoot::ProductStatic(declaration),
                selection: None,
                projections: [].into(),
            },
        };

        let thread = StorageTarget {
            origin: StorageOrigin::Static {
                root: DependencySubjectRoot::ExactThreadStatic(declaration),
                selection: None,
                projections: [].into(),
            },
            ..root.clone()
        };

        let field = root.project(&[StorageProjection::TupleElement(SymbolOrdinal::new(0))]);
        let payload = root.project(&[StorageProjection::OwnedTarget]);

        assert!(root.is_valid_for(BoundUnitId::new(1)));
        assert!(!root.is_exact());
        assert!(crate::StorageMutationTarget::location(root.clone()).address.is_none());
        assert!(root.observation_path().is_none());
        assert!(root.contains(&field));
        assert_eq!(payload.allocation_region(), Some(payload.clone()));
        assert_eq!(root.relationship(&thread), StorageRelationship::Disjoint);

        assert_eq!(
            root.relationship(&StorageTarget::storage(slot(0))),
            StorageRelationship::Disjoint
        );

        assert_eq!(root.relationship(&payload), StorageRelationship::Disjoint);

        let borrowed = StorageTarget {
            previous: false,
            origin: StorageOrigin::entry(slot(0), bray_symbols::DependencyReferenceKind::Address),
        };

        assert_eq!(
            root.relationship(&borrowed),
            StorageRelationship::PotentiallyOverlapping
        );
    }

    #[test]
    fn allocation_regions_preserve_owned_boundaries_for_every_origin() {
        let path = slot(0);

        let origins = [
            StorageOrigin::Storage(path.clone()),
            StorageOrigin::Allocation { allocation: path.clone(), projections: [].into() },
            StorageOrigin::entry(path.clone(), bray_symbols::DependencyReferenceKind::Address),
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

                let region = payload
                    .allocation_region()
                    .expect("fixture `allocation_region` call must succeed");

                assert_eq!(region, header.project(&[StorageProjection::OwnedTarget]));
                assert_eq!(region.previous, previous);
                assert!(header.contains(&payload));
                assert!(!header.contains_storage(&payload));
                assert!(region.contains_storage(&payload));
                assert!(header.contains_storage(&header.project(&[StorageProjection::UninitPayload])));
                assert!(!header.contains_storage(&StorageTarget { previous: !previous, ..header.clone() }));

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
