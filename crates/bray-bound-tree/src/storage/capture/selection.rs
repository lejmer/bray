use std::collections::BTreeMap;
use crate::{StorageOrigin as Origin, StorageProjection, StorageTarget as Target};

/// The exact prefix before the first dynamic selector, retaining origin and epoch.
pub fn storage_selector_parent(target: &Target) -> Option<Target> {
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

pub fn storage_value_subtree<'a, V>(
    values: &'a BTreeMap<Target, V>,
    root: &'a Target,
) -> impl Iterator<Item = (&'a Target, &'a V)> + 'a {
    values.range(root.clone()..)
        .take_while(|(slot, _)| root.contains(slot))
        .filter(|(slot, _)| root.contains_storage(slot))
}

pub fn selected_storage_values<'a, V>(
    values: &'a BTreeMap<Target, V>,
    source: &'a Target,
) -> impl Iterator<Item = (&'a Target, &'a V)> + 'a {
    let dynamic = storage_selector_parent(source).is_some();

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
                && StorageProjection::relationship(
                    source.projections(), &slot.projections()[..length],
                ) != crate::StorageRelationship::Disjoint
                && (dynamic || slot.projections()[..length].iter().zip(source.projections()).all(|(actual, requested)| {
                    actual == requested || matches!(actual, StorageProjection::AnyElement)
                }))
        })
}
