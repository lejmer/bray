use super::{Place, State, Target};
use crate::test_support::{error_type, expression_unit, push_expression};
use bray_bound_tree::{
    BoundErrorExpression, BoundExpression, BoundUnitId, BoundUnitKind, StorageIdentity,
    StoragePlanBuilder,
};
use bray_bound_tree::{
    BoundExpressionId, MemoryOperationStatus, MemoryReadKind, StorageProjection,
};
use std::collections::BTreeSet;
use std::sync::Arc;

fn places() -> ([Place; 4], Vec<BoundExpressionId>) {
    let (places, expressions, _) = borrowed_places();

    (places, expressions)
}

fn borrowed_places() -> (
    [Place; 4],
    Vec<BoundExpressionId>,
    bray_bound_tree::BorrowCapabilityId,
) {
    let unit = BoundUnitId::new(41);

    let (bound, expressions) = expression_unit(unit, |tree, origin| {
        (0..4)
            .map(|_| {
                push_expression(
                    tree,
                    BoundExpression::Error(BoundErrorExpression::new(origin, error_type())),
                )
            })
            .collect::<Vec<_>>()
    });

    let mut storage = StoragePlanBuilder::new(unit, BoundUnitKind::CallableBody);

    let places = expressions
        .iter()
        .map(|expression| {
            Place::root(
                storage
                    .push_identity(StorageIdentity::Temporary(*expression))
                    .expect("test storage"),
            )
        })
        .collect::<Vec<_>>();

    let source = bound
        .tree()
        .expression(expressions[0])
        .expect("fixture expression must exist in its bound unit")
        .origin()
        .source_anchor();

    let access = storage
        .push_access(bray_bound_tree::StorageAccess::new(
            bray_bound_tree::StorageAccessRoot::Storage(places[0].root),
            [],
            error_type(),
            source,
            false,
        ))
        .expect("fixture access must fit its identity space");

    let borrow = storage
        .push_borrow_capability(bray_bound_tree::PlannedBorrowCapability::new(
            bray_bound_tree::BorrowCapabilityOrigin::Operation((expressions[0]).into()),
            bray_symbols::BorrowKind::Shared,
            access,
            None,
            source,
            false,
        ))
        .expect("fixture borrow capability must fit its identity space");

    (
        places.try_into().expect("four test places"),
        expressions,
        borrow,
    )
}

#[test]
fn consuming_and_deallocating_through_an_alias_updates_the_original_target() {
    let ([source, alias, _, _], origins) = places();

    let mut state = State::default();
    let ty = error_type();

    state.fresh(source.clone(), origins[0]);
    state.transfer(&source, alias.clone());

    assert_eq!(
        state.write(&Target::storage(source.clone()), ty, origins[1], None),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        state.read(
            &Target::storage(alias.clone()),
            ty,
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        state.deallocate(&Target::storage(alias.clone()), origins[2]),
        MemoryOperationStatus::OutstandingObligations
    );

    assert_eq!(
        state
            .initialization_origins(&Target::storage(alias.clone()))
            .collect::<Vec<_>>(),
        vec![origins[1]]
    );

    assert_eq!(
        state.read(
            &Target::storage(alias.clone()),
            ty,
            MemoryReadKind::Move,
            None,
            None
        ),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        state.read(
            &Target::storage(source.clone()),
            ty,
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::UninitializedRawStorage
    );

    assert_eq!(
        state.deallocate(&Target::storage(source.clone()), origins[2]),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        state.write(&Target::storage(alias.clone()), ty, origins[3], None),
        MemoryOperationStatus::InvalidatedAllocation
    );

    assert_eq!(
        state
            .allocation_origins(&Target::storage(alias.clone()))
            .collect::<Vec<_>>(),
        vec![origins[0]]
    );

    assert_eq!(
        state
            .invalidation_origins(&Target::storage(alias.clone()))
            .collect::<Vec<_>>(),
        vec![origins[2]]
    );

    assert_eq!(state.contents.len(), 1);
}

#[test]
fn raw_copy_creates_independent_contents_and_preserves_its_source() {
    let ([source, destination, _, _], origins) = places();

    let mut state = State::default();
    let ty = error_type();

    state.fresh(source.clone(), origins[0]);
    state.fresh(destination.clone(), origins[1]);
    state.write(&Target::storage(source.clone()), ty, origins[0], None);

    assert_eq!(
        state.copy(
            &Target::storage(source.clone()),
            &Target::storage(destination.clone()),
            ty,
            origins[1],
            None
        ),
        MemoryOperationStatus::Valid
    );

    state.consume(&Target::storage(source.clone()), ty);

    assert_eq!(
        state.read(
            &Target::storage(destination.clone()),
            ty,
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        state.read(
            &Target::storage(source.clone()),
            ty,
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::UninitializedRawStorage
    );
}

#[test]
fn empty_range_copy_has_no_scalar_effect_and_nonempty_copy_keeps_obligations() {
    let ([source, destination, _, _], origins) = places();

    let mut state = State::default();
    let ty = error_type();

    state.fresh(source.clone(), origins[0]);
    state.fresh(destination.clone(), origins[1]);

    assert_eq!(
        state.copy(
            &Target::storage(source.clone()),
            &Target::storage(destination.clone()),
            ty,
            origins[1],
            Some(true)
        ),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        state.read(
            &Target::storage(destination.clone()),
            ty,
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::UninitializedRawStorage
    );

    assert_eq!(
        state.copy(
            &Target::storage(source.clone()),
            &Target::storage(destination.clone()),
            ty,
            origins[1],
            Some(false)
        ),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        state.copy(
            &Target::storage(source.clone()),
            &Target::storage(destination.clone()),
            ty,
            origins[1],
            Some(true)
        ),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        state.deallocate(&Target::storage(destination.clone()), origins[2]),
        MemoryOperationStatus::OutstandingObligations
    );

    state.deallocate(&Target::storage(source.clone()), origins[2]);

    assert_eq!(
        state.copy(
            &Target::storage(source.clone()),
            &Target::storage(destination.clone()),
            ty,
            origins[1],
            Some(true)
        ),
        MemoryOperationStatus::InvalidatedAllocation
    );
}

#[test]
fn trusted_consumption_uses_checked_authority_without_inventing_initialized_state() {
    let ([source, _, _, _], origins) = places();

    let mut state = State::default();

    assert_eq!(
        state.consume(&Target::storage(source.clone()), error_type()),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        state.read(
            &Target::storage(source.clone()),
            error_type(),
            MemoryReadKind::Copy,
            Some(origins[0]),
            None
        ),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        state.read(
            &Target::storage(source.clone()),
            error_type(),
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::UninitializedRawStorage
    );
}

#[test]
fn retiring_a_pointer_slot_keeps_the_contents_reached_by_surviving_aliases() {
    let ([source, alias, _, _], origins) = places();

    let mut state = State::default();

    state.fresh(source.clone(), origins[0]);

    state.write(
        &Target::storage(source.clone()),
        error_type(),
        origins[1],
        None,
    );

    state.transfer(&source, alias.clone());
    state.forget(source.root);
    assert!(!state.has_stored_value(&source));

    assert_eq!(
        state.read(
            &Target::storage(alias.clone()),
            error_type(),
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        state
            .allocation_origins(&Target::storage(alias.clone()))
            .collect::<Vec<_>>(),
        vec![origins[0]]
    );

    state.retain_live(&BTreeSet::new());
    assert!(!state.has_stored_value(&alias));
}

#[test]
fn reassignment_changes_the_slot_without_mutating_its_previous_target() {
    let ([source, alias, unknown, _], origins) = places();

    let mut state = State::default();

    state.fresh(source.clone(), origins[0]);

    state.write(
        &Target::storage(source.clone()),
        error_type(),
        origins[1],
        None,
    );

    state.transfer(&source, alias.clone());
    state.transfer(&unknown, source.clone());
    assert!(!state.has_stored_value(&source));

    assert_eq!(
        state.read(
            &Target::storage(alias.clone()),
            error_type(),
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::Valid
    );
}

#[test]
fn join_intersects_initialization_but_preserves_possible_obligations_and_origins() {
    let ([source, _, _, _], origins) = places();

    let mut left = State::default();

    left.fresh(source.clone(), origins[0]);

    let mut right = left.clone();

    left.write(
        &Target::storage(source.clone()),
        error_type(),
        origins[1],
        None,
    );

    assert!(left.merge(&right, &Default::default(), &Default::default()));
    assert!(!left.merge(&right, &Default::default(), &Default::default()));

    assert_eq!(
        left.read(
            &Target::storage(source.clone()),
            error_type(),
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::UninitializedRawStorage
    );

    assert_eq!(
        left.deallocate(&Target::storage(source.clone()), origins[2]),
        MemoryOperationStatus::OutstandingObligations
    );

    right.deallocate(&Target::storage(source.clone()), origins[2]);
    assert!(left.merge(&right, &Default::default(), &Default::default()));

    assert_eq!(
        left.read(
            &Target::storage(source.clone()),
            error_type(),
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::InvalidatedAllocation
    );

    assert_eq!(
        left.invalidation_origins(&Target::storage(source.clone()))
            .collect::<Vec<_>>(),
        vec![origins[2]]
    );
}

#[test]
fn joined_targets_require_all_possible_contents_and_weak_writes_do_not_initialize_both() {
    let ([first, second, alias, _], origins) = places();

    let mut left = State::default();

    left.fresh(first.clone(), origins[0]);
    left.fresh(second.clone(), origins[1]);
    left.transfer(&first, alias.clone());

    let mut right = left.clone();

    right.transfer(&second, alias.clone());
    assert!(left.merge(&right, &Default::default(), &Default::default()));

    left.write(
        &Target::storage(alias.clone()),
        error_type(),
        origins[2],
        None,
    );

    for target in [&first, &second, &alias] {
        assert_eq!(
            left.read(
                &Target::storage(target.clone()),
                error_type(),
                MemoryReadKind::Copy,
                None,
                None
            ),
            MemoryOperationStatus::UninitializedRawStorage
        );

        assert_eq!(
            left.deallocate(&Target::storage(target.clone()), origins[3]),
            MemoryOperationStatus::OutstandingObligations
        );
    }

    left.write(
        &Target::storage(first.clone()),
        error_type(),
        origins[2],
        None,
    );

    left.write(
        &Target::storage(second.clone()),
        error_type(),
        origins[2],
        None,
    );

    assert_eq!(
        left.read(
            &Target::storage(alias.clone()),
            error_type(),
            MemoryReadKind::Move,
            None,
            None
        ),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        left.read(
            &Target::storage(first.clone()),
            error_type(),
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::UninitializedRawStorage
    );

    assert_eq!(
        left.deallocate(&Target::storage(first.clone()), origins[3]),
        MemoryOperationStatus::OutstandingObligations
    );
}

#[test]
fn repeated_creation_separates_old_aliases_and_has_a_finite_summary() {
    let ([source, alias, _, _], origins) = places();

    let mut state = State::default();

    state.fresh(source.clone(), origins[0]);

    state.write(
        &Target::storage(source.clone()),
        error_type(),
        origins[1],
        None,
    );

    state.transfer(&source, alias.clone());
    state.fresh(source.clone(), origins[0]);

    state.write(
        &Target::storage(source.clone()),
        error_type(),
        origins[1],
        None,
    );

    assert_eq!(
        state.read(
            &Target::storage(alias.clone()),
            error_type(),
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::UninitializedRawStorage
    );

    state.consume(&Target::storage(alias.clone()), error_type());

    assert_eq!(
        state.deallocate(&Target::storage(alias.clone()), origins[2]),
        MemoryOperationStatus::OutstandingObligations
    );

    assert_eq!(
        state.read(
            &Target::storage(source.clone()),
            error_type(),
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::Valid
    );

    let stable = state.clone();

    state.fresh(source.clone(), origins[0]);

    state.write(
        &Target::storage(source.clone()),
        error_type(),
        origins[1],
        None,
    );

    assert_eq!(state, stable);
    assert_eq!(state.contents.len(), 2);
}

#[test]
fn nested_address_observes_the_reference_slot_and_follows_reassignment() {
    let ([source, other, address, result], origins) = places();

    let mut state = State::default();

    state.fresh(source.clone(), origins[0]);
    state.fresh(other.clone(), origins[1]);

    state.write(
        &Target::storage(source.clone()),
        error_type(),
        origins[0],
        None,
    );

    state.address(source.clone(), address.clone(), error_type(), origins[2]);

    assert_eq!(
        state.read(
            &Target::storage(address.clone()),
            error_type(),
            MemoryReadKind::Copy,
            None,
            Some(result.clone())
        ),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        state.read(
            &Target::storage(result.clone()),
            error_type(),
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::Valid
    );

    state.transfer(&other, source.clone());

    state.read(
        &Target::storage(address.clone()),
        error_type(),
        MemoryReadKind::Copy,
        None,
        Some(result.clone()),
    );

    assert_eq!(
        state.read(
            &Target::storage(result.clone()),
            error_type(),
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::UninitializedRawStorage
    );
}

#[test]
fn origin_join_unions_and_deduplicates_diagnostic_causes() {
    let ([source, _, _, _], origins) = places();

    let mut left = State::default();

    left.fresh(source.clone(), origins[0]);

    let mut right = left.clone();

    left.write(
        &Target::storage(source.clone()),
        error_type(),
        origins[1],
        None,
    );

    right.write(
        &Target::storage(source.clone()),
        error_type(),
        origins[2],
        None,
    );

    left.merge(&right, &Default::default(), &Default::default());

    assert_eq!(
        left.initialization_origins(&Target::storage(source.clone()))
            .collect::<Vec<_>>(),
        vec![origins[1], origins[2]]
    );

    assert_eq!(
        left.read(
            &Target::storage(source.clone()),
            error_type(),
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::Valid
    );

    assert!(!left.merge(&right, &Default::default(), &Default::default()));
}

#[test]
fn field_writes_invalidate_the_parent_but_preserve_siblings() {
    let ([aggregate, parent, left_pointer, right_pointer], origins) = places();

    let left = Place {
        root: aggregate.root,
        projections: Arc::from([StorageProjection::TupleElement(
            bray_symbols::SymbolOrdinal::new(0),
        )]),
    };

    let right = Place {
        root: aggregate.root,
        projections: Arc::from([StorageProjection::TupleElement(
            bray_symbols::SymbolOrdinal::new(1),
        )]),
    };

    let mut state = State::default();

    state.address(aggregate, parent.clone(), error_type(), origins[0]);
    state.address(left, left_pointer.clone(), error_type(), origins[1]);
    state.address(right, right_pointer.clone(), error_type(), origins[2]);

    state.write(
        &Target::storage(left_pointer.clone()),
        error_type(),
        origins[3],
        None,
    );

    assert_eq!(
        state.read(
            &Target::storage(parent.clone()),
            error_type(),
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::UninitializedRawStorage
    );

    assert_eq!(
        state.read(
            &Target::storage(left_pointer.clone()),
            error_type(),
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        state.read(
            &Target::storage(right_pointer.clone()),
            error_type(),
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::Valid
    );

    state.consume(&Target::storage(left_pointer.clone()), error_type());

    assert_eq!(
        state.read(
            &Target::storage(right_pointer.clone()),
            error_type(),
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::Valid
    );
}

#[test]
fn whole_value_transfer_retains_reference_fields_and_replaces_old_fields() {
    let ([aggregate, destination, allocation, unknown], origins) = places();

    let field = |place: &Place| Place {
        root: place.root,
        projections: Arc::from([StorageProjection::TupleElement(
            bray_symbols::SymbolOrdinal::new(0),
        )]),
    };

    let mut state = State::default();

    state.fresh(allocation.clone(), origins[0]);
    state.transfer(&allocation, field(&aggregate));
    state.transfer(&aggregate, destination.clone());

    state.write(
        &Target::storage(allocation.clone()),
        error_type(),
        origins[1],
        None,
    );

    assert_eq!(
        state.read(
            &Target::storage(field(&destination)),
            error_type(),
            MemoryReadKind::Move,
            None,
            None
        ),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        state.read(
            &Target::storage(field(&aggregate)),
            error_type(),
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::UninitializedRawStorage
    );

    state.transfer(&unknown, destination.clone());
    assert!(!state.has_stored_value(&destination));
}

#[test]
fn recreated_local_does_not_revive_a_pointer_to_its_previous_incarnation() {
    let ([local, old, current, _], origins) = places();

    let mut state = State::default();

    state.address(local.clone(), old.clone(), error_type(), origins[0]);
    state.begin(local.root);
    state.address(local, current.clone(), error_type(), origins[1]);

    assert_eq!(
        state.read(
            &Target::storage(old.clone()),
            error_type(),
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::UninitializedRawStorage
    );

    assert_eq!(
        state.read(
            &Target::storage(current.clone()),
            error_type(),
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::Valid
    );
}

#[test]
fn offset_retains_allocation_identity_without_claiming_base_contents() {
    let ([allocation, interior, _, _], origins) = places();

    let mut state = State::default();

    state.fresh(allocation.clone(), origins[0]);

    state.write(
        &Target::storage(allocation.clone()),
        error_type(),
        origins[1],
        None,
    );

    state.offset(&Target::storage(allocation.clone()), interior.clone());

    assert_eq!(
        state.read(
            &Target::storage(interior.clone()),
            error_type(),
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::UninitializedRawStorage
    );

    assert_eq!(
        state.deallocate(&Target::storage(interior.clone()), origins[2]),
        MemoryOperationStatus::OutstandingObligations
    );

    state.consume(&Target::storage(allocation.clone()), error_type());
    state.deallocate(&Target::storage(allocation.clone()), origins[2]);

    assert_eq!(
        state.write(
            &Target::storage(interior.clone()),
            error_type(),
            origins[3],
            None
        ),
        MemoryOperationStatus::InvalidatedAllocation
    );
}

#[test]
fn checked_read_evidence_retains_cleanup_obligations() {
    let ([source, _, _, _], origins) = places();

    let mut state = State::default();

    state.fresh(source.clone(), origins[0]);

    assert_eq!(
        state.read(
            &Target::storage(source.clone()),
            error_type(),
            MemoryReadKind::Copy,
            Some(origins[1]),
            None
        ),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        state.deallocate(&Target::storage(source.clone()), origins[2]),
        MemoryOperationStatus::OutstandingObligations
    );

    assert_eq!(
        state
            .initialization_origins(&Target::storage(source.clone()))
            .collect::<Vec<_>>(),
        vec![origins[1]]
    );
}

#[test]
fn an_unresolved_stored_pointer_cannot_prove_disjointness() {
    let ([allocation, loaded, _, _], origins) = places();

    let mut state = State::default();

    state.fresh(allocation.clone(), origins[0]);

    state.write(
        &Target::storage(allocation.clone()),
        error_type(),
        origins[1],
        None,
    );

    state.read(
        &Target::storage(allocation.clone()),
        error_type(),
        MemoryReadKind::Copy,
        None,
        Some(loaded.clone()),
    );

    state.write(
        &Target::storage(loaded.clone()),
        error_type(),
        origins[2],
        None,
    );

    assert_eq!(
        state.read(
            &Target::storage(allocation.clone()),
            error_type(),
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::UninitializedRawStorage
    );
}

#[test]
fn dynamic_selectors_keep_unknown_locations_without_equating_evaluations() {
    let ([aggregate, pointer, replacement, loaded], origins) = places();

    let element = Place {
        root: aggregate.root,
        projections: Arc::from([StorageProjection::Element(origins[0])]),
    };

    let mut state = State::default();

    state.address(element.clone(), pointer.clone(), error_type(), origins[1]);

    assert_eq!(
        state.read(
            &Target::storage(pointer.clone()),
            error_type(),
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::UninitializedRawStorage
    );

    state.fresh(replacement.clone(), origins[2]);
    state.transfer(&replacement, element.clone());
    state.transfer(&element, loaded.clone());
    assert!(state.references(&Target::storage(loaded.clone())).unknown);
}
#[test]
fn direct_moves_and_reinitialization_change_existing_pointer_targets() {
    let ([owner, pointer, alias, _], origins) = places();

    let mut state = State::default();
    let ty = error_type();

    state.address(owner.clone(), pointer.clone(), ty, origins[0]);
    state.transfer(&pointer, alias.clone());
    state.consume_place(owner.clone(), ty);

    assert_eq!(
        state.read(
            &Target::storage(alias.clone()),
            ty,
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::UninitializedRawStorage
    );

    state.write_place(owner.clone(), ty, origins[1]);

    assert_eq!(
        state.read(
            &Target::storage(alias.clone()),
            ty,
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::Valid
    );

    state.begin(owner.root);
    state.write_place(owner, ty, origins[2]);

    assert_eq!(
        state.read(
            &Target::storage(alias.clone()),
            ty,
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::UninitializedRawStorage
    );
}

#[test]
fn reinterpreted_views_do_not_relabel_the_original_typed_storage() {
    let ([owner, pointer, alias, _], origins, capability) = borrowed_places();

    let mut state = State::default();
    let ty = error_type();

    let other = crate::test_support::semantic_values()
        .intern_type(bray_symbols::TypeData::Borrow {
            kind: bray_symbols::BorrowKind::Shared,
            target: ty,
        })
        .expect("fixture type must be interned");

    state.address(owner.clone(), pointer.clone(), ty, origins[0]);
    state.transfer(&pointer, alias.clone());
    assert_eq!(state.referent(&alias, ty), Some(owner));
    assert_eq!(state.referent(&alias, other), None);

    assert_eq!(
        state.reached_storage(&Target::storage(alias.clone()))[0].1,
        Some(ty)
    );

    state.begin_borrow(capability, state.referent(&alias, ty).expect("fixture `referent` call must succeed"), other);
    state.retain_borrow(capability, alias.clone());
    assert_eq!(state.referent(&alias, other), None);

    assert_eq!(
        state.reached_storage(&Target::storage(alias.clone()))[0].1,
        Some(ty)
    );
}

#[test]
fn retained_borrow_values_share_targets_and_outlive_expression_capabilities() {
    let ([source, alias, _, _], _, capability) = borrowed_places();

    let mut state = State::default();

    state.begin_borrow(capability, source.clone(), error_type());
    state.retain_borrow(capability, alias.clone());
    state.end_borrows(&BTreeSet::from([capability]));

    assert!(!state.has_borrow(capability));
    assert_eq!(state.referent(&alias, error_type()), Some(source.clone()));
    assert!(!state.initialized(&Target::storage(alias.clone()), error_type()));

    state.begin(source.root);
    assert_eq!(state.referent(&alias, error_type()), None);
}

#[test]
fn joined_borrow_capabilities_require_one_current_target_on_every_path() {
    let ([source, other, alias, _], _, capability) = borrowed_places();

    let mut state = State::default();

    state.begin_borrow(capability, source.clone(), error_type());

    for incoming in [Some(source.clone()), Some(other), None] {
        let mut incoming_state = State::default();

        if let Some(place) = &incoming {
            incoming_state.begin_borrow(capability, place.clone(), error_type());
        }

        let mut joined = state.clone();

        joined.merge(&incoming_state, &Default::default(), &Default::default());
        assert!(!joined.merge(&incoming_state, &Default::default(), &Default::default()));
        joined.retain_borrow(capability, alias.clone());

        assert_eq!(
            joined.referent(&alias, error_type()),
            (incoming.as_ref() == Some(&source)).then(|| source.clone())
        );
    }
}

#[test]
fn opaque_mutation_forgets_stored_references_without_retargeting_live_borrows() {
    let ([source, slot, alias, _], origins, capability) = borrowed_places();

    let mut state = State::default();

    state.address(source.clone(), slot.clone(), error_type(), origins[0]);
    state.begin_borrow(capability, source.clone(), error_type());

    let storage =
        StoragePlanBuilder::new(BoundUnitId::new(41), BoundUnitKind::CallableBody).finish();

    state.invalidate(
        &bray_bound_tree::StorageInvalidation::unknown(),
        &storage,
        bray_symbols::DependencyMutationKind::Retire,
    );

    assert!(!state.has_stored_value(&slot));
    state.retain_borrow(capability, alias.clone());
    assert_eq!(state.referent(&alias, error_type()), Some(source));
    assert!(!state.initialized(&Target::storage(alias.clone()), error_type()));
}

#[test]
fn call_results_keep_entry_reference_identity_across_slot_reassignment() {
    let ([first, argument, second, result], expressions) = places();

    let call = expressions[0].into();
    let mut state = State::default();

    state.address(
        first.clone(),
        argument.clone(),
        error_type(),
        expressions[0],
    );

    state.begin_transition(call);
    state.stage_slot(call, &argument, result.clone());

    state.address(
        second.clone(),
        argument.clone(),
        error_type(),
        expressions[1],
    );

    state.complete_transition(call);
    assert_eq!(state.referent(&argument, error_type()), Some(second));
    assert_eq!(state.referent(&result, error_type()), Some(first));
}

#[test]
fn call_entry_snapshots_do_not_restore_consumed_or_retired_contents() {
    let ([owner, argument, result, _], expressions) = places();

    let call = expressions[0].into();
    let mut state = State::default();

    state.address(
        owner.clone(),
        argument.clone(),
        error_type(),
        expressions[0],
    );

    state.begin_transition(call);
    state.stage_slot(call, &argument, result.clone());

    state.read(
        &Target::storage(argument.clone()),
        error_type(),
        MemoryReadKind::Move,
        Some(expressions[1]),
        None,
    );

    state.complete_transition(call);
    assert_eq!(state.referent(&result, error_type()), Some(owner.clone()));
    assert!(!state.initialized(&Target::storage(result.clone()), error_type()));
    state.begin_transition(call);
    state.stage_slot(call, &argument, result.clone());
    state.begin(owner.root);
    state.complete_transition(call);
    assert_eq!(state.referent(&result, error_type()), None);
}

#[test]
fn a_call_snapshot_present_on_only_one_path_has_no_must_target() {
    let ([owner, argument, result, _], expressions) = places();

    let call = expressions[0].into();
    let mut state = State::default();

    state.address(owner, argument.clone(), error_type(), expressions[0]);
    state.begin_transition(call);
    state.stage_slot(call, &argument, result.clone());

    let mut missing = State::default();

    missing.merge(&state, &Default::default(), &Default::default());
    state.merge(&State::default(), &Default::default(), &Default::default());

    for mut joined in [state, missing] {
        joined.complete_transition(call);
        assert_eq!(joined.referent(&result, error_type()), None);
    }
}

#[test]
fn original_entry_references_survive_slot_reassignment_without_proving_disjointness() {
    let ([first, second, captured, _], origins) = places();

    let mut state = State::default();

    state.entry_reference(first.clone());
    state.entry_reference(second.clone());
    state.transfer(&first, captured.clone());
    state.transfer(&second, first.clone());

    let exact = state.exact_references(&captured).collect::<Vec<_>>();

    assert_eq!(
        exact,
        vec![(&captured, &super::model::Origin::entry(first.clone()))]
    );

    state.write(
        &Target::storage(first.clone()),
        error_type(),
        origins[0],
        None,
    );

    assert!(
        state.has_stored_value(&second),
        "writing through an input does not rebind another by-value parameter slot"
    );

    assert!(
        state.has_stored_value(&captured),
        "overwriting an input slot does not rewrite a copied reference value"
    );

    assert!(
        state
            .located_storage(&Target::storage(captured.clone()))
            .is_none(),
        "original value identity does not locate its target"
    );
}

#[test]
fn absent_payload_is_ignored_but_unknown_or_moved_payload_blocks_exact_identity() {
    let ([source, payload, _, _], _) = places();

    for absence in [
        None,
        Some(crate::analysis::storage_flow::model::StorageAbsence::NoPayload),
        Some(crate::analysis::storage_flow::model::StorageAbsence::Moved),
    ] {
        for present_first in [false, true] {
            let mut present = State::default();

            present.entry_reference(source.clone());
            present.transfer(&source, payload.clone());

            let absent = State::default();

            let absent_places = absence
                .map(|reason| (payload.clone(), reason))
                .into_iter()
                .collect();

            let empty = Default::default();

            let mut joined = if present_first {
                present.clone()
            } else {
                absent.clone()
            };

            if present_first {
                joined.merge(&absent, &empty, &absent_places);
            } else {
                joined.merge(&present, &absent_places, &empty);
            }

            assert_eq!(
                joined.exact_references(&payload).count(),
                usize::from(
                    absence
                        == Some(crate::analysis::storage_flow::model::StorageAbsence::NoPayload)
                )
            );
        }
    }
}

#[test]
fn pending_payload_absence_survives_source_retirement_without_proving_unknown_origins() {
    let ([source, payload, _, _], origins) = places();

    let call = origins[0].into();
    let empty = Default::default();

    for absent in [false, true] {
        for present_first in [false, true] {
            let mut present = State::default();

            present.entry_reference(source.clone());
            present.stage_slot(call, &source, payload.clone());
            present.forget(source.root);

            let mut other = State::default();

            if absent {
                other.stage_null(call, Target::storage(payload.clone()));
            } else {
                other.stage_unknown(call, Target::storage(payload.clone()));
            }

            let mut joined = if present_first {
                present.merge(&other, &empty, &empty);

                present
            } else {
                other.merge(&present, &empty, &empty);

                other
            };

            joined.complete_transition(call);

            assert_eq!(
                joined
                    .referenced_regions(&Target::storage(payload.clone()))
                    .is_some(),
                absent
            );

            assert_eq!(joined.exact_references(&payload).count(), 0);
        }
    }
}

#[test]
fn exposed_entry_reference_keeps_its_origin_without_claiming_a_physical_place() {
    let ([source, pointer, _, _], origins) = places();

    let mut state = State::default();

    state.entry_reference(source.clone());
    state.transfer(&source, pointer.clone());
    state.expose_address(&pointer, error_type(), origins[0]);

    assert_eq!(
        state.exact_references(&pointer).collect::<Vec<_>>(),
        vec![(&pointer, &super::model::Origin::entry(source))]
    );

    assert!(
        state
            .located_storage(&Target::storage(pointer.clone()))
            .is_none()
    );

    assert_eq!(
        state.read(
            &Target::storage(pointer.clone()),
            error_type(),
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::Valid
    );
}
#[test]
fn entry_borrow_targets_are_not_parameter_slots() {
    let ([parameter, copy, _, _], _, capability) = borrowed_places();

    let mut state = State::default();

    state.begin_entry_borrow(capability, parameter.clone(), error_type());
    state.retain_borrow(capability, copy.clone());
    assert_eq!(state.referent(&copy, error_type()), None);
    assert!(state.borrowed_storage(capability).is_none());
    assert!(!state.initialized(&Target::storage(copy.clone()), error_type()));
    state.begin(parameter.root);

    assert!(
        state.borrow_target(capability, error_type()).is_some(),
        "a parameter slot's epoch is distinct from the original referent"
    );
}

#[test]
fn projected_entry_address_does_not_read_the_reference_held_in_that_field() {
    let ([parameter, field_reference, address, copied], _, capability) = borrowed_places();

    let mut state = State::default();
    let ty = error_type();

    state.begin_entry_borrow(capability, parameter.clone(), ty);

    let projection =
        bray_bound_tree::StorageProjection::TupleElement(bray_symbols::SymbolOrdinal::new(0));

    let target = state
        .borrow_target(capability, ty)
        .expect("fixture `borrow_target` call must succeed")
        .project(&[projection]);

    let slot = Place {
        root: parameter.root,
        projections: [projection].into(),
    };

    state.entry_reference_at(slot.clone(), 1, target.clone(), Some(ty));
    state.transfer_value(&target, field_reference.clone());
    state.borrow_target_storage(capability, target, ty);
    state.retain_borrow(capability, address.clone());
    state.transfer(&address, copied.clone());

    assert_eq!(
        state.exact_references(&field_reference).next().expect("fixture iterator must provide its expected item").1,
        &super::model::Origin::entry(slot)
    );

    assert_ne!(
        state.exact_references(&address).next().expect("fixture iterator must provide its expected item").1,
        state.exact_references(&field_reference).next().expect("fixture iterator must provide its expected item").1
    );

    state.clear(&parameter);
    assert_eq!(state.exact_references(&copied).count(), 1);

    assert!(
        state
            .located_storage(&Target::storage(copied.clone()))
            .is_none()
    );
}

#[test]
fn entry_targets_alias_other_inputs_but_distinguish_fields_of_the_same_referent() {
    let ([first, second, _, _], _) = places();

    let target = |reference| super::model::Target {
        origin: super::model::Origin::entry(reference),
        previous: false,
    };

    let field = |index| {
        bray_bound_tree::StorageProjection::TupleElement(bray_symbols::SymbolOrdinal::new(index))
    };

    let first = target(first);
    let second = target(second);

    assert_eq!(
        first.relationship(&second),
        bray_bound_tree::StorageRelationship::PotentiallyOverlapping
    );

    assert_eq!(
        first
            .project(&[field(0)])
            .relationship(&first.project(&[field(1)])),
        bray_bound_tree::StorageRelationship::Disjoint
    );

    assert_eq!(
        first.relationship(&first.project(&[field(0)])),
        bray_bound_tree::StorageRelationship::PotentiallyOverlapping
    );

    assert_eq!(
        first
            .project(&[field(0)])
            .relationship(&second.project(&[field(1)])),
        bray_bound_tree::StorageRelationship::PotentiallyOverlapping
    );
}

#[test]
fn previous_epoch_cells_do_not_interrupt_current_subtree_transfers() {
    let ([source, destination, pointer, _], _) = places();

    let mut state = State::default();
    let projection = StorageProjection::TupleElement(bray_symbols::SymbolOrdinal::new(0));

    let field = Place {
        root: source.root,
        projections: [projection].into(),
    };

    state.entry_reference(field.clone());
    state.entry_reference(source.clone());

    state.values.insert(
        super::model::Target {
            previous: true,
            origin: super::model::Origin::Storage(source.clone()),
        },
        super::model::References::default(),
    );

    state.transfer(&source, destination.clone());
    assert_eq!(state.exact_references(&destination).count(), 2);
    state.transfer(&destination, pointer.clone());
    assert_eq!(state.exact_references(&pointer).count(), 2);
    state.clear(&source);
    assert!(!state.has_stored_value(&field));
}

#[test]
fn projected_entry_selectors_do_not_establish_exact_initialization() {
    let ([parameter, pointer, _, _], origins, capability) = borrowed_places();

    let mut state = State::default();

    state.begin_entry_borrow(capability, parameter, error_type());

    let selected = state
        .borrow_target(capability, error_type())
        .expect("fixture `borrow_target` call must succeed")
        .project(&[StorageProjection::Element(origins[0])]);

    state.borrow_target_storage(capability, selected, error_type());
    state.retain_borrow(capability, pointer.clone());
    state.expose_address(&pointer, error_type(), origins[1]);
    assert_eq!(state.exact_references(&pointer).count(), 0);
    assert!(!state.initialized(&Target::storage(pointer.clone()), error_type()));
}

#[test]
fn aggregate_mutation_revokes_reference_fields_without_address_metadata() {
    let ([aggregate, source, _, _], _) = places();

    let field = Place {
        root: aggregate.root,
        projections: [StorageProjection::TupleElement(
            bray_symbols::SymbolOrdinal::new(0),
        )]
        .into(),
    };

    let mut state = State::default();

    state.entry_reference(source.clone());
    state.transfer(&source, field.clone());
    assert!(state.has_stored_value(&field));

    assert!(
        !state
            .contents
            .contains_key(&super::model::Target::storage(field.clone()))
    );

    state.invalidate_target(
        &super::model::Target::storage(aggregate),
        bray_symbols::DependencyMutationKind::Write,
    );

    assert!(!state.has_stored_value(&field));
    assert!(state.has_stored_value(&source));
}

#[test]
fn protected_payload_address_does_not_prove_initialization() {
    let ([owner, pointer, _, _], _) = places();

    let mut state = State::default();
    let payload = super::Target::storage(owner).project(&[StorageProjection::UninitPayload]);

    state.uninit(payload.clone(), error_type());

    state.point_at(
        payload,
        pointer.clone(),
        error_type(),
        bray_symbols::DependencyReferenceKind::Address,
    );

    assert_eq!(
        state.read(
            &Target::storage(pointer.clone()),
            error_type(),
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::UninitializedRawStorage
    );
}

#[test]
fn whole_value_transfer_moves_inline_payload_facts_without_retargeting_pointers() {
    let ([owner, destination, old_pointer, new_pointer], origins) = places();

    let mut state = State::default();
    let field = StorageProjection::TupleElement(bray_symbols::SymbolOrdinal::new(0));
    let source = super::Target::storage(owner.clone());
    let payload = source.project(&[field, StorageProjection::UninitPayload]);

    state.uninit(payload.clone(), error_type());
    state.write_value(payload.clone(), error_type(), origins[0], None);

    state.point_at(
        payload.clone(),
        old_pointer.clone(),
        error_type(),
        bray_symbols::DependencyReferenceKind::Address,
    );

    let occurrence = origins[1].into();

    state.begin_transition(occurrence);

    state.stage_value(
        occurrence,
        &source,
        super::Target::storage(destination.clone()),
    );

    state.consume_target(source, error_type());
    assert!(!state.initialized(&Target::storage(old_pointer.clone()), error_type()));
    state.complete_transition(occurrence);
    state.finish_value_transfers();

    let moved =
        super::Target::storage(destination).project(&[field, StorageProjection::UninitPayload]);

    state.point_at(
        moved,
        new_pointer.clone(),
        error_type(),
        bray_symbols::DependencyReferenceKind::Address,
    );

    assert!(state.initialized(&Target::storage(new_pointer.clone()), error_type()));
    assert!(!state.initialized(&Target::storage(old_pointer.clone()), error_type()));
    assert!(state.contents[&payload].pending.is_empty());

    assert_eq!(
        state.read(
            &Target::storage(new_pointer.clone()),
            error_type(),
            MemoryReadKind::Move,
            None,
            None
        ),
        MemoryOperationStatus::Valid
    );

    assert!(!state.initialized(&Target::storage(new_pointer.clone()), error_type()));
}

#[test]
fn reusing_inline_storage_does_not_revive_old_payload_pointers() {
    let ([owner, old_pointer, new_pointer, _], origins) = places();

    let mut state = State::default();

    let payload =
        super::Target::storage(owner.clone()).project(&[StorageProjection::UninitPayload]);

    state.uninit(payload.clone(), error_type());
    state.write_value(payload.clone(), error_type(), origins[0], None);

    state.point_at(
        payload.clone(),
        old_pointer.clone(),
        error_type(),
        bray_symbols::DependencyReferenceKind::Address,
    );

    state.begin(owner.root);
    state.uninit(payload.clone(), error_type());
    state.write_value(payload.clone(), error_type(), origins[1], None);

    state.point_at(
        payload,
        new_pointer.clone(),
        error_type(),
        bray_symbols::DependencyReferenceKind::Address,
    );

    assert!(state.initialized(&Target::storage(new_pointer.clone()), error_type()));
    assert!(!state.initialized(&Target::storage(old_pointer.clone()), error_type()));
}

#[test]
fn constructed_aggregate_and_return_transfer_the_evaluated_payload() {
    let ([owner, aggregate, result, pointer], origins) = places();

    let mut state = State::default();

    let payload =
        super::Target::storage(owner.clone()).project(&[StorageProjection::UninitPayload]);

    state.uninit(payload.clone(), error_type());
    state.write_value(payload, error_type(), origins[0], None);

    let field = StorageProjection::TupleElement(bray_symbols::SymbolOrdinal::new(0));
    let occurrence = origins[1].into();

    state.stage_value(
        occurrence,
        &super::Target::storage(owner.clone()),
        super::Target::storage(aggregate.clone()).project(&[field]),
    );

    state.complete_transition(occurrence);

    state.stage_value(
        occurrence,
        &super::Target::storage(aggregate),
        super::Target::storage(result.clone()),
    );

    state.consume_target(super::Target::storage(owner), error_type());
    state.complete_transition(occurrence);

    state.point_at(
        super::Target::storage(result).project(&[field, StorageProjection::UninitPayload]),
        pointer.clone(),
        error_type(),
        bray_symbols::DependencyReferenceKind::Address,
    );

    assert!(state.initialized(&Target::storage(pointer.clone()), error_type()));
}

#[test]
fn consumed_inline_value_transfers_after_borrow_plan_completion() {
    let ([owner, destination, pointer, _], origins) = places();

    let mut state = State::default();

    let payload =
        super::Target::storage(owner.clone()).project(&[StorageProjection::UninitPayload]);

    state.uninit(payload.clone(), error_type());
    state.write_value(payload, error_type(), origins[0], None);
    state.consume_target(super::Target::storage(owner.clone()), error_type());
    state.transfer(&owner, destination.clone());
    state.finish_value_transfers();

    state.point_at(
        super::Target::storage(destination).project(&[StorageProjection::UninitPayload]),
        pointer.clone(),
        error_type(),
        bray_symbols::DependencyReferenceKind::Address,
    );

    assert!(state.initialized(&Target::storage(pointer.clone()), error_type()));
    assert!(state.evaluated.is_empty());
}

#[test]
fn alternative_payload_sources_require_initialization_on_every_path() {
    let ([first, second, result, pointer], origins) = places();

    let mut state = State::default();

    let first_payload =
        super::Target::storage(first.clone()).project(&[StorageProjection::UninitPayload]);

    let second_payload =
        super::Target::storage(second.clone()).project(&[StorageProjection::UninitPayload]);

    state.uninit(first_payload.clone(), error_type());
    state.uninit(second_payload, error_type());
    state.write_value(first_payload, error_type(), origins[0], None);

    let occurrence = origins[1].into();

    state.stage_value(
        occurrence,
        &super::Target::storage(first),
        super::Target::storage(result.clone()),
    );

    state.stage_value(
        occurrence,
        &super::Target::storage(second),
        super::Target::storage(result.clone()),
    );

    state.complete_transition(occurrence);

    state.point_at(
        super::Target::storage(result).project(&[StorageProjection::UninitPayload]),
        pointer.clone(),
        error_type(),
        bray_symbols::DependencyReferenceKind::Address,
    );

    assert!(!state.initialized(&Target::storage(pointer.clone()), error_type()));
}

#[test]
fn allocation_reuse_retires_addresses_into_nested_inline_payloads() {
    let ([allocation, pointer, new_pointer, _], origins) = places();

    let mut state = State::default();

    state.fresh(allocation.clone(), origins[0]);

    let target = state
        .value_targets(&super::Target::storage(allocation.clone()))
        .expect("fixture `value_targets` call must succeed")
        .pop()
        .expect("fixture `pop` call must succeed");

    let payload = target.project(&[StorageProjection::UninitPayload]);

    state.uninit(payload.clone(), error_type());
    state.write_value(payload.clone(), error_type(), origins[1], None);

    state.point_at(
        payload.clone(),
        pointer.clone(),
        error_type(),
        bray_symbols::DependencyReferenceKind::Address,
    );

    state.fresh(allocation, origins[0]);
    state.uninit(payload.clone(), error_type());
    state.write_value(payload.clone(), error_type(), origins[2], None);

    state.point_at(
        payload,
        new_pointer.clone(),
        error_type(),
        bray_symbols::DependencyReferenceKind::Address,
    );

    assert!(!state.initialized(&Target::storage(pointer.clone()), error_type()));
    assert!(state.initialized(&Target::storage(new_pointer.clone()), error_type()));
}

#[test]
fn replacing_inline_storage_does_not_revalidate_a_pointer_to_its_old_contents() {
    let ([first, replacement, pointer, new_pointer], origins) = places();

    let mut state = State::default();

    let payload =
        super::Target::storage(first.clone()).project(&[StorageProjection::UninitPayload]);

    state.uninit(payload.clone(), error_type());
    state.write_value(payload.clone(), error_type(), origins[0], None);

    state.point_at(
        payload.clone(),
        pointer.clone(),
        error_type(),
        bray_symbols::DependencyReferenceKind::Address,
    );

    let replacement_payload =
        super::Target::storage(replacement.clone()).project(&[StorageProjection::UninitPayload]);

    state.uninit(replacement_payload.clone(), error_type());
    state.write_value(replacement_payload, error_type(), origins[1], None);
    state.transfer(&replacement, first);

    state.point_at(
        payload,
        new_pointer.clone(),
        error_type(),
        bray_symbols::DependencyReferenceKind::Address,
    );

    assert!(!state.initialized(&Target::storage(pointer.clone()), error_type()));
    assert!(state.initialized(&Target::storage(new_pointer.clone()), error_type()));
}

#[test]
fn memory_writes_capture_formed_borrows_and_reads_transfer_the_whole_value() {
    let ([referent, owner, pointer, result], origins, capability) = borrowed_places();

    let mut state = State::default();
    let payload = super::Target::storage(owner).project(&[StorageProjection::UninitPayload]);

    state.begin_borrow(capability, referent.clone(), error_type());
    state.uninit(payload.clone(), error_type());

    state.write_value(
        payload.clone(),
        error_type(),
        origins[0],
        Some(super::Source::Borrow(capability)),
    );

    state.point_at(
        payload,
        pointer.clone(),
        error_type(),
        bray_symbols::DependencyReferenceKind::Address,
    );

    state.read(
        &Target::storage(pointer.clone()),
        error_type(),
        MemoryReadKind::Move,
        None,
        Some(result.clone()),
    );

    assert_eq!(state.referent(&result, error_type()), Some(referent));
}

#[test]
fn ending_inline_storage_lifetime_expires_its_exposed_payload_address() {
    let ([owner, pointer, _, _], origins) = places();

    let mut state = State::default();

    let payload =
        super::Target::storage(owner.clone()).project(&[StorageProjection::UninitPayload]);

    state.uninit(payload.clone(), error_type());
    state.write_value(payload.clone(), error_type(), origins[0], None);

    state.point_at(
        payload,
        pointer.clone(),
        error_type(),
        bray_symbols::DependencyReferenceKind::Address,
    );

    state.forget(owner.root);
    assert!(state.initialized(&Target::storage(pointer.clone()), error_type()));
    state.end_storage(|identity| identity == owner.root);
    assert!(!state.initialized(&Target::storage(pointer.clone()), error_type()));
}

#[test]
fn unlocated_aggregate_writes_keep_possible_origins_without_growing_paths() {
    let ([source, referent, destination, pointer], origins) = places();

    let mut state = State::default();
    let field = StorageProjection::TupleElement(bray_symbols::SymbolOrdinal::new(0));

    state.entry_reference(referent.clone());

    state.transfer_value(
        &super::Target::storage(referent),
        Place {
            root: source.root,
            projections: [field].into(),
        },
    );

    state.uninit(super::Target::storage(destination), error_type());

    for _ in 0..4 {
        state.write(
            &Target::storage(pointer.clone()),
            error_type(),
            origins[0],
            Some(super::Source::Value(super::Target::storage(source.clone()))),
        );
    }

    assert!(
        state
            .values
            .keys()
            .all(|target| target.projections().len() <= 1)
    );

    assert!(state.values.values().all(|references| references.unknown));

    assert!(
        state
            .values
            .values()
            .all(|references| !references.targets.is_empty())
    );

    assert!(
        state
            .contents
            .values()
            .all(|contents| contents.initialized.is_empty())
    );
}

#[test]
fn repeated_dereferences_have_distinct_entry_snapshots() {
    let ([root, _, _, _], _) = places();

    let ty = error_type();
    let mut state = State::default();
    let slot = super::model::Target::storage(root.clone());

    state.entry_reference_at(root.clone(), 1, slot.clone(), Some(ty));

    let first = state.value_target(&slot, ty).expect("first entry target");

    state.entry_reference_at(root.clone(), 2, first.clone(), Some(ty));

    let second = state.value_target(&first, ty).expect("second entry target");

    assert_ne!(first, second);
    assert!(!first.contains(&second));
    assert!(!second.contains(&first));
    assert!(second.observation_path().is_none());

    let path = bray_bound_tree::StorageReferencePath::new([].into(), [(0, ty), (0, ty)].into());

    assert_eq!(state.reference_location(slot, &path, None), Some(second));
}

#[test]
fn owned_regions_keep_headers_disjoint_and_survive_owner_moves_without_exact_offsets() {
    use super::{Origin, Target};

    let ([owner, pointer, moved, other], _) = places();

    let mut state = State::default();

    let header = Target {
        origin: Origin::entry(owner.clone()),
        previous: false,
    };

    state.owned_region(header.clone(), pointer.clone(), error_type());
    state.offset(&Target::storage(pointer.clone()), pointer.clone());

    let regions = state
        .referenced_regions(&Target::storage(pointer.clone()))
        .expect("fixture `referenced_regions` call must succeed");

    assert_eq!(regions.len(), 1);

    assert_eq!(
        state.target_relationship(&header, &regions[0]),
        bray_bound_tree::StorageRelationship::Disjoint
    );

    assert!(
        state
            .value_targets(&Target::storage(pointer.clone()))
            .is_none()
    );

    assert_eq!(
        state.read(
            &Target::storage(pointer.clone()),
            error_type(),
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::UninitializedRawStorage
    );

    state.transfer_value(&header, moved.clone());
    state.owned_region(Target::storage(moved), other.clone(), error_type());
    state.offset(&Target::storage(other.clone()), other.clone());

    assert_eq!(
        state.referenced_regions(&Target::storage(other)),
        Some(regions.clone())
    );

    assert!(regions[0].observation_path().is_none());

    let unrelated = Target {
        origin: Origin::entry(Place::root(pointer.root)),
        previous: false,
    };

    assert_eq!(
        state.target_relationship(&unrelated, &regions[0]),
        bray_bound_tree::StorageRelationship::PotentiallyOverlapping
    );
}

#[test]
fn displaced_regions_require_a_known_allocation_boundary() {
    let ([storage, pointer, displaced, allocation], origins) = places();

    let mut state = State::default();

    state.address(storage.clone(), pointer.clone(), error_type(), origins[0]);
    state.offset(&Target::storage(pointer.clone()), displaced.clone());

    assert!(
        state
            .referenced_regions(&Target::storage(displaced.clone()))
            .is_none()
    );

    state.fresh(allocation.clone(), origins[1]);
    state.offset(&Target::storage(allocation.clone()), displaced.clone());

    assert_eq!(
        state.referenced_regions(&Target::storage(displaced.clone())),
        state.value_targets(&Target::storage(allocation))
    );

    let owner = Target::storage(storage);

    state.owned_region(owner.clone(), pointer.clone(), error_type());
    state.offset(&Target::storage(pointer), displaced.clone());

    assert_eq!(
        state.referenced_regions(&Target::storage(displaced)),
        Some(vec![owner.project(&[StorageProjection::OwnedTarget])])
    );
}

#[test]
fn raw_memory_reads_pointer_values_held_behind_an_entry_borrow() {
    let ([storage, pointer, destination, result], origins) = places();

    let mut state = State {
        owned_storage: vec![storage.root].into(),
        ..State::default()
    };

    state.address(storage.clone(), pointer.clone(), error_type(), origins[0]);

    let indirect = Target {
        origin: super::Origin::entry(destination),
        previous: false,
    };

    state.begin_transition(origins[1].into());

    state.stage_value(
        origins[1].into(),
        &Target::storage(pointer),
        indirect.clone(),
    );

    state.complete_transition(origins[1].into());

    assert_eq!(
        state.write(&indirect, error_type(), origins[2], None),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        state.read(
            &indirect,
            error_type(),
            MemoryReadKind::Move,
            None,
            Some(result)
        ),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        state.read(&indirect, error_type(), MemoryReadKind::Copy, None, None),
        MemoryOperationStatus::UninitializedRawStorage
    );
}

#[test]
fn dynamic_element_writes_keep_reference_descriptors_but_revoke_possible_elements() {
    let ([array, pointer, element_pointer, _], origins) = places();

    let mut state = State::default();

    state.address(array.clone(), pointer.clone(), error_type(), origins[0]);

    let fixed = Place {
        root: array.root,
        projections: [StorageProjection::ElementFromStart(
            bray_symbols::SymbolOrdinal::new(0),
        )]
        .into(),
    };

    state.address(
        fixed.clone(),
        element_pointer.clone(),
        error_type(),
        origins[1],
    );

    let dynamic = Target::storage(Place {
        root: array.root,
        projections: [StorageProjection::Element(origins[2])].into(),
    });

    assert!(!super::model::References::one(dynamic.clone()).is_exact());
    state.write_target(dynamic, error_type(), origins[2]);
    assert_eq!(state.referent(&pointer, error_type()), Some(array));

    assert_eq!(
        state.read(
            &Target::storage(element_pointer),
            error_type(),
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::UninitializedRawStorage
    );
}

#[test]
fn incoming_aliases_can_reach_allocations_under_invocation_owned_headers() {
    use super::Origin;
    use bray_bound_tree::StorageRelationship;

    let ([owner, pointer, alias, moved], _) = places();

    let mut state = State {
        owned_storage: vec![owner.root, moved.root].into(),
        ..State::default()
    };

    let header = Target::storage(owner);

    let incoming = Target {
        origin: Origin::entry(alias),
        previous: false,
    };

    state.owned_region(header.clone(), pointer.clone(), error_type());

    let region = state
        .referenced_regions(&Target::storage(pointer))
        .expect("fixture `referenced_regions` call must succeed")
        .pop()
        .expect("fixture `pop` call must succeed");

    for (left, right) in [(&header, &incoming), (&incoming, &header)] {
        assert_eq!(
            state.target_relationship(left, right),
            StorageRelationship::Disjoint
        );
    }

    for (left, right) in [(&region, &incoming), (&incoming, &region)] {
        assert_eq!(
            state.target_relationship(left, right),
            StorageRelationship::PotentiallyOverlapping
        );
    }

    state.contents.entry(incoming.clone()).or_default();

    let incoming_write = state.affected_targets(&super::model::References::one(incoming.clone()));

    assert!(incoming_write.contains(&region));
    assert!(!incoming_write.contains(&header));

    let payload_write = state.affected_targets(&super::model::References::one(region.clone()));

    assert!(payload_write.contains(&incoming));
    assert!(!payload_write.contains(&header));
    state.transfer_value(&header, moved.clone());

    assert_eq!(
        state.target_relationship(&region, &incoming),
        StorageRelationship::PotentiallyOverlapping
    );

    assert_eq!(
        state.target_relationship(&Target::storage(moved), &incoming),
        StorageRelationship::Disjoint
    );
}

#[test]
fn opaque_effects_preserve_unexposed_reference_slots_but_revoke_contents() {
    for (owned_target, exposed) in [(false, false), (false, true), (true, false)] {
        let ([source, slot, address, _], origins) = places();

        let mut state = State {
            owned_storage: vec![slot.root].into(),
            ..State::default()
        };

        let component = Place {
            root: slot.root,
            projections: owned_target
                .then_some(StorageProjection::OwnedTarget)
                .into_iter()
                .chain([StorageProjection::TupleElement(
                    bray_symbols::SymbolOrdinal::new(0),
                )])
                .collect(),
        };

        state.address(source.clone(), component.clone(), error_type(), origins[0]);

        if exposed {
            state.address(component.clone(), address, error_type(), origins[1]);
            state.escape_targets([Target::storage(component.clone())]);
        }

        let storage =
            StoragePlanBuilder::new(BoundUnitId::new(41), BoundUnitKind::CallableBody).finish();

        let candidates = [slot.clone()];
        let stable_descriptors = state.unexposed_storage(&candidates, []).cloned().collect();

        state.invalidate(
            &bray_bound_tree::StorageInvalidation::All {
                stable_descriptors,
                stable_values: Default::default(),
            },
            &storage,
            bray_symbols::DependencyMutationKind::Retire,
        );

        assert_eq!(
            state.referent(&component, error_type()),
            (!exposed && !owned_target).then_some(source)
        );

        assert!(!state.initialized(&Target::storage(component), error_type()));
    }
}

#[test]
fn opaque_effects_preserve_protected_entry_reference_slots_without_referent_authority() {
    for protected in [false, true] {
        let ([parameter, field_reference, ..], _, capability) = borrowed_places();

        let ty = error_type();
        let mut state = State::default();

        state.begin_entry_borrow(capability, parameter.clone(), ty);

        let projection = StorageProjection::TupleElement(bray_symbols::SymbolOrdinal::new(0));

        let target = state
            .borrow_target(capability, ty)
            .expect("fixture `borrow_target` call must succeed")
            .project(&[projection]);

        let slot = Place {
            root: parameter.root,
            projections: [projection].into(),
        };

        state.entry_reference_at(slot.clone(), 1, target.clone(), Some(ty));
        state.transfer_value(&target, field_reference.clone());

        let expected = state.value_targets(&target);

        let storage =
            StoragePlanBuilder::new(BoundUnitId::new(41), BoundUnitKind::CallableBody).finish();

        state.invalidate(
            &bray_bound_tree::StorageInvalidation::All {
                stable_descriptors: protected.then_some(parameter).into_iter().collect(),
                stable_values: Default::default(),
            },
            &storage,
            bray_symbols::DependencyMutationKind::Retire,
        );

        assert_eq!(
            state.value_targets(&target),
            protected.then_some(expected).flatten()
        );

        assert!(!state.initialized(&target, ty));
    }
}

#[test]
fn null_pointer_has_no_effect_targets_or_initialization_authority() {
    let ([slot, copied, ..], _) = places();

    let mut state = State::default();

    state.null(slot.clone());
    state.transfer(&slot, copied.clone());

    for place in [slot, copied] {
        let target = Target::storage(place);

        assert_eq!(state.referenced_regions(&target), Some(Vec::new()));
        assert!(!state.initialized(&target, error_type()));
    }
}

#[test]
fn opaque_reachability_distinguishes_safe_borrows_raw_escape_and_nested_arguments() {
    let ([owner, reference, holder, sibling], origins, capability) = borrowed_places();

    let candidates = [owner.root, reference.root, holder.root, sibling.root];

    let mut state = State {
        owned_storage: candidates.into(),
        ..State::default()
    };

    let candidates = candidates.map(Place::root);

    state.begin_borrow(capability, owner.clone(), error_type());
    state.retain_borrow(capability, reference.clone());
    state.end_borrows(&[capability].into_iter().collect());

    assert!(
        state
            .unexposed_storage(&candidates, [])
            .any(|path| path.root == owner.root)
    );

    state.transfer(&reference, holder.clone());

    let reached = state
        .contained_regions(&Target::storage(holder))
        .cloned()
        .collect::<Vec<_>>();

    let unexposed = state
        .unexposed_storage(&candidates, reached)
        .collect::<BTreeSet<_>>();

    assert!(!unexposed.contains(&owner));
    assert!(unexposed.contains(&sibling));

    state.expose_address(&reference, error_type(), origins[0]);

    assert!(
        state
            .unexposed_storage(&candidates, [])
            .any(|path| path.root == owner.root)
    );

    state.escape_targets([Target::storage(owner.clone())]);
    state.clear(&reference);

    assert!(
        !state
            .unexposed_storage(&candidates, [])
            .any(|path| path.root == owner.root)
    );
}

#[test]
fn opaque_payload_borrows_preserve_disjoint_markers_but_raw_exposure_does_not() {
    let ([owner, reference, ..], origins, capability) = borrowed_places();

    let marker = Place {
        root: owner.root,
        projections: [StorageProjection::TupleElement(
            bray_symbols::SymbolOrdinal::new(0),
        )]
        .into(),
    };

    let payload = Place {
        root: owner.root,
        projections: [StorageProjection::TupleElement(
            bray_symbols::SymbolOrdinal::new(1),
        )]
        .into(),
    };

    let mut state = State {
        owned_storage: vec![owner.root, reference.root].into(),
        ..State::default()
    };

    state.begin_borrow(capability, payload.clone(), error_type());
    state.retain_borrow(capability, reference.clone());

    let candidates = [owner.clone(), marker.clone(), payload.clone()];

    let reached = state
        .contained_regions(&Target::storage(reference.clone()))
        .cloned()
        .collect::<Vec<_>>();

    assert_eq!(
        state
            .unexposed_storage(&candidates, reached)
            .cloned()
            .collect::<Vec<_>>(),
        vec![marker]
    );

    state.expose_address(&reference, error_type(), origins[0]);
    assert_eq!(state.unexposed_storage(&candidates, []).count(), 3);
    state.escape_targets([Target::storage(payload)]);
    assert_eq!(state.unexposed_storage(&candidates, []).count(), 0);
}

#[test]
fn returned_borrow_regions_bound_aliases_without_claiming_argument_identity() {
    for forwarded_unknown in [false, true] {
        let ([first, second, result, copied], origins) = places();

        let call = origins[0].into();

        let mut state = State {
            owned_storage: [first.root, second.root, result.root, copied.root].into(),
            ..State::default()
        };

        state.begin_transition(call);

        if forwarded_unknown {
            state.stage_unknown(call, Target::storage(result.clone()));
        }

        state.stage_returned_references(
            call,
            [(result.clone(), error_type())],
            [Target::storage(first.clone())],
        );

        state.complete_transition(call);

        let returned = state
            .value_target(&Target::storage(result.clone()), error_type())
            .expect("returned reference identity");

        assert_ne!(returned, Target::storage(first.clone()));

        assert_eq!(
            state.target_relationship(&returned, &Target::storage(first.clone())),
            bray_bound_tree::StorageRelationship::PotentiallyOverlapping
        );

        assert_eq!(
            state.target_relationship(&returned, &Target::storage(second.clone())),
            bray_bound_tree::StorageRelationship::Disjoint
        );

        let candidates = [first.clone(), second.clone()];

        assert_eq!(
            state
                .unexposed_storage(&candidates, [returned.clone()])
                .collect::<Vec<_>>(),
            vec![&candidates[1]],
        );

        state.transfer(&result, copied.clone());
        state.begin_transition(call);

        state.stage_returned_references(
            call,
            [(result.clone(), error_type())],
            [Target::storage(second.clone())],
        );

        state.complete_transition(call);

        let current = state
            .value_target(&Target::storage(result), error_type())
            .expect("current returned reference");

        let previous = state
            .referenced_regions(&Target::storage(copied))
            .expect("older reference region")
            .pop()
            .expect("fixture `pop` call must succeed");

        assert_eq!(
            state.target_relationship(&current, &previous),
            bray_bound_tree::StorageRelationship::PotentiallyOverlapping
        );

        assert_eq!(
            state.target_relationship(&current, &Target::storage(first)),
            bray_bound_tree::StorageRelationship::PotentiallyOverlapping
        );
    }
}

#[test]
fn retired_local_reference_slots_do_not_expose_their_former_targets() {
    let ([owner, reference, ..], origins, capability) = borrowed_places();

    let mut state = State {
        owned_storage: vec![owner.root, reference.root].into(),
        ..State::default()
    };

    state.begin_borrow(capability, owner.clone(), error_type());
    state.retain_borrow(capability, reference.clone());
    state.begin_borrow(capability, reference.clone(), error_type());
    state.end_borrows(&[capability].into_iter().collect());

    let candidates = [owner.clone()];

    assert_eq!(
        state
            .unexposed_storage(&candidates, [Target::storage(reference.clone())])
            .count(),
        0
    );

    state.end_storage(|identity| identity == reference.root);
    assert_eq!(state.unexposed_storage(&candidates, []).count(), 1);

    state.address(owner.clone(), reference, error_type(), origins[0]);
    assert_eq!(state.unexposed_storage(&candidates, []).count(), 1);
    state.escape_targets([Target::storage(owner)]);
    assert_eq!(state.unexposed_storage(&candidates, []).count(), 0);
}

#[test]
fn unknown_references_cannot_reach_unaddressed_inline_storage() {
    let ([local, borrowed, _, _], _) = places();

    let state = State {
        unaddressed_storage: [local.root].into(),
        ..State::default()
    };

    assert!(!state.unknown_reference_may_reach(&Target::storage(local.clone())));
    assert!(state.unknown_reference_may_reach(&Target::storage(borrowed)));

    assert!(state.unknown_reference_may_reach(
        &Target::storage(local).project(&[bray_bound_tree::StorageProjection::OwnedTarget]),
    ));
}

#[test]
fn local_mutation_does_not_publish_an_invocation_address() {
    let ([local, external, _, _], _) = places();

    let mut state = State {
        owned_storage: [local.root].into(),
        ..State::default()
    };

    let local_write = super::model::Effect::Mutation(
        Target::storage(local.clone()),
        bray_symbols::DependencyMutationKind::Replace(error_type()),
    );

    let external_write = super::model::Effect::Mutation(
        Target::storage(external),
        bray_symbols::DependencyMutationKind::Replace(error_type()),
    );

    let heap_write = super::model::Effect::Mutation(
        Target::storage(local.clone()).project(&[bray_bound_tree::StorageProjection::OwnedTarget]),
        bray_symbols::DependencyMutationKind::Write,
    );

    assert!(!local_write.may_retain_address(&state));
    assert!(external_write.may_retain_address(&state));
    assert!(heap_write.may_retain_address(&state));

    state.escape_targets([Target::storage(local)]);
    assert!(local_write.may_retain_address(&state));
}

#[test]
fn targeted_atomic_effects_preserve_reference_slots_under_possible_aliasing() {
    let ([source, slot, external, _], origins) = places();

    let mut state = State::default();

    state.address(source.clone(), slot.clone(), error_type(), origins[0]);

    let external = Target {
        origin: super::Origin::entry(external),
        previous: false,
    };

    state.invalidate_target(&external, bray_symbols::DependencyMutationKind::Atomic);
    assert_eq!(state.referent(&slot, error_type()), Some(source));

    state.invalidate_target(&external, bray_symbols::DependencyMutationKind::Write);
    assert_eq!(state.referent(&slot, error_type()), None);
}

#[test]
fn normal_completion_discards_captured_failure_effects() {
    let ([source, _, _, _], origins) = places();

    let occurrence = origins[0].into();
    let mut state = State::default();

    state.begin_transition(occurrence);

    state.capture_failure_effects(
        occurrence,
        super::CapturedEffects {
            mutations: [super::Effect::Mutation(
                Target::storage(source.clone()),
                bray_symbols::DependencyMutationKind::Write,
            )]
            .into(),
            escaped: [Target::storage(source)].into(),
            invalidation: bray_bound_tree::StorageInvalidation::unknown(),
        },
    );

    state.complete_transition(occurrence);

    assert!(state.take_failure_effects(occurrence).is_none());
    assert!(state.contents.is_empty());
}

#[test]
fn captured_failure_targets_follow_storage_retirement() {
    let ([source, slot, _, _], origins) = places();

    let occurrence = origins[1].into();
    let mut state = State::default();

    state.address(source.clone(), slot, error_type(), origins[0]);
    state.begin_transition(occurrence);

    let previous = Target {
        previous: true,
        ..Target::storage(source.clone())
    };

    state.capture_failure_effects(
        occurrence,
        super::CapturedEffects {
            mutations: [
                super::Effect::Mutation(
                    previous.clone(),
                    bray_symbols::DependencyMutationKind::Write,
                ),
                super::Effect::Mutation(
                    Target::storage(source.clone()),
                    bray_symbols::DependencyMutationKind::Write,
                ),
            ]
            .into(),
            escaped: [previous, Target::storage(source.clone())].into(),
            invalidation: bray_bound_tree::StorageInvalidation::unknown(),
        },
    );

    state.begin(source.root);

    let effects = state
        .take_failure_effects(occurrence)
        .expect("failed invocation retains its effects");

    let previous = Target {
        previous: true,
        ..Target::storage(source)
    };

    assert_eq!(
        effects.mutations.as_slice(),
        &[super::Effect::Mutation(
            previous.clone(),
            bray_symbols::DependencyMutationKind::Write
        )]
    );

    assert_eq!(effects.escaped.as_slice(), &[previous]);
    assert!(state.take_failure_effects(occurrence).is_none());
}

#[test]
fn null_join_retains_bounds_without_exact_identity_or_initialization() {
    let ([slot, ..], _, capability) = borrowed_places();

    let ty = error_type();
    let mut state = State::default();

    state.begin_entry_borrow(capability, slot.clone(), ty);

    let target = state.borrow_target(capability, ty).expect("fixture `borrow_target` call must succeed");
    let mut absent = state.clone();

    absent.null(slot.clone());
    state.merge(&absent, &Default::default(), &Default::default());

    assert_eq!(state.value_targets(&Target::storage(slot.clone())), None);

    assert_eq!(
        state.referenced_regions(&Target::storage(slot)),
        Some(vec![target.clone()])
    );

    assert!(!state.initialized(&target, ty));
}

#[test]
fn raw_exposure_of_an_entry_referent_revokes_its_shared_protection() {
    let ([parameter, pointer, ..], origins, capability) = borrowed_places();

    let ty = error_type();
    let mut state = State::default();

    state.begin_entry_borrow(capability, parameter.clone(), ty);

    let target = state.borrow_target(capability, ty).expect("fixture `borrow_target` call must succeed");

    state.stage_address(
        origins[0].into(),
        target,
        Target::storage(pointer),
        ty,
        bray_symbols::DependencyReferenceKind::Address,
    );

    assert_eq!(state.raw_exposed_roots(), BTreeSet::from([parameter.root]));
}

#[test]
fn returned_pointers_can_reach_published_local_storage_without_current_inputs() {
    let ([local, result, ..], origins) = places();

    let call = origins[0].into();

    let mut state = State {
        owned_storage: [local.root, result.root].into(),
        ..State::default()
    };

    state.stage_returned_references(call, [(result.clone(), error_type())], []);
    state.complete_transition(call);

    let pointer = state
        .value_target(&Target::storage(result), error_type())
        .expect("fixture `value_target` call must succeed");

    let local = Target::storage(local);

    assert_eq!(
        state.target_relationship(&pointer, &local),
        bray_bound_tree::StorageRelationship::Disjoint
    );

    state.escape_targets([local.clone()]);

    assert_ne!(
        state.target_relationship(&pointer, &local),
        bray_bound_tree::StorageRelationship::Disjoint
    );

    assert!(
        state.effect_origins(&pointer).next().is_none(),
        "external reachability does not imply this call received a local input"
    );
}

#[test]
fn native_escape_of_owned_payload_does_not_publish_the_owner_header() {
    let ([owner, ..], _) = places();

    let mut state = State {
        owned_storage: [owner.root].into(),
        ..State::default()
    };

    let header = Target::storage(owner.clone());
    let payload = header.project(&[StorageProjection::OwnedTarget]);

    let element = payload.project(&[StorageProjection::TupleElement(
        bray_symbols::SymbolOrdinal::new(1),
    )]);

    state.escape_targets([element]);

    let reached = state.reachable_targets([]);

    assert!(reached.contains(&payload));
    assert!(!reached.contains(&header));

    assert_eq!(
        state
            .unexposed_storage(std::slice::from_ref(&owner), [])
            .collect::<Vec<_>>(),
        vec![&owner]
    );
}

#[test]
fn returned_pointer_bounds_do_not_merge_owned_payloads_with_their_headers() {
    let ([owner, result, ..], origins) = places();

    let call = origins[0].into();

    let mut state = State {
        owned_storage: [owner.root, result.root].into(),
        ..State::default()
    };

    let header = Target::storage(owner.clone());
    let payload = header.project(&[StorageProjection::OwnedTarget]);

    state.stage_returned_references(call, [(result.clone(), error_type())], [payload.clone()]);
    state.complete_transition(call);

    let pointer = state
        .value_target(&Target::storage(result), error_type())
        .expect("fixture `value_target` call must succeed");

    assert_eq!(
        state.target_relationship(&pointer, &header),
        bray_bound_tree::StorageRelationship::Disjoint
    );

    state.escape_targets([pointer]);

    assert_eq!(
        state
            .unexposed_storage(std::slice::from_ref(&owner), [])
            .collect::<Vec<_>>(),
        vec![&owner]
    );

    assert!(state.reachable_targets([]).iter().any(|target| {
        state.target_relationship(target, &payload)
            != bray_bound_tree::StorageRelationship::Disjoint
    }));
}

#[test]
fn independent_returned_references_may_alias_and_merge_with_input_alternatives() {
    let ([owner, first, second, ..], origins) = places();

    let call = origins[0].into();
    let mut state = State::default();

    state.stage_returned_reference(
        call,
        Target::storage(first.clone()),
        first.clone(),
        error_type(),
        [],
    );

    state.stage_returned_reference(
        call,
        Target::storage(second.clone()),
        second.clone(),
        error_type(),
        [],
    );

    state.complete_transition(call);

    let first_target = state
        .value_target(&Target::storage(first.clone()), error_type())
        .expect("fixture `value_target` call must succeed");

    let second_target = state
        .value_target(&Target::storage(second), error_type())
        .expect("fixture `value_target` call must succeed");

    assert_ne!(
        state.target_relationship(&first_target, &second_target),
        bray_bound_tree::StorageRelationship::Disjoint
    );

    state.stage_returned_reference(
        call,
        Target::storage(first.clone()),
        first.clone(),
        error_type(),
        [],
    );

    state.stage_address(
        call,
        Target::storage(owner.clone()),
        Target::storage(first.clone()),
        error_type(),
        bray_symbols::DependencyReferenceKind::Address,
    );

    state.complete_transition(call);

    let alternatives = state.value_targets(&Target::storage(first)).expect("fixture `value_targets` call must succeed");

    assert!(
        alternatives
            .iter()
            .any(|target| target == &Target::storage(owner.clone()))
    );

    assert!(alternatives.iter().any(|target| matches!(
        target.origin,
        bray_bound_tree::StorageOrigin::Returned { .. }
    )));
}

#[test]
fn bounded_address_views_preserve_the_allocation_type_without_initializing_the_region() {
    let ([owner, pointer, ..], origins) = places();

    let region_type = error_type();
    let view_type = crate::test_support::nullable_type(region_type);
    let mut state = State::default();
    let owner = Target::storage(owner);

    state.owned_region(owner.clone(), pointer.clone(), region_type);
    state.offset(&Target::storage(pointer.clone()), pointer.clone());
    state.expose_address(&pointer, view_type, origins[0]);

    let region = state.owned_regions(&owner).expect("fixture `owned_regions` call must succeed").pop().expect("fixture `pop` call must succeed");
    let contents = state.contents.get(&region).expect("fixture lookup must contain its requested entry");

    assert_eq!(contents.storage_type, Some(region_type));
    assert!(!contents.initialized.contains(&view_type));
    assert!(contents.exposed);
    assert!(contents.pending[&view_type].contains(&origins[0]));
}

#[test]
#[should_panic(expected = "exact address formation preserves the declared storage type")]
fn exact_address_views_reject_a_conflicting_declared_storage_type() {
    let ([owner, pointer, ..], origins) = places();

    let region_type = error_type();
    let view_type = crate::test_support::nullable_type(region_type);
    let mut state = State::default();

    state.owned_region(Target::storage(owner), pointer.clone(), region_type);
    state.expose_address(&pointer, view_type, origins[0]);
}

#[test]
fn bounded_forwarding_retains_a_typed_result_and_the_call_input_regions() {
    let ([owner, result, ..], origins) = places();

    let call = origins[0].into();
    let region_type = error_type();
    let view_type = crate::test_support::nullable_type(region_type);
    let payload = Target::storage(owner.clone()).project(&[StorageProjection::OwnedTarget]);
    let destination = Target::storage(result.clone());

    let mut state = State {
        owned_storage: [owner.root, result.root].into(),
        ..State::default()
    };

    state.stage_address(
        call,
        payload.clone(),
        destination.clone(),
        region_type,
        bray_symbols::DependencyReferenceKind::Borrow,
    );

    state.bound_staged_reference(call, &destination);
    state.stage_returned_references(call, [(result, view_type)], [payload.clone()]);
    state.complete_transition(call);

    let target = state
        .value_target(&destination, view_type)
        .expect("a bounded returned cell retains its declared view type");

    assert_eq!(
        state.effect_origins(&target).collect::<Vec<_>>(),
        vec![&payload]
    );

    assert_eq!(
        state.target_relationship(&target, &Target::storage(owner)),
        bray_bound_tree::StorageRelationship::Disjoint,
    );

    assert!(!state.initialized(&destination, view_type));

    let exported = state.exit_references(
        destination.storage_path().expect("fixture `storage_path` call must succeed").root,
        &Default::default(),
    );

    assert_eq!(
        exported.slots[destination.storage_path().expect("fixture `storage_path` call must succeed")].targets,
        BTreeSet::from([payload])
    );

    assert!(!exported.slots[destination.storage_path().expect("fixture `storage_path` call must succeed")].is_exact());
}

#[test]
fn returned_null_does_not_invent_an_outside_origin() {
    let ([result, ..], origins) = places();

    let call = origins[0].into();
    let mut state = State::default();

    state.stage_null(call, Target::storage(result.clone()));
    state.stage_returned_references(call, [(result.clone(), error_type())], []);
    state.complete_transition(call);

    assert_eq!(
        state.value_targets(&Target::storage(result)),
        Some(Vec::new())
    );
}
