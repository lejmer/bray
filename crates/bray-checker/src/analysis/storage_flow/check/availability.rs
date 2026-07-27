use bray_bound_tree::{
    PatternPredicate, RefinementFact, RefinementFactKind, StorageAccessId, StorageIdentity,
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
    refinements: &[RefinementFact],
) -> bool {
    let related = refinements.iter().filter(|fact| {
        fact.dependencies().iter().any(|dependency| {
            storage.relationship(*dependency, access) != StorageRelationship::Disjoint
        })
    });

    match projection {
        StorageProjection::NullableValue => related.into_iter().any(|fact| {
            matches!(
                fact.kind(),
                RefinementFactKind::NullablePresence {
                    is_present: true,
                    ..
                } | RefinementFactKind::Pattern {
                    predicate: PatternPredicate::NullablePresent,
                    ..
                }
            )
        }),
        StorageProjection::ActiveUnionPayloadField { variant, .. } => {
            related.into_iter().any(|fact| {
                matches!(
                    fact.kind(),
                    RefinementFactKind::Pattern {
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
