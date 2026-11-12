use super::super::model::{Origin, Place, References, State, Target};
use bray_bound_tree::{BorrowCapabilityId, StorageIdentityId, StorageProjection};
use bray_symbols::TypeId;

impl State {
    pub(in crate::analysis::storage_flow) fn for_storage(
        storage: &bray_bound_tree::StoragePlan,
        cleanup_free: &std::collections::BTreeSet<bray_bound_tree::StorageIdentityId>,
        complete_reference_types: &std::sync::Arc<std::collections::BTreeMap<TypeId, bool>>,
    ) -> Self {
        let borrowed = storage
            .borrow_capabilities()
            .iter()
            .filter(|borrow| {
                // Borrowing an entry referent or reborrowing through a capability
                // does not form an address of the slot holding its reference.
                borrow.entry_binding().is_none()
                    && storage.access(borrow.access()).is_some_and(|access| {
                        matches!(
                            access.root(),
                            bray_bound_tree::StorageAccessRoot::Storage(_)
                        )
                    })
            })
            .flat_map(|borrow| storage.retained_roots(borrow.access()))
            .collect::<std::collections::BTreeSet<_>>();

        let entry_borrows = storage
            .borrow_capabilities()
            .iter()
            .filter(|borrow| borrow.entry_binding().is_some() && borrow.parent().is_none())
            .filter_map(|borrow| {
                let path = storage.logical_path(borrow.access())?;

                path.projections
                    .is_empty()
                    .then_some((path.root, borrow.kind()))
            })
            .collect::<std::collections::BTreeMap<_, _>>();

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
            complete_reference_types: std::sync::Arc::clone(complete_reference_types),
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
        if Self::unaddressed_inline_slot(&self.unaddressed_storage, target) {
            return false;
        }

        let Some(path) = target.storage_path().filter(|path| self.is_owned_inline_storage(path)) else {
            return true;
        };

        // A language loan alone does not publish an address to ambient code.
        // Actual raw exposure or an escaping call does. Explicit call inputs are
        // reached separately, including during the call that first publishes them.
        let root = Target::storage(Place::root(path.root));

        self.contents.range(root.clone()..)
            .take_while(|(candidate, _)| root.contains(candidate))
            .any(|(_, contents)| contents.exposed || contents.escaped)
    }

    pub(super) fn unaddressed_inline_slot(
        unaddressed: &[StorageIdentityId],
        target: &Target,
    ) -> bool {
        // Even an unknown pointer cannot reach invocation-local inline storage whose
        // address is never formed, including implicit lifecycle receivers. Heap
        // payloads can have aliases from before entry.
        target.storage_path().is_some_and(|path| {
            unaddressed.binary_search(&path.root).is_ok()
                && !path.projections.contains(&StorageProjection::OwnedTarget)
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

    pub(super) fn bounded_reference_contents<C: crate::CheckerRequestContext + ?Sized>(
        &self,
        request: crate::CheckerUnitView<'_, C>,
        location: &Target,
    ) -> crate::CheckerQueryResult<Option<References>, C::UpstreamError> {
        if location.previous { return Ok(None); }

        let Origin::Returned { reference, projections } = &location.origin else { return Ok(None); };
        let bounds = self.returned_reference_bounds(reference);
        if projections.is_empty() || bounds.unknown || bounds.displaced || bounds.inexact || bounds.targets.is_empty() {
            return Ok(None);
        }

        let parent = Target { previous: false, origin: Origin::Returned { reference: reference.clone(), projections: [].into() } };
        let Some(ty) = self.declared_type(&parent) else { return Ok(None); };
        let mut contents = References::default();

        for target in &bounds.targets {
            if target.previous || !target.is_exact() || self.projected_type(request, target)? != Some(ty) {
                return Ok(None);
            }

            let Some(value) = self.values.get(&target.project(projections)) else { return Ok(None); };
            if value.unknown || value.displaced { return Ok(None); }

            contents.merge(value);
        }

        Ok(Some(contents))
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
            Origin::Entry { .. } | Origin::Allocation { .. } | Origin::Static { .. } => (None, None),
        };

        storage.into_iter().chain(
            returned
                .into_iter()
                .flat_map(|bounds| &bounds.targets)
                .filter_map(|target| target.storage_path().map(|path| path.root)),
        )
    }

    /// Physical input regions and whether an independent origin remains possible.
    pub(in crate::analysis::storage_flow) fn effect_origins<'a>(
        &'a self,
        target: &'a Target,
    ) -> (impl Iterator<Item = &'a Target>, bool) {
        let returned = match &target.origin {
            Origin::Returned { reference, .. } => Some(self.returned_reference_bounds(reference)),
            _ => None,
        };

        let unbounded = returned.is_some_and(|bounds| bounds.unknown || target.previous
            || target.projections().contains(&StorageProjection::OwnedTarget));

        let origins = std::iter::once(target)
            .filter(|target| !matches!(target.origin, Origin::Returned { .. }))
            .chain(returned.into_iter().flat_map(|bounds| &bounds.targets));

        (origins, unbounded)
    }

    pub(in crate::analysis::storage_flow) fn target_relationship(
        &self,
        left: &Target,
        right: &Target,
    ) -> bray_bound_tree::StorageRelationship {
        if !left.previous && !right.previous {
            for (returned, other) in [(left, right), (right, left)] {
                if let Origin::Returned { reference, projections } = &returned.origin
                    && !projections.contains(&StorageProjection::OwnedTarget)
                {
                    let bounds = self.returned_reference_bounds(reference);

                    // Complete producer bounds exclude unrelated storage, without
                    // equating the returned address to any one possible source.
                    // Reaching another owner requires its own physical bounds.
                    if !bounds.unknown && !bounds.targets.is_empty()
                        && bounds.targets.iter().all(|region| self.target_relationship(region, other)
                            == bray_bound_tree::StorageRelationship::Disjoint)
                    {
                        return bray_bound_tree::StorageRelationship::Disjoint;
                    }
                }
            }
        }

        if !left.previous
            && !right.previous
            && let (
                Origin::Entry {
                    reference: left,
                    layer: 1,
                    projections: left_path,
                                    ..
                },
                Origin::Entry {
                    reference: right,
                    layer: 1,
                    projections: right_path,
                                    ..
                },
            ) = (&left.origin, &right.origin)
            && left.root != right.root
            && left.projections.is_empty()
            && right.projections.is_empty()
            && let (Some(left), Some(right)) = (
                self.entry_borrows.get(&left.root),
                self.entry_borrows.get(&right.root),
            )
            && (*left == bray_symbols::BorrowKind::Mutable
                || *right == bray_symbols::BorrowKind::Mutable)
            && (*left == bray_symbols::BorrowKind::Mutable || Place::inline_projections(left_path))
            && (*right == bray_symbols::BorrowKind::Mutable
                || Place::inline_projections(right_path))
        {
            // Mutable owners exclude aliases through their owned storage. Shared
            // owners can contain interior mutable storage, so their non-inline
            // projections remain conservative. Stored pointers have separate origins.
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
        values: &bray_symbols::SemanticValueStore,
        target: Target,
        path: &bray_bound_tree::StorageReferencePath,
        root_borrow: Option<Target>,
    ) -> Option<Target> {
        self.reference_location_in(values, target, path, root_borrow, |target| self.values.get(target))
    }

    pub(in crate::analysis::storage_flow) fn staged_reference_location(
        &self,
        invocation: bray_bound_tree::BoundExecutionSite,
        values: &bray_symbols::SemanticValueStore,
        root: &Target,
        path: &bray_bound_tree::StorageReferencePath,
    ) -> Option<Target> {
        let staged = self.pending.get(&invocation)?;
        self.reference_location_in(values, root.clone(), path, None, |target| {
            if root.contains(target) { staged.values.get(target) } else { self.values.get(target) }
        })
    }

    pub(in crate::analysis::storage_flow) fn captured_input_target(
        &self,
        invocation: bray_bound_tree::BoundExecutionSite,
        values: &bray_symbols::SemanticValueStore,
        root: &Target,
        path: &bray_bound_tree::StorageReferencePath,
    ) -> Option<Target> {
        let captured = &self.pending.get(&invocation)?.inputs;
        let lookup = |target: &Target| {
            if root.contains(target) { captured.get(target) } else { self.values.get(target) }
        };
        let location = self.reference_location_in(values, root.clone(), path, None, lookup)?;

        let references = lookup(&location)?;

        references.is_exact().then(|| references.targets.first()
            .expect("exact captured input has one referent").clone())
    }

    pub(in crate::analysis::storage_flow) fn staged_reference_regions(
        &self,
        invocation: bray_bound_tree::BoundExecutionSite,
        root: &Target,
        location: &Target,
    ) -> Option<Vec<Target>> {
        if root.contains(location) {
            self.pending.get(&invocation)?.values.get(location)?.regions()
        } else {
            self.referenced_regions(location)
        }
    }

    pub(in crate::analysis::storage_flow) fn staged_contained_regions(
        &self,
        invocation: bray_bound_tree::BoundExecutionSite,
        value: &Target,
    ) -> (std::collections::BTreeSet<Target>, bool) {
        let staged = self.pending.get(&invocation).expect("captured completion references retain their prepared transfer");
        let mut targets = std::collections::BTreeSet::new();
        let mut unknown = false;
        for (_, references) in staged.values.range(value.clone()..).take_while(|(slot, _)| value.contains(slot)) {
            targets.extend(references.targets.iter().cloned());
            unknown |= references.unknown || references.displaced;
        }
        (targets, unknown)
    }

    fn reference_location_in<'a>(
        &'a self,
        values: &bray_symbols::SemanticValueStore,
        mut target: Target,
        path: &bray_bound_tree::StorageReferencePath,
        mut root_borrow: Option<Target>,
        lookup: impl Fn(&Target) -> Option<&'a References>,
    ) -> Option<Target> {
        let mut copied = 0;

        for (step, (index, ty)) in path.borrows().iter().enumerate() {
            target = target.project(&path.projections()[copied..*index]);

            target = match root_borrow.take().filter(|_| step == 0 && *index == 0) {
                Some(root) => root,
                None => lookup(&target).and_then(|reference| self.typed_target(reference, *ty)).or_else(|| {
                    let data = values.type_data(*ty);

                    let bray_symbols::TypeData::Slice(element) = data.as_ref() else {
                        return None;
                    };

                    let reference = lookup(&target)?;

                    if reference.unknown || reference.displaced || reference.targets.len() != 1 {
                        return None;
                    }

                    let reached = reference.targets.first().expect("one bounded slice target");
                    let declared = self.contents.get(reached)?.storage_type;

                    // A buffer view reaches its existing element allocation. A
                    // dynamic owner projection remains inexact in the target itself.
                    // Never turn a merely inexact view into an exact cell identity.
                    (!reached.previous
                        && (!reference.inexact || !reached.is_exact())
                        && (declared == Some(*ty) || declared == Some(*element)))
                    .then(|| reached.clone())
                })?,
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
        kind: bray_symbols::DependencyReferenceKind,
    ) {
        let target = Target {
            origin: Origin::Entry {
                reference,
                layer,
                kind,
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
                .deferred
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
            origin: Origin::entry(reference.clone(), bray_symbols::DependencyReferenceKind::Borrow),
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

    pub(in crate::analysis::storage_flow) fn declared_type(&self, target: &Target) -> Option<TypeId> {
        self.contents.get(target)?.storage_type
    }

    pub(in crate::analysis::storage_flow) fn projected_type<C: crate::CheckerRequestContext + ?Sized>(
        &self,
        request: crate::CheckerUnitView<'_, C>,
        target: &Target,
    ) -> crate::CheckerQueryResult<Option<TypeId>, C::UpstreamError> {
        // A projection keeps its containing storage's checked type even when no
        // operation has declared that component as a separate memory cell.
        let mut ancestor = target.clone();

        loop {
            // An enclosing element region stores its element type. Resolve a
            // longer route from its owner so an array element is not mistaken
            // for an element of an array-valued cell.
            if let Some(mut ty) = self.declared_type(&ancestor)
                && (ancestor == *target || !matches!(ancestor.projections().last(),
                    Some(StorageProjection::OwnedTarget | StorageProjection::SliceRange { .. })))
            {
                let mut element_region = false;

                for projection in &target.projections()[ancestor.projections().len()..] {
                    if element_region && !matches!(projection,
                        StorageProjection::Element(_) | StorageProjection::ElementFromStart(_)
                            | StorageProjection::AnyElement
                            | StorageProjection::ElementFromEnd(_) | StorageProjection::SliceRange { .. }) {
                        ty = Self::owned_element_type(request.semantic_values(), ty);
                    }

                    let projected = crate::storage::projected_storage_type(request, ty, *projection)?;

                    let Some(projected) = *projected.value() else {
                        return Ok(None);
                    };

                    element_region = matches!(projection,
                        StorageProjection::OwnedTarget | StorageProjection::SliceRange { .. })
                        && matches!(request.semantic_values().type_data(projected).as_ref(),
                            bray_symbols::TypeData::Slice(_));

                    ty = projected;
                }

                return Ok(Some(if element_region {
                    Self::owned_element_type(request.semantic_values(), ty)
                } else { ty }));
            }

            let length = ancestor.projections().len();

            if length == 0 {
                return Ok(None);
            }

            match &mut ancestor.origin {
                Origin::Entry { projections, .. }
                | Origin::Returned { projections, .. }
                | Origin::Static { projections, .. } | Origin::Allocation { projections, .. } => {
                    *projections = projections[..length - 1].into();
                }
                Origin::Storage(path) => {
                    path.projections = path.projections[..length - 1].into();
                }
            }
        }
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
        values: &bray_symbols::SemanticValueStore,
        capability: BorrowCapabilityId,
        target: Target,
        ty: TypeId,
    ) {
        let ty = if target.projections().last() == Some(&StorageProjection::OwnedTarget) {
            Self::owned_element_type(values, ty)
        } else {
            ty
        };

        self.contents
            .entry(target.clone())
            .or_default()
            .storage_type
            .get_or_insert(ty);

        self.borrows.insert(capability, References::one(target));
    }

    fn owned_element_type(values: &bray_symbols::SemanticValueStore, ty: TypeId) -> TypeId {
        // An owned slice denotes an element region in physical memory, rather
        // than a separately stored slice descriptor.
        match values.type_data(ty).as_ref() {
            bray_symbols::TypeData::Slice(element) => *element,
            _ => ty,
        }
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
