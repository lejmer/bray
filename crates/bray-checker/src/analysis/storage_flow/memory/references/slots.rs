use super::super::model::{
    Contents, Origin, Place, ReferenceExit, References, State, Target, Transfer,
};
use bray_bound_tree::StorageIdentityId;
use std::collections::BTreeSet;

impl State {
    pub(super) fn slots_in<'a>(
        &'a self,
        place: &'a Place,
    ) -> impl Iterator<Item = (&'a Place, &'a References)> {
        // Place clones share the immutable projection path used as the ordered range boundary.
        self.values
            .range(Target::storage(place.clone())..)
            .map_while(|(target, value)| target.storage_path().map(|path| (path, value)))
            .take_while(move |(slot, _)| place.contains(slot))
    }

    pub(in crate::analysis::storage_flow) fn observed_references(
        &self,
    ) -> impl Iterator<Item = (Place, &BTreeSet<Target>, bool)> {
        self.values.iter().filter_map(|(location, references)| {
            Some((
                location.observation_path()?,
                &references.targets,
                references.unknown,
            ))
        })
    }

    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn entry_reference(&mut self, slot: Place) {
        self.entry_reference_at(slot.clone(), Target::storage(slot), None);
    }

    pub(in crate::analysis::storage_flow) fn exit_references(
        &self,
        result: StorageIdentityId,
        absent: &std::collections::BTreeMap<
            Place,
            crate::analysis::storage_flow::model::StorageAbsence,
        >,
    ) -> ReferenceExit {
        ReferenceExit {
            // Boundary summaries retain only observable reference values, not the body state.
            slots: self
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
                .map(|(slot, references)| (slot.clone(), references.clone()))
                .collect(),
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
        self.slots_in(root).filter_map(|(slot, references)| {
            references.is_exact().then(|| {
                (
                    slot,
                    &references
                        .targets
                        .first()
                        .expect("exact reference has one target")
                        .origin,
                )
            })
        })
    }

    pub(in crate::analysis::storage_flow) fn has_stored_value(&self, slot: &Place) -> bool {
        slot.is_exact() && self.has_value(&Target::storage(slot.clone()))
    }

    pub(super) fn transferred_slots<'a>(
        &'a self,
        source: &'a Target,
        destination: &'a Target,
    ) -> impl Iterator<Item = (Target, References)> + 'a {
        self.values
            .range(source.clone()..)
            .take_while(move |(target, _)| source.contains(target))
            .map(move |(slot, references)| {
                let slot = destination.project(&slot.projections()[source.projections().len()..]);

                // The receiver owns each copied value while target paths remain shared.
                (slot, references.clone())
            })
    }

    pub(in crate::analysis::storage_flow) fn transfer(
        &mut self,
        source: &Place,
        destination: Place,
    ) {
        if !source.is_exact() {
            self.clear(&destination);
            return;
        }

        self.transfer_value(&Target::storage(source.clone()), destination);
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
                        [(destination.clone(), references.clone())]
                            .into_iter()
                            .collect()
                    })
                    .unwrap_or_default(),
                payloads: Default::default(),
            },
        }
    }

    pub(in crate::analysis::storage_flow::memory) fn capture_value(
        &self,
        source: &Target,
        destination: &Target,
    ) -> Transfer {
        let mut transfer = Transfer {
            values: self.transferred_slots(source, destination).collect(),
            payloads: Default::default(),
        };

        for (target, contents) in self
            .contents
            .range(source.clone()..)
            .take_while(|(target, _)| source.contains(target))
        {
            let suffix = &target.projections()[source.projections().len()..];

            if suffix.contains(&bray_bound_tree::StorageProjection::UninitPayload) {
                // Evaluated inline storage owns its contents after the source is consumed.
                transfer.payloads.insert(
                    destination.project(suffix),
                    self.evaluated.get(target).unwrap_or(contents).clone(),
                );
            }
        }

        transfer
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
            if enclosing.is_some_and(|enclosing| enclosing.contains(target)) {
                continue;
            }

            // Installing an inline owner starts a new storage incarnation. Incoming
            // raw pointers can still name the destination's previous contents.
            self.retire(target);

            for references in transfer.values.values_mut() {
                references.retire(target);
            }

            enclosing = Some(target);
        }

        self.values.extend(transfer.values);
        self.contents.extend(transfer.payloads);
    }

    pub(in crate::analysis::storage_flow) fn clear(&mut self, place: &Place) {
        self.clear_value(&Target::storage(place.clone()));
    }

    pub(in crate::analysis::storage_flow) fn clear_value(&mut self, target: &Target) {
        // Only the replaced subtree is visited, even when the function has many live values.
        take_subtree(&mut self.values, target).for_each(drop);
    }

    pub(in crate::analysis::storage_flow) fn offset(&mut self, source: &Place, destination: Place) {
        let mut references = self.references(source);

        references.displaced = true;
        self.values.insert(Target::storage(destination), references);
    }

    pub(in crate::analysis::storage_flow) fn forget(&mut self, identity: StorageIdentityId) {
        let root = Place::root(identity);

        let removed = self
            .slots_in(&root)
            .filter(|(slot, _)| !is_entry_slot(&self.contents, slot))
            .map(|(slot, _)| slot.clone())
            .collect::<Vec<_>>();

        for slot in removed {
            self.values.remove(&Target::storage(slot));
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
        self.values.retain(|target, _| {
            target.storage_path().is_none_or(|place| {
                live.contains(&place.root) || is_entry_slot(&self.contents, place)
            })
        });
    }
}

fn is_entry_slot(contents: &std::collections::BTreeMap<Target, Contents>, slot: &Place) -> bool {
    // An entry value snapshot remains useful after its local slot's lifetime ends.
    // Keeping its value does not retain initialization or access authority.
    contents.contains_key(&Target {
        origin: Origin::entry(slot.clone()),
        previous: false,
    })
}

pub(super) fn take_subtree<'a, V>(
    values: &'a mut std::collections::BTreeMap<Target, V>,
    root: &Target,
) -> impl Iterator<Item = (Target, V)> + 'a {
    // Only the selected subtree is visited. Keys share their immutable projection paths.
    let keys = values
        .range(root.clone()..)
        .take_while(|(key, _)| root.contains(key))
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();

    keys.into_iter().map(|key| {
        let value = values
            .remove(&key)
            .expect("captured subtree key is present");

        (key, value)
    })
}
