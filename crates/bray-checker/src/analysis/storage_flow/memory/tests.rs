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
        .unwrap()
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
        .unwrap();

    let borrow = storage
        .push_borrow_capability(bray_bound_tree::PlannedBorrowCapability::new(
            bray_bound_tree::BorrowCapabilityOrigin::Expression(expressions[0]),
            bray_symbols::BorrowKind::Shared,
            access,
            None,
            source,
            false,
        ))
        .unwrap();

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
    assert!(left.merge(&right, &Default::default(), &Default::default()));
    assert!(!left.merge(&right, &Default::default(), &Default::default()));

    assert_eq!(
        left.read(&source, error_type(), MemoryReadKind::Copy, None, None),
        MemoryOperationStatus::UninitializedRawStorage
    );

    assert_eq!(
        left.deallocate(&source, origins[2]),
        MemoryOperationStatus::OutstandingObligations
    );

    right.deallocate(&source, origins[2]);
    assert!(left.merge(&right, &Default::default(), &Default::default()));

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
    assert!(left.merge(&right, &Default::default(), &Default::default()));
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
    left.merge(&right, &Default::default(), &Default::default());

    assert_eq!(
        left.initialization_origins(&source).collect::<Vec<_>>(),
        vec![origins[1], origins[2]]
    );

    assert_eq!(
        left.read(&source, error_type(), MemoryReadKind::Copy, None, None),
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
    let ([owner, pointer, alias, _], origins, capability) = borrowed_places();

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
    state.begin_borrow(capability, state.referent(&alias, ty).unwrap(), other);
    state.retain_borrow(capability, alias.clone());
    assert_eq!(state.referent(&alias, other), None);
    assert_eq!(state.reached_storage(&alias)[0].1, Some(ty));
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
    assert!(!state.initialized(&alias, error_type()));

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
        &bray_bound_tree::StorageInvalidation::All,
        &storage,
        bray_symbols::DependencyMutationKind::Retire,
    );

    assert!(!state.has_reference(&slot));
    state.retain_borrow(capability, alias.clone());
    assert_eq!(state.referent(&alias, error_type()), Some(source));
    assert!(!state.initialized(&alias, error_type()));
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
        &argument,
        error_type(),
        MemoryReadKind::Move,
        Some(expressions[1]),
        None,
    );

    state.complete_transition(call);
    assert_eq!(state.referent(&result, error_type()), Some(owner.clone()));
    assert!(!state.initialized(&result, error_type()));
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
        vec![(&captured, &super::model::Origin::Entry(first.clone()))]
    );

    state.write(&first, error_type(), origins[0], None);

    assert!(
        !state.has_reference(&second),
        "an unknown input target may overwrite another input slot"
    );

    assert!(
        state.has_reference(&captured),
        "overwriting an input slot does not rewrite a copied reference value"
    );

    assert!(
        state.located_storage(&captured).is_none(),
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
fn exposed_entry_reference_keeps_its_origin_without_claiming_a_physical_place() {
    let ([source, pointer, _, _], origins) = places();

    let mut state = State::default();

    state.entry_reference(source.clone());
    state.transfer(&source, pointer.clone());
    state.expose_address(&pointer, error_type(), origins[0]);

    assert_eq!(
        state.exact_references(&pointer).collect::<Vec<_>>(),
        vec![(&pointer, &super::model::Origin::Entry(source))]
    );

    assert!(state.located_storage(&pointer).is_none());

    assert_eq!(
        state.read(&pointer, error_type(), MemoryReadKind::Copy, None, None),
        MemoryOperationStatus::Valid
    );
}
