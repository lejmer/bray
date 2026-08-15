use bray_bound_tree::{
    PatternPredicate, Refinement, RefinementKind, StorageAccessId, StorageIdentity,
    StoragePlan, StorageProjection, StorageRelationship,
};

pub(super) fn storage_is_recovered(storage: &StoragePlan) -> bool {
    storage
        .identities()
        .iter()
        .any(|identity| matches!(identity, StorageIdentity::Error(_)))
        || storage
            .accesses()
            .iter()
            .any(|access| access.is_recovered())
        || storage
            .borrow_capabilities()
            .iter()
            .any(|capability| capability.is_recovered())
}

pub(super) fn projection_is_available(
    storage: &StoragePlan,
    access: StorageAccessId,
    projection: StorageProjection,
    refinements: &[Refinement],
) -> bool {
    let related = refinements.iter().filter(|refinement| {
        refinement.dependencies().iter().any(|dependency| {
            storage.relationship(*dependency, access) != StorageRelationship::Disjoint
        })
    });

    match projection {
        StorageProjection::NullableValue => related.into_iter().any(|refinement| {
            matches!(
                refinement.kind(),
                RefinementKind::NullablePresence {
                    is_present: true,
                    ..
                } | RefinementKind::Pattern {
                    predicate: PatternPredicate::NullablePresent,
                    ..
                }
            )
        }),
        StorageProjection::ActiveUnionPayloadField { variant, .. } => {
            related.into_iter().any(|refinement| {
                matches!(
                    refinement.kind(),
                    RefinementKind::Pattern {
                        predicate: PatternPredicate::ActiveUnionVariant(active),
                        ..
                    } if active == variant
                )
            })
        }
        StorageProjection::ProductField(_)
        | StorageProjection::TupleElement(_)
        | StorageProjection::ElementFromStart(_)
        | StorageProjection::ElementFromEnd(_)
        | StorageProjection::Element(_)
        | StorageProjection::SliceRange { .. }
        | StorageProjection::OwnedTarget => true,
    }
}
