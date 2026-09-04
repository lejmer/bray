use bray_symbols::SymbolOrdinal;

use super::StoragePlan;
use crate::{
    BoundExpressionId, BoundUnitId, BoundUnitKind, StorageAccess, StorageAccessId,
    StorageAccessPurpose, StorageAccessRoot, StorageIdentity, StorageIdentityId, StorageProjection,
    StorageRelationship,
};

#[test]
fn storage_plans_are_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<StoragePlan>();
}

#[test]
fn value_transfer_accepts_its_checked_copy_or_move_resolution() {
    assert!(
        StorageAccessPurpose::ValueTransfer.matches_checked(StorageAccessPurpose::ValueTransfer)
    );

    assert!(StorageAccessPurpose::ValueTransfer.matches_checked(StorageAccessPurpose::Copy));
    assert!(StorageAccessPurpose::ValueTransfer.matches_checked(StorageAccessPurpose::Move));

    assert!(!StorageAccessPurpose::Read.matches_checked(StorageAccessPurpose::Move));
}

#[test]
fn storage_relationships_distinguish_disjoint_and_overlapping_substorage() {
    let unit = BoundUnitId::new(4);
    let root = StorageIdentityId::from_slot(unit, 0);
    let source = crate::test_support::source_anchor();
    let ty = crate::test_support::error_type();

    let first = StorageAccess::new(
        StorageAccessRoot::Storage(root),
        [StorageProjection::TupleElement(SymbolOrdinal::new(0))],
        ty,
        source,
        false,
    );

    let second = StorageAccess::new(
        StorageAccessRoot::Storage(root),
        [StorageProjection::TupleElement(SymbolOrdinal::new(1))],
        ty,
        source,
        false,
    );

    let nested = StorageAccess::new(
        StorageAccessRoot::Storage(root),
        [
            StorageProjection::TupleElement(SymbolOrdinal::new(0)),
            StorageProjection::Element(BoundExpressionId::from_slot(unit, 0)),
        ],
        ty,
        source,
        false,
    );

    let mut builder = crate::StoragePlanBuilder::new(unit, BoundUnitKind::CallableBody);

    let root = builder
        .push_identity(StorageIdentity::Temporary(BoundExpressionId::from_slot(
            unit, 1,
        )))
        .unwrap_or_else(|error| panic!("test storage identity must build: {error:?}"));

    assert_eq!(root, StorageIdentityId::from_slot(unit, 0));

    for access in [first, second, nested] {
        builder
            .push_access(access)
            .unwrap_or_else(|error| panic!("test storage access must build: {error:?}"));
    }

    let plan = builder.finish();

    let first = StorageAccessId::from_slot(unit, 0);
    let second = StorageAccessId::from_slot(unit, 1);
    let nested = StorageAccessId::from_slot(unit, 2);

    assert_eq!(
        plan.relationship(first, second),
        StorageRelationship::Disjoint
    );

    assert_eq!(
        plan.relationship(first, nested),
        StorageRelationship::PotentiallyOverlapping
    );

    assert!(plan.access_contains(first, nested));
    assert!(!plan.access_contains(nested, first));
    assert!(!plan.access_contains(first, second));

    assert_eq!(
        plan.resolved_projections(nested),
        Some(
            [
                StorageProjection::TupleElement(SymbolOrdinal::new(0)),
                StorageProjection::Element(BoundExpressionId::from_slot(unit, 0)),
            ]
            .as_slice()
        )
    );

    assert_eq!(
        plan.relationship(first, StorageAccessId::from_slot(BoundUnitId::new(9), 0)),
        StorageRelationship::Error
    );
}

#[test]
fn unique_storage_is_disjoint_from_symbolic_storage() {
    let unit = BoundUnitId::new(5);
    let source = crate::test_support::source_anchor();
    let ty = crate::test_support::error_type();

    let parameter = StorageAccess::new(
        StorageAccessRoot::Storage(StorageIdentityId::from_slot(unit, 0)),
        [],
        ty,
        source,
        false,
    );

    let result = StorageAccess::new(
        StorageAccessRoot::Storage(StorageIdentityId::from_slot(unit, 1)),
        [],
        ty,
        source,
        false,
    );

    let mut builder = crate::StoragePlanBuilder::new(unit, BoundUnitKind::CallableBody);

    for identity in [
        StorageIdentity::Parameter(bray_symbols::CallableParameterSymbolId::from_symbol_id(
            bray_symbols::SymbolId::new(1),
        )),
        StorageIdentity::Result(crate::AnyBoundNodeId::Expression(
            BoundExpressionId::from_slot(unit, 0),
        )),
    ] {
        builder
            .push_identity(identity)
            .unwrap_or_else(|error| panic!("test storage identity must build: {error:?}"));
    }

    for access in [parameter, result] {
        builder
            .push_access(access)
            .unwrap_or_else(|error| panic!("test storage access must build: {error:?}"));
    }

    let plan = builder.finish();

    assert_eq!(
        plan.relationship(
            StorageAccessId::from_slot(unit, 0),
            StorageAccessId::from_slot(unit, 1)
        ),
        StorageRelationship::Disjoint
    );
}
