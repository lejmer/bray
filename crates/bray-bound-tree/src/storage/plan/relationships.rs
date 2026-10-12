use crate::{StorageProjection, StorageRelationship};

impl StorageProjection {
    /// Compares two projection paths within the same physical storage root.
    pub fn relationship(
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
