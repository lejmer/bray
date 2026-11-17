use std::collections::BTreeMap;
use crate::{StorageContents, StorageTarget as Target, StorageValueBinding};

pub fn merge_storage_contents(
    target: &mut BTreeMap<Target, StorageContents>,
    incoming: &BTreeMap<Target, StorageContents>,
) -> bool {
    let mut changed = false;

    for (key, contents) in target.iter_mut() {
        match incoming.get(key) {
            Some(incoming) => changed |= contents.merge(incoming, key),
            None => {
                changed |= !contents.initialized.is_empty();
                contents.initialized.clear();
            }
        }
    }

    for (key, contents) in incoming {
        if !target.contains_key(key) {
            // Joined contents retain possible obligations, but no one-sided initialization proof.
            let mut contents = contents.clone();

            contents.initialized.clear();
            target.insert(key.clone(), contents);
            changed = true;
        }
    }

    changed
}

pub fn merge_storage_bindings<K: Clone + Ord>(
    target: &mut BTreeMap<K, StorageValueBinding>,
    incoming: &BTreeMap<K, StorageValueBinding>,
    target_absent: impl Fn(&K) -> bool,
    incoming_absent: impl Fn(&K) -> bool,
) -> bool {
    let mut changed = false;

    for (slot, value) in target.iter_mut() {
        let previous = value.clone();

        match incoming.get(slot) {
            Some(other) if target_absent(slot) => *value = other.clone(),
            Some(_) if incoming_absent(slot) => {},
            Some(other) => {
                value.join(other);
            }
            None if !incoming_absent(slot) => {
                value.lose_information();
            }
            None => {}
        }

        changed |= *value != previous;
    }

    for (slot, value) in incoming {
        if !target.contains_key(slot) {
            let mut value = value.clone();

            if !target_absent(slot) {
                value.lose_information();
            }

            target.insert(slot.clone(), value);
            changed = true;
        }
    }

    changed
}
