use bray_bound_tree::{
    BoundDependencyGuard, BoundDependencyRequirement, BoundDependencyRequirementKind,
    BoundDependencySubject, StorageAccessId, StorageAccessPurpose, StorageAccessRoot, StoragePlan,
    StorageProjection,
};
use bray_symbols::BorrowKind;

pub(super) fn operation_requirements(
    storage: &StoragePlan,
    operation: &bray_bound_tree::StorageOperationDecision,
) -> Vec<BoundDependencyRequirement> {
    let mut requirements = requirement_kinds(operation.purpose())
        .map(|kind| {
            BoundDependencyRequirement::direct(
                BoundDependencySubject::StorageAccess(operation.access()),
                kind,
            )
        })
        .collect::<Vec<_>>();

    let capabilities = operation
        .borrow()
        .into_iter()
        .chain(access_borrow(storage, operation.access()));

    for capability in capabilities {
        let Some(capability_record) = storage.borrow_capability(capability) else {
            continue;
        };

        requirements.push(BoundDependencyRequirement::direct(
            BoundDependencySubject::BorrowCapability(capability),
            BoundDependencyRequirementKind::BorrowCapabilityActive(capability_record.kind()),
        ));
    }

    guard_projected_requirements(storage, operation.access(), requirements)
}

pub(super) fn operation_access_requirements(
    storage: &StoragePlan,
    access: StorageAccessId,
) -> Vec<BoundDependencyRequirement> {
    let mut requirements = vec![BoundDependencyRequirement::direct(
        BoundDependencySubject::StorageAccess(access),
        BoundDependencyRequirementKind::StorageAlive,
    )];

    if let Some(capability) = access_borrow(storage, access)
        && let Some(capability_record) = storage.borrow_capability(capability)
    {
        requirements.push(BoundDependencyRequirement::direct(
            BoundDependencySubject::BorrowCapability(capability),
            BoundDependencyRequirementKind::BorrowCapabilityActive(capability_record.kind()),
        ));
    }

    guard_projected_requirements(storage, access, requirements)
}

fn guard_projected_requirements(
    storage: &StoragePlan,
    access: StorageAccessId,
    mut requirements: Vec<BoundDependencyRequirement>,
) -> Vec<BoundDependencyRequirement> {
    let Some(root) = storage.root_identity(access) else {
        return requirements;
    };

    let Some(projections) = storage.resolved_projections(access) else {
        return requirements;
    };

    let guards = projections
        .iter()
        .enumerate()
        .filter_map(|(index, projection)| {
            let guard_access = access_for_projection_prefix(storage, root, &projections[..index])
                .unwrap_or(access);

            match projection {
                StorageProjection::NullableValue => {
                    Some(BoundDependencyGuard::NullablePresent(guard_access))
                }
                StorageProjection::ActiveUnionPayloadField { variant, .. } => {
                    Some(BoundDependencyGuard::ActiveUnionVariant {
                        access: guard_access,
                        variant: *variant,
                    })
                }
                StorageProjection::ProductField(_)
                | StorageProjection::TupleElement(_)
                | StorageProjection::ElementFromStart(_)
                | StorageProjection::ElementFromEnd(_)
                | StorageProjection::Element(_)
                | StorageProjection::SliceRange { .. }
                | StorageProjection::OwnedTarget => None,
            }
        })
        .collect::<Vec<_>>();

    for guard in guards.into_iter().rev() {
        requirements = vec![BoundDependencyRequirement::guarded(guard, requirements)];
    }

    requirements
}

fn access_for_projection_prefix(
    storage: &StoragePlan,
    root: bray_bound_tree::StorageIdentityId,
    projections: &[StorageProjection],
) -> Option<StorageAccessId> {
    storage.access_entries().find_map(|(access, _)| {
        (storage.root_identity(access) == Some(root)
            && storage.resolved_projections(access) == Some(projections))
        .then_some(access)
    })
}

fn access_borrow(
    storage: &StoragePlan,
    access: StorageAccessId,
) -> Option<bray_bound_tree::BorrowCapabilityId> {
    match storage.access(access)?.root() {
        StorageAccessRoot::Borrow(capability) => Some(capability),
        StorageAccessRoot::Storage(_)
        | StorageAccessRoot::OwnedIndirection { .. }
        | StorageAccessRoot::Recovery(_) => None,
    }
}

fn requirement_kinds(
    purpose: StorageAccessPurpose,
) -> impl Iterator<Item = BoundDependencyRequirementKind> {
    let initialized = matches!(
        purpose,
        StorageAccessPurpose::Read
            | StorageAccessPurpose::Move
            | StorageAccessPurpose::Copy
            | StorageAccessPurpose::ValueTransfer
            | StorageAccessPurpose::Borrow(_)
            | StorageAccessPurpose::Member
            | StorageAccessPurpose::Index
            | StorageAccessPurpose::Slice
            | StorageAccessPurpose::Projection
    )
    .then_some(BoundDependencyRequirementKind::StorageInitialized);

    let exclusive = matches!(
        purpose,
        StorageAccessPurpose::Write
            | StorageAccessPurpose::Assignment
            | StorageAccessPurpose::Borrow(BorrowKind::Mutable)
    )
    .then_some(BoundDependencyRequirementKind::ExclusiveMutationAuthority);

    std::iter::once(BoundDependencyRequirementKind::StorageAlive)
        .chain(initialized)
        .chain(exclusive)
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundDependencyGuard, BoundDependencyRequirement, BoundErrorExpression, BoundExpression,
        BoundExpressionId, BoundNodeOrigin, BoundTreeBuilder, BoundUnitId, BoundUnitKind,
        StorageAccess, StorageAccessPurpose, StorageAccessRoot, StorageIdentity,
        StorageOperationDecision, StorageOperationStatus, StoragePlan, StoragePlanBuilder,
        StorageProjection,
    };
    use bray_symbols::{SymbolId, UnionPayloadFieldSymbolId, UnionVariantSymbolId};

    use super::operation_requirements;
    use crate::test_support::{error_type, push_expression, test_source_origins};

    #[test]
    fn nullable_projection_requirements_retain_presence_guards() {
        let (storage, expression, parent, projected) =
            projected_storage([StorageProjection::NullableValue]);

        let requirements = operation_requirements(&storage, &operation(expression, projected));

        let [BoundDependencyRequirement::Guarded(guarded)] = requirements.as_slice() else {
            panic!("nullable projection requirements must be guarded");
        };

        assert_eq!(
            guarded.guard(),
            BoundDependencyGuard::NullablePresent(parent)
        );
    }

    #[test]
    fn union_payload_requirements_retain_active_variant_guards() {
        let variant = UnionVariantSymbolId::from_symbol_id(SymbolId::new(41));
        let field = UnionPayloadFieldSymbolId::from_symbol_id(SymbolId::new(42));

        let projection = StorageProjection::ActiveUnionPayloadField { variant, field };
        let (storage, expression, parent, projected) = projected_storage([projection]);

        let requirements = operation_requirements(&storage, &operation(expression, projected));

        let [BoundDependencyRequirement::Guarded(guarded)] = requirements.as_slice() else {
            panic!("union payload requirements must be guarded");
        };

        assert_eq!(
            guarded.guard(),
            BoundDependencyGuard::ActiveUnionVariant {
                access: parent,
                variant,
            }
        );
    }

    fn projected_storage(
        projections: impl IntoIterator<Item = StorageProjection>,
    ) -> (
        StoragePlan,
        BoundExpressionId,
        bray_bound_tree::StorageAccessId,
        bray_bound_tree::StorageAccessId,
    ) {
        let unit = BoundUnitId::new(17);
        let source = test_source_origins()[0].source_anchor();
        let origin = BoundNodeOrigin::source(source);

        let mut tree = BoundTreeBuilder::new(unit);

        let expression = push_expression(
            &mut tree,
            BoundExpression::Error(BoundErrorExpression::new(origin, error_type())),
        );

        let mut builder = StoragePlanBuilder::new(unit, BoundUnitKind::RuntimeDefault);

        let storage = builder
            .push_identity(StorageIdentity::Temporary(expression))
            .unwrap_or_else(|error| panic!("test storage identity must build: {error:?}"));

        let parent = builder
            .push_access(StorageAccess::new(
                StorageAccessRoot::Storage(storage),
                [],
                error_type(),
                source,
                false,
            ))
            .unwrap_or_else(|error| panic!("test parent access must build: {error:?}"));

        let projected = builder
            .push_access(StorageAccess::new(
                StorageAccessRoot::Storage(storage),
                projections,
                error_type(),
                source,
                false,
            ))
            .unwrap_or_else(|error| panic!("test projected access must build: {error:?}"));

        (builder.finish(), expression, parent, projected)
    }

    const fn operation(
        expression: BoundExpressionId,
        access: bray_bound_tree::StorageAccessId,
    ) -> StorageOperationDecision {
        StorageOperationDecision::new(
            expression,
            StorageAccessPurpose::Read,
            access,
            None,
            StorageOperationStatus::Valid,
        )
    }
}
