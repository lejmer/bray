use super::{Place, State};
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
    let unit = BoundUnitId::new(41);

    let (_, expressions) = expression_unit(unit, |tree, origin| {
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

    (places.try_into().expect("four test places"), expressions)
}

#[test]
fn consuming_and_deallocating_through_an_alias_updates_the_original_target() {
    let ([source, alias, _, _], origins) = places();

    let mut state = State::default();
    let ty = error_type();

    state.fresh(source.clone(), Some(origins[0]));
    state.transfer(&source, alias.clone());

    assert_eq!(
        state.write(&source, ty, origins[1], None),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        state.read(&alias, ty, MemoryReadKind::Copy, None, None),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        state.deallocate(&alias, origins[2]),
        MemoryOperationStatus::OutstandingObligations
    );

    assert_eq!(
        state.initialization_origins(&alias).collect::<Vec<_>>(),
        vec![origins[1]]
    );

    assert_eq!(
        state.read(&alias, ty, MemoryReadKind::Move, None, None),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        state.read(&source, ty, MemoryReadKind::Copy, None, None),
        MemoryOperationStatus::UninitializedRawStorage
    );

    assert_eq!(
        state.deallocate(&source, origins[2]),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        state.write(&alias, ty, origins[3], None),
        MemoryOperationStatus::InvalidatedAllocation
    );

    assert_eq!(
        state.allocation_origins(&alias).collect::<Vec<_>>(),
        vec![origins[0]]
    );

    assert_eq!(
        state.invalidation_origins(&alias).collect::<Vec<_>>(),
        vec![origins[2]]
    );

    assert_eq!(state.contents.len(), 1);
}

#[test]
fn raw_copy_creates_independent_contents_and_preserves_its_source() {
    let ([source, destination, _, _], origins) = places();

    let mut state = State::default();
    let ty = error_type();

    state.fresh(source.clone(), Some(origins[0]));
    state.fresh(destination.clone(), Some(origins[1]));
    state.write(&source, ty, origins[0], None);

    assert_eq!(
        state.copy(&source, &destination, ty, origins[1], None),
        MemoryOperationStatus::Valid
    );

    state.consume(&source, ty);

    assert_eq!(
        state.read(&destination, ty, MemoryReadKind::Copy, None, None),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        state.read(&source, ty, MemoryReadKind::Copy, None, None),
        MemoryOperationStatus::UninitializedRawStorage
    );
}

#[test]
fn empty_range_copy_has_no_scalar_effect_and_nonempty_copy_keeps_obligations() {
    let ([source, destination, _, _], origins) = places();

    let mut state = State::default();
    let ty = error_type();

    state.fresh(source.clone(), Some(origins[0]));
    state.fresh(destination.clone(), Some(origins[1]));

    assert_eq!(
        state.copy(&source, &destination, ty, origins[1], Some(true)),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        state.read(&destination, ty, MemoryReadKind::Copy, None, None),
        MemoryOperationStatus::UninitializedRawStorage
    );

    assert_eq!(
        state.copy(&source, &destination, ty, origins[1], Some(false)),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        state.copy(&source, &destination, ty, origins[1], Some(true)),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        state.deallocate(&destination, origins[2]),
        MemoryOperationStatus::OutstandingObligations
    );

    state.deallocate(&source, origins[2]);

    assert_eq!(
        state.copy(&source, &destination, ty, origins[1], Some(true)),
        MemoryOperationStatus::InvalidatedAllocation
    );
}

#[test]
fn trusted_consumption_uses_checked_authority_without_inventing_initialized_state() {
    let ([source, _, _, _], origins) = places();

    let mut state = State::default();

    assert_eq!(
        state.consume(&source, error_type()),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        state.read(
            &source,
            error_type(),
            MemoryReadKind::Copy,
            Some(origins[0]),
            None
        ),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        state.read(&source, error_type(), MemoryReadKind::Copy, None, None),
        MemoryOperationStatus::UninitializedRawStorage
    );
}

#[test]
fn retiring_a_pointer_slot_keeps_the_contents_reached_by_surviving_aliases() {
    let ([source, alias, _, _], origins) = places();

    let mut state = State::default();

    state.fresh(source.clone(), Some(origins[0]));
    state.write(&source, error_type(), origins[1], None);
    state.transfer(&source, alias.clone());
    state.forget(source.root);
    assert!(!state.has_reference(&source));

    assert_eq!(
        state.read(&alias, error_type(), MemoryReadKind::Copy, None, None),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        state.allocation_origins(&alias).collect::<Vec<_>>(),
        vec![origins[0]]
    );

    state.retain_live(&BTreeSet::new());
    assert!(!state.has_reference(&alias));
}

#[test]
fn reassignment_changes_the_slot_without_mutating_its_previous_target() {
    let ([source, alias, unknown, _], origins) = places();

    let mut state = State::default();

    state.fresh(source.clone(), Some(origins[0]));
    state.write(&source, error_type(), origins[1], None);
    state.transfer(&source, alias.clone());
    state.transfer(&unknown, source.clone());
    assert!(!state.has_reference(&source));

    assert_eq!(
        state.read(&alias, error_type(), MemoryReadKind::Copy, None, None),
        MemoryOperationStatus::Valid
    );
}

#[test]
fn join_intersects_initialization_but_preserves_possible_obligations_and_origins() {
    let ([source, _, _, _], origins) = places();

    let mut left = State::default();

    left.fresh(source.clone(), Some(origins[0]));

    let mut right = left.clone();

    left.write(&source, error_type(), origins[1], None);
    assert!(left.merge(&right));
    assert!(!left.merge(&right));

    assert_eq!(
        left.read(&source, error_type(), MemoryReadKind::Copy, None, None),
        MemoryOperationStatus::UninitializedRawStorage
    );

    assert_eq!(
        left.deallocate(&source, origins[2]),
        MemoryOperationStatus::OutstandingObligations
    );

    right.deallocate(&source, origins[2]);
    assert!(left.merge(&right));

    assert_eq!(
        left.read(&source, error_type(), MemoryReadKind::Copy, None, None),
        MemoryOperationStatus::InvalidatedAllocation
    );

    assert_eq!(
        left.invalidation_origins(&source).collect::<Vec<_>>(),
        vec![origins[2]]
    );
}

#[test]
fn joined_targets_require_all_possible_contents_and_weak_writes_do_not_initialize_both() {
    let ([first, second, alias, _], origins) = places();

    let mut left = State::default();

    left.fresh(first.clone(), Some(origins[0]));
    left.fresh(second.clone(), Some(origins[1]));
    left.transfer(&first, alias.clone());

    let mut right = left.clone();

    right.transfer(&second, alias.clone());
    assert!(left.merge(&right));
    left.write(&alias, error_type(), origins[2], None);

    for target in [&first, &second, &alias] {
        assert_eq!(
            left.read(target, error_type(), MemoryReadKind::Copy, None, None),
            MemoryOperationStatus::UninitializedRawStorage
        );

        assert_eq!(
            left.deallocate(target, origins[3]),
            MemoryOperationStatus::OutstandingObligations
        );
    }

    left.write(&first, error_type(), origins[2], None);
    left.write(&second, error_type(), origins[2], None);

    assert_eq!(
        left.read(&alias, error_type(), MemoryReadKind::Move, None, None),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        left.read(&first, error_type(), MemoryReadKind::Copy, None, None),
        MemoryOperationStatus::UninitializedRawStorage
    );

    assert_eq!(
        left.deallocate(&first, origins[3]),
        MemoryOperationStatus::OutstandingObligations
    );
}

#[test]
fn repeated_creation_separates_old_aliases_and_has_a_finite_summary() {
    let ([source, alias, _, _], origins) = places();

    let mut state = State::default();

    state.fresh(source.clone(), Some(origins[0]));
    state.write(&source, error_type(), origins[1], None);
    state.transfer(&source, alias.clone());
    state.fresh(source.clone(), Some(origins[0]));
    state.write(&source, error_type(), origins[1], None);

    assert_eq!(
        state.read(&alias, error_type(), MemoryReadKind::Copy, None, None),
        MemoryOperationStatus::UninitializedRawStorage
    );

    state.consume(&alias, error_type());

    assert_eq!(
        state.deallocate(&alias, origins[2]),
        MemoryOperationStatus::OutstandingObligations
    );

    assert_eq!(
        state.read(&source, error_type(), MemoryReadKind::Copy, None, None),
        MemoryOperationStatus::Valid
    );

    let stable = state.clone();

    state.fresh(source.clone(), Some(origins[0]));
    state.write(&source, error_type(), origins[1], None);
    assert_eq!(state, stable);
    assert_eq!(state.contents.len(), 2);
}

#[test]
fn nested_address_observes_the_reference_slot_and_follows_reassignment() {
    let ([source, other, address, result], origins) = places();

    let mut state = State::default();

    state.fresh(source.clone(), Some(origins[0]));
    state.fresh(other.clone(), Some(origins[1]));
    state.write(&source, error_type(), origins[0], None);
    state.address(source.clone(), address.clone(), error_type(), origins[2]);

    assert_eq!(
        state.read(
            &address,
            error_type(),
            MemoryReadKind::Copy,
            None,
            Some(result.clone())
        ),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        state.read(&result, error_type(), MemoryReadKind::Copy, None, None),
        MemoryOperationStatus::Valid
    );

    state.transfer(&other, source.clone());

    state.read(
        &address,
        error_type(),
        MemoryReadKind::Copy,
        None,
        Some(result.clone()),
    );

    assert_eq!(
        state.read(&result, error_type(), MemoryReadKind::Copy, None, None),
        MemoryOperationStatus::UninitializedRawStorage
    );
}

#[test]
fn origin_join_unions_and_deduplicates_diagnostic_causes() {
    let ([source, _, _, _], origins) = places();

    let mut left = State::default();

    left.fresh(source.clone(), Some(origins[0]));

    let mut right = left.clone();

    left.write(&source, error_type(), origins[1], None);
    right.write(&source, error_type(), origins[2], None);
    left.merge(&right);

    assert_eq!(
        left.initialization_origins(&source).collect::<Vec<_>>(),
        vec![origins[1], origins[2]]
    );

    assert_eq!(
        left.read(&source, error_type(), MemoryReadKind::Copy, None, None),
        MemoryOperationStatus::Valid
    );

    assert!(!left.merge(&right));
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
    state.write(&left_pointer, error_type(), origins[3], None);

    assert_eq!(
        state.read(&parent, error_type(), MemoryReadKind::Copy, None, None),
        MemoryOperationStatus::UninitializedRawStorage
    );

    assert_eq!(
        state.read(
            &left_pointer,
            error_type(),
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        state.read(
            &right_pointer,
            error_type(),
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::Valid
    );

    state.consume(&left_pointer, error_type());

    assert_eq!(
        state.read(
            &right_pointer,
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

    state.fresh(allocation.clone(), Some(origins[0]));
    state.transfer(&allocation, field(&aggregate));
    state.transfer(&aggregate, destination.clone());
    state.write(&allocation, error_type(), origins[1], None);

    assert_eq!(
        state.read(
            &field(&destination),
            error_type(),
            MemoryReadKind::Move,
            None,
            None
        ),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        state.read(
            &field(&aggregate),
            error_type(),
            MemoryReadKind::Copy,
            None,
            None
        ),
        MemoryOperationStatus::UninitializedRawStorage
    );

    state.transfer(&unknown, destination.clone());
    assert!(!state.has_reference(&destination));
}

#[test]
fn recreated_local_does_not_revive_a_pointer_to_its_previous_incarnation() {
    let ([local, old, current, _], origins) = places();

    let mut state = State::default();

    state.address(local.clone(), old.clone(), error_type(), origins[0]);
    state.begin(local.root);
    state.address(local, current.clone(), error_type(), origins[1]);

    assert_eq!(
        state.read(&old, error_type(), MemoryReadKind::Copy, None, None),
        MemoryOperationStatus::UninitializedRawStorage
    );

    assert_eq!(
        state.read(&current, error_type(), MemoryReadKind::Copy, None, None),
        MemoryOperationStatus::Valid
    );
}

#[test]
fn offset_retains_allocation_identity_without_claiming_base_contents() {
    let ([allocation, interior, _, _], origins) = places();

    let mut state = State::default();

    state.fresh(allocation.clone(), Some(origins[0]));
    state.write(&allocation, error_type(), origins[1], None);
    state.offset(&allocation, interior.clone());

    assert_eq!(
        state.read(&interior, error_type(), MemoryReadKind::Copy, None, None),
        MemoryOperationStatus::UninitializedRawStorage
    );

    assert_eq!(
        state.deallocate(&interior, origins[2]),
        MemoryOperationStatus::OutstandingObligations
    );

    state.consume(&allocation, error_type());
    state.deallocate(&allocation, origins[2]);

    assert_eq!(
        state.write(&interior, error_type(), origins[3], None),
        MemoryOperationStatus::InvalidatedAllocation
    );
}

#[test]
fn checked_read_evidence_retains_cleanup_obligations() {
    let ([source, _, _, _], origins) = places();

    let mut state = State::default();

    state.fresh(source.clone(), Some(origins[0]));

    assert_eq!(
        state.read(
            &source,
            error_type(),
            MemoryReadKind::Copy,
            Some(origins[1]),
            None
        ),
        MemoryOperationStatus::Valid
    );

    assert_eq!(
        state.deallocate(&source, origins[2]),
        MemoryOperationStatus::OutstandingObligations
    );

    assert_eq!(
        state.initialization_origins(&source).collect::<Vec<_>>(),
        vec![origins[1]]
    );
}

#[test]
fn an_unresolved_stored_pointer_cannot_prove_disjointness() {
    let ([allocation, loaded, _, _], origins) = places();

    let mut state = State::default();

    state.fresh(allocation.clone(), Some(origins[0]));
    state.write(&allocation, error_type(), origins[1], None);

    state.read(
        &allocation,
        error_type(),
        MemoryReadKind::Copy,
        None,
        Some(loaded.clone()),
    );

    state.write(&loaded, error_type(), origins[2], None);

    assert_eq!(
        state.read(&allocation, error_type(), MemoryReadKind::Copy, None, None),
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
        state.read(&pointer, error_type(), MemoryReadKind::Copy, None, None),
        MemoryOperationStatus::UninitializedRawStorage
    );

    state.fresh(replacement.clone(), Some(origins[2]));
    state.transfer(&replacement, element.clone());
    state.transfer(&element, loaded.clone());
    assert!(state.references(&loaded).unknown);
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
        state.read(&alias, ty, MemoryReadKind::Copy, None, None),
        MemoryOperationStatus::UninitializedRawStorage
    );

    state.write_place(owner.clone(), ty, origins[1]);

    assert_eq!(
        state.read(&alias, ty, MemoryReadKind::Copy, None, None),
        MemoryOperationStatus::Valid
    );

    state.begin(owner.root);
    state.write_place(owner, ty, origins[2]);

    assert_eq!(
        state.read(&alias, ty, MemoryReadKind::Copy, None, None),
        MemoryOperationStatus::UninitializedRawStorage
    );
}

#[test]
fn reinterpreted_views_do_not_relabel_the_original_typed_storage() {
    let ([owner, pointer, alias, _], origins) = places();

    let mut state = State::default();
    let ty = error_type();

    let other = crate::test_support::semantic_values()
        .intern_type(bray_symbols::TypeData::Borrow {
            kind: bray_symbols::BorrowKind::Shared,
            target: ty,
        })
        .unwrap();

    state.address(owner.clone(), pointer.clone(), ty, origins[0]);
    state.transfer(&pointer, alias.clone());
    assert_eq!(state.referent(&alias, ty), Some(owner));
    assert_eq!(state.referent(&alias, other), None);
    assert_eq!(state.reached_storage(&alias)[0].1, Some(ty));
}
