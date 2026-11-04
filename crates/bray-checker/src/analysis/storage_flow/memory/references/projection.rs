use super::super::model::{Origin, Place, References, State, Target};
use bray_bound_tree::{BorrowCapabilityId, StorageIdentityId, StorageProjection};
use bray_symbols::TypeId;

impl State {
    pub(in crate::analysis::storage_flow) fn for_storage(
        storage: &bray_bound_tree::StoragePlan,
        cleanup_free: &std::collections::BTreeSet<bray_bound_tree::StorageIdentityId>,
    ) -> Self {
        let borrowed = storage
            .borrow_capabilities()
            .iter()
            .flat_map(|borrow| storage.retained_roots(borrow.access()))
            .collect::<std::collections::BTreeSet<_>>();

        let entry_borrows = storage.borrow_capabilities().iter().filter(|borrow| {
            borrow.entry_binding().is_some() && borrow.parent().is_none()
        }).filter_map(|borrow| {
            let path = storage.logical_path(borrow.access())?;

            path.projections.is_empty().then_some((path.root, borrow.kind()))
        }).collect::<std::collections::BTreeMap<_, _>>();

        let owned_storage = storage
            .identity_entries()
            .filter_map(|(id, identity)| identity.is_invocation_owned(storage.kind()).then_some(id))
            .collect::<std::sync::Arc<[_]>>();

        let unaddressed_storage = owned_storage
            .iter()
            .copied()
            .filter(|identity| cleanup_free.contains(identity) && !borrowed.contains(identity))
            .collect();

        Self {
            entry_borrows: std::sync::Arc::new(entry_borrows),
            owned_storage,
            unaddressed_storage,
            ..Self::default()
        }
    }

    pub(in crate::analysis::storage_flow) fn unknown_reference_may_reach(
        &self,
        target: &Target,
    ) -> bool {
        // Even an unknown pointer cannot reach invocation-local inline storage whose
        // address is never formed, including implicit lifecycle receivers. Heap
        // payloads can have aliases from before entry.
        target.storage_path().is_none_or(|path| {
            self.unaddressed_storage.binary_search(&path.root).is_err()
                || path.projections.contains(&StorageProjection::OwnedTarget)
        })
    }

    pub(super) fn is_owned_inline_storage(&self, path: &Place) -> bool {
        self.owned_storage.binary_search(&path.root).is_ok()
            && !path.projections.contains(&StorageProjection::OwnedTarget)
    }

    pub(super) fn returned_reference_bounds(&self, reference: &Place) -> &References {
        self.returned_inputs.get(reference).unwrap_or_else(|| {
            panic!("returned reference {reference:?} has no complete input-region bound")
        })
    }

    pub(super) fn reachable_inline_roots(
        &self,
        target: &Target,
    ) -> impl Iterator<Item = StorageIdentityId> + '_ {
        let (storage, returned) = match &target.origin {
            Origin::Storage(path) => (Some(path.root), None),
            Origin::Returned { reference, .. } => {
                (None, Some(self.returned_reference_bounds(reference)))
            }
            Origin::Entry { .. } | Origin::Allocation(_) => (None, None),
        };

        storage.into_iter().chain(
            returned
                .into_iter()
                .flat_map(|bounds| &bounds.targets)
                .filter_map(|target| target.storage_path().map(|path| path.root)),
        )
    }

    pub(in crate::analysis::storage_flow) fn effect_origins<'a>(
        &'a self,
        target: &'a Target,
    ) -> impl Iterator<Item = &'a Target> {
        let returned = match &target.origin {
            Origin::Returned { reference, .. } => Some(self.returned_reference_bounds(reference)),
            _ => None,
        };

        std::iter::once(target)
            .filter(|target| !matches!(target.origin, Origin::Returned { .. }))
            .chain(returned.into_iter().flat_map(|bounds| &bounds.targets))
    }

    pub(in crate::analysis::storage_flow) fn target_relationship(
        &self,
        left: &Target,
        right: &Target,
    ) -> bray_bound_tree::StorageRelationship {
        if !left.previous && !right.previous
            && let (
                Origin::Entry { reference: left, layer: 1, projections: left_path },
                Origin::Entry { reference: right, layer: 1, projections: right_path },
            ) = (&left.origin, &right.origin)
            && left.root != right.root
            && left.projections.is_empty() && right.projections.is_empty()
            && Place::inline_projections(left_path) && Place::inline_projections(right_path)
            && let (Some(left), Some(right)) = (self.entry_borrows.get(&left.root), self.entry_borrows.get(&right.root))
            && (*left == bray_symbols::BorrowKind::Mutable || *right == bray_symbols::BorrowKind::Mutable)
        {
            // Incoming loans already establish exclusivity. This bounds only their
            // immediate referents, never raw pointers or references stored inside them.
            return bray_bound_tree::StorageRelationship::Disjoint;
        }

        let returned_slot = match (&left.origin, &right.origin) {
            (Origin::Returned { reference, .. }, Origin::Storage(slot))
            | (Origin::Storage(slot), Origin::Returned { reference, .. }) => {
                Some((reference, slot))
            }
            _ => None,
        };

        if let Some((reference, slot)) = returned_slot
            && self.is_owned_inline_storage(slot)
            && !self
                .contents
                .get(&Target::storage(Place::root(slot.root)))
                .is_some_and(|contents| contents.escaped)
            && !self
                .returned_reference_bounds(reference)
                .targets
                .iter()
                .any(|target| {
                    target.storage_path().is_some_and(|path| {
                        path.root == slot.root && self.is_owned_inline_storage(path)
                    })
                })
        {
            return bray_bound_tree::StorageRelationship::Disjoint;
        }

        let slot = match (&left.origin, &right.origin) {
            (Origin::Entry { .. }, Origin::Storage(slot))
            | (Origin::Storage(slot), Origin::Entry { .. }) => Some(slot),
            _ => None,
        };

        if slot.is_some_and(|slot| self.is_owned_inline_storage(slot)) {
            // An original input cannot address this invocation's inline slots. Owned
            // heap storage can predate the invocation and already have incoming aliases.
            return bray_bound_tree::StorageRelationship::Disjoint;
        }

        left.relationship(right)
    }

    pub(super) fn addressed_target(
        &mut self,
        target: Target,
        ty: TypeId,
        kind: bray_symbols::DependencyReferenceKind,
    ) -> References {
        let contents = self.contents.entry(target.clone()).or_default();

        contents.storage_type.get_or_insert(ty);
        contents.exposed |= kind == bray_symbols::DependencyReferenceKind::Address;

        References::one(target)
    }

    pub(in crate::analysis::storage_flow) fn reference_location(
        &self,
        mut target: Target,
        path: &bray_bound_tree::StorageReferencePath,
        mut root_borrow: Option<Target>,
    ) -> Option<Target> {
        let mut copied = 0;

        for (step, (index, ty)) in path.borrows().iter().enumerate() {
            target = target.project(&path.projections()[copied..*index]);

            target = match root_borrow.take().filter(|_| step == 0 && *index == 0) {
                Some(root) => root,
                None => self.value_target(&target, *ty)?,
            };

            copied = *index;
        }

        Some(target.project(&path.projections()[copied..]))
    }

    pub(in crate::analysis::storage_flow) fn referenced_regions(
        &self,
        location: &Target,
    ) -> Option<Vec<Target>> {
        self.values.get(location)?.regions()
    }

    pub(in crate::analysis::storage_flow) fn value_targets(
        &self,
        location: &Target,
    ) -> Option<Vec<Target>> {
        let references = self.values.get(location)?;

        (!references.unknown
            && !references.displaced
            && !references.inexact
            && references.targets.iter().all(Target::is_exact))
        .then(|| references.targets.iter().cloned().collect())
    }

    pub(in crate::analysis::storage_flow) fn borrow_regions(
        &self,
        capability: BorrowCapabilityId,
    ) -> Option<Vec<Target>> {
        self.borrows.get(&capability)?.regions()
    }

    pub(in crate::analysis::storage_flow) fn borrow_targets(
        &self,
        capability: BorrowCapabilityId,
    ) -> Option<Vec<Target>> {
        let references = self.borrows.get(&capability)?;

        (!references.unknown
            && !references.displaced
            && !references.inexact
            && references.targets.iter().all(Target::is_exact))
        .then(|| references.targets.iter().cloned().collect())
    }

    pub(in crate::analysis::storage_flow) fn borrow_observation(
        &self,
        capability: BorrowCapabilityId,
    ) -> Option<Place> {
        let references = self.borrows.get(&capability)?;

        if !references.is_exact() {
            return None;
        }

        references
            .targets
            .first()
            .expect("exact borrow has one target")
            .observation_path()
    }

    pub(in crate::analysis::storage_flow) fn entry_reference_at(
        &mut self,
        reference: Place,
        layer: usize,
        location: Target,
        ty: Option<TypeId>,
    ) {
        let target = Target {
            origin: Origin::Entry {
                reference,
                layer,
                projections: std::sync::Arc::from([]),
            },
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

impl References {
    pub(super) fn regions(&self) -> Option<Vec<Target>> {
        if self.unknown {
            return None;
        }

        self.targets
            .iter()
            .map(|target| {
                // Closed allocations bound displaced pointers. Ordinary address arithmetic
                // has no such region proof and cannot narrow an unknown mutation.
                if self.displaced {
                    target.allocation_region()
                } else {
                    Some(target.clone())
                }
            })
            .collect()
    }
}
