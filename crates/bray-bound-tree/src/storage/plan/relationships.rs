use crate::{StorageIdentity, StorageProjection, StorageRelationship};

use super::model::ResolvedStoragePath;

pub(super) const fn identity_is_distinct_storage(identity: StorageIdentity) -> bool {
    matches!(
        identity,
        StorageIdentity::LocalOwned(_)
            | StorageIdentity::Parameter(_)
            | StorageIdentity::Receiver(_)
            | StorageIdentity::AnonymousParameter(_)
            | StorageIdentity::PredicateParameter(_)
            | StorageIdentity::PostconditionResult(_)
            | StorageIdentity::Result(_)
            | StorageIdentity::Temporary(_)
            | StorageIdentity::IterationCursor(_)
            | StorageIdentity::IterationElement(_)
            | StorageIdentity::Allocation(_)
            | StorageIdentity::CompilerCreated(_)
    )
}

pub(super) fn path_contains(
    container: &ResolvedStoragePath,
    contained: &ResolvedStoragePath,
) -> bool {
    container.root == contained.root && contained.projections.starts_with(&container.projections)
}

pub(super) fn projection_relationship(
    left: &[StorageProjection],
    right: &[StorageProjection],
) -> StorageRelationship {
    for (left, right) in left.iter().zip(right) {
        if left == right {
            continue;
        }

        return if projections_are_disjoint(*left, *right) {
            StorageRelationship::Disjoint
        } else {
            StorageRelationship::PotentiallyOverlapping
        };
    }

    if left == right {
        StorageRelationship::Identical
    } else {
        StorageRelationship::PotentiallyOverlapping
    }
}

fn projections_are_disjoint(left: StorageProjection, right: StorageProjection) -> bool {
    match (left, right) {
        (StorageProjection::ProductField(left), StorageProjection::ProductField(right)) => {
            left != right
        }
        (StorageProjection::TupleElement(left), StorageProjection::TupleElement(right))
        | (StorageProjection::ElementFromStart(left), StorageProjection::ElementFromStart(right))
        | (StorageProjection::ElementFromEnd(left), StorageProjection::ElementFromEnd(right)) => {
            left != right
        }
        (
            StorageProjection::ActiveUnionPayloadField {
                variant: left_variant,
                field: left_field,
            },
            StorageProjection::ActiveUnionPayloadField {
                variant: right_variant,
                field: right_field,
            },
        ) => left_variant != right_variant || left_field != right_field,
        _ => false,
    }
}
