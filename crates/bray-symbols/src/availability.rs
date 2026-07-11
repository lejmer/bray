use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn resolve_owned_availability<I: Copy + Ord>(
    item: I,
    direct_availability: &BTreeMap<I, bool>,
    resolved_availability: &mut BTreeMap<I, bool>,
    resolving: &mut BTreeSet<I>,
    parent_of: impl Copy + Fn(I) -> Option<I>,
) -> bool {
    if let Some(available) = resolved_availability.get(&item).copied() {
        return available;
    }

    if !resolving.insert(item) {
        return false;
    }

    let parent_available = match parent_of(item) {
        Some(parent) => resolve_owned_availability(
            parent,
            direct_availability,
            resolved_availability,
            resolving,
            parent_of,
        ),
        None => true,
    };

    let available = direct_availability.get(&item).copied().unwrap_or(false) && parent_available;

    resolving.remove(&item);
    resolved_availability.insert(item, available);

    available
}
