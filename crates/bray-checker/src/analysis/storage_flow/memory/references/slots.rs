use super::super::model::{Contents, Origin, Place, ReferenceExit, References, State, Target};
use bray_bound_tree::StorageIdentityId;
use std::collections::BTreeSet;

impl State {
    pub(super) fn slots_in<'a>(
        &'a self,
        place: &'a Place,
    ) -> impl Iterator<Item = (&'a Place, &'a References)> {
        // Place clones share the immutable projection path used as the ordered range boundary.
        self.slots
            .range(place.clone()..)
            .take_while(move |(slot, _)| place.contains(slot))
    }

    pub(in crate::analysis::storage_flow) fn entry_reference(&mut self, slot: Place) {
        let target = Target {
            origin: Origin::Entry(slot.clone()),
            previous: false,
        };

        self.contents.entry(target.clone()).or_default();
        self.slots.insert(slot, References::one(target));
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
                .slots
                .iter()
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
        self.slots
            .range(Place::root(root)..)
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
        source: &'a Place,
        destination: &'a Place,
    ) -> impl Iterator<Item = (Place, References)> + 'a {
        self.slots_in(source).map(move |(slot, references)| {
            let slot = Place {
                root: destination.root,
                projections: destination
                    .projections
                    .iter()
                    .copied()
                    .chain(slot.projections[source.projections.len()..].iter().copied())
                    .collect(),
            };

            // The receiver owns each copied value while target paths remain shared.
            (slot, references.clone())
        })
    }

    pub(in crate::analysis::storage_flow) fn transfer(
        &mut self,
        source: &Place,
        destination: Place,
    ) {
        if !destination.is_exact() {
            self.clear(&Place::root(destination.root));
            return;
        }

        if !source.is_exact() {
            self.clear(&destination);
            return;
        }

        if *source == destination {
            return;
        }

        // Snapshot only the transferred subtree before replacing the destination subtree.
        let values = self
            .transferred_slots(source, &destination)
            .collect::<Vec<_>>();

        self.clear(&destination);
        self.slots.extend(values);
    }

    pub(in crate::analysis::storage_flow) fn clear(&mut self, place: &Place) {
        // Only the replaced subtree is visited, even when the function has many live slots.
        let removed = self
            .slots_in(place)
            .map(|(slot, _)| slot)
            .cloned()
            .collect::<Vec<_>>();

        for slot in removed {
            self.slots.remove(&slot);
        }
    }

    pub(in crate::analysis::storage_flow) fn offset(&mut self, source: &Place, destination: Place) {
        let mut references = self.references(source);

        references.displaced = true;
        self.slots.insert(destination, references);
    }

    pub(in crate::analysis::storage_flow) fn forget(&mut self, identity: StorageIdentityId) {
        let root = Place::root(identity);

        let removed = self
            .slots_in(&root)
            .filter(|(slot, _)| !is_entry_slot(&self.contents, slot))
            .map(|(slot, _)| slot.clone())
            .collect::<Vec<_>>();

        for slot in removed {
            self.slots.remove(&slot);
        }
    }

    pub(in crate::analysis::storage_flow) fn retain_live(
        &mut self,
        live: &BTreeSet<StorageIdentityId>,
    ) {
        self.slots
            .retain(|place, _| live.contains(&place.root) || is_entry_slot(&self.contents, place));
    }
}

fn is_entry_slot(contents: &std::collections::BTreeMap<Target, Contents>, slot: &Place) -> bool {
    // An entry value snapshot remains useful after its local slot's lifetime ends.
    // Keeping its value does not retain initialization or access authority.
    contents.contains_key(&Target {
        origin: Origin::Entry(slot.clone()),
        previous: false,
    })
}
