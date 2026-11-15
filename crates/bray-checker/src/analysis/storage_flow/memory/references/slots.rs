use super::super::model::{
    Contents, Origin, Place, ReferenceExit, Retirement, State, StoredValue, Target, Transfer,
};
#[cfg(test)]
use super::super::model::References;

use bray_bound_tree::{StorageIdentityId, StorageProjection};
use std::collections::{BTreeMap, BTreeSet};

impl State {
    pub(in crate::analysis::storage_flow) fn seed_empty_whole(&mut self, destination: Target) {
        let value = self.values.entry(destination).or_default();

        value.sources.clear();
        value.tracked = true;
        value.complete = true;
    }

    pub(in crate::analysis::storage_flow) fn seed_whole_source(&mut self, destination: Target, source: Target) {
        let value = self.values.entry(destination).or_default();

        value.sources.insert(source);

        value.tracked = true;
        value.complete = true;
    }

    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn whole_sources(&self) -> impl Iterator<Item = (&Target, &BTreeSet<Target>, bool)> {
        self.values.iter().filter(|(_, value)| value.tracked)
            .map(|(target, value)| (target, &value.sources, value.complete))
    }

    #[cfg(test)]
    pub(super) fn slots_in<'a>(
        &'a self,
        place: &'a Place,
    ) -> impl Iterator<Item = (&'a Place, &'a References)> {
        // Place clones share the immutable projection path used as the ordered range boundary.
        self.values
            .range(Target::storage(place.clone())..)
            .map_while(|(target, value)| target.storage_path().map(|path| (path, value)))
            .take_while(move |(slot, _)| place.contains(slot))
            .filter(move |(slot, _)| place.contains_storage(slot))
            .filter_map(|(slot, value)| Some((slot, value.references.as_ref()?)))
    }

    pub(in crate::analysis::storage_flow) fn observed_references(
        &self,
    ) -> impl Iterator<Item = (&Target, &BTreeSet<Target>, bool)> {
        self.values
            .iter()
            .filter_map(|(location, value)| value.references.as_ref().map(|references| (location, &references.targets, references.unknown)))
    }

    pub(in crate::analysis::storage_flow) fn contained_regions<'a>(
        &'a self,
        location: &'a Target,
    ) -> impl Iterator<Item = &'a Target> {
        self.values
            .range(location.clone()..)
            .take_while(|(slot, _)| location.contains(slot))
            .filter(|(slot, _)| location.contains_storage(slot))
            .flat_map(|(_, value)| value.references.iter().flat_map(|references| references.targets.iter()))
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
                        value.references.is_some() && !target.previous && self.reference_slot_is_external(target)
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
                value.references.iter().flat_map(|references| references.targets.iter().cloned())
            }))
            .chain(self.borrows.values().flat_map(|references| references.targets.iter().cloned()))
            .chain(self.returned_inputs.values().flat_map(|references| references.targets.iter().cloned()))
            .filter(|target| self.unknown_reference_may_reach(target))
            .collect::<Vec<_>>();

        targets.extend(self.transitive_regions(candidates, |_, _| true).0);
    }

    fn reference_slot_is_external(&self, target: &Target) -> bool {
        match &target.origin {
            Origin::Storage(path) => !self.is_owned_inline_storage(path),
            Origin::Returned { reference, projections }
            if !projections.contains(&bray_bound_tree::StorageProjection::OwnedTarget) => {
                let bounds = self.returned_reference_bounds(reference, target.previous);

                // A complete bound to private inline storage does not publish its
                // owner. An inexact location within that region remains private.
                bounds.unknown || bounds.displaced || bounds.targets.is_empty()
                    || bounds.targets.iter().any(|target| target.previous
                        || target.storage_path().is_none_or(|path| !self.is_owned_inline_storage(path)))
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
        let reached = self.transitive_regions(roots.iter().cloned(), |region, slot| region.contains_storage(slot)).0;

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
            .chain([bray_bound_tree::StorageProjection::AddressTarget])
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
            .and_then(|value| value.references.as_ref())
            .is_some_and(|references| references.targets.contains(target))
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

            for (slot, value) in &self.values {
                if follow(&target, slot)
                    // Typed scalars cannot load unrelated pointer slots. Explicit
                    // stored references and raw allocation views remain authoritative.
                    && (!reference_free || target.contains_storage(slot))
                    && self.target_relationship(&target, slot)
                        != bray_bound_tree::StorageRelationship::Disjoint
                {
                    unknown |= !reference_free && value.tracked && !value.complete;

                    if let Some(references) = &value.references {
                        pending.extend(references.targets.iter().cloned());
                        unknown |= references.unknown || references.displaced;
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
        absent: &std::collections::BTreeMap<
            Place,
            crate::analysis::storage_flow::model::StorageAbsence,
        >,
    ) -> ReferenceExit {
        let mut input_regions = BTreeSet::new();

        // Boundary summaries retain only observable reference values, not the body state.
        let slots = self
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
                let mut exported = value.clone();

                if let Some(references) = &value.references { for target in &references.targets {
                    if let Origin::Returned { reference, .. } = &target.origin {
                        // Expand established origins without turning a region bound into
                        // exact forwarding or inventing an independent alternative.
                        let expanded = exported.references.as_mut().expect("copied reference slot");

                        expanded.inexact = true;

                        let bounds = self.returned_reference_bounds(reference, target.previous);

                        if (bounds.unknown || bounds.inexact) && !bounds.targets.is_empty() {
                            input_regions.insert(slot.clone());
                        }

                        if !bounds.unknown {
                            expanded.targets.remove(target);
                        }

                        expanded.targets.extend(bounds.targets.iter().cloned());
                    }
                } }

                (slot.clone(), exported)
            })
            .collect();

        ReferenceExit {
            slots,
            stored: self.values.iter().filter(|(target, _)| !target.previous)
                .map(|(target, value)| (target.clone(), value.clone())).collect(),
            input_regions,
            absent: absent
                .iter()
                .filter(|(_, reason)| {
                    **reason == crate::analysis::storage_flow::model::StorageAbsence::NoPayload
                })
                .map(|(slot, _)| slot.clone())
                .collect(),
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
            .filter_map(|(slot, value)| Some((slot, value.references.as_ref()?)))
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

    pub(super) fn transferred_slots<'a>(
        &'a self,
        source: &'a Target,
        destination: &'a Target,
    ) -> impl Iterator<Item = (Target, StoredValue, bool)> + 'a {
        selected_subtree(&self.values, source)
            .map(move |(slot, references)| {
                let overlap = slot.projections()[..source.projections().len()] != *source.projections();
                let slot = destination.project(&slot.projections()[source.projections().len()..]);

                // The receiver owns each copied value while target paths remain shared.
                (slot, references.clone(), overlap)
            })
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
            super::Source::Borrow(capability) => Transfer {
                // The evaluated value owns the formed reference after its capability ends.
                values: self
                    .borrows
                    .get(capability)
                    .map(|references| {
                        [(destination.clone(), StoredValue::from(references.clone()))]
                            .into_iter()
                            .collect()
                    })
                    .unwrap_or_default(),
                ..Default::default()
            },
        }
    }

    pub(in crate::analysis::storage_flow::memory) fn capture_value(
        &self,
        source: &Target,
        destination: &Target,
    ) -> Transfer {
        let inexact = selector_parent(source).is_some();
        let complete = self.values.get(source).is_some_and(|value| value.tracked && value.complete);
        let mut values = BTreeMap::<Target, StoredValue>::new();

        for (slot, mut value, overlap) in self.transferred_slots(source, destination) {
            if inexact || overlap {
                if let Some(references) = &mut value.references {
                    references.inexact = true;
                }

                if inexact && value.tracked && !complete {
                    value.complete = false;
                }
            }

            values.entry(slot).and_modify(|current| { current.merge(&value); }).or_insert(value);
        }

        if inexact && !complete {
            let value = values.entry(destination.clone()).or_default();

            value.tracked = true;
            value.complete = false;
        }

        let mut transfer = Transfer {
            values,
            payloads: Default::default(),
            deferred: self.capture_deferred_value(source, destination),
            ..Default::default()
        };

        for (target, contents) in selected_subtree(&self.contents, source) {
            let suffix = &target.projections()[source.projections().len()..];

            if suffix.contains(&bray_bound_tree::StorageProjection::UninitPayload) {
                // Evaluated inline storage owns its contents after the source is consumed.
                let slot = destination.project(suffix);
                let mut contents = self.evaluated.get(target).unwrap_or(contents).clone();

                if inexact || target.projections()[..source.projections().len()] != *source.projections() {
                    contents.initialized.clear();
                }

                transfer.payloads.entry(slot.clone()).and_modify(|previous| { previous.merge(&contents, &slot); }).or_insert(contents);
            }
        }

        transfer
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
        captured.deferred.retain(|target, _| inline(target));

        captured
    }

    pub(in crate::analysis::storage_flow) fn finish_value_transfers(&mut self) {
        self.evaluated.clear();
    }

    pub(in crate::analysis::storage_flow::memory) fn commit_value(
        &mut self,
        mut transfer: Transfer,
    ) {
        let mut enclosing: Option<&Target> = None;

        for target in transfer.payloads.keys() {
            if enclosing.is_some_and(|enclosing| enclosing.contains_storage(target)) {
                continue;
            }

            // Installing an inline owner starts a new storage incarnation. Incoming
            // raw pointers can still name the destination's previous contents.
            let retirement = Retirement::for_payload(target);

            self.retire_region(target, retirement);

            if matches!(retirement, Retirement::Producer) {
                continue;
            }

            for references in transfer.values.values_mut().filter_map(|value| value.references.as_mut())
                .chain(transfer.returned_inputs.values_mut()) {
                references.retire(target, Retirement::Storage);
            }

            enclosing = Some(target);
        }

        self.values.extend(transfer.values);
        self.contents.extend(transfer.payloads);
        self.deferred.extend(transfer.deferred);

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
        take_subtree(&mut self.deferred, target, Retirement::Storage).for_each(drop);
    }

    pub(in crate::analysis::storage_flow) fn offset(
        &mut self,
        source: &Target,
        destination: Place,
    ) {
        let mut references = self.references(source);

        references.displaced = true;
        self.values.insert(Target::storage(destination), references.into());
    }

    pub(in crate::analysis::storage_flow) fn forget(&mut self, identity: StorageIdentityId) {
        for previous in [false, true] {
            let root = Target { previous, ..Target::storage(Place::root(identity)) };

            if self.has_addressed_storage(&root) {
                continue;
            }

            let removed = self.values.range(root.clone()..)
                .take_while(|(target, _)| root.contains(target))
                .filter(|(target, _)| root.contains_storage(target)
                    && matches!(&target.origin, Origin::Storage(path) if !is_entry_slot(&self.contents, path)))
                .map(|(target, _)| target.clone()).collect::<Vec<_>>();

            for target in removed {
                self.values.remove(&target);
            }
        }
    }

    pub(in crate::analysis::storage_flow) fn end_storage(
        &mut self,
        mut ended: impl FnMut(StorageIdentityId) -> bool,
    ) {
        // Last-use pruning drops local value facts, not the addressed storage's lifetime.
        // Physical storage ends at its owning boundary, including locals pruned earlier.
        let expired = self
            .contents
            .keys()
            .filter_map(|target| target.storage_path())
            .map(|place| place.root)
            .collect::<BTreeSet<_>>();

        for identity in expired.into_iter().filter(|identity| ended(*identity)) {
            self.retire(&Target::storage(Place::root(identity)));
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

/// The exact prefix before the first dynamic selector, retaining origin and epoch.
pub(super) fn selector_parent(target: &Target) -> Option<Target> {
    let length = target.projections().iter().position(|projection| !projection.is_exact())?;

    Some(target_prefix(target, length))
}

fn target_prefix(target: &Target, length: usize) -> Target {
    let mut prefix = target.clone();

    match &mut prefix.origin {
        Origin::Storage(path) => path.projections = path.projections[..length].into(),
        Origin::Entry { projections, .. } | Origin::Returned { projections, .. }
        | Origin::Allocation { projections, .. } | Origin::Static { projections, .. } => {
            *projections = projections[..length].into();
        }
    }

    prefix
}

pub(super) fn selected_subtree<'a, V>(
    values: &'a BTreeMap<Target, V>,
    source: &'a Target,
) -> impl Iterator<Item = (&'a Target, &'a V)> + 'a {
    let dynamic = selector_parent(source).is_some();

    let prefix = source.projections().iter().position(|projection| {
        !projection.is_exact() || matches!(projection, StorageProjection::ElementFromStart(_) | StorageProjection::ElementFromEnd(_))
    }).map_or_else(|| source.clone(), |length| target_prefix(source, length));

    let widened = prefix != *source;
    let limit = prefix.clone();

    values.range(prefix.clone()..)
        .take_while(move |(slot, _)| limit.contains(slot))
        .filter(move |(slot, _)| {
            let length = source.projections().len();

            if slot.projections().len() < length
                || !StorageProjection::remains_in_allocation(&slot.projections()[length..])
            {
                return false;
            }

            if !widened {
                return true;
            }

            target_prefix(slot, prefix.projections().len()) == prefix
                && bray_bound_tree::StorageProjection::relationship(
                    source.projections(), &slot.projections()[..length],
                ) != bray_bound_tree::StorageRelationship::Disjoint
                && (dynamic || slot.projections()[..length].iter().zip(source.projections()).all(|(actual, requested)| {
                    actual == requested || matches!(actual, StorageProjection::AnyElement)
                }))
        })
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
