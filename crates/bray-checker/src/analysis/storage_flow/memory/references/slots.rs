use super::super::model::{Contents, Origin, Place, ReferenceExit, References, State, Target};
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

    pub(in crate::analysis::storage_flow) fn has_reference(&self, slot: &Place) -> bool {
        slot.is_exact() && self.slots_in(slot).next().is_some()
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
        let values = self
            .transferred_slots(source, &Target::storage(destination.clone()))
            .collect::<Vec<_>>();

        self.clear(&destination);
        self.values.extend(values);
    }

    pub(in crate::analysis::storage_flow) fn clear(&mut self, place: &Place) {
        self.clear_value(&Target::storage(place.clone()));
    }

    pub(in crate::analysis::storage_flow) fn clear_value(&mut self, target: &Target) {
        // Only the replaced subtree is visited, even when the function has many live values.
        let removed = self
            .values
            .range(target.clone()..)
            .take_while(|(candidate, _)| target.contains(candidate))
            .map(|(target, _)| target.clone())
            .collect::<Vec<_>>();

        for target in removed {
            self.values.remove(&target);
        }
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
