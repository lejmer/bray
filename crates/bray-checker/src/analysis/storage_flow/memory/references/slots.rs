pub(in crate::analysis::storage_flow::memory) use bray_bound_tree::{selected_storage_values as selected_subtree, storage_selector_parent as selector_parent, storage_value_subtree as storage_subtree};
use super::super::model::{
    Contents, DependencyCompleteness, Origin, Place, ReferenceExit, Retirement, State, ValueBinding, Target, Transfer,
};
#[cfg(test)]
use super::super::model::AddressValue;

use bray_bound_tree::{StorageIdentityId, StorageProjection};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

impl State {
    pub(in crate::analysis::storage_flow) fn seed_empty_whole(&mut self, destination: Target) {
        let value = self.values.entry(destination).or_default();

        if value.is_no_payload() { return; }

        value.sources.clear();
        value.completeness = DependencyCompleteness::Complete;
    }

    pub(in crate::analysis::storage_flow) fn seed_whole_source(&mut self, destination: Target, source: Target) {
        let value = self.values.entry(destination).or_default();

        if value.is_no_payload() { return; }

        value.sources.insert(source);

        value.completeness = DependencyCompleteness::Complete;
    }

    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn whole_sources(&self) -> impl Iterator<Item = (&Target, &BTreeSet<Target>, bool)> {
        self.values.iter().filter_map(|(target, value)| value.whole_sources().map(|(sources, complete)| (target, sources, complete)))
    }

    #[cfg(test)]
    pub(super) fn slots_in<'a>(
        &'a self,
        place: &'a Place,
    ) -> impl Iterator<Item = (&'a Place, &'a AddressValue)> {
        // Place clones share the immutable projection path used as the ordered range boundary.
        self.values
            .range(Target::storage(place.clone())..)
            .map_while(|(target, value)| target.storage_path().map(|path| (path, value)))
            .take_while(move |(slot, _)| place.contains(slot))
            .filter(move |(slot, _)| place.contains_storage(slot))
            .filter_map(|(slot, value)| Some((slot, value.reference()?)))
    }

    pub(in crate::analysis::storage_flow) fn observed_references(
        &self,
    ) -> impl Iterator<Item = (&Target, &BTreeSet<Target>, bool)> {
        self.values
            .iter()
            .flat_map(|(location, value)| value.possible_references().map(move |references| (location, &references.targets, references.is_unknown())))
    }

    pub(in crate::analysis::storage_flow) fn contained_regions<'a>(
        &'a self,
        location: &'a Target,
    ) -> impl Iterator<Item = &'a Target> {
        storage_subtree(&self.values, location)
            .flat_map(|(_, value)| value.carried_references().flat_map(|references| references.targets.iter()))
    }

    pub(in crate::analysis::storage_flow) fn raw_exposed_regions(&self) -> BTreeSet<Target> {
        self.contents
            .iter()
            // A callee can publish an address without forming it in this caller.
            .filter(|(_, contents)| contents.exposed || contents.escaped)
            .map(|(target, _)| exposure_region(target))
            .collect()
    }

    pub(in crate::analysis::storage_flow) fn unexposed_storage<'a>(
        &'a self,
        candidates: &'a [Place],
        roots: impl IntoIterator<Item = Target>,
    ) -> impl Iterator<Item = &'a Place> + 'a {
        let (reached, unknown) = self.reachable_regions(roots);

        let mut inline = BTreeMap::<StorageIdentityId, Vec<&Target>>::new();

        for target in &reached {
            for root in self.reachable_inline_roots(target) {
                inline.entry(root).or_default().push(target);
            }
        }

        candidates
            .iter()
            .filter(|path| {
                let candidate = Target::storage((*path).clone());

                let disjoint = |target: &Target| {
                    self.target_relationship(target, &candidate)
                        == bray_bound_tree::StorageRelationship::Disjoint
                };

                if unknown && self.unknown_reference_may_reach(&candidate) {
                    return false;
                }

                if self.is_owned_inline_storage(path) {
                    // Entry and allocation origins cannot reach owned inline storage.
                    // Returned origins use the same established roots as target_relationship.
                    inline
                        .get(&path.root)
                        .into_iter()
                        .flatten()
                        .all(|target| disjoint(target))
                } else {
                    reached.iter().all(disjoint)
                }
            })
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn reachable_targets(
        &self,
        roots: impl IntoIterator<Item = Target>,
    ) -> BTreeSet<Target> {
        let (mut targets, unknown) = self.reachable_regions(roots);

        self.include_unknown_targets(&mut targets, unknown);

        targets
    }

    pub(in crate::analysis::storage_flow) fn reachable_regions(
        &self,
        roots: impl IntoIterator<Item = Target>,
    ) -> (BTreeSet<Target>, bool) {
        // Escaped addresses remain reachable after the local pointer's last use.
        let pending = roots
            .into_iter()
            .chain(
                self.contents
                    .iter()
                    .filter(|(target, contents)| {
                        contents.escaped
                            || !target.previous && matches!(target.origin, Origin::Entry { .. })
                                && self.is_external_region(target)
                    })
                    .map(|(target, _)| {
                        // Native address arithmetic can reach other fields of the allocation.
                        exposure_region(target)
                    }),
            )
            .chain(
                self.values
                    .iter()
                    .filter(|(target, value)| {
                        // Retired reference slots are history, not external roots. A surviving raw
                        // address can still reach them through the explicit exposure roots above.
                        value.reference().is_some() && !target.previous && self.is_external_region(target)
                    })
                    .map(|(target, _)| target.clone()),
            )
            .collect::<Vec<_>>();

        self.transitive_regions(pending, |_, _| true)
    }

    pub(super) fn include_unknown_targets(&self, targets: &mut BTreeSet<Target>, unknown: bool) {
        if !unknown { return; }

        let candidates = self.contents.keys().chain(self.values.keys()).cloned()
            .chain(self.values.values().flat_map(|value| value.sources.iter().cloned()))
            .chain(self.values.values().flat_map(|value| {
                value.carried_references().flat_map(|references| references.targets.iter().cloned())
            }))
            .chain(self.borrows.values().flat_map(|references| references.address.targets.iter().cloned()))
            .chain(self.returned_inputs.values().flat_map(|references| references.targets.iter().cloned()))
            .filter(|target| self.unknown_reference_may_reach(target))
            .collect::<Vec<_>>();

        targets.extend(self.transitive_regions(candidates, |_, _| true).0);
    }

    fn is_external_region(&self, target: &Target) -> bool {
        match &target.origin {
            Origin::Storage(path) => !self.is_owned_inline_storage(path),
            Origin::Entry { reference, layer: 1, kind: bray_symbols::DependencyReferenceKind::Borrow, projections }
                if reference.projections.is_empty()
                    && self.entry_borrows.get(&reference.root) == Some(&bray_symbols::BorrowKind::Mutable) => {
                // An original exclusive borrow does not publish its inline referent.
                // Shared/nested borrows, separate backing and raw origins stay external.
                !Place::inline_projections(projections)
            }
            Origin::Returned { reference, projections }
            if !projections.contains(&StorageProjection::OwnedTarget) => {
                let bounds = self.returned_reference_bounds(reference, target.previous);

                // A complete bound to private inline storage does not publish its
                // owner. An inexact location within that region remains private.
                bounds.is_unknown() || bounds.is_displaced() || bounds.targets.is_empty()
                    || bounds.targets.iter().any(|target| target.previous || self.is_external_region(target))
            }
            _ => true,
        }
    }

    /// Returns escaped operands and their stored references before caller-side alias resolution.
    pub(in crate::analysis::storage_flow) fn escape_roots(
        &self,
        roots: impl IntoIterator<Item = Target>,
    ) -> BTreeSet<Target> {
        let roots = roots.into_iter().map(|root| exposure_region(&root)).collect::<BTreeSet<_>>();
        let reached = self.walk_regions(roots.iter().cloned(), true, |region, slot| region.contains_storage(slot)).0;

        reached.iter().filter(|target| roots.contains(*target)
            || !reached.iter().any(|region| self.entry_field_is_reachable(region, target)))
            .cloned().collect()
    }

    fn entry_field_is_reachable(&self, region: &Target, target: &Target) -> bool {
        let Origin::Entry { reference: parent, projections, kind: bray_symbols::DependencyReferenceKind::Address, .. } = &region.origin
        else {
            return false;
        };

        let Origin::Entry { reference: field, .. } = &target.origin
        else {
            return false;
        };

        let prefix = parent.projections.iter().copied()
            .chain([StorageProjection::AddressTarget])
            .chain(projections.iter().copied());

        let prefix_len = parent.projections.len() + 1 + projections.len();

        if region.previous != target.previous || parent.root != field.root
            || field.projections.len() < prefix_len
            || !field.projections.iter().copied().zip(prefix).all(|(field, parent)| field == parent) {
            return false;
        }

        // The caller expands the escaped raw address. Only a field still holding its
        // original incoming pointer is redundant. Writes and captured old copies differ.
        self.values.get(&region.project(&field.projections[prefix_len..]))
            .and_then(|value| value.address.as_ref())
            .is_some_and(|references| references.is_exact() && references.targets.contains(target))
    }

    pub(in crate::analysis::storage_flow) fn escaped_regions(
        &self,
        roots: impl IntoIterator<Item = Target>,
    ) -> BTreeSet<Target> {
        let (targets, unknown) = self.transitive_regions(roots, |_, _| true);

        let mut implicit = BTreeSet::new();

        self.include_unknown_targets(&mut implicit, unknown);

        // Publishing a symbolic result also exposes each possible input allocation.
        // Keep its own identity for backing produced independently of those inputs.
        targets
            .iter()
            .flat_map(|target| std::iter::once(target).chain(self.effect_origins(target).0))
            .chain(implicit.iter().flat_map(|target| self.effect_origins(target).0)
                .filter(|target| self.unknown_reference_may_reach(target)))
            .map(exposure_region)
            .collect()
    }

    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn escape_targets(
        &mut self,
        roots: impl IntoIterator<Item = Target>,
    ) {
        self.escape_regions(self.escaped_regions(roots));
    }

    pub(in crate::analysis::storage_flow) fn escape_regions(
        &mut self,
        regions: impl IntoIterator<Item = Target>,
    ) {
        for target in regions {
            self.contents.entry(target).or_default().escaped = true;
        }
    }

    pub(in crate::analysis::storage_flow) fn transitive_regions(
        &self,
        roots: impl IntoIterator<Item = Target>,
        follow: impl Fn(&Target, &Target) -> bool,
    ) -> (BTreeSet<Target>, bool) {
        self.walk_regions(roots, false, follow)
    }

    fn walk_regions(
        &self,
        roots: impl IntoIterator<Item = Target>,
        contained_only: bool,
        follow: impl Fn(&Target, &Target) -> bool,
    ) -> (BTreeSet<Target>, bool) {
        let mut pending = roots.into_iter().collect::<Vec<_>>();
        let mut reached = BTreeSet::new();
        let mut unknown = false;

        while let Some(target) = pending.pop() {
            if !reached.insert(target.clone()) {
                continue;
            }

            let reference_free = self.contents.get(&target).is_some_and(|contents| {
                !contents.exposed
                    && contents
                        .storage_type
                        .is_some_and(|ty| self.complete_reference_types.get(&ty) == Some(&true))
            });

            // The same allocation and projection prefix form a contiguous range.
            // Entry kinds and static selections have separate ordering coordinates.
            let ranged = reference_free || contained_only
                && matches!(target.origin, Origin::Storage(_) | Origin::Allocation { .. } | Origin::Returned { .. });

            let contained = ranged.then(|| storage_subtree(&self.values, &target));
            let all = (!ranged).then_some(&self.values);

            for (slot, value) in contained.into_iter().flatten().chain(all.into_iter().flatten()) {
                if follow(&target, slot)
                    // Typed scalars cannot load unrelated pointer slots. Explicit
                    // stored references and raw allocation views remain authoritative.
                    && (!reference_free || target.contains_storage(slot))
                    && self.target_relationship(&target, slot)
                        != bray_bound_tree::StorageRelationship::Disjoint
                {
                    unknown |= !reference_free && value.unknown_dependencies();

                    for references in value.carried_references() {
                        pending.extend(references.targets.iter().cloned());
                        unknown |= references.is_unknown() || references.is_displaced();
                    }
                }
            }
        }

        (reached, unknown)
    }

    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn entry_reference(&mut self, slot: Place) {
        self.entry_reference_at(slot.clone(), 1, Target::storage(slot), None, bray_symbols::DependencyReferenceKind::Address);
    }

    pub(in crate::analysis::storage_flow) fn exit_references(
        &self,
        result: StorageIdentityId,
        absent: &BTreeMap<
            Place,
            bray_bound_tree::StorageAbsence,
        >,
    ) -> ReferenceExit<'_> {
        let mut input_regions = BTreeSet::new();

        // Boundary summaries retain only observable reference values, not the body state.
        let slots: BTreeMap<_, _> = self
            .values
            .iter()
            .filter_map(|(target, references)| {
                let slot = target.observation_path()?;

                Some((slot, references))
            })
            .filter(|(slot, _)| {
                slot.root == result
                    || !slot.projections.is_empty() && is_entry_slot(&self.contents, slot)
            })
            .map(|(slot, value)| {
                let mut exported = Cow::Borrowed(value);

                if value.address.is_none() && value.address_bounds.is_some() {
                    input_regions.insert(slot.clone());
                }

                if let Some(references) = value.reference() { for target in &references.targets {
                    if let Origin::Returned { reference, .. } = &target.origin {
                        // Expand established origins without turning a region bound into
                        // exact forwarding or inventing an independent alternative.
                        let expanded = exported.to_mut().reference_mut().expect("expanded reference slot");

                        expanded.bound();

                        let bounds = self.returned_reference_bounds(reference, target.previous);

                        if (bounds.is_unknown() || bounds.is_bounded()) && !bounds.targets.is_empty() {
                            input_regions.insert(slot.clone());
                        }

                        if !bounds.is_unknown() {
                            expanded.targets.remove(target);
                        }

                        expanded.targets.extend(bounds.targets.iter().cloned());
                    }
                } }

                (slot, exported)
            })
            .collect();

        let absent = absent.iter().filter_map(|(slot, reason)| {
            (*reason == bray_bound_tree::StorageAbsence::NoPayload).then_some(slot)
        }).chain(slots.iter().filter_map(|(slot, value)| value.is_no_payload().then_some(slot))).cloned().collect();

        ReferenceExit {
            slots,
            stored: &self.values,
            input_regions,
            absent,
        }
    }

    pub(in crate::analysis::storage_flow) fn reference_targets(
        &self,
        root: StorageIdentityId,
    ) -> impl Iterator<Item = (&Place, &Origin)> {
        self.values
            .range(Target::storage(Place::root(root))..)
            .map_while(|(target, value)| target.storage_path().map(|path| (path, value)))
            .take_while(move |(slot, _)| slot.root == root)
            .filter_map(|(slot, value)| Some((slot, value.reference()?)))
            .flat_map(|(slot, references)| {
                references
                    .targets
                    .iter()
                    .map(move |target| (slot, &target.origin))
            })
    }

    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn exact_references<'a>(
        &'a self,
        root: &'a Place,
    ) -> impl Iterator<Item = (&'a Place, &'a Origin)> {
        self.slots_in(root)
            .filter(|&(_, references)| references.is_exact())
            .map(|(slot, references)| {
                (
                    slot,
                    &references
                        .targets
                        .first()
                        .expect("exact reference has one target")
                        .origin,
                )
            })
    }

    pub(in crate::analysis::storage_flow) fn has_stored_value(&self, slot: &Place) -> bool {
        slot.is_exact() && self.has_value(&Target::storage(slot.clone()))
    }

    pub(in crate::analysis::storage_flow) fn transfer_value(
        &mut self,
        source: &Target,
        destination: Place,
    ) {
        if !destination.is_exact() {
            self.clear(&Place::root(destination.root));
            return;
        }

        if source.storage_path() == Some(&destination) {
            return;
        }

        // Snapshot the source subtree before replacing overlapping destination storage.
        let transfer = self.capture_value(source, &Target::storage(destination.clone()));

        self.clear(&destination);
        self.commit_value(transfer);
    }

    pub(in crate::analysis::storage_flow::memory) fn capture_source(
        &self,
        source: &super::Source,
        destination: &Target,
    ) -> Transfer {
        match source {
            super::Source::Value(source) => self.capture_value(source, destination),
            super::Source::Captured(source) => source.materialize(destination),
            super::Source::Borrow(capability) => Transfer {
                // The evaluated value owns the formed reference after its capability ends.
                values: self
                    .borrow_address(*capability)
                    .map(|references| {
                        [(destination.clone(), ValueBinding::borrowed(references.clone(), *capability))]
                            .into_iter()
                            .collect()
                    })
                    .unwrap_or_default(),
                ..Default::default()
            },
        }
    }

    pub(in crate::analysis::storage_flow::memory) fn capture_value(&self, source: &Target, destination: &Target) -> Transfer {
        Transfer::capture(&self.values, &self.contents, &self.evaluated, source, destination)
    }

    /// Select the element directly addressed by a buffer's base pointer.
    pub(super) fn capture_base_element(&self, source: &Target, destination: &Target) -> Transfer {
        let mut captured = self.capture_value(source, destination);
        let depth = destination.projections().len();

        let inline = |target: &Target| !matches!(target.projections().get(depth),
            Some(StorageProjection::AnyElement | StorageProjection::Element(_)
                | StorageProjection::ElementFromStart(_) | StorageProjection::ElementFromEnd(_)
                | StorageProjection::SliceRange { .. }));

        captured.values.retain(|target, _| inline(target));
        captured.payloads.retain(|target, _| inline(target));

        captured
    }

    pub(in crate::analysis::storage_flow) fn finish_value_transfers(&mut self) {
        self.evaluated.clear();
    }

    pub(in crate::analysis::storage_flow) fn take_retirements(&mut self) -> Vec<(Target, Retirement)> {
        std::mem::take(&mut self.retirements).into_iter().collect()
    }

    pub(in crate::analysis::storage_flow::memory) fn commit_value(
        &mut self,
        mut transfer: Transfer,
    ) {
        let mut enclosing: Option<(&Target, Retirement)> = None;

        for target in transfer.payloads.keys() {
            if enclosing.is_some_and(|(owner, scope)| scope.contains(owner, target)) {
                continue;
            }

            // Installing an inline owner starts a new storage incarnation. Incoming
            // raw pointers can still name the destination's previous contents.
            let retirement = Retirement::for_payload(target);

            self.retire_region(target, retirement);
            enclosing = Some((target, retirement));

            if matches!(retirement, Retirement::Producer) {
                continue;
            }

            for value in transfer.values.values_mut() {
                value.retire_references(target, Retirement::Storage);
            }

            for references in transfer.returned_inputs.values_mut() {
                references.retire(target, Retirement::Storage);
            }

        }

        let reported = &mut self.may_have_report_values;

        self.values.extend(transfer.values.into_iter().inspect(|(_, value)| {
            *reported = *reported || value.report_dependencies().is_some();
        }));

        self.contents.extend(transfer.payloads);

        // Retired bounds stay in their historical epoch. This completion publishes
        // only the regions of the newly produced reference.
        for (reference, bounds) in transfer.returned_inputs {
            self.returned_inputs
                .entry((reference, false))
                .or_default()
                .merge(&bounds);
        }
    }

    pub(in crate::analysis::storage_flow) fn clear(&mut self, place: &Place) {
        self.clear_value(&Target::storage(place.clone()));
    }

    pub(in crate::analysis::storage_flow) fn clear_value(&mut self, target: &Target) {
        // Only the replaced subtree is visited, even when the function has many live values.
        take_subtree(&mut self.values, target, Retirement::Storage).for_each(drop);
    }

    pub(in crate::analysis::storage_flow) fn offset(
        &mut self,
        source: &Target,
        destination: Place,
    ) {
        let mut references = self.references(source);

        references.displace();
        self.values.insert(Target::storage(destination), references.into());
    }

    pub(in crate::analysis::storage_flow) fn forget(&mut self, identity: StorageIdentityId) {
        for previous in [false, true] {
            let root = Target { previous, ..Target::storage(Place::root(identity)) };

            if self.has_addressed_storage(&root) {
                continue;
            }

            let removed = storage_subtree(&self.values, &root)
                .filter(|(target, _)| matches!(&target.origin, Origin::Storage(path) if !is_entry_slot(&self.contents, path)))
                .map(|(target, _)| target.clone()).collect::<Vec<_>>();

            for target in removed {
                self.values.remove(&target);
            }
        }
    }

    pub(in crate::analysis::storage_flow) fn end_storage(
        &mut self,
        identities: &[StorageIdentityId],
    ) {
        // Last-use pruning drops local value facts, not the addressed storage's lifetime.
        // Physical storage ends at its owning boundary, including locals pruned earlier.
        for &identity in identities {
            let root = Target::storage(Place::root(identity));

            if self.contents.range(root.clone()..).next().is_some_and(|(target, _)| root.contains(target)) {
                self.retire(&root);
            }
        }
    }

    pub(in crate::analysis::storage_flow) fn retain_live(
        &mut self,
        live: &BTreeSet<StorageIdentityId>,
    ) {
        let expired = self.values.keys().filter_map(Target::storage_path)
            .map(|place| place.root).filter(|root| !live.contains(root)).collect::<BTreeSet<_>>();

        for identity in expired {
            self.forget(identity);
        }
    }
}

fn is_entry_slot(contents: &BTreeMap<Target, Contents>, slot: &Place) -> bool {
    // An entry value snapshot remains useful after its local slot's lifetime ends.
    // Keeping its value does not retain initialization or access authority.
    [bray_symbols::DependencyReferenceKind::Borrow, bray_symbols::DependencyReferenceKind::Address]
        .into_iter().any(|kind| contents.contains_key(&Target {
            origin: Origin::entry(slot.clone(), kind),
            previous: false,
        }))
}

pub(super) fn take_subtree<'a, V>(
    values: &'a mut BTreeMap<Target, V>,
    root: &Target,
    retirement: Retirement,
) -> impl Iterator<Item = (Target, V)> + 'a {
    // Only the selected subtree is visited. Keys share their immutable projection paths.
    let keys = values
        .range(root.clone()..)
        .take_while(|(key, _)| root.contains(key))
        .filter(|(key, _)| retirement.contains(root, key))
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();

    keys.into_iter().map(|key| {
        let value = values
            .remove(&key)
            .expect("captured subtree key is present");

        (key, value)
    })
}

pub(in crate::analysis::storage_flow::memory) fn exposure_region(target: &Target) -> Target {
    target.allocation_region().unwrap_or_else(|| {
        // An inline local has one enclosing allocation. Keep symbolic entry
        // references unchanged when their allocation boundary is not established.
        match &target.origin {
            Origin::Storage(path) => Target { previous: target.previous, ..Target::storage(Place::root(path.root)) },
            _ => target.clone(),
        }
    })
}
