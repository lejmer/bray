use std::sync::Arc;
use super::super::model::{AddressValue, Contents, Origin, Place, RetainedBorrow, State, Target, Transfer, ValueBinding};
use super::slots::storage_subtree;
use bray_bound_tree::{BorrowCapabilityId, StorageIdentityId, StorageProjection};
use bray_symbols::TypeId;

impl State {
    pub(in crate::analysis::storage_flow) fn for_storage(
        storage: &bray_bound_tree::StoragePlan,
        cleanup: &crate::asynchronous::StorageCleanupRequirements,
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
            .filter(|identity| cleanup.ends_at_last_use(*identity) && !borrowed.contains(identity))
            .collect();

        Self {
            storage_slots: storage.shared_identities(),
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

        storage_subtree(&self.contents, &root)
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
        Self::owned_inline_path(&self.owned_storage, path)
    }

    fn owned_inline_path(owned: &[StorageIdentityId], path: &Place) -> bool {
        owned.binary_search(&path.root).is_ok() && !path.projections.contains(&StorageProjection::OwnedTarget)
    }

    pub(in crate::analysis::storage_flow) fn unlocated_deallocation_may_reach(&self, target: &Target) -> bool {
        Self::deallocation_may_reach(&self.owned_storage, target)
    }

    pub(super) fn deallocation_may_reach(owned: &[StorageIdentityId], target: &Target) -> bool {
        target.storage_path().is_none_or(|path| !Self::owned_inline_path(owned, path))
    }

    #[track_caller]
    pub(super) fn returned_reference_bounds(&self, reference: &Place, previous: bool) -> &AddressValue {
        let Some(bounds) = self.returned_inputs.get(&(reference.clone(), previous)) else {
            panic!("returned reference {reference:?} (previous={previous}) has no complete input-region bound");
        };

        bounds
    }

    pub(super) fn actual_borrow_choices(&self, target: &Target, staged: Option<(&Transfer, &Target)>) -> Option<Vec<Target>> {
        if !self.current_route_target(target, staged) { return None; }

        let Origin::Returned { projections, .. } = &target.origin else {
            return Some(vec![target.clone()]);
        };

        let bounds = self.route_returned_bounds(target, staged)?;

        if !Place::inline_projections(projections) || !bounds.has_values() || bounds.targets.is_empty() {
            return None;
        }

        bounds.targets.iter().map(|choice| {
            (self.current_route_target(choice, staged)
                && !matches!(choice.origin, Origin::Returned { .. })).then(|| choice.project(projections))
        }).collect()
    }

    pub(super) fn route_returned_bounds<'a>(&'a self, target: &Target, staged: Option<(&'a Transfer, &Target)>) -> Option<&'a AddressValue> {
        let Origin::Returned { reference, staged: private, .. } = &target.origin else { return None; };

        if *private {
            staged?.0.returned_inputs.get(reference)
        } else {
            self.returned_inputs.get(&(reference.clone(), target.previous))
        }
    }

    pub(super) fn forwarded_reference_contents(
        &self,
        location: &Target,
        staged: Option<(&Transfer, &Target)>,
    ) -> Option<AddressValue> {
        if location.previous || !location.is_exact() { return None; }

        let Origin::Returned { projections, .. } = &location.origin else {
            return None;
        };

        if projections.is_empty() || !Place::inline_projections(projections) {
            return None;
        }

        let parent = location.prefix(0);

        let bounds = self.route_returned_bounds(location, staged)?;

        if !bounds.has_values() || bounds.targets.is_empty() { return None; }

        let ty = self.route_declared_type(&parent, staged)?;

        let mut addresses = bounds.targets.iter().map(|target| {
            if !self.current_route_target(target, staged) || self.route_declared_type(target, staged) != Some(ty) {
                return None;
            }

            self.direct_route_binding(&target.project(projections), staged)?.address.as_ref()
                .filter(|value| !value.is_unknown() && !value.is_displaced())
        });

        let first = addresses.next().flatten()?;
        let mut contents = first.clone();

        for address in addresses {
            let address = address?;

            if address != first {
                contents.merge(address);
                contents.bound();
            }
        }

        Some(contents)
    }

    fn direct_route_binding<'a>(
        &'a self, target: &Target, staged: Option<(&'a Transfer, &Target)>,
    ) -> Option<&'a ValueBinding> {
        if let Some((transfer, root)) = staged {
            transfer.values.get(target).or_else(|| {
                (!Self::staged_target(root, target)).then(|| self.values.get(target)).flatten()
            })
        } else {
            self.values.get(target)
        }
    }

    pub(super) fn route_declared_type(&self, target: &Target, staged: Option<(&Transfer, &Target)>) -> Option<TypeId> {
        self.route_contents(target, staged)?.storage_type
    }

    fn route_contents<'a>(&'a self, target: &Target, staged: Option<(&'a Transfer, &Target)>) -> Option<&'a Contents> {
        if let Some((transfer, root)) = staged {
            if let Some(contents) = transfer.payloads.get(target) {
                return Some(contents);
            }

            if Self::staged_target(root, target) {
                return None;
            }
        }

        self.contents.get(target)
    }

    fn current_route_target(&self, target: &Target, staged: Option<(&Transfer, &Target)>) -> bool {
        !target.previous && target.is_exact()
            && self.route_contents(target, staged).is_none_or(|contents| contents.invalidations.is_empty())
            && target.allocation_region().is_none_or(|allocation| {
                self.route_contents(&allocation, staged).is_none_or(|contents| contents.invalidations.is_empty())
            })
    }

    fn staged_target(root: &Target, target: &Target) -> bool {
        root.contains_storage(target) || target.is_staged()
    }

    pub(super) fn reached_route_binding<'a>(
        &'a self, target: &Target, staged: Option<(&'a Transfer, &Target)>,
    ) -> Option<std::borrow::Cow<'a, ValueBinding>> {
        self.direct_route_binding(target, staged).map(std::borrow::Cow::Borrowed).or_else(|| {
            self.forwarded_reference_contents(target, staged).map(|address| std::borrow::Cow::Owned(address.into()))
        })
    }

    pub(super) fn reachable_inline_roots(
        &self,
        target: &Target,
    ) -> impl Iterator<Item = StorageIdentityId> + '_ {
        let (storage, returned) = match &target.origin {
            Origin::Storage(path) => (Some(path.root), None),
            Origin::Returned { reference, .. } => {
                (None, Some(self.returned_reference_bounds(reference, target.previous)))
            }
            Origin::Entry { .. } | Origin::Allocation { .. } | Origin::Static { .. } | Origin::BackingRegion => (None, None),
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
            Origin::Returned { reference, .. } => Some(self.returned_reference_bounds(reference, target.previous)),
            _ => None,
        };

        Self::effect_origins_from(target, returned)
    }

    pub(super) fn effect_origins_from<'a>(
        target: &'a Target,
        returned: Option<&'a AddressValue>,
    ) -> (impl Iterator<Item = &'a Target>, bool) {
        let unbounded = target.origin.is_backing_region() || returned.is_some_and(|bounds| bounds.is_unknown() || target.previous
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
        if left.is_staged() || right.is_staged() {
            return left.relationship_in_slots(right, &self.storage_slots);
        }

        if [(left, right), (right, left)].iter().any(|(backing, other)| {
            backing.origin.is_backing_region()
                && other.storage_path().is_some_and(|path| self.is_owned_inline_storage(path))
        }) {
            return bray_bound_tree::StorageRelationship::Disjoint;
        }

        if !left.previous && !right.previous {
            for (returned, other) in [(left, right), (right, left)] {
                if let Origin::Returned { reference, projections, .. } = &returned.origin
                    && !projections.contains(&StorageProjection::OwnedTarget)
                {
                    let bounds = self.returned_reference_bounds(reference, returned.previous);

                    // Complete producer bounds exclude unrelated storage, without
                    // equating the returned address to any one possible source.
                    // Reaching another owner requires its own physical bounds.
                    if !bounds.is_unknown() && !bounds.targets.is_empty()
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
            (Origin::Returned { reference, .. }, Origin::Storage(slot)) => Some((reference, left.previous, slot, right.previous)),
            (Origin::Storage(slot), Origin::Returned { reference, .. }) => Some((reference, right.previous, slot, left.previous)),
            _ => None,
        };

        if let Some((reference, previous, slot, slot_previous)) = returned_slot
            && self.is_owned_inline_storage(slot)
            && !self
                .returned_reference_bounds(reference, previous)
                .targets
                .iter()
                .any(|target| {
                    target.previous == slot_previous && target.storage_path().is_some_and(|path| {
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

        if slot.is_some_and(|slot| {
            self.is_owned_inline_storage(slot) || slot.is_inline()
                && usize::try_from(slot.root.ordinal()).ok().and_then(|index| self.storage_slots.get(index))
                    .is_some_and(|identity| matches!(identity, bray_bound_tree::StorageIdentity::PostconditionResult(_)))
        }) {
            // Entry references predate both local inline slots and the produced
            // result observed by a postcondition. Reached pointees keep their own origins.
            return bray_bound_tree::StorageRelationship::Disjoint;
        }

        left.relationship_in_slots(right, &self.storage_slots)
    }

    pub(super) fn addressed_target(
        &mut self,
        target: Target,
        ty: TypeId,
        kind: bray_symbols::DependencyReferenceKind,
    ) -> AddressValue {
        self.observe_addressed_target(&target, ty, kind);

        AddressValue::one(target)
    }

    pub(super) fn observe_addressed_target(
        &mut self,
        target: &Target,
        ty: TypeId,
        kind: bray_symbols::DependencyReferenceKind,
    ) {
        let contents = self.contents.entry(target.clone()).or_default();

        contents.storage_type.get_or_insert(ty);
        contents.exposed |= kind == bray_symbols::DependencyReferenceKind::Address;
    }

    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn reference_location(
        &self,
        values: &bray_symbols::SemanticValueStore,
        target: Target,
        path: &bray_bound_tree::StorageReferencePath,
        root_borrow: Option<Target>,
    ) -> Option<Target> {
        let locations = self.reference_locations(values, target, path, root_borrow)?;

        let [location] = locations.as_slice() else {
            return None;
        };

        Some(location.clone())
    }

    pub(in crate::analysis::storage_flow) fn referenced_regions(
        &self,
        location: &Target,
    ) -> Option<Vec<Target>> {
        self.reached_route_binding(location, None)?.reference_regions()
    }

    pub(in crate::analysis::storage_flow) fn value_targets(
        &self,
        location: &Target,
    ) -> Option<Vec<Target>> {
        let binding = self.reached_route_binding(location, None)?;
        let references = binding.address.as_ref()?;

        references.actual_targets()
    }

    pub(in crate::analysis::storage_flow) fn borrow_regions(
        &self,
        capability: BorrowCapabilityId,
    ) -> Option<Vec<Target>> {
        self.borrow_address(capability)?.regions()
    }

    pub(in crate::analysis::storage_flow) fn borrow_address(
        &self,
        capability: BorrowCapabilityId,
    ) -> Option<&AddressValue> {
        self.borrows.get(&capability).map(|borrow| &borrow.address)
    }

    pub(in crate::analysis::storage_flow) fn borrow_ancestors(
        &self,
        capability: BorrowCapabilityId,
    ) -> Option<&[BorrowCapabilityId]> {
        let borrow = self.borrows.get(&capability)?;

        (!borrow.address.is_unknown() && borrow.address.targets.iter().all(|target| !target.previous))
            .then_some(borrow.ancestors.as_ref())
    }

    pub(in crate::analysis::storage_flow) fn borrow_relationship<'target>(
        &self,
        capability: BorrowCapabilityId,
        targets: impl Iterator<Item = &'target Target> + Clone,
        located: bool,
    ) -> bray_bound_tree::StorageRelationship {
        use bray_bound_tree::StorageRelationship;

        let Some(borrow) = self.borrow_address(capability) else {
            return StorageRelationship::PotentiallyOverlapping;
        };

        if targets.clone().next().is_none() || borrow.targets.is_empty() || borrow.is_unknown() || borrow.is_displaced() {
            return StorageRelationship::PotentiallyOverlapping;
        }

        let mut relationship = StorageRelationship::Disjoint;

        for region in &borrow.targets {
            for target in targets.clone() {
                match self.target_relationship(region, target) {
                    StorageRelationship::Identical if located && borrow.has_values()
                        && region.is_exact() && target.is_exact() => return StorageRelationship::Identical,
                    StorageRelationship::Disjoint => {}
                    _ => relationship = StorageRelationship::PotentiallyOverlapping,
                }
            }
        }

        relationship
    }

    pub(in crate::analysis::storage_flow) fn borrow_targets(
        &self,
        capability: BorrowCapabilityId,
    ) -> Option<Vec<Target>> {
        let references = self.borrow_address(capability)?;

        references.actual_targets()
    }

    pub(in crate::analysis::storage_flow) fn borrow_observation(
        &self,
        capability: BorrowCapabilityId,
    ) -> Option<Place> {
        let references = self.borrow_address(capability)?;

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

        Arc::make_mut(&mut self.values).insert(location, AddressValue::one(target).into());
    }

    pub(in crate::analysis::storage_flow) fn has_value(&self, target: &Target) -> bool {
        storage_subtree(&self.values, target)
            .any(|(_, value)| value.reference().is_some() || value.owned_backing().is_some()
                || value.whole_sources().is_some() || value.report_dependencies().is_some())
            || storage_subtree(&self.contents, target)
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

        Arc::make_mut(&mut self.values).insert(Target::storage(reference),
            super::super::model::ValueBinding::borrowed(AddressValue::one(target.clone()), capability));

        self.borrows.insert(capability, RetainedBorrow { address: AddressValue::one(target), ancestors: Default::default() });
    }

    pub(in crate::analysis::storage_flow) fn borrow_target(
        &self,
        capability: BorrowCapabilityId,
        ty: TypeId,
    ) -> Option<Target> {
        self.typed_target(self.borrow_address(capability)?, ty)
    }

    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn value_target(
        &self,
        location: &Target,
        ty: TypeId,
    ) -> Option<Target> {
        self.typed_target(self.values.get(location)?.address.as_ref()?, ty)
    }

    pub(in crate::analysis::storage_flow) fn captured_value_target(
        &self,
        binding: &super::super::model::ValueBinding,
        ty: TypeId,
    ) -> Option<Target> {
        self.typed_target(binding.reference()?, ty)
    }

    pub(in crate::analysis::storage_flow) fn declared_type(&self, target: &Target) -> Option<TypeId> {
        self.contents.get(target)?.storage_type
    }

    pub(in crate::analysis::storage_flow) fn enclosing_typed_view<C: crate::CheckerRequestContext + ?Sized>(
        &self,
        request: crate::CheckerUnitView<'_, C>,
        target: &Target,
        ty: TypeId,
    ) -> crate::CheckerQueryResult<Option<bray_bound_tree::StorageObservedTarget>, C::UpstreamError> {
        if target.previous || !target.is_exact() || target.value_observation_path().is_some()
            || !self.contents.get(target)
                .is_some_and(|contents| contents.storage_type == Some(ty) && contents.invalidations.is_empty())
        {
            return Ok(None);
        }

        for length in (0..target.projections().len()).rev() {
            let path = &target.projections()[length..];

            if !bray_bound_tree::StoragePath::inline_projections(path) {
                break;
            }

            let ancestor = target.prefix(length);

            let Some(contents) = self.contents.get(&ancestor) else { continue; };

            let Some(owner) = contents.storage_type else { continue; };

            if !contents.invalidations.is_empty()
                || matches!(ancestor.projections().last(), Some(StorageProjection::OwnedTarget | StorageProjection::SliceRange { .. }))
            {
                break;
            }

            let mut projected = owner;

            for projection in path {
                // An inline view cannot pass through a stored loan descriptor.
                if request.semantic_values().unborrowed_type(projected) != projected {
                    return Ok(None);
                }

                let checked = crate::storage::projected_storage_type(request, projected, *projection)?;

                let Some(component) = *checked.value() else { return Ok(None); };

                projected = component;
            }

            return Ok((projected == ty).then(|| bray_bound_tree::StorageObservedTarget::borrowed(ancestor, owner)));
        }

        Ok(None)
    }

    pub(in crate::analysis::storage_flow) fn affected_pointee_paths<'target>(
        &self,
        mutations: impl Iterator<Item = &'target Target> + Clone,
    ) -> std::collections::BTreeMap<Target, bray_bound_tree::StoragePointeeChange> {
        use bray_bound_tree::{StoragePointeeChange, StorageRelationship};
        use std::collections::{BTreeMap, BTreeSet};

        let mut changes = BTreeMap::new();

        for (target, ty) in self.typed_storage() {
            let mut paths = BTreeSet::new();

            for mutation in mutations.clone().filter(|mutation| {
                self.target_relationship(target, mutation) != StorageRelationship::Disjoint
            }) {
                let extent = target.projections().len();

                let relative = if target.contains_storage(mutation) && mutation.prefix(extent) == *target {
                    &mutation.projections()[extent..]
                } else {
                    &[]
                };

                let length = relative.iter().take_while(|projection| {
                    projection.is_exact() && Place::inline_projections(std::slice::from_ref(*projection))
                }).count();

                if length == 0 {
                    changes.insert(target.clone(), StoragePointeeChange::Whole);
                    paths.clear();
                    break;
                }

                paths.insert(relative[..length].into());
            }

            if !paths.is_empty() {
                changes.insert(target.clone(), StoragePointeeChange::Components { ty, paths });
            }
        }

        changes
    }

    pub(in crate::analysis::storage_flow) fn typed_storage(&self) -> impl Iterator<Item = (&Target, TypeId)> {
        self.contents.iter().filter(|(target, _)| !target.previous && target.is_exact())
            .filter_map(|(target, contents)| contents.storage_type.map(|ty| (target, ty)))
    }

    pub(in crate::analysis::storage_flow) fn entry_observations(&self, raw_addresses: bool)
        -> Vec<bray_bound_tree::StorageObservedTarget>
    {
        use bray_symbols::DependencyReferenceKind::{Address, Borrow};

        let addresses = raw_addresses.then(|| self.typed_storage()).into_iter().flatten()
            .filter(|(target, _)| matches!(target.origin, Origin::Entry { kind: Address, .. }));

        let loans = self.values.iter().filter_map(|(slot, binding)| {
            let path = slot.storage_path()?;

            if path.projections.is_empty() || !path.is_exact() || !path.is_inline()
                || path.projections.iter().any(|projection| matches!(projection,
                    StorageProjection::NullableValue | StorageProjection::ActiveUnionPayloadField { .. })) {
                return None;
            }

            let target = binding.reference()?.single_location()?;

            let Origin::Entry { reference, layer: 1, kind: Borrow, projections } = &target.origin else { return None; };

            if reference != path || !projections.is_empty() { return None; }

            let pointee = self.contents.get(target)?.storage_type?;

            Some((target, pointee))
        });

        addresses.chain(loans).map(|(target, pointee)| bray_bound_tree::StorageObservedTarget {
            target: target.clone(), pointee: Some(pointee),
        }).collect()
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
                            | StorageProjection::ElementFromInput(_)
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

            ancestor = target.prefix(length - 1);
        }
    }

    pub(super) fn typed_targets<'a>(&self, references: &'a AddressValue, ty: TypeId) -> Option<&'a std::collections::BTreeSet<Target>> {
        self.typed_route_targets(references, ty, None)
    }

    pub(super) fn typed_route_targets<'a>(
        &self, references: &'a AddressValue, ty: TypeId, staged: Option<(&Transfer, &Target)>,
    ) -> Option<&'a std::collections::BTreeSet<Target>> {
        (references.has_values() && !references.targets.is_empty()
            && references.targets.iter().all(|target| !target.previous
                && self.route_declared_type(target, staged) == Some(ty)))
            .then_some(&references.targets)
    }

    fn typed_target(&self, references: &AddressValue, ty: TypeId) -> Option<Target> {
        let targets = self.typed_targets(references, ty)?;

        // A typed dynamic selection supplies a location, without making it an exact cell.
        (targets.len() == 1).then(|| targets.first().expect("one current typed reference has one target").clone())
    }

    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn borrow_target_storage(
        &mut self,
        values: &bray_symbols::SemanticValueStore,
        capability: BorrowCapabilityId,
        targets: impl IntoIterator<Item = Target>,
        ty: TypeId,
    ) {
        self.borrow_reached_storage(values, capability, AddressValue::from_targets(targets), ty, Default::default());
    }

    pub(in crate::analysis::storage_flow) fn borrow_reached_storage(
        &mut self,
        values: &bray_symbols::SemanticValueStore,
        capability: BorrowCapabilityId,
        address: AddressValue,
        ty: TypeId,
        ancestors: std::sync::Arc<[BorrowCapabilityId]>,
    ) {
        for target in address.targets.iter().filter(|_| address.has_values()) {
            let cell = if target.projections().last() == Some(&StorageProjection::OwnedTarget) {
                Self::owned_element_type(values, ty)
            } else {
                ty
            };

            self.contents.entry(target.clone()).or_default().storage_type.get_or_insert(cell);
        }

        self.borrows.insert(capability, RetainedBorrow { address, ancestors });
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
