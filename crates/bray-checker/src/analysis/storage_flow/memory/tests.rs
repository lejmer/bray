use super::{Effect, Origin, Place, State, Target};
use super::model::{AddressValue, Retirement};
use crate::test_support::{
    TestCheckerContext, array_type, callable_entry, error_type, expression_unit, push_expression,
    semantic_values, tuple_type,
};
use bray_bound_tree::{
    BoundErrorExpression, BoundExpression, BoundExpressionId, BoundUnitId, BoundUnitKind,
    MemoryOperationStatus, MemoryReadKind, StorageIdentity, StorageOrigin, StoragePlanBuilder,
    StorageProjection, StorageRelationship,
};
use bray_symbols::{BorrowKind, DependencyFailureKind, DependencyMutationKind, DependencyProjection, DependencyReferenceKind, TypeData};
use std::collections::BTreeSet;
use std::sync::Arc;

const BUFFER_POINTER: StorageProjection =
    StorageProjection::TupleElement(bray_symbols::SymbolOrdinal::new(0));

fn captured_frame(producer: bray_bound_tree::BoundExecutionSite, transition: Arc<bray_bound_tree::StorageDeferredTransition>)
    -> crate::ExecutionCondition
{
    crate::ExecutionCondition::result().with_deferred_value(Arc::new(crate::ExecutionDeferredValue {
        producers: [(producer.into(), crate::ExecutionDeferredInvocation {
            callee: None, transition: Some(transition), entry: Default::default(),
            completed: None,
            reports: Default::default(), selection: Default::default(),
        })].into(),
        unknown: None,
    }))
}

#[test]
fn report_payload_dependencies_survive_publication_move_and_join() {
    let ([observed, moved, dependency, _], origins) = places();

    let call = origins[0].into();
    let observed = Target::storage(observed);
    let payload = observed.project(&[BUFFER_POINTER]);
    let moved_root = Target::storage(moved.clone());
    let moved_payload = moved_root.project(&[BUFFER_POINTER]);

    let report = Arc::new(bray_bound_tree::ReportDependencies {
        subjects: [bray_bound_tree::BoundDependencySubject::Storage(dependency.root)].into(),
        ..Default::default()
    });

    let mut state = State::default();

    state.stage_report(call, observed.clone(), payload.clone(), Arc::clone(&report), true);
    assert!(!state.has_value(&observed));
    state.complete_transition(call);
    assert!(state.has_value(&observed));

    state.transfer_value(&observed, moved);
    state.retire_region(&observed, Retirement::Storage);

    let (transferred, complete) = state.report_dependencies(&moved_payload)
        .expect("move retains the report payload");

    assert!(Arc::ptr_eq(transferred, &report));
    assert!(complete);

    state.take_retirements();
    state.merge(&State::default(), &Default::default(), &Default::default());

    let (possible, complete) = state.report_dependencies(&moved_payload)
        .expect("an opaque alternative retains the established report dependencies");

    assert!(Arc::ptr_eq(possible, &report));
    assert!(!complete);

    state.retire_region(&moved_root, Retirement::Storage);

    assert!(state.report_dependencies(&moved_payload).is_none());
    assert!(state.carried_reports(|_| true).next().is_none());
}

#[test]
fn returned_allocations_publish_once_and_keep_alias_groups_across_replay() {
    let ([allocation, first, second, other], origins) = places();

    let call = origins[0].into();
    let mut state = State::default();
    let other_allocation = Place { root: allocation.root, projections: [BUFFER_POINTER].into() };

    state.begin_transition(call);
    state.stage_allocation_reference(call, Target::storage(first.clone()), allocation.clone(), &[]);
    state.stage_allocation_reference(call, Target::storage(second.clone()), allocation.clone(), &[]);
    state.stage_allocation_reference(call, Target::storage(other.clone()), other_allocation, &[]);

    assert!(state.referenced_regions(&Target::storage(first.clone())).is_none());
    state.complete_transition(call);

    let target = state.value_targets(&Target::storage(first.clone())).expect("first allocated address")[0].clone();
    let alias = state.value_targets(&Target::storage(second.clone())).expect("aliased allocated address")[0].clone();
    let distinct = state.value_targets(&Target::storage(other)).expect("separate allocated address")[0].clone();

    assert_eq!(target, alias);
    assert_eq!(state.target_relationship(&target, &distinct), bray_bound_tree::StorageRelationship::Disjoint);
    assert!(!state.initialized(&target, error_type()), "freshness grants no initialized-memory authority");

    let stale = Place { root: second.root, projections: [BUFFER_POINTER].into() };

    state.stage_slot(call, &second, stale.clone());
    state.complete_transition(call);
    state.begin_transition(call);
    state.stage_allocation_reference(call, Target::storage(first.clone()), allocation, &[]);
    state.complete_transition(call);

    assert!(!state.value_targets(&Target::storage(first)).expect("new allocation")[0].previous);

    assert!(state.referenced_regions(&Target::storage(stale)).expect("old raw address remains stored")
        .iter().all(|target| target.previous), "replay cannot revive earlier allocations");
}

fn addressed_reference(
    ty: bray_symbols::TypeId,
    kind: bray_symbols::DependencyReferenceKind,
) -> bray_bound_tree::BoundCallReference {
    let entry = bray_symbols::DependencySubject::root(bray_symbols::DependencySubjectRoot::Parameter(
        bray_symbols::SymbolOrdinal::new(0),
    ));

    let exit = bray_symbols::DependencySubject::root(bray_symbols::DependencySubjectRoot::Result);

    bray_bound_tree::BoundCallReference::new(
        bray_symbols::DependencyBorrowRelation::new(Some(entry), exit, kind),
        false,
        Some(ty),
    )
}

#[test]
fn retained_address_bits_keep_their_targets_without_preserving_pointee_state() {
    let ([pointer, referent, ..], origins) = places();

    let mut state = State::default();

    state.address(referent.clone(), pointer.clone(), error_type(), origins[0]);

    let target = Target::storage(pointer.clone());
    let reached = state.value_targets(&target).expect("stored address").to_vec();

    assert!(state.initialized(&target, error_type()));

    let invalidation = bray_bound_tree::StorageInvalidation::All {
        stable_descriptors: Default::default(),
        stable_values: [(pointer.clone(), bray_bound_tree::StorageObservationKind::Address)].into(),
        stable_reference_slots: Default::default(),
        normal_snapshots: Default::default(), stable_pointees: Default::default(),
    };

    state.apply_effects(None, &invalidation, origins[1].into());

    assert_eq!(state.value_targets(&target).as_deref(), Some(reached.as_slice()));
    assert!(!state.initialized(&target, error_type()), "retaining pointer bits does not retain reached data");
    assert!(invalidation.invalidates_path(&pointer), "address survival does not shield arbitrary observations of its reachable state");
}

#[test]
fn cleanup_retains_its_value_scope_and_follows_actual_contained_references() {

    for references_sibling in [false, true] {
        let ([owner, left_source, right_source, _], origins) = places();

        let left = Place { root: owner.root, projections: [StorageProjection::TupleElement(bray_symbols::SymbolOrdinal::new(0))].into() };
        let right = Place { root: owner.root, projections: [StorageProjection::TupleElement(bray_symbols::SymbolOrdinal::new(1))].into() };

        let mut state = State {
            owned_storage: vec![owner.root, left_source.root, right_source.root].into(),
            ..State::default()
        };

        state.address(left_source.clone(), left.clone(), error_type(), origins[0]);
        state.address(if references_sibling { left.clone() } else { right_source.clone() }, right.clone(), error_type(), origins[1]);

        let effects = state.cleanup_effects(Some(&Target::storage(right.clone())), error_type());

        assert!(effects.iter().filter_map(super::Effect::mutation)
            .all(|(_, kind)| kind == DependencyMutationKind::Dispose(error_type())));

        let protected = [left.clone()];
        let stable = state.unexposed_storage(&protected, [Target::storage(right.clone())]).cloned().collect();

        let invalidation = bray_bound_tree::StorageInvalidation::All {
            stable_descriptors: stable, stable_values: Default::default(), stable_reference_slots: Default::default(), normal_snapshots: Default::default(), stable_pointees: Default::default(),
        };

        state.apply_effects(Some(&effects), &invalidation, origins[2].into());
        assert_eq!(state.referent(&left, error_type()), (!references_sibling).then_some(left_source));
        assert_eq!(state.referent(&right, error_type()), None);
    }
}

#[test]
fn projected_region_types_keep_array_elements_distinct_from_array_valued_cells() {
    let values = semantic_values();
    let element = tuple_type([]);
    let array = array_type(element, 2);
    let generator = values.intern_type(bray_symbols::TypeData::Generator(array)).unwrap();

    let ([owner, ..], _, capability) = borrowed_places();

    let (unit, _) = expression_unit(owner.root.unit(), |_, _| Vec::new());

    let context = TestCheckerContext::new(false);
    let entry = callable_entry(unit.key());
    let request = crate::CheckerUnitView::new(&unit, &entry, &context);
    let mut state = State::default();
    let owner = Target::storage(owner);

    state.borrow_target_storage(values, capability, [owner.clone()], generator);

    let backing = owner.project(&[StorageProjection::OwnedTarget]);

    state.observe_effect_types(&[Effect::bounded(backing.clone(), DependencyMutationKind::Replace(array), None)]);

    assert_eq!(state.declared_type(&backing), None, "a possible typed write grants no backing layout");

    state.observe_effect_types(&[super::Effect::at(backing.clone(), bray_symbols::DependencyMutationKind::Replace(array), None)]);

    let cell = backing.project(&[StorageProjection::ElementFromStart(
        bray_symbols::SymbolOrdinal::new(0),
    )]);

    let member = cell.project(&[StorageProjection::ElementFromStart(
        bray_symbols::SymbolOrdinal::new(1),
    )]);

    assert_eq!(state.projected_type(request, &backing).expect("checked backing type"), Some(array));
    assert_eq!(state.projected_type(request, &cell).expect("checked array-valued cell type"), Some(array));
    assert_eq!(state.projected_type(request, &member).expect("checked array member type"), Some(element));
    assert_eq!(state.declared_type(&cell), None);
}

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
fn a_checked_move_leaves_only_duties_that_already_existed_at_possible_aliases() {
    let ([pointer, first, other, _], origins) = places();

    let first = Target { previous: false, origin: Origin::entry(first, DependencyReferenceKind::Address) };
    let other = Target { previous: false, origin: Origin::entry(other, DependencyReferenceKind::Address) };
    let pointer = Target::storage(pointer);
    let ty = error_type();

    for has_existing_duty in [false, true] {
        let mut state = State::default();

        state.values.insert(pointer.clone(), AddressValue::one(first.clone()).into());

        let contents = state.contents.entry(other.clone()).or_default();

        if has_existing_duty {
            contents.record_write(ty);
            contents.pending.entry(ty).or_default().insert(origins[0]);
        }

        assert_eq!(state.target_relationship(&first, &other), StorageRelationship::PotentiallyOverlapping);
        assert_eq!(state.read(&pointer, ty, MemoryReadKind::Move, Some(origins[1]), None), MemoryOperationStatus::Valid);
        assert_eq!(state.contents[&first].consumed_types().collect::<Vec<_>>(), vec![ty]);
        assert_eq!(state.retirement_obligations(&other, &[]), has_existing_duty.then_some(vec![origins[0]]));
    }
}

#[test]
fn an_exact_access_records_one_duty_visible_through_possible_aliases() {
    let ([pointer, written, possible_alias, _], origins) = places();

    let owned_storage: std::sync::Arc<[_]> = [pointer.root].into();
    let pointer = Target::storage(pointer);
    let written = Target { previous: false, origin: Origin::entry(written, DependencyReferenceKind::Address) };
    let possible_alias = Target { previous: false, origin: Origin::entry(possible_alias, DependencyReferenceKind::Address) };
    let ty = error_type();

    for write in [false, true] {
        let mut state = State { owned_storage: owned_storage.clone(), ..Default::default() };

        state.values.insert(pointer.clone(), AddressValue::one(written.clone()).into());
        state.contents.entry(possible_alias.clone()).or_default();

        assert_eq!(state.target_relationship(&written, &possible_alias), StorageRelationship::PotentiallyOverlapping);

        let status = if write {
            state.write(&pointer, ty, origins[0], None)
        } else {
            state.read(&pointer, ty, MemoryReadKind::Copy, Some(origins[0]), None)
        };

        assert_eq!(status, MemoryOperationStatus::Valid);
        assert!(state.contents[&possible_alias].pending.is_empty());
        assert_eq!(state.retirement_obligations(&possible_alias, &[]), Some(vec![origins[0]]));

        let mut caller = state.clone();

        assert_eq!(state.read(&pointer, ty, MemoryReadKind::Move, None, None), MemoryOperationStatus::Valid);
        assert_eq!(state.retirement_obligations(&possible_alias, &[]), None);

        let prefix = state.retirement_discharges(&possible_alias).collect::<Vec<_>>();

        assert_eq!(prefix, [bray_bound_tree::StorageDischarge { cell: written.clone(), ty }]);
        assert_eq!(caller.retirement_obligations(&possible_alias, &prefix), None);

        let mut retirement = Effect::at(possible_alias.clone(), DependencyMutationKind::Retire, Some(ty))
            .with_retirement_discharges(prefix.iter().cloned());

        assert!(!retirement.references(&written, Retirement::Backing),
            "ending a backing lifetime does not erase the typed source coordinate");

        retirement.retire(&written, Retirement::Backing);
        assert!(retirement.references(&written, Retirement::Producer));
        retirement.retire(&written, Retirement::Producer);

        assert!(matches!(retirement, Effect::Mutation { address: Some(address), discharged: None, .. }
            if address == possible_alias));

        caller.contents.get_mut(&possible_alias).unwrap().pending.entry(ty).or_default().insert(origins[1]);

        assert_eq!(caller.retirement_obligations(&possible_alias, &prefix), Some(vec![origins[1]]));

        state.apply_effects(
            Some(&[Effect::bounded(possible_alias.clone(), DependencyMutationKind::Consume, None)]),
            &bray_bound_tree::StorageInvalidation::unknown(),
            origins[1].into(),
        );

        assert_eq!(state.retirement_discharges(&possible_alias).collect::<Vec<_>>(), prefix);

        let mut opaque = state.clone();

        opaque.apply_effects(Some(&[Effect::Unknown]), &bray_bound_tree::StorageInvalidation::unknown(), origins[2].into());

        assert!(opaque.retirement_discharges(&possible_alias).next().is_none());
        assert_eq!(state.write(&pointer, ty, origins[2], None), MemoryOperationStatus::Valid);
        assert!(state.retirement_discharges(&possible_alias).next().is_none());
    }
}

#[test]
fn typed_root_discharge_covers_nested_duties_after_source_retirement() {
    let ([root, unrelated, _, _], origins) = places();

    let root = Target::storage(root);
    let child = root.project(&[BUFFER_POINTER]);
    let unrelated = Target::storage(unrelated);
    let mut state = State::default();

    state.contents.entry(child).or_default().pending.entry(error_type()).or_default().insert(origins[0]);
    state.contents.entry(unrelated.clone()).or_default().pending.entry(error_type()).or_default().insert(origins[1]);

    let discharge = bray_bound_tree::StorageDischarge { cell: root.clone(), ty: tuple_type([]) };

    assert_eq!(state.retirement_obligations(&root, &[]), Some(vec![origins[0]]));
    assert_eq!(state.retirement_obligations(&root, &[discharge.clone()]), None);
    assert_eq!(state.retirement_obligations(&unrelated, &[discharge]), Some(vec![origins[1]]));
}

#[test]
fn consuming_and_deallocating_through_an_alias_updates_the_original_target() {
    let ([source, alias, _, _], origins) = places();

    let mut state = State::default();
    let ty = error_type();

    let allocation = state.fresh(source.clone(), origins[0]);

    state.transfer_value(&Target::storage(source.clone()), alias.clone());

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
        state.deallocate_slot(&Target::storage(alias.clone()), origins[2]),
        MemoryOperationStatus::OutstandingObligations
    );

    assert_eq!(
        state
            .initialization_origins(&Target::storage(alias.clone()))
            .collect::<Vec<_>>(),
        vec![origins[1]]
    );

    assert!(state.take_retirements().contains(&(allocation.clone(), Retirement::Producer)));

    let caller = state.clone();

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

    let discharged = state.retirement_discharges(&allocation).collect::<Vec<_>>();

    assert!(caller.retirement_obligations(&allocation, &discharged).is_none());

    let mut conditional = state.clone();

    conditional.merge(&caller, &Default::default(), &Default::default());

    let maybe_discharged = conditional.retirement_discharges(&allocation).collect::<Vec<_>>();

    assert_eq!(caller.retirement_obligations(&allocation, &maybe_discharged), Some(vec![origins[1]]));

    assert_eq!(
        state.deallocate_slot(&Target::storage(source.clone()), origins[2]),
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
        vec![origins[2].into()]
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
        state.deallocate_slot(&Target::storage(destination.clone()), origins[2]),
        MemoryOperationStatus::OutstandingObligations
    );

    state.deallocate_slot(&Target::storage(source.clone()), origins[2]);

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

    state.transfer_value(&Target::storage(source.clone()), alias.clone());
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

    state.transfer_value(&Target::storage(source.clone()), alias.clone());
    state.transfer_value(&Target::storage(unknown.clone()), source.clone());
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
    left.take_retirements();

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
        left.deallocate_slot(&Target::storage(source.clone()), origins[2]),
        MemoryOperationStatus::OutstandingObligations
    );

    right.deallocate_slot(&Target::storage(source.clone()), origins[2]);
    left.take_retirements();
    right.take_retirements();
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
        vec![origins[2].into()]
    );
}

#[test]
fn joined_targets_require_all_possible_contents_and_weak_writes_do_not_initialize_both() {
    let ([first, second, alias, _], origins) = places();

    let mut left = State::default();

    left.fresh(first.clone(), origins[0]);
    left.fresh(second.clone(), origins[1]);
    left.transfer_value(&Target::storage(first.clone()), alias.clone());

    left.take_retirements();

    let mut right = left.clone();

    right.transfer_value(&Target::storage(second.clone()), alias.clone());
    assert!(left.merge(&right, &Default::default(), &Default::default()));

    let targets = left.value_targets(&Target::storage(alias.clone())).unwrap();

    for target in &targets {
        left.contents.entry(target.clone()).or_default().storage_type = Some(error_type());
    }

    let path = bray_bound_tree::StorageReferencePath::new([].into(), [bray_bound_tree::StorageReferenceStep::ReadBorrow {
        index: 0, target: error_type(), mode: BorrowKind::Shared,
    }].into());

    assert_eq!(left.reference_locations(crate::test_support::semantic_values(),
        Target::storage(alias.clone()), &path, None), Some(targets.clone()));

    assert!(left.value_target(&Target::storage(alias.clone()), error_type()).is_none(),
        "a finite choice does not identify one cell");

    let mut staged = left.clone();
    let transition = origins[2].into();

    staged.begin_transition(transition);

    for target in &targets {
        super::Source::Value(Target::storage(first.clone())).stage(&mut staged, transition, target.clone());
    }

    staged.weaken_transition(transition);
    staged.write_locations(&AddressValue::from_targets(targets.iter().cloned()), error_type(), origins[2], None);
    staged.complete_transition(transition);

    for target in &targets {
        assert!(staged.value_targets(target).is_none(), "neither possible destination receives a definite pointer value");

        assert!(!staged.contents[target].is_initialized(error_type()),
            "one selected write cannot initialize both candidates");
    }

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
            left.deallocate_slot(&Target::storage(target.clone()), origins[3]),
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
        left.deallocate_slot(&Target::storage(first.clone()), origins[3]),
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

    state.transfer_value(&Target::storage(source.clone()), alias.clone());
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
        state.deallocate_slot(&Target::storage(alias.clone()), origins[2]),
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

    state.transfer_value(&Target::storage(other.clone()), source.clone());

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

    left.take_retirements();

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
    state.transfer_value(&Target::storage(allocation.clone()), field(&aggregate));
    state.transfer_value(&Target::storage(aggregate.clone()), destination.clone());

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

    state.transfer_value(&Target::storage(unknown.clone()), destination.clone());
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
        state.deallocate_slot(&Target::storage(interior.clone()), origins[2]),
        MemoryOperationStatus::OutstandingObligations
    );

    state.consume(&Target::storage(allocation.clone()), error_type());
    state.deallocate_slot(&Target::storage(allocation.clone()), origins[2]);

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
        state.deallocate_slot(&Target::storage(source.clone()), origins[2]),
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
    state.transfer_value(&Target::storage(replacement.clone()), element.clone());
    state.transfer_value(&Target::storage(element.clone()), loaded.clone());
    assert!(state.references(&Target::storage(loaded.clone())).is_unknown());
}
#[test]
fn direct_moves_and_reinitialization_change_existing_pointer_targets() {
    let ([owner, pointer, alias, _], origins) = places();

    let mut state = State::default();
    let ty = error_type();

    state.address(owner.clone(), pointer.clone(), ty, origins[0]);
    state.transfer_value(&Target::storage(pointer.clone()), alias.clone());
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
    state.transfer_value(&Target::storage(pointer.clone()), alias.clone());
    assert_eq!(state.referent(&alias, ty), Some(owner));
    assert_eq!(state.referent(&alias, other), None);

    assert_eq!(
        state.reached_storage(&Target::storage(alias.clone()))[0].1,
        Some(ty)
    );

    state.begin_borrow(
        capability,
        state
            .referent(&alias, ty)
            .expect("fixture `referent` call must succeed"),
        other,
    );

    state.retain_borrow(capability, alias.clone());
    state.borrow_view(&alias, other);
    state.expose_address(&alias, other, origins[0]);
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

    state.invalidate(
        &bray_bound_tree::StorageInvalidation::unknown(),
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
fn unresolved_replacement_discards_previous_reference_bounds_without_inventing_root_aliases() {
    let ([owner, destination, ..], expressions) = places();

    let root = Target::storage(destination.clone());
    let field_path = [BUFFER_POINTER];
    let field = Place { root: destination.root, projections: field_path.into() };
    let call = expressions[0].into();

    for (complete, references) in [(true, vec![field_path.as_slice()]), (true, Vec::new()), (false, Vec::new())] {
        let mut state = State::default();

        state.address(owner.clone(), field.clone(), error_type(), expressions[0]);
        state.begin_transition(call);
        state.stage_unresolved_value(call, root.clone(), complete, references.iter().copied());
        state.complete_transition(call);

        assert!(state.referenced_regions(&Target::storage(field.clone())).is_none(),
            "arbitrary replacement cannot keep the previous field's region bound");

        assert!(state.observed_references().all(|(slot, _, _)| slot != &root),
            "aggregate and scalar roots are not pointer slots");

        assert_eq!(state.transitive_regions([root.clone()], |_, _| true).1,
            !complete || !references.is_empty(), "unknown fields and opaque contents retain uncertainty");
    }
}

#[test]
fn original_entry_references_survive_slot_reassignment_without_proving_disjointness() {
    let ([first, second, captured, _], origins) = places();

    let mut state = State::default();

    state.entry_reference(first.clone());
    state.entry_reference(second.clone());
    state.transfer_value(&Target::storage(first.clone()), captured.clone());
    state.transfer_value(&Target::storage(second.clone()), first.clone());

    let exact = state.exact_references(&captured).collect::<Vec<_>>();

    assert_eq!(
        exact,
        vec![(&captured, &super::model::Origin::entry(first.clone(), bray_symbols::DependencyReferenceKind::Address))]
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
        Some(bray_bound_tree::StorageAbsence::NoPayload),
        Some(bray_bound_tree::StorageAbsence::Moved),
    ] {
        for present_first in [false, true] {
            let mut present = State::default();

            present.entry_reference(source.clone());
            present.transfer_value(&Target::storage(source.clone()), payload.clone());

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
                        == Some(bray_bound_tree::StorageAbsence::NoPayload)
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

            if absent && present_first {
                present.stage_no_payload_alternative(call, Target::storage(payload.clone()));
            }

            present.stage_slot(call, &source, payload.clone());

            if absent && !present_first {
                present.stage_no_payload_alternative(call, Target::storage(payload.clone()));
            }

            present.forget(source.root);

            let mut other = State::default();

            if absent {
                other.stage_no_payload_alternative(call, Target::storage(payload.clone()));
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

            assert_eq!(joined.exit_references(payload.root, &Default::default()).empty_slots().count(), 0,
                "an inactive payload is not a live null pointer alternative");

            assert_eq!(
                joined
                    .referenced_regions(&Target::storage(payload.clone()))
                    .is_some(),
                absent
            );

            assert_eq!(joined.exact_references(&payload).count(), usize::from(absent),
                "only inactive paths can be omitted from the live value's identity");
        }
    }
}

#[test]
fn exposed_entry_reference_keeps_its_origin_without_claiming_a_physical_place() {
    let ([source, pointer, _, _], origins) = places();

    let mut state = State::default();

    state.entry_reference(source.clone());
    state.transfer_value(&Target::storage(source.clone()), pointer.clone());
    state.expose_address(&pointer, error_type(), origins[0]);

    assert_eq!(
        state.exact_references(&pointer).collect::<Vec<_>>(),
        vec![(&pointer, &super::model::Origin::entry(source, bray_symbols::DependencyReferenceKind::Address))]
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
    let ([parameter, field_reference, address, copied], origins, capability) = borrowed_places();

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

    state.entry_reference_at(slot.clone(), 1, target.clone(), Some(ty), bray_symbols::DependencyReferenceKind::Address);
    state.transfer_value(&target, field_reference.clone());
    state.borrow_target_storage(crate::test_support::semantic_values(), capability, [target], ty);
    state.retain_borrow(capability, address.clone());
    state.transfer_value(&Target::storage(address.clone()), copied.clone());

    assert_eq!(
        state
            .exact_references(&field_reference)
            .next()
            .expect("fixture iterator must provide its expected item")
            .1,
        &super::model::Origin::entry(slot, bray_symbols::DependencyReferenceKind::Address)
    );

    assert_ne!(
        state
            .exact_references(&address)
            .next()
            .expect("fixture iterator must provide its expected item")
            .1,
        state
            .exact_references(&field_reference)
            .next()
            .expect("fixture iterator must provide its expected item")
            .1
    );

    state.clear(&parameter);
    assert_eq!(state.exact_references(&copied).count(), 1);

    assert!(
        state
            .located_storage(&Target::storage(copied.clone()))
            .is_none()
    );

    let values = crate::test_support::semantic_values();

    let stored = values.intern_type(bray_symbols::TypeData::Borrow {
        kind: bray_symbols::BorrowKind::Shared, target: ty,
    }).unwrap();

    let aggregate = values.intern_type(bray_symbols::TypeData::Tuple([stored].into())).unwrap();

    let mut layered = State::default();

    layered.begin_entry_borrow(capability, parameter.clone(), ty);

    let cell = layered.borrow_target(capability, ty).unwrap();

    layered.entry_reference_at(parameter.clone(), 2, cell.clone(), Some(aggregate), bray_symbols::DependencyReferenceKind::Address);

    let route = bray_bound_tree::StorageReferencePath::new(
        [StorageProjection::AddressTarget, projection].into(), [
            bray_bound_tree::StorageReferenceStep::ReadBorrow { index: 0, target: ty, mode: BorrowKind::Shared },
            bray_bound_tree::StorageReferenceStep::ReadAddress { index: 0, target: aggregate },
        ].into());

    let reached = layered.reference_location(values, Target::storage(parameter.clone()), &route, Some(cell.clone())).unwrap();

    assert_eq!(reached.projections(), &[projection], "a language borrow followed by a raw-address load consumes one route marker");

    let path = bray_bound_tree::StorageReferencePath::new(
        [StorageProjection::AddressTarget, projection].into(), [
            bray_bound_tree::StorageReferenceStep::ReadAddress { index: 0, target: aggregate },
        ].into());

    let operands = layered.follow_effect_path(
        values, vec![bray_bound_tree::StorageMutationTarget::location(cell.clone())], &path,
    ).unwrap();

    assert_eq!(operands, [bray_bound_tree::StorageMutationTarget::location(reached)]);

    assert!(layered.follow_effect_path(
        values, vec![bray_bound_tree::StorageMutationTarget::bounded(cell)], &path,
    ).is_none(), "a bounding region cannot identify a stored pointer");

    let mut state = State { owned_storage: [parameter.root, copied.root].into(), ..State::default() };

    state.entry_reference_at(parameter.clone(), 1, Target::storage(parameter.clone()), Some(aggregate),
        bray_symbols::DependencyReferenceKind::Address);

    let location = state.reference_location(values, Target::storage(parameter.clone()), &path, None).unwrap();
    let field = Place { root: parameter.root, projections: [StorageProjection::AddressTarget, projection].into() };

    assert!(!field.is_inline(), "a raw-address load cannot be mistaken for inline owner storage");
    state.entry_reference_at(field.clone(), 1, location.clone(), Some(ty), bray_symbols::DependencyReferenceKind::Borrow);

    let original = state.value_target(&location, ty).unwrap();

    state.transfer_value(&location, copied.clone());
    state.write_target(location.clone(), stored, origins[0], None);

    assert!(state.referenced_regions(&location).is_none(), "a write retires the entry field record");

    assert_eq!(state.referenced_regions(&Target::storage(copied.clone())), Some(vec![original.clone()]),
        "the copied pointer retains its actual old referent");

    assert!(!state.initialized(&Target::storage(copied), ty), "provenance supplies no initialized authority");

    let other = Target { previous: false, origin: super::model::Origin::entry(field_reference,
        bray_symbols::DependencyReferenceKind::Address) };

    assert_ne!(state.target_relationship(&original, &other), bray_bound_tree::StorageRelationship::Disjoint,
        "an incoming field pointer can alias another input");
}

#[test]
fn entry_targets_alias_other_inputs_but_distinguish_fields_of_the_same_referent() {
    let ([first, second, _, _], _) = places();

    let target = |reference| super::model::Target {
        origin: super::model::Origin::entry(reference, bray_symbols::DependencyReferenceKind::Address),
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
        super::model::AddressValue::empty().into(),
    );

    state.transfer_value(&Target::storage(source.clone()), destination.clone());

    assert_eq!(state.exact_references(&destination).count(), 2);

    state.transfer_value(&Target::storage(destination.clone()), pointer.clone());

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

    state.borrow_target_storage(crate::test_support::semantic_values(), capability, [selected], error_type());
    state.retain_borrow(capability, pointer.clone());
    state.expose_address(&pointer, error_type(), origins[1]);
    assert_eq!(state.exact_references(&pointer).count(), 0);
    assert!(!state.initialized(&Target::storage(pointer.clone()), error_type()));
}

#[test]
fn aggregate_mutation_revokes_reference_fields_without_address_metadata() {
    let ([aggregate, source, _, _], origins) = places();

    let field = Place {
        root: aggregate.root,
        projections: [StorageProjection::TupleElement(
            bray_symbols::SymbolOrdinal::new(0),
        )]
        .into(),
    };

    let mut state = State::default();

    state.entry_reference(source.clone());
    state.transfer_value(&Target::storage(source.clone()), field.clone());
    assert!(state.has_stored_value(&field));

    assert!(
        !state
            .contents
            .contains_key(&super::model::Target::storage(field.clone()))
    );

    state.apply_effects(
        Some(&[super::Effect::at(super::model::Target::storage(aggregate), bray_symbols::DependencyMutationKind::Write, None)]),
        &bray_bound_tree::StorageInvalidation::unknown(),
        origins[0].into(),
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
fn stored_whole_sources_follow_transfers_and_weak_replay() {
    let ([old, incoming, destination, copied], origins) = places();

    let mut state = State::default();
    let destination = Target::storage(destination);
    let incoming_target = Target::storage(incoming.clone());

    state.seed_whole_source(destination.clone(), Target::storage(old.clone()));
    state.seed_whole_source(incoming_target.clone(), incoming_target.clone());

    assert!(state.value_targets(&destination).is_none(), "a whole-value source is not a reference");

    let call = origins[0].into();

    state.begin_transition(call);
    state.stage_whole_sources(call, destination.clone(), [super::Source::Value(incoming_target.clone())], true, true);
    state.clear_value(&destination);
    state.complete_transition(call);

    let value = state.whole_sources().find(|(target, _, _)| **target == destination).unwrap();

    assert_eq!(value.1, &BTreeSet::from([Target::storage(old.clone()), incoming_target.clone()]));
    assert!(value.2, "a complete weak union retains its complete prior and incoming sources");

    state.begin_transition(call);
    state.stage_whole_sources(call, destination.clone(), [], false, true);
    state.complete_transition(call);

    assert!(state.transitive_regions([destination.clone()], |_, _| true).1,
        "an incomplete stored subtree cannot prove reference-free reachability");

    let mut escaped = state.clone();

    escaped.owned_storage = std::sync::Arc::from([old.root]);
    escaped.escape_targets([destination.clone()]);
    escaped.clear_value(&destination);

    assert!(escaped.contents.get(&incoming_target).is_some_and(|contents| contents.escaped),
        "an unknown escaped reference retains possible external targets after its source dies");

    assert!(!escaped.contents.get(&Target::storage(old.clone())).is_some_and(|contents| contents.escaped),
        "unknown references cannot publish an unexposed private owner");

    let untracked = Target::storage(copied.clone());

    state.begin_transition(call);
    state.stage_whole_sources(call, untracked.clone(), [super::Source::Value(incoming_target.clone())], true, true);
    state.complete_transition(call);

    assert!(!state.whole_sources().find(|(target, _, _)| **target == untracked).unwrap().2,
        "a weak update cannot infer an empty prior subtree");

    state.transfer_value(&destination, copied.clone());

    let copied = Target::storage(copied);

    assert_eq!(state.whole_sources().find(|(target, _, _)| **target == copied).unwrap().1,
        &BTreeSet::from([Target::storage(old.clone()), incoming_target.clone()]));

    state.begin_transition(call);
    state.stage_whole_sources(call, destination.clone(), [super::Source::Value(incoming_target.clone())], true, false);
    state.complete_transition(call);

    let value = state.whole_sources().find(|(target, _, _)| **target == destination).unwrap();

    assert_eq!(value.1, &BTreeSet::from([incoming_target.clone()]));
    assert!(value.2);

    state.begin_transition(call);

    let exact = Target::storage(old);

    state.stage_address(call, exact.clone(), destination.clone(),
        &addressed_reference(error_type(), bray_symbols::DependencyReferenceKind::Borrow));

    state.stage_whole_sources(call, destination.clone(), [super::Source::Value(incoming_target.clone())], false, true);
    state.complete_transition(call);

    assert_eq!(state.value_target(&destination, error_type()), Some(exact.clone()),
        "possible whole-value carriers must not weaken an established exact descriptor");

    state.begin_transition(call);
    state.stage_whole_sources(call, destination.clone(), [super::Source::Value(exact.clone())], true, false);
    state.complete_transition(call);

    assert!(!state.whole_sources().find(|(target, _, _)| **target == destination).unwrap().2,
        "an unrecorded source cannot make a complete stored subtree");

    for (descriptor_first, known) in [(true, true), (false, true), (true, false), (false, false)] {
        let mut state = State::default();
        let carried = copied.clone();

        state.values.insert(incoming_target.clone(), AddressValue::one(carried.clone()).into());
        state.seed_whole_source(incoming_target.clone(), incoming_target.clone());
        state.begin_transition(call);

        let descriptor = addressed_reference(error_type(), bray_symbols::DependencyReferenceKind::Borrow);

        let produce = |state: &mut State| {
            if known { state.stage_address(call, exact.clone(), destination.clone(), &descriptor); }
            else { state.stage_unknown(call, destination.clone()); }
        };

        if descriptor_first { produce(&mut state); }
        state.stage_whole_sources(call, destination.clone(), [super::Source::Value(incoming_target.clone())], true, false);

        if !descriptor_first {
            assert!(state.pending[&call].values[&destination].address.is_none(),
                "a dependency bound never becomes the output's evaluated address");
        }

        if !descriptor_first { produce(&mut state); }
        state.complete_transition(call);

        assert!(state.whole_sources().find(|(target, _, _)| **target == destination).unwrap().2,
            "a staged descriptor and a complete whole source must retain completeness in either order");

        assert_eq!(state.value_target(&destination, error_type()), known.then(|| exact.clone()));

        assert_eq!(state.contained_regions(&destination).any(|target| *target == carried), !known,
            "uncertain evaluated addresses retain explicit carried bounds in either staging order");
    }

    let first = StorageProjection::TupleElement(bray_symbols::SymbolOrdinal::new(0));
    let second = StorageProjection::TupleElement(bray_symbols::SymbolOrdinal::new(1));
    let element = |index| StorageProjection::ElementFromStart(bray_symbols::SymbolOrdinal::new(index));
    let first_target = Target::storage(incoming);
    let second_target = destination.clone();
    let mut state = State::default();

    state.values.insert(exact.project(&[element(0), first]),
        super::model::AddressValue::one(first_target.clone()).into());

    state.values.insert(exact.project(&[element(1), second]),
        super::model::AddressValue::one(second_target.clone()).into());

    state.begin_transition(call);

    state.stage_whole_sources(call, copied.clone(),
        [super::Source::Value(exact.project(&[StorageProjection::AnyElement]))], true, false);

    state.complete_transition(call);

    for (field, target) in [(first, first_target), (second, second_target)] {
        let references = state.values[&copied.project(&[field])].reference().unwrap();

        assert!(references.targets.contains(&target));
        assert!(references.is_bounded(), "an arbitrary element cannot establish an exact descriptor");
    }

    state.begin_transition(call);

    state.stage_reference_regions(call, exact.project(&[StorageProjection::AnyElement]),
        copied.storage_path().unwrap().clone(), error_type(), [destination.clone()]);

    state.clear_value(&exact);
    state.complete_transition(call);

    for (index, field) in [(0, first), (1, second)] {
        let references = state.values[&exact.project(&[element(index), field])].address.as_ref().unwrap();

        assert!(references.is_bounded(), "a possibly overwritten element is retained only as an alternative");
    }

    let mut state = State::default();

    state.values.insert(exact.clone(), super::model::AddressValue::one(exact.clone()).into());

    state.values.insert(exact.project(&[element(0), first]),
        super::model::AddressValue::one(destination.clone()).into());

    let source = state.capture_reached_value(&exact);

    state.include_captured_bounds(&copied, &source);

    let possible = state.capture_reached_value(&copied);

    assert!(possible.known_address().is_none());
    assert!(possible.address_regions().unwrap().contains(&exact));

    let bounds = state.capture_element_bounds(std::slice::from_ref(&exact), &copied);

    assert!(bounds.known_address().is_none());
    assert!(bounds.project(first).known_address().is_none());
    state.publish_captured_value(&copied, &bounds);

    assert!(state.values[&copied].address_bounds.as_ref().unwrap().targets.contains(&exact));
    assert!(state.values[&copied.project(&[first])].address_bounds.as_ref().unwrap().targets.contains(&destination));

    assert!(!state.values.contains_key(&copied.project(&[StorageProjection::AnyElement])),
        "a popped element cannot retain the buffer's collection selector");

    let mut state = State::default();

    state.values.insert(exact.project(&[element(0), first]),
        super::model::AddressValue::one(exact.clone()).into());

    state.values.insert(exact.project(&[StorageProjection::AnyElement, first]),
        super::model::AddressValue::one(destination.clone()).into());

    state.begin_transition(call);

    state.stage_whole_sources(call, copied.clone(),
        [super::Source::Value(exact.project(&[element(0)]))], true, false);

    state.complete_transition(call);

    let references = state.values[&copied.project(&[first])].reference().unwrap();

    assert!(references.targets.contains(&exact));
    assert!(references.targets.contains(&destination));
    assert!(references.is_bounded(), "a wildcard summary is only a possible exact-element value");
}

#[test]
fn consumed_inline_value_transfers_after_borrow_plan_completion() {
    let ([owner, destination, pointer, _], origins) = places();

    let mut state = State::default();

    let payload =
        Target::storage(owner.clone()).project(&[StorageProjection::UninitPayload]);

    state.uninit(payload.clone(), error_type());
    state.write_value(payload, error_type(), origins[0], None);
    state.consume_target(Target::storage(owner.clone()), error_type());
    state.transfer_value(&Target::storage(owner.clone()), destination.clone());
    state.finish_value_transfers();

    state.point_at(
        Target::storage(destination).project(&[StorageProjection::UninitPayload]),
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
        Target::storage(first.clone()).project(&[StorageProjection::UninitPayload]);

    let second_payload =
        Target::storage(second.clone()).project(&[StorageProjection::UninitPayload]);

    state.uninit(first_payload.clone(), error_type());
    state.uninit(second_payload, error_type());
    state.write_value(first_payload, error_type(), origins[0], None);

    let occurrence = origins[1].into();

    state.stage_value(
        occurrence,
        &Target::storage(first),
        Target::storage(result.clone()),
    );

    state.stage_value(
        occurrence,
        &Target::storage(second),
        Target::storage(result.clone()),
    );

    state.complete_transition(occurrence);

    state.point_at(
        Target::storage(result).project(&[StorageProjection::UninitPayload]),
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
        .value_targets(&Target::storage(allocation.clone()))
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
        Target::storage(first.clone()).project(&[StorageProjection::UninitPayload]);

    state.uninit(payload.clone(), error_type());
    state.write_value(payload.clone(), error_type(), origins[0], None);

    state.point_at(
        payload.clone(),
        pointer.clone(),
        error_type(),
        bray_symbols::DependencyReferenceKind::Address,
    );

    let replacement_payload =
        Target::storage(replacement.clone()).project(&[StorageProjection::UninitPayload]);

    state.uninit(replacement_payload.clone(), error_type());
    state.write_value(replacement_payload, error_type(), origins[1], None);
    state.transfer_value(&Target::storage(replacement.clone()), first);

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

    let field = StorageProjection::TupleElement(bray_symbols::SymbolOrdinal::new(0));

    for projections in [Vec::new(), vec![field]] {
        let mut state = State::default();
        let payload = Target::storage(owner.clone()).project(&[StorageProjection::UninitPayload]);

        state.begin_borrow(capability, referent.clone(), error_type());
        state.uninit(payload.clone(), error_type());
        state.write_value(payload.clone(), error_type(), origins[0], None);

        state.write_value(payload.project(&projections), error_type(), origins[0],
            Some(super::Source::Borrow(capability)));

        state.point_at(payload, pointer.clone(), error_type(), bray_symbols::DependencyReferenceKind::Address);

        // The opaque aggregate type needs checked whole-value initialization evidence.
        let proven = (!projections.is_empty()).then_some(origins[1]);

        assert_eq!(state.read(&Target::storage(pointer.clone()), error_type(), MemoryReadKind::Move, proven, Some(result.clone())),
            MemoryOperationStatus::Valid);

        let copied = Place { root: result.root, projections: projections.clone().into() };

        assert_eq!(state.referent(&copied, error_type()), Some(referent.clone()));

        if !projections.is_empty() {
            assert!(state.observed_references().all(|(slot, _, _)| slot != &Target::storage(result.clone())),
                "the copied aggregate retains its field's loan without inventing a root alias");
        }
    }
}

#[test]
fn ending_inline_storage_lifetime_expires_its_exposed_payload_address() {
    let ([owner, pointer, referent, sibling], origins) = places();

    let mut state = State::default();

    let payload =
        Target::storage(owner.clone()).project(&[StorageProjection::UninitPayload]);

    state.uninit(payload.clone(), error_type());
    state.write_value(payload.clone(), error_type(), origins[0], None);

    let sibling_payload = Target::storage(sibling.clone()).project(&[StorageProjection::UninitPayload]);

    state.uninit(sibling_payload.clone(), error_type());
    state.write_value(sibling_payload.clone(), error_type(), origins[1], None);

    let field = payload.project(&[BUFFER_POINTER]);
    let referent = Target::storage(referent);

    state.values.insert(field.clone(), AddressValue::one(referent.clone()).into());

    state.point_at(
        payload,
        pointer.clone(),
        error_type(),
        bray_symbols::DependencyReferenceKind::Address,
    );

    state.forget(owner.root);
    state.retain_live(&BTreeSet::from([pointer.root, sibling.root]));

    let sibling_address = AddressValue::one(sibling_payload);

    assert!(state.initialized(&Target::storage(pointer.clone()), error_type()));
    assert!(state.initialized_targets(&sibling_address, error_type()));
    assert_eq!(state.value_targets(&field), Some(vec![referent]), "the surviving aggregate address retains its stored pointer fields");

    state.end_storage(&[owner.root]);

    assert!(!state.initialized(&Target::storage(pointer.clone()), error_type()));
    assert!(state.value_targets(&field).is_none(), "physical storage end still expires the old field");
    assert!(state.initialized_targets(&sibling_address, error_type()));
}

#[test]
fn unlocated_aggregate_writes_keep_possible_origins_without_growing_paths() {
    let ([source, referent, destination, pointer], origins) = places();

    let mut state = State::default();
    let field = StorageProjection::TupleElement(bray_symbols::SymbolOrdinal::new(0));

    state.entry_reference(referent.clone());

    state.transfer_value(
        &Target::storage(referent),
        Place {
            root: source.root,
            projections: [field].into(),
        },
    );

    state.uninit(Target::storage(destination), error_type());

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

    assert!(state.values.values().all(|value| value.reference().is_some_and(|references| references.is_unknown())));

    assert!(
        state
            .values
            .values()
            .all(|value| value.reference().is_some_and(|references| !references.targets.is_empty()))
    );

    assert!(
        state
            .contents
            .values()
            .all(|contents| !contents.has_initialization())
    );
}

#[test]
fn repeated_dereferences_have_distinct_entry_snapshots() {
    let ([root, _, _, _], _) = places();

    let ty = error_type();
    let mut state = State::default();
    let slot = Target::storage(root.clone());

    state.entry_reference_at(root.clone(), 1, slot.clone(), Some(ty), bray_symbols::DependencyReferenceKind::Address);

    let first = state.value_target(&slot, ty).expect("first entry target");

    state.entry_reference_at(root.clone(), 2, first.clone(), Some(ty), bray_symbols::DependencyReferenceKind::Address);

    let second = state.value_target(&first, ty).expect("second entry target");

    assert_ne!(first, second);
    assert!(!first.contains(&second));
    assert!(!second.contains(&first));
    assert!(second.observation_path().is_none());

    let path = bray_bound_tree::StorageReferencePath::new([].into(), [
        bray_bound_tree::StorageReferenceStep::ReadBorrow { index: 0, target: ty, mode: BorrowKind::Shared },
        bray_bound_tree::StorageReferenceStep::ReadBorrow { index: 0, target: ty, mode: BorrowKind::Shared },
    ].into());

    assert_eq!(state.reference_location(crate::test_support::semantic_values(), slot, &path, None), Some(second));
}

#[test]
fn slice_projection_retains_the_element_allocation_without_retyping_it() {
    let values = crate::test_support::semantic_values();

    let ([owner, view, nested, _], selectors) = places();

    let element = error_type();
    let slice = values.intern_type(bray_symbols::TypeData::Slice(element)).unwrap();
    let mut state = State::default();
    let owner = Target::storage(owner);
    let view = Target::storage(view);

    state.declare_owned_buffer(owner.clone(), owner.project(&[BUFFER_POINTER]), element);
    state.owned_region(&[owner.clone()], view.clone(), element, BUFFER_POINTER, false);

    let projection = StorageProjection::Element(selectors[0]);

    let path = bray_bound_tree::StorageReferencePath::new(
        [projection, BUFFER_POINTER].into(),
        [bray_bound_tree::StorageReferenceStep::ReadBorrow { index: 0, target: slice, mode: BorrowKind::Shared }].into(),
    );

    let backing = owner.project(&[StorageProjection::OwnedTarget]);
    let selected = backing.project(&[projection, BUFFER_POINTER]);

    assert_eq!(
        state.reference_location(values, view.clone(), &path, None),
        Some(selected.clone())
    );

    assert!(!selected.is_exact());
    assert_eq!(state.contents[&backing].storage_type, Some(element));
    assert!(state.value_target(&view, slice).is_none());

    let (_, _, capability) = borrowed_places();

    let mut alternate = State::default();

    alternate.borrow_target_storage(values, capability, [backing.clone()], slice);

    assert_eq!(alternate.declared_type(&backing), Some(element));

    let mut merged = state.clone();

    merged.merge(&alternate, &Default::default(), &Default::default());

    assert_eq!(merged.declared_type(&backing), Some(element));

    let nested_slot = nested.clone();
    let nested = Target::storage(nested);

    state.declare_owned_buffer(selected.clone(), selected.project(&[BUFFER_POINTER]), element);

    let call = selectors[1].into();
    let admitted = nested.clone();

    state.borrow_target_storage(values, capability, [selected.clone()], element);

    super::Source::Borrow(capability).stage(&mut state, call, admitted.clone());

    state.stage_returned_references(call, [(nested_slot, element)], |_| [selected.clone()]);
    state.complete_transition(call);

    assert_eq!(state.referenced_regions(&admitted), Some(vec![selected.clone()]),
        "admission carries an evaluated dynamic borrow without wrapping away its owned pointer");

    assert!(state.value_targets(&admitted).is_none(), "a dynamic selector remains inexact");

    assert_eq!(state.value_target(&admitted, element), Some(selected.clone()),
        "the current typed view retains its dynamic location without exact-cell authority");

    let mut region_only = state.clone();

    region_only.values.get_mut(&admitted).unwrap().address.as_mut().unwrap().bound();

    assert!(region_only.value_target(&admitted, element).is_none(),
        "a region bound alone supplies no typed layout");

    let owners = state.referenced_regions(&admitted).unwrap();

    state.owned_region(&owners, nested.clone(), element, BUFFER_POINTER, true);

    assert!(state.reference_location(values, nested.clone(), &path, None).is_none());

    let bound = selected.project(&[StorageProjection::OwnedTarget, projection, BUFFER_POINTER]);
    let captured = state.capture_reached_value(&nested);

    assert_eq!(state.captured_effect_targets(values, &captured, &path, false),
        Some(vec![bray_bound_tree::StorageMutationTarget::bounded(bound)]));

    assert!(state.value_targets(&nested).is_none());

    let wrong_slice = values.intern_type(bray_symbols::TypeData::Slice(slice)).unwrap();

    let wrong = bray_bound_tree::StorageReferencePath::new([].into(), [
        bray_bound_tree::StorageReferenceStep::ReadBorrow { index: 0, target: wrong_slice, mode: BorrowKind::Shared },
    ].into());

    assert!(state.reference_location(values, view.clone(), &wrong, None).is_none());

    state.values.get_mut(&view).unwrap().address.as_mut().unwrap().bound();

    assert!(state.reference_location(values, view.clone(), &path, None).is_none());

    let reference = state.values.get_mut(&view).unwrap().address.as_mut().unwrap();

        reference.forget_value();

    assert!(state.reference_location(values, view, &path, None).is_none());

    for (recorded, regions) in [
        (super::model::AddressValue::one(backing.clone()), Some(vec![backing.clone()])),
        (AddressValue::unknown(), None),
    ] {
        let pointer = selected.project(&[BUFFER_POINTER]);

        state.values.insert(selected.clone(), super::model::AddressValue::one(selected.clone()).into());
        state.values.insert(pointer.clone(), recorded.clone().into());
        state.declare_owned_buffer(selected.clone(), pointer.clone(), element);

        assert_eq!(state.values[&pointer].address.as_ref(), Some(&recorded), "a declaration cannot reconstruct or overwrite pointer provenance");

        assert_eq!(state.owned_regions(&selected, BUFFER_POINTER), regions,
            "the backing is established by the stored pointer");
    }
}

#[test]
fn bounded_returned_regions_do_not_declare_or_initialize_a_cell() {
    let values = crate::test_support::semantic_values();

    let ([owner, output, view, _], calls) = places();

    let element = error_type();
    let slice = values.intern_type(TypeData::Slice(element)).unwrap();
    let exact = addressed_reference(slice, DependencyReferenceKind::Address);

    let bounded = bray_bound_tree::BoundCallReference::new(
        exact.relation().clone().with_exactness(false)
            .with_target_projections([DependencyProjection::OwnedTarget]),
        false,
        Some(slice),
    );

    let owner = Target::storage(owner);
    let backing = owner.project(&[StorageProjection::OwnedTarget]);
    let output = Target::storage(output);
    let mut state = State::default();

    state.stage_address(calls[0].into(), backing.clone(), output.clone(), &bounded);
    state.complete_transition(calls[0].into());

    assert_eq!(state.declared_type(&backing), None);
    assert_eq!(state.referenced_regions(&output), Some(vec![backing.clone()]));
    assert!(state.value_targets(&output).is_none());
    assert_eq!(state.raw_exposed_regions(), BTreeSet::from([backing.clone()]));

    assert_eq!(
        state.read(&output, element, MemoryReadKind::Copy, None, None),
        MemoryOperationStatus::UninitializedRawStorage
    );

    let mut actual = State::default();

    actual.declare_owned_buffer(owner.clone(), owner.project(&[BUFFER_POINTER]), element);
    actual.owned_region(&[owner], Target::storage(view), element, BUFFER_POINTER, false);
    state.merge(&actual, &Default::default(), &Default::default());

    assert_eq!(state.declared_type(&backing), Some(element));
}

#[test]
fn returned_references_snapshot_previously_published_inline_storage() {

    let ([first, owner, second, _], calls) = places();

    let mut state = State {
        owned_storage: [first.root, owner.root, second.root].into(),
        ..State::default()
    };

    let allocation = Target::storage(owner);
    let first_slot = Target::storage(first.clone());
    let second_slot = Target::storage(second.clone());

    state.stage_returned_reference(calls[0].into(), first_slot.clone(), first, error_type(), []);
    state.complete_transition(calls[0].into());

    let before = state
        .value_targets(&first_slot)
        .expect("returned reference has its origin")[0]
        .clone();

    state.escape_regions(BTreeSet::from([allocation.clone()]));

    state.stage_returned_reference(
        calls[1].into(),
        second_slot.clone(),
        second,
        error_type(),
        [],
    );

    state.complete_transition(calls[1].into());

    let after = state
        .value_targets(&second_slot)
        .expect("returned reference has its origin")[0]
        .clone();

    assert_eq!(
        state.target_relationship(&before, &allocation),
        StorageRelationship::Disjoint
    );

    assert_eq!(
        state.target_relationship(&after, &allocation),
        StorageRelationship::PotentiallyOverlapping
    );

    assert_eq!(
        state.target_relationship(&before, &after),
        StorageRelationship::PotentiallyOverlapping
    );
}

#[test]
fn bounded_owned_views_keep_alternative_regions_without_exact_pointer_identity() {

    let ([first, second, view, _], _) = places();

    let mut state = State::default();

    let owners = [first, second].map(|owner| Target {
        origin: Origin::entry(owner, bray_symbols::DependencyReferenceKind::Address),
        previous: false,
    });

    let destination = Target::storage(view);

    for owner in &owners {
        state.declare_owned_buffer(owner.clone(), owner.project(&[BUFFER_POINTER]), error_type());
    }

    state.owned_region(
        &owners,
        destination.clone(),
        error_type(),
        BUFFER_POINTER,
        true,
    );

    let regions = state
        .referenced_regions(&destination)
        .expect("bounded view retains both possible allocations");

    assert_eq!(regions.len(), 2);
    assert!(state.value_targets(&destination).is_none());

    state.owned_region(
        &owners[..1],
        destination.clone(),
        error_type(),
        BUFFER_POINTER,
        true,
    );

    assert_eq!(
        state
            .referenced_regions(&destination)
            .expect("bounded view retains its allocation")
            .len(),
        1
    );

    assert!(state.value_targets(&destination).is_none());
}

#[test]
fn owned_regions_keep_headers_disjoint_and_survive_owner_moves_without_exact_offsets() {

    let ([owner, pointer, moved, other], _) = places();

    let mut state = State::default();

    let header = Target {
        origin: Origin::entry(owner.clone(), bray_symbols::DependencyReferenceKind::Address),
        previous: false,
    };

    state.declare_owned_buffer(header.clone(), header.project(&[BUFFER_POINTER]), error_type());

    state.owned_region(
        &[header.clone()],
        Target::storage(pointer.clone()),
        error_type(),
        BUFFER_POINTER,
        false,
    );

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

    state.owned_region(
        &[Target::storage(moved)],
        Target::storage(other.clone()),
        error_type(),
        BUFFER_POINTER,
        false,
    );

    state.offset(&Target::storage(other.clone()), other.clone());

    assert_eq!(
        state.referenced_regions(&Target::storage(other)),
        Some(regions.clone())
    );

    assert!(regions[0].observation_path().is_none());

    let unrelated = Target {
        origin: Origin::entry(Place::root(pointer.root), bray_symbols::DependencyReferenceKind::Address),
        previous: false,
    };

    assert_eq!(
        state.target_relationship(&unrelated, &regions[0]),
        bray_bound_tree::StorageRelationship::PotentiallyOverlapping
    );
}

#[test]
fn moved_structural_owner_keeps_its_backing_in_a_deferred_capture() {
    let ([owner, moved, ..], origins) = places();

    let mut state = State::default();
    let source = Target::storage(owner.clone());

    state.acquire_owned_backing(owner.clone(), origins[0]);

    let backing = state.owned_backing(&source).expect("construction retains its backing").clone();

    state.transfer_value(&source, moved.clone());
    state.clear(&owner);

    let moved = Target::storage(moved);
    let captured = state.capture_reached_value(&moved);

    let route = bray_bound_tree::StorageReferencePath::new(
        [StorageProjection::OwnedTarget].into(),
        [bray_bound_tree::StorageReferenceStep::Backing { index: 0, target: error_type(), pointer: None }].into(),
    );

    let values = crate::test_support::semantic_values();

    assert_eq!(state.reference_locations(values, moved, &route, None), Some(vec![backing.clone()]));
    assert_eq!(state.captured_dependency_targets(values, &captured, &route), Some((vec![backing], None)));
    assert!(state.reference_locations(values, source, &route, None).is_none());
}

#[test]
fn nested_backing_survives_descriptor_moves_and_pruning() {
    let ([owner, moved, alias, ..], origins) = places();

    let pointer = bray_symbols::StructFieldSymbolId::from_symbol_id(bray_symbols::SymbolId::new(3));
    let buffer_pointer = StorageProjection::ProductField(pointer);

    let mut state = State::default();
    let descriptor = Target::storage(owner.clone());

    state.declare_owned_buffer(descriptor.clone(), descriptor.project(&[buffer_pointer]), error_type());

    let outer = state.owned_regions(&descriptor, buffer_pointer).unwrap().remove(0);
    let element = outer.project(&[StorageProjection::AnyElement]);

    state.declare_owned_buffer(element.clone(), element.project(&[buffer_pointer]), error_type());

    let inner = state.owned_regions(&element, buffer_pointer).unwrap().remove(0);

    let values = crate::test_support::semantic_values();
    let slice = values.intern_type(bray_symbols::TypeData::Slice(error_type())).unwrap();

    let route = bray_bound_tree::StorageReferencePath::new(
        [StorageProjection::OwnedTarget, StorageProjection::AnyElement, StorageProjection::OwnedTarget].into(),
        [
            bray_bound_tree::StorageReferenceStep::Backing { index: 0, target: slice, pointer: Some(pointer) },
            bray_bound_tree::StorageReferenceStep::Backing { index: 2, target: slice, pointer: Some(pointer) },
        ].into(),
    );

    assert_eq!(state.reference_locations(values, descriptor.clone(), &route, None), Some(vec![inner.clone()]));

    state.contents.get_mut(&outer).unwrap().storage_type = None;

    assert_eq!(state.reference_locations(values, descriptor.clone(), &route, None), Some(vec![inner.clone()]),
        "selecting actual backing precedes element layout and initialization");

    assert_eq!(state.contents[&outer].storage_type, None, "following a descriptor establishes no element layout");

    state.contents.get_mut(&outer).unwrap().storage_type = Some(error_type());

    let mut bounded = state.clone();
    let pointer_binding = bounded.values.get_mut(&descriptor.project(&[buffer_pointer])).unwrap();

    pointer_binding.address_bounds = pointer_binding.address.take();

    assert!(bounded.reference_locations(values, descriptor.clone(), &route, None).is_none(),
        "a possible backing region cannot supply the actual descriptor read");

    let terminal = route.prefix(1, 1);
    let bound = bray_bound_tree::StorageMutationTarget::bounded(outer.clone());
    let captured_bound = bounded.capture_reached_value(&descriptor);

    assert!(bounded.reference_locations(values, descriptor.clone(), &terminal, None).is_none());

    assert_eq!(bounded.reference_effect_targets(values, descriptor.clone(), Some(&terminal), false),
        Some(vec![bound.clone()]), "a terminal effect uses the existing region without obtaining an address");

    assert_eq!(bounded.captured_effect_targets(values, &captured_bound, &terminal, false),
        Some(vec![bound]), "captured routes preserve the same region precision");

    state.contents.get_mut(&inner).unwrap().observe_initialized(error_type());
    state.values.insert(Target::storage(alias.clone()), AddressValue::one(inner.clone()).into());
    state.transfer_value(&descriptor, moved.clone());
    state.forget(owner.root);
    state.retain_live(&BTreeSet::from([moved.root, alias.root]));
    state.end_storage(&[owner.root]);

    assert!(!state.values.contains_key(&descriptor.project(&[buffer_pointer])), "backing records alone do not retain their descriptor");
    assert_eq!(state.owned_regions(&Target::storage(moved.clone()), buffer_pointer), Some(vec![outer.clone()]));
    assert_eq!(state.reference_locations(values, Target::storage(moved.clone()), &route, None), Some(vec![inner.clone()]));
    assert_eq!(state.owned_regions(&element, buffer_pointer), Some(vec![inner.clone()]));
    assert!(state.contents[&inner].is_initialized(error_type()), "descriptor storage end must preserve the moved allocation facts");

    assert!(!state.initialized_targets(&AddressValue::one(inner.clone()), error_type()),
        "a dynamic element's region must remain inexact");

    let call = origins[0].into();

    state.begin_transition(call);

    state.stage_preserved_value(call, &super::Source::Value(Target::storage(moved.clone())), Target::storage(owner.clone()),
        &bray_symbols::DependencyValuePreservation::all());

    let captured = state.capture_reached_value(&Target::storage(moved.clone()));

    let selected = StorageProjection::ElementFromStart(bray_symbols::SymbolOrdinal::new(0));
    let absent = outer.project(&[selected, StorageProjection::NullableValue]);

    let optional_route = bray_bound_tree::StorageReferencePath::new(
        [StorageProjection::OwnedTarget, selected, StorageProjection::NullableValue, StorageProjection::OwnedTarget].into(),
        [
            bray_bound_tree::StorageReferenceStep::Backing { index: 0, target: slice, pointer: Some(pointer) },
            bray_bound_tree::StorageReferenceStep::Backing { index: 3, target: slice, pointer: Some(pointer) },
        ].into(),
    );

    assert!(state.captured_effect_targets(values, &captured, &optional_route, false).is_none(),
        "a missing pointer is unresolved while the selected owner may be active");

    state.values.insert(absent.clone(), super::model::ValueBinding::no_payload());

    assert_eq!(state.owned_mutation_targets(&bray_bound_tree::StorageMutationTarget::location(absent.clone()), buffer_pointer),
        Some(Vec::new()), "an inactive buffer contributes no effects to an intrinsic release");

    assert_eq!(state.reference_effect_targets(values, Target::storage(moved.clone()), Some(&optional_route), false),
        Some(Vec::new()), "an inactive owner has no current backing effect");

    assert_eq!(state.captured_effect_targets(values, &captured, &optional_route, false),
        Some(Vec::new()), "activation observes the explicit absence of the nested owner");

    state.values.remove(&absent);

    state.null(Place { root: moved.root, projections: [buffer_pointer].into() });

    assert_eq!(
        state.captured_effect_targets(values, &captured, &route, false),
        Some(vec![bray_bound_tree::StorageMutationTarget::bounded(inner.clone())]),
        "activation uses the captured descriptor and keeps the selected element region inexact",
    );

    assert!(state.captured_effect_targets(values, &captured.bounded(), &route, false).is_none(),
        "a bound on the captured descriptor cannot name a location for the next read");

    assert_eq!(state.reference_locations(values, Target::storage(moved), &route, None), Some(Vec::new()),
        "an overwritten descriptor cannot be followed using its former backing region");

    assert_eq!(state.staged_reference_locations(call, values, &Target::storage(owner.clone()), &Target::storage(owner.clone()), &route),
        Some(vec![inner.clone()]), "captured inline descriptors survive source replacement");

    state.clear_value(&element.project(&[buffer_pointer]));

    assert!(state.captured_effect_targets(values, &captured, &route, false).is_none(),
        "only the first descriptor came from the evaluated capture");

    assert!(state.staged_reference_locations(call, values, &Target::storage(owner.clone()), &Target::storage(owner), &route).is_none(),
        "capturing a descriptor does not freeze nested pointees");

    state.retire_region(&outer, Retirement::Storage);

    assert!(state.contents[&inner].is_initialized(error_type()), "retiring an element descriptor does not free its owned allocation");
    assert_eq!(state.referenced_regions(&Target::storage(alias.clone())), Some(vec![inner.clone()]));

    state.retire_region(&inner, Retirement::Storage);

    assert!(state.contents.get(&inner).is_none_or(|contents| !contents.is_initialized(error_type())));

    assert!(state.referenced_regions(&Target::storage(alias)).unwrap().iter().all(|target| target.previous),
        "actual backing retirement must invalidate saved aliases");
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

    state.declare_owned_buffer(owner.clone(), owner.project(&[BUFFER_POINTER]), error_type());

    state.owned_region(
        &[owner.clone()],
        Target::storage(pointer.clone()),
        error_type(),
        BUFFER_POINTER,
        false,
    );

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
        origin: super::Origin::entry(destination, bray_symbols::DependencyReferenceKind::Address),
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

    assert!(!super::model::AddressValue::one(dynamic.clone()).is_exact());
    state.write_target(dynamic, error_type(), origins[2], None);
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

    let ([owner, pointer, alias, moved], _) = places();

    let mut state = State {
        owned_storage: vec![owner.root, moved.root].into(),
        ..State::default()
    };

    let header = Target::storage(owner);

    let incoming = Target {
        origin: Origin::entry(alias, bray_symbols::DependencyReferenceKind::Address),
        previous: false,
    };

    state.declare_owned_buffer(header.clone(), header.project(&[BUFFER_POINTER]), error_type());

    state.owned_region(
        &[header.clone()],
        Target::storage(pointer.clone()),
        error_type(),
        BUFFER_POINTER,
        false,
    );

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

    let incoming_write = state.affected_targets(&super::model::AddressValue::one(incoming.clone()));

    assert!(incoming_write.contains(&region));
    assert!(!incoming_write.contains(&header));

    let payload_write = state.affected_targets(&super::model::AddressValue::one(region.clone()));

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

        let candidates = [slot.clone()];
        let stable_descriptors = state.unexposed_storage(&candidates, []).cloned().collect();

        state.invalidate(
            &bray_bound_tree::StorageInvalidation::All {
                stable_descriptors,
                stable_values: Default::default(),
                stable_reference_slots: Default::default(), normal_snapshots: Default::default(), stable_pointees: Default::default(),
            },
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
fn replaced_reference_reads_retire_retained_pointer_identity() {
    let ([slot, saved, owner, ..], _) = places();

    let mut state = State::default();
    let location = Target::storage(slot.clone());
    let inputs = [Target::storage(owner)];

    state.observe_reference(location.clone(), slot.clone(), error_type(), inputs.clone());
    state.transfer_value(&location, saved.clone());
    state.clear(&slot);
    state.observe_reference(location.clone(), slot, error_type(), inputs);

    let saved = Target::storage(saved);

    let previous = state
        .referenced_regions(&saved)
        .expect("retained pointer keeps its possible referent");

    let current = state
        .value_targets(&location)
        .expect("replacement read establishes its current referent");

    assert!(previous[0].previous);
    assert_eq!(state.value_targets(&saved), None);
    assert!(!current[0].previous);

    assert_eq!(
        state.target_relationship(&previous[0], &current[0]),
        bray_bound_tree::StorageRelationship::PotentiallyOverlapping,
    );
}

#[test]
fn typed_writes_keep_protected_reference_slots_until_exact_replacement() {
    let ([parameter, other, ..], origins, capability) = borrowed_places();

    let ty = error_type();
    let mut state = State::default();

    state.begin_entry_borrow(capability, parameter.clone(), ty);

    let projection = StorageProjection::TupleElement(bray_symbols::SymbolOrdinal::new(0));

    let target = state
        .borrow_target(capability, ty)
        .expect("entry borrow identifies its referent")
        .project(&[projection]);

    let slot = Place {
        root: parameter.root,
        projections: [projection].into(),
    };

    state.entry_reference_at(slot, 1, target.clone(), Some(ty), bray_symbols::DependencyReferenceKind::Address);

    let expected = state.value_targets(&target);

    let invalidation = bray_bound_tree::StorageInvalidation::All {
        stable_descriptors: [parameter].into(),
        stable_values: Default::default(),
        stable_reference_slots: Default::default(), normal_snapshots: Default::default(), stable_pointees: Default::default(),
    };

    let other = Target {
        previous: false,
        origin: super::Origin::Entry {
            reference: other,
            layer: 1,
            kind: bray_symbols::DependencyReferenceKind::Address,
            projections: Arc::from([]),
        },
    };

    state.write_target(other, ty, origins[0], Some(&invalidation));
    assert_eq!(state.value_targets(&target), expected);

    state.write_target(target.clone(), ty, origins[1], Some(&invalidation));
    assert_eq!(state.value_targets(&target), None);
}

#[test]
fn effects_preserve_protected_entry_reference_slots_without_referent_authority() {
    for nested in [false, true] {
    for (protected, resolved) in [(false, false), (true, false), (false, true), (true, true)] {
        let ([parameter, field_reference, ..], origins, capability) = borrowed_places();

        let ty = error_type();
        let mut state = State::default();

        state.begin_entry_borrow(capability, parameter.clone(), ty);

        let projection = StorageProjection::TupleElement(bray_symbols::SymbolOrdinal::new(0));

        let nested_reference = Place { root: parameter.root, projections: [projection].into() };

        let base = if nested {
            Target { previous: false, origin: super::Origin::entry(nested_reference.clone(), bray_symbols::DependencyReferenceKind::Borrow) }
        } else {
            state.borrow_target(capability, ty).expect("entry referent")
        };

        let target = base.project(&[projection]);

        let slot = Place {
            root: parameter.root,
            projections: [projection].into(),
        };

        state.entry_reference_at(slot.clone(), 1, target.clone(), Some(ty), bray_symbols::DependencyReferenceKind::Address);
        state.transfer_value(&target, field_reference.clone());

        let expected = state.value_targets(&target);

        let mut invalidation = bray_bound_tree::StorageInvalidation::All {
            stable_descriptors: protected.then_some(parameter).into_iter().collect(),
            stable_values: Default::default(),
            stable_reference_slots: Default::default(), normal_snapshots: Default::default(), stable_pointees: Default::default(),
        };

        if nested && protected {
            invalidation.preserve_reference_slot(Place { root: nested_reference.root, projections: [projection, projection].into() }, target.clone());
        }

        if resolved {
            state.apply_effects(
                Some(&[super::Effect::at(target.clone(), bray_symbols::DependencyMutationKind::Retire, None)]),
                &invalidation,
                origins[0].into(),
            );
        } else {
            state.invalidate(
                &invalidation,
                bray_symbols::DependencyMutationKind::Retire,
            );
        }

        assert_eq!(
            state.value_targets(&target),
            protected.then_some(expected).flatten()
        );

        assert!(!state.initialized(&target, ty));
    }
}
    }

#[test]
fn null_pointer_has_no_effect_targets_or_initialization_authority() {
    let ([slot, copied, ..], _) = places();

    let mut state = State::default();

    state.null(slot.clone());
    state.transfer_value(&Target::storage(slot.clone()), copied.clone());

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

    state.transfer_value(&Target::storage(reference.clone()), holder.clone());

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
            |_| [Target::storage(first.clone())],
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

        state.transfer_value(&Target::storage(result.clone()), copied.clone());
        state.begin_transition(call);

        state.stage_returned_references(
            call,
            [(result.clone(), error_type())],
            |_| [Target::storage(second.clone())],
        );

        assert_eq!(state.target_relationship(&returned, &Target::storage(second.clone())),
            StorageRelationship::Disjoint,
            "pending output cannot change the bounds of an existing reference");

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
            StorageRelationship::PotentiallyOverlapping
        );

        assert_eq!(state.target_relationship(&current, &Target::storage(first.clone())),
            StorageRelationship::Disjoint,
            "new return bounds cannot inherit another evaluation's input regions");

        assert_eq!(state.target_relationship(&previous, &Target::storage(first.clone())),
            StorageRelationship::PotentiallyOverlapping,
            "an older returned reference can still reach its unchanged input");

        state.begin(first.root);

        assert_eq!(state.target_relationship(&previous, &Target::storage(first.clone())),
            StorageRelationship::Disjoint,
            "an older returned reference cannot reach a recreated local");

        state.escape_targets([previous]);

        assert_eq!(state.unexposed_storage(std::slice::from_ref(&first), []).count(), 1,
            "escaped retired returns cannot publish a fresh local");

        state.escape_targets([Target::storage(first.clone())]);

        assert_eq!(state.unexposed_storage(std::slice::from_ref(&first), []).count(), 0);
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

    state.end_storage(&[reference.root]);
    assert_eq!(state.unexposed_storage(&candidates, []).count(), 1);

    state.address(owner.clone(), reference, error_type(), origins[0]);
    assert_eq!(state.unexposed_storage(&candidates, []).count(), 1);
    state.escape_targets([Target::storage(owner)]);
    assert_eq!(state.unexposed_storage(&candidates, []).count(), 0);
}

#[test]
fn ambient_reachability_requires_raw_publication_of_an_owned_slot() {
    let ([owner, pointer, view, opaque], origins) = places();

    let target = Target::storage(owner.clone());

    let mut state = State {
        owned_storage: [owner.root, pointer.root, view.root, opaque.root].into(),
        ..State::default()
    };

    assert!(!state.unknown_reference_may_reach(&target));

    let call = origins[0].into();

    state.stage_returned_reference(call, Target::storage(view.clone()), view.clone(), error_type(), [target.clone()]);
    state.complete_transition(call);
    state.values.insert(Target::storage(opaque.clone()), AddressValue::unknown().into());

    let returned = state.value_target(&Target::storage(view), error_type()).expect("returned view retains its bounded identity");

    assert!(!state.escaped_regions([Target::storage(opaque.clone())]).contains(&target),
        "an unknown external pointer cannot publish a private alternative of another returned view");

    assert!(state.escaped_regions([returned]).contains(&target),
        "explicitly publishing a returned pointer exposes each possible input region");

    state.address(owner, pointer, error_type(), origins[0]);

    assert!(state.unknown_reference_may_reach(&target));

    assert!(state.escaped_regions([Target::storage(opaque)]).contains(&target),
        "unknown pointers can reach actually exposed local storage");

    let ([parameter, raw, view, _], origins, capability) = borrowed_places();

    let mut state = State {
        owned_storage: [parameter.root, raw.root, view.root].into(),
        entry_borrows: Arc::new([(parameter.root, BorrowKind::Mutable)].into()),
        ..State::default()
    };

    state.begin_entry_borrow(capability, parameter.clone(), error_type());

    let borrowed = state.borrow_target(capability, error_type()).expect("entry borrow retains its referent");

    assert_eq!(state.ambient_regions(&[Effect::Ambient]), Some((BTreeSet::new(), false)),
        "receiving a language borrow does not publish its inline owner");

    assert!(state.ambient_regions(&[Effect::Ambient, Effect::Escape(None)]).is_none(),
        "an opaque invocation still has unbounded input reachability");

    assert_eq!(
        state.ambient_regions(&[Effect::Ambient, Effect::Escape(Some([borrowed.clone()].into()))]),
        Some((BTreeSet::from([borrowed.clone()]), false)),
        "a bounded escape supplies the evaluated invocation region",
    );

    let mut shared = state.clone();

    shared.entry_borrows = Arc::new([(parameter.root, BorrowKind::Shared)].into());

    assert!(shared.ambient_regions(&[Effect::Ambient]).expect("shared external region").0.contains(&borrowed),
        "shared interior state remains externally reachable");

    let call = origins[0].into();

    state.stage_reference_regions(call, Target::storage(view.clone()), view.clone(), error_type(), [borrowed.clone()]);
    state.complete_transition(call);

    let forwarded = state.value_targets(&Target::storage(view)).expect("bounded forwarded view")[0].clone();

    state.values.insert(forwarded, AddressValue::one(borrowed.clone()).into());

    assert_eq!(state.ambient_regions(&[Effect::Ambient]), Some((BTreeSet::new(), false)),
        "forwarding a borrow cannot publish its private input bound");

    let mut escaped = state.clone();

    escaped.escape_targets([borrowed.clone()]);
    assert!(escaped.ambient_regions(&[Effect::Ambient]).expect("finite escaped region").0.contains(&borrowed));

    state.declare_owned_buffer(borrowed.clone(), borrowed.project(&[BUFFER_POINTER]), error_type());

    let backing = borrowed.project(&[StorageProjection::OwnedTarget]);

    assert!(state.ambient_regions(&[Effect::Ambient]).expect("finite backing region").0.contains(&backing),
        "a raw buffer keeps its separate backing reachable through its stored pointer");

    state.entry_reference(raw.clone());

    let alias = state.value_targets(&Target::storage(raw)).expect("incoming raw alias")[0].clone();

    assert!(state.ambient_regions(&[Effect::Ambient]).expect("finite raw region").0.contains(&alias));

    assert_ne!(state.target_relationship(&alias, &borrowed), StorageRelationship::Disjoint,
        "an actual raw input can still invalidate a borrowed owner");
}

#[test]
fn unknown_references_cannot_reach_unaddressed_inline_storage() {
    let ([local, borrowed, pointer, backing], origins) = places();

    let mut state = State {
        unaddressed_storage: [local.root].into(),
        ..State::default()
    };

    assert!(!state.unknown_reference_may_reach(&Target::storage(local.clone())));
    assert!(state.unknown_reference_may_reach(&Target::storage(borrowed.clone())));

    assert!(state.unknown_reference_may_reach(
        &Target::storage(local.clone()).project(&[StorageProjection::OwnedTarget]),
    ));

    let retained = super::model::AddressValue::one(Target::storage(backing));

    state
        .values
        .insert(Target::storage(local.clone()), retained.clone().into());

    state
        .values
        .insert(Target::storage(borrowed.clone()), retained.clone().into());

    state.values.insert(
        Target::storage(pointer.clone()),
        AddressValue::unknown().into(),
    );

    assert_eq!(
        state.write(
            &Target::storage(pointer.clone()),
            error_type(),
            origins[0],
            None
        ),
        MemoryOperationStatus::Valid,
    );

    assert_eq!(
        state.values.get(&Target::storage(local.clone())).and_then(|value| value.reference()),
        Some(&retained)
    );

    assert!(state.values[&Target::storage(borrowed)].address.as_ref().unwrap().is_unknown());

    state.invalidate(
        &bray_bound_tree::StorageInvalidation::unknown(),
        bray_symbols::DependencyMutationKind::Write,
    );

    assert_eq!(
        state.values.get(&Target::storage(local.clone())).and_then(|value| value.reference()),
        Some(&retained)
    );

    state.values.insert(
        Target::storage(pointer.clone()),
        { let mut address = AddressValue::one(Target::storage(local.clone())); address.forget_value(); address }.into(),
    );

    state.write(&Target::storage(pointer), error_type(), origins[0], None);

    assert!(state.values[&Target::storage(local.clone())].address.as_ref().unwrap().is_unknown());

    // A known target remains authoritative even when its slot is otherwise private.
    state.write_value(
        Target::storage(local.clone()),
        error_type(),
        origins[1],
        None,
    );

    assert!(!state.values.contains_key(&Target::storage(local)));
}

#[test]
fn local_mutation_does_not_publish_an_invocation_address() {
    let ([local, external, _, _], _) = places();

    let mut state = State {
        owned_storage: [local.root].into(),
        ..State::default()
    };

    let local_write = super::Effect::at(Target::storage(local.clone()), bray_symbols::DependencyMutationKind::Replace(error_type()), None);

    let external_write = super::Effect::at(Target::storage(external), bray_symbols::DependencyMutationKind::Replace(error_type()), None);

    let heap_write = super::Effect::at(Target::storage(local.clone()).project(&[StorageProjection::OwnedTarget]), bray_symbols::DependencyMutationKind::Write, None);

    assert!(!state.may_retain_address(&local_write));
    assert!(state.may_retain_address(&external_write));
    assert!(state.may_retain_address(&heap_write));

    state.escape_targets([Target::storage(local)]);

    assert!(state.may_retain_address(&local_write));
}

#[test]
fn targeted_atomic_effects_preserve_reference_slots_under_possible_aliasing() {
    let ([source, slot, external, _], origins) = places();

    let mut state = State::default();

    state.address(source.clone(), slot.clone(), error_type(), origins[0]);

    let external = Target {
        origin: super::Origin::entry(external, bray_symbols::DependencyReferenceKind::Address),
        previous: false,
    };

    state.apply_effects(
        Some(&[super::Effect::at(external.clone(), bray_symbols::DependencyMutationKind::Atomic, None)]),
        &bray_bound_tree::StorageInvalidation::unknown(),
        origins[0].into(),
    );

    assert_eq!(state.referent(&slot, error_type()), Some(source));

    state.apply_effects(
        Some(&[super::Effect::at(external.clone(), bray_symbols::DependencyMutationKind::Write, None)]),
        &bray_bound_tree::StorageInvalidation::unknown(),
        origins[0].into(),
    );

    assert_eq!(state.referent(&slot, error_type()), None);
}

#[test]
fn private_production_retains_captures_until_normal_completion() {
    let ([source, input, output, _], origins) = places();

    let occurrence = origins[0].into();
    let mut state = State::default();
    let input_target = Target::storage(input.clone());
    let output_target = Target::storage(output.clone());
    let source_target = Target::storage(source);

    state.point_at(source_target.clone(), input.clone(), error_type(), bray_symbols::DependencyReferenceKind::Address);

    let captured = state.capture_reached_value(&input_target);

    state.begin_transition(occurrence);
    state.stage_slot(occurrence, &input, output.clone());

    state.capture_failure_effects(
        occurrence,
        (DependencyFailureKind::Panic, None),
        super::CapturedEffects {
            cell_changes: None,
            mutations: [super::Effect::at(source_target.clone(), bray_symbols::DependencyMutationKind::Write, None)]
            .into(),
            escaped: [source_target.clone()].into(),
            escape_prefix: Default::default(),
            invalidation: bray_bound_tree::StorageInvalidation::unknown(),
        }.into(),
    );

    assert!(state.value_targets(&output_target).is_none(), "staged output has not been produced");

    state.produce_transition(occurrence);

    assert_eq!(state.value_targets(&output_target), Some(vec![source_target.clone()]));

    state.null(input);
    state.null(output);
    state.produce_transition(occurrence);

    assert_eq!(state.value_targets(&output_target), Some(Vec::new()), "cleanup changes survive repeated production");

    assert_eq!(captured.known_address(), Some(&source_target),
        "private production retains the evaluated input independently of its current slot");

    let mut failed = state.clone();

    assert!(failed.clone().fail_transition(occurrence, (DependencyFailureKind::Cancellation, None), false).is_none());

    let effects = failed.fail_transition(occurrence, (DependencyFailureKind::Panic, None), false).expect("private production retains panic effects");

    assert_eq!(effects.escaped.as_slice(), &[source_target]);
    assert!(failed.fail_transition(occurrence, (DependencyFailureKind::Panic, None), false).is_none(), "an outcome consumes its capture once");

    state.complete_transition(occurrence);

    assert!(state.fail_transition(occurrence, (DependencyFailureKind::Panic, None), false).is_none());
    assert_eq!(state.value_targets(&output_target), Some(Vec::new()), "normal completion publishes the surviving private value");
}

#[test]
fn captured_failure_targets_follow_storage_retirement() {
    let ([source, slot, _, _], origins) = places();

    let occurrence = origins[1].into();
    let mut state = State::default();

    state.address(source.clone(), slot.clone(), error_type(), origins[0]);
    state.begin_transition(occurrence);

    let operand = super::Source::Value(Target::storage(slot.clone())).mutation_targets(&state)
        .expect("evaluated pointer has an effect operand").into_iter().next().expect("one pointer alternative");

    let saved = Effect::through(operand, DependencyMutationKind::Write, None);

    let mut bounds = AddressValue::one(Target::storage(source.clone()));

    bounds.bound();
    state.values.insert(Target::storage(slot.clone()), bounds.into());

    let operand = super::Source::Value(Target::storage(slot.clone())).mutation_targets(&state)
        .expect("the same finite bound has an effect operand").into_iter().next().expect("one bound");

    let bounded = Effect::through(operand, DependencyMutationKind::Dispose(error_type()), Some(error_type()));

    assert!(matches!(bounded, Effect::Mutation {
        address: None, footprint, discharged: None, kind: DependencyMutationKind::Cleanup(ty), access_type: Some(access),
    } if footprint == Target::storage(source.clone()) && ty == error_type() && access == error_type()),
        "a possible region retains its cleanup footprint and checked type without claiming that layout");

    state.null(slot);

    let previous = Target {
        previous: true,
        ..Target::storage(source.clone())
    };

    state.capture_failure_effects(
        occurrence,
        (DependencyFailureKind::Cancellation, None),
        super::CapturedEffects {
            cell_changes: None,
            mutations: [
                super::Effect::at(previous.clone(), bray_symbols::DependencyMutationKind::Write, None),
                saved,
            ]
            .into(),
            escaped: [previous, Target::storage(source.clone())].into(),
            escape_prefix: Default::default(),
            invalidation: bray_bound_tree::StorageInvalidation::unknown(),
        }.into(),
    );

    state.begin(source.root);

    let effects = state
        .fail_transition(occurrence, (DependencyFailureKind::Cancellation, None), false)
        .expect("failed invocation retains its effects");

    let previous = Target {
        previous: true,
        ..Target::storage(source)
    };

    assert_eq!(
        effects.mutations.as_ref(),
        &[super::Effect::at(previous.clone(), bray_symbols::DependencyMutationKind::Write, None)]
    );

    assert_eq!(effects.escaped.as_slice(), &[previous]);
    assert!(state.fail_transition(occurrence, (DependencyFailureKind::Cancellation, None), false).is_none());
}

#[test]
fn null_join_retains_bounds_without_exact_identity_or_initialization() {
    let ([slot, ..], _, capability) = borrowed_places();

    let ty = error_type();
    let mut state = State::default();

    state.begin_entry_borrow(capability, slot.clone(), ty);

    let target = state
        .borrow_target(capability, ty)
        .expect("fixture `borrow_target` call must succeed");

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

    let field =
        StorageProjection::TupleElement(bray_symbols::SymbolOrdinal::new(0));

    for reference in [
        parameter.clone(),
        Place {
            root: parameter.root,
            projections: [field].into(),
        },
    ] {
        let ty = error_type();
        let mut state = State::default();

        state.begin_entry_borrow(capability, reference, ty);

        let target = state
            .borrow_target(capability, ty)
            .expect("fixture entry borrow retains its physical reference identity");

        state.stage_address(
            origins[0].into(),
            target.clone(),
            Target::storage(pointer.clone()),
            &addressed_reference(ty, bray_symbols::DependencyReferenceKind::Address),
        );

        assert_eq!(state.raw_exposed_regions(), BTreeSet::from([target]));
    }
}

#[test]
fn returned_references_capture_external_exposure_when_they_are_created() {
    let ([local, result, later_result, ..], origins) = places();

    let call = origins[0].into();

    let mut state = State {
        owned_storage: [local.root, result.root].into(),
        ..State::default()
    };

    state.stage_returned_references(call, [(result.clone(), error_type())], |_| []);
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

    assert_eq!(
        state.target_relationship(&pointer, &local),
        bray_bound_tree::StorageRelationship::Disjoint
    );

    assert!(
        state.effect_origins(&pointer).0.next().is_none(),
        "external reachability does not imply this call received a local input"
    );

    state.stage_returned_references(call, [(later_result.clone(), error_type())], |_| []);
    state.complete_transition(call);

    let later = state
        .value_target(&Target::storage(later_result), error_type())
        .expect("a later opaque reference retains the current exposure bound");

    assert_ne!(
        state.target_relationship(&later, &local),
        bray_bound_tree::StorageRelationship::Disjoint
    );

    assert!(state.effect_origins(&later).0.any(|target| target == &local));
}

#[test]
fn published_returned_aliases_survive_repeated_reference_reads() {
    let ([reference, pointer, ..], origins) = places();

    let mut state = State::default();
    let slot = Target::storage(reference.clone());

    state.observe_reference(slot.clone(), reference.clone(), error_type(), []);

    let original = state
        .value_target(&slot, error_type())
        .expect("the first read retains an established reference");

    state.stage_address(
        origins[0].into(),
        original.clone(),
        Target::storage(pointer),
        &addressed_reference(error_type(), bray_symbols::DependencyReferenceKind::Address),
    );

    state.observe_reference(slot.clone(), reference, error_type(), []);

    let current = state
        .value_target(&slot, error_type())
        .expect("the repeated read retains a current reference");

    let exposed = state.raw_exposed_regions();

    assert!(exposed.iter().any(|target| target.previous));

    assert!(
        exposed
            .iter()
            .any(|target| state.target_relationship(target, &current)
                != bray_bound_tree::StorageRelationship::Disjoint)
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

    state.stage_returned_references(call, [(result.clone(), error_type())], |_| [payload.clone()]);
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
fn observed_pointer_fields_keep_scope_without_initialization_authority() {
    let ([payload, parent_slot, observed_slot, unrelated], origins) = places();

    let call = origins[0].into();

    let mut state = State {
        owned_storage: [
            payload.root,
            parent_slot.root,
            observed_slot.root,
            unrelated.root,
        ]
        .into(),
        ..State::default()
    };

    state.stage_returned_reference(
        call,
        Target::storage(parent_slot.clone()),
        parent_slot.clone(),
        error_type(),
        [Target::storage(payload.clone())],
    );

    state.complete_transition(call);

    let parent = state
        .value_target(&Target::storage(parent_slot), error_type())
        .expect("native object retains its opaque reference");

    let field = parent.project(&[StorageProjection::TupleElement(
        bray_symbols::SymbolOrdinal::new(0),
    )]);

    let inputs = state.reachable_targets([parent]);

    state.observe_reference(field.clone(), observed_slot, error_type(), inputs);

    let pointer = state
        .value_target(&field, error_type())
        .expect("native pointer field retains its observed value");

    assert_ne!(
        state.target_relationship(&pointer, &Target::storage(payload)),
        bray_bound_tree::StorageRelationship::Disjoint
    );

    assert_eq!(
        state.target_relationship(&pointer, &Target::storage(unrelated)),
        bray_bound_tree::StorageRelationship::Disjoint
    );

    assert!(!state.initialized(&field, error_type()));
}

#[test]
fn bounded_borrowed_choices_do_not_read_possible_regions_as_addresses() {
    let ([owner, backing, result, observed], origins) = places();

    let call = origins[0].into();
    let field = StorageProjection::TupleElement(bray_symbols::SymbolOrdinal::new(0));
    let destination = Target::storage(result.clone());

    let mut state = State {
        owned_storage: [owner.root, backing.root, result.root, observed.root].into(),
        ..State::default()
    };

    for index in [0, 1] {
        let component = StorageProjection::TupleElement(bray_symbols::SymbolOrdinal::new(index));
        let candidate = Target::storage(owner.clone()).project(&[component]);
        let pointer = Target::storage(backing.clone()).project(&[StorageProjection::OwnedTarget, component]);

        state.stage_address(call, candidate.clone(), destination.clone(),
            &addressed_reference(error_type(), bray_symbols::DependencyReferenceKind::Borrow));

        state.values.insert(candidate.project(&[field]), AddressValue::one(pointer).into());
    }

    state.bound_staged_reference(call, &destination);
    state.stage_returned_references(call, [(result, error_type())], |_| []);
    state.complete_transition(call);

    let parent = state.value_target(&destination, error_type()).expect("bounded borrowed choice");
    let location = parent.project(&[field]);

    state.observe_reference(location.clone(), observed, error_type(), [Target::storage(owner.clone())]);

    let read = state.value_targets(&location).expect("pointer read retains its current value");

    assert!(matches!(read.as_slice(), [Target { origin: Origin::Returned { .. }, .. }]));
    assert!(!state.initialized(&location, error_type()));
    assert_eq!(state.unexposed_storage(std::slice::from_ref(&owner), []).collect::<Vec<_>>(), vec![&owner]);
}

#[test]
fn retired_returned_projection_keeps_its_bound_after_a_new_publication() {
    let ([result, retained, first, second], origins) = places();

    let call = origins[0].into();
    let output = Target::storage(result.clone());
    let first = Target::storage(first);
    let second = Target::storage(second);
    let mut state = State::default();

    state.stage_returned_reference(call, output.clone(), result.clone(), error_type(), [first.clone()]);
    state.complete_transition(call);

    let parent = state.value_target(&output, error_type()).expect("the call produces a bounded reference");

    assert!(!state.take_retirements().contains(&(parent.clone(), Retirement::Producer)),
        "the first result has no prior incarnation to retire");

    let field = parent.project(&[BUFFER_POINTER]);
    let retained_slot = Target::storage(retained.clone());

    state.point_at(field.clone(), retained, error_type(), DependencyReferenceKind::Borrow);
    state.contents.remove(&field);

    let captured = state.capture_reached_value(&retained_slot);

    state.retire_region(&field, Retirement::Storage);

    let historical = state.referenced_regions(&retained_slot).expect("the retired view retains an input region");

    let [historical] = historical.as_slice() else { panic!("the view retains a single origin"); };

    assert!(historical.previous);
    assert_eq!(state.effect_origins(historical).0.cloned().collect::<Vec<_>>(), [first.clone()]);
    assert_eq!(state.effect_origins(&parent).0.cloned().collect::<Vec<_>>(), [first.clone()]);

    state.stage_returned_reference(call, output.clone(), result, error_type(), [second.clone()]);
    state.complete_transition(call);

    let captured = captured.retired(&[(field, Retirement::Storage)]).expect("the evaluated view reaches the retired field");

    state.publish_captured_value(&retained_slot, &captured);

    let historical = state.referenced_regions(&retained_slot).expect("publication retains the captured view");

    assert_eq!(state.effect_origins(&historical[0]).0.cloned().collect::<Vec<_>>(), [first]);
    assert_eq!(state.effect_origins(&parent).0.cloned().collect::<Vec<_>>(), [second]);
}

#[test]
fn rewritten_returned_references_keep_flat_input_bounds() {
    let ([payload, first, second, unrelated], origins) = places();

    let call = origins[0].into();

    let mut state = State {
        owned_storage: [payload.root, first.root, second.root, unrelated.root].into(),
        ..State::default()
    };

    let mut input = Target::storage(payload.clone());

    for output in [&first, &second, &first] {
        state.stage_returned_reference(
            call,
            Target::storage(output.clone()),
            output.clone(),
            error_type(),
            [input],
        );

        state.bound_staged_reference(call, &Target::storage(output.clone()));
        state.stage_returned_references(call, [(output.clone(), error_type())], |_| []);
        state.complete_transition(call);

        input = state
            .value_target(&Target::storage(output.clone()), error_type())
            .expect("opaque output retains its symbolic reference");

        assert!(
            state
                .effect_origins(&input)
                .0
                .all(|target| !matches!(target.origin, super::Origin::Returned { .. })),
            "effect bounds contain physical input regions"
        );

        assert_ne!(
            state.target_relationship(&input, &Target::storage(payload.clone())),
            bray_bound_tree::StorageRelationship::Disjoint
        );

        assert_eq!(
            state.target_relationship(&input, &Target::storage(unrelated.clone())),
            bray_bound_tree::StorageRelationship::Disjoint
        );
    }
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
        &addressed_reference(error_type(), bray_symbols::DependencyReferenceKind::Address),
    );

    state.complete_transition(call);

    let alternatives = state
        .value_targets(&Target::storage(first))
        .expect("fixture `value_targets` call must succeed");

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
fn opaque_owned_values_keep_their_backing_identity_without_claiming_initialization() {
    let ([owner, pointer, moved, ..], origins) = places();

    let call = origins[0].into();

    let backing_slot = Place {
        root: owner.root,
        projections: [BUFFER_POINTER].into(),
    };

    let mut state = State {
        owned_storage: [owner.root, moved.root].into(),
        ..State::default()
    };

    let owner = Target::storage(owner);
    let moved = Target::storage(moved);
    let ty = error_type();

    state.stage_unknown(call, owner.clone());
    state.complete_transition(call);

    assert_eq!(state.owned_regions(&owner, BUFFER_POINTER), None);

    state.stage_unknown(call, owner.clone());

    state.stage_returned_reference(
        call,
        Target::storage(backing_slot.clone()),
        backing_slot,
        ty,
        [],
    );

    state.complete_transition(call);

    state.owned_region(
        &[owner.clone()],
        Target::storage(pointer.clone()),
        ty,
        BUFFER_POINTER,
        false,
    );

    let regions = state
        .owned_regions(&owner, BUFFER_POINTER)
        .expect("an owning value retains its physical backing region");

    assert_eq!(regions.len(), 1);

    assert_eq!(
        state.target_relationship(&owner, &regions[0]),
        bray_bound_tree::StorageRelationship::Disjoint
    );

    assert!(!state.initialized(&regions[0], ty));

    state.stage_value(call, &owner, moved.clone());
    state.stage_null(call, owner.project(&[BUFFER_POINTER]));
    state.complete_transition(call);

    assert_eq!(
        state.owned_regions(&moved, BUFFER_POINTER),
        Some(regions.clone())
    );

    assert_eq!(
        state.owned_regions(&owner, BUFFER_POINTER),
        Some(Vec::new())
    );

    assert!(!state.initialized(&regions[0], ty));

    assert_eq!(
        state.referenced_regions(&Target::storage(pointer)),
        Some(regions)
    );
}

#[test]
fn bounded_address_views_preserve_the_allocation_type_without_initializing_the_region() {
    let ([owner, pointer, ..], origins) = places();

    let region_type = error_type();
    let view_type = crate::test_support::nullable_type(region_type);
    let mut state = State::default();
    let owner = Target::storage(owner);

    state.declare_owned_buffer(owner.clone(), owner.project(&[BUFFER_POINTER]), region_type);

    state.owned_region(
        &[owner.clone()],
        Target::storage(pointer.clone()),
        region_type,
        BUFFER_POINTER,
        false,
    );

    state.offset(&Target::storage(pointer.clone()), pointer.clone());
    state.expose_address(&pointer, view_type, origins[0]);

    let region = state
        .owned_regions(&owner, BUFFER_POINTER)
        .expect("fixture `owned_regions` call must succeed")
        .pop()
        .expect("fixture `pop` call must succeed");

    let contents = state
        .contents
        .get(&region)
        .expect("fixture lookup must contain its requested entry");

    assert_eq!(contents.storage_type, Some(region_type));
    assert!(!contents.is_initialized(view_type));
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
    let owner = Target::storage(owner);

    state.declare_owned_buffer(owner.clone(), owner.project(&[BUFFER_POINTER]), region_type);

    state.owned_region(
        &[owner],
        Target::storage(pointer.clone()),
        region_type,
        BUFFER_POINTER,
        false,
    );

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
        &addressed_reference(region_type, bray_symbols::DependencyReferenceKind::Borrow),
    );

    state.bound_staged_reference(call, &destination);
    state.stage_returned_references(call, [(result.clone(), view_type)], |_| [payload.clone()]);
    state.complete_transition(call);

    assert!(state.returned_inputs[&(result, false)].is_bounded());

    let target = state
        .value_target(&destination, view_type)
        .expect("a bounded returned cell retains its declared view type");

    assert_eq!(
        state.effect_origins(&target).0.collect::<Vec<_>>(),
        vec![&payload]
    );

    assert_eq!(
        state.target_relationship(&target, &Target::storage(owner.clone())),
        bray_bound_tree::StorageRelationship::Disjoint,
    );

    let owner = Target::storage(owner);

    state.escape_targets([owner.clone()]);

    assert_eq!(
        state.target_relationship(&target, &owner),
        bray_bound_tree::StorageRelationship::Disjoint
    );

    assert!(!state.initialized(&destination, view_type));

    let exported = state.exit_references(
        destination
            .storage_path()
            .expect("fixture `storage_path` call must succeed")
            .root,
        &Default::default(),
    );

    assert_eq!(
        exported.slots[destination
            .storage_path()
            .expect("fixture `storage_path` call must succeed")]
        .address.as_ref().unwrap().targets,
        BTreeSet::from([payload.clone()])
    );

    assert!(
        !exported.slots[destination
            .storage_path()
            .expect("fixture `storage_path` call must succeed")]
        .address.as_ref().unwrap().is_exact()
    );

    let nested = payload.project(&[StorageProjection::AnyElement, BUFFER_POINTER, StorageProjection::OwnedTarget]);

    let actual = payload.project(&[StorageProjection::ElementFromStart(bray_symbols::SymbolOrdinal::new(0)),
        BUFFER_POINTER, StorageProjection::OwnedTarget]);

    let mut checked = State { owned_storage: state.owned_storage.clone(), ..State::default() };

    checked.stage_reference_regions(call, destination.clone(), destination.storage_path().unwrap().clone(),
        view_type, [nested.clone()]);

    checked.complete_transition(call);

    let returned = checked.value_target(&destination, view_type).unwrap();

    let (regions, unknown) = checked.effect_origins(&returned);

    assert!(!unknown, "a checked closed region does not introduce independent storage");
    assert_eq!(regions.collect::<Vec<_>>(), [&nested]);

    assert_ne!(checked.target_relationship(&returned, &actual), bray_bound_tree::StorageRelationship::Disjoint,
        "an unspecified selector must retain real aliases into the same backing");

    assert_eq!(checked.target_relationship(&returned, &owner), bray_bound_tree::StorageRelationship::Disjoint,
        "nested backing does not overlap its inline owner header");

    assert!(!checked.initialized(&destination, view_type), "region bounds grant no initialized authority");

    let exit = checked.exit_references(destination.storage_path().unwrap().root, &Default::default());

    assert!(exit.input_regions.contains(destination.storage_path().unwrap()), "republication retains region provenance");

    let mut opaque = checked.clone();

    opaque.stage_returned_reference(call, destination.clone(), destination.storage_path().unwrap().clone(), view_type, []);
    opaque.complete_transition(call);

    assert!(opaque.effect_origins(&opaque.value_target(&destination, view_type).unwrap()).1,
        "an independent alternative cannot be narrowed to the checked input bound");

    checked.stage_reference_regions(call, destination.clone(), destination.storage_path().unwrap().clone(),
        view_type, [returned.project(&[StorageProjection::OwnedTarget])]);

    checked.complete_transition(call);

    assert!(checked.effect_origins(&checked.value_target(&destination, view_type).unwrap()).1,
        "an additional owned indirection is not bounded by its descriptor's region");
}

#[test]
fn returned_null_does_not_invent_an_outside_origin() {
    let ([result, input, ..], origins) = places();

    let call = origins[0].into();
    let mut state = State::default();

    state.stage_null(call, Target::storage(result.clone()));

    state.stage_returned_references(call, [(result.clone(), error_type())], |_| -> [Target; 0] {
        panic!("an established null result needs no opaque input bounds")
    });

    state.complete_transition(call);

    assert_eq!(
        state.value_targets(&Target::storage(result.clone())),
        Some(Vec::new())
    );

    let exported = state.exit_references(result.root, &Default::default());

    assert_eq!(exported.empty_slots().collect::<Vec<_>>(), vec![&result]);

    let mut displaced = state.clone();

    displaced.offset(&Target::storage(result.clone()), result.clone());

    assert!(displaced.referenced_regions(&Target::storage(result.clone())).is_none(),
        "arithmetic from a null address cannot supply an empty allocation footprint");

    assert_eq!(displaced.exit_references(result.root, &Default::default()).empty_slots().count(), 0);

    let target = Target::storage(input);

    state.begin_transition(call);

    state.stage_address(call, target.clone(), Target::storage(result.clone()),
        &addressed_reference(error_type(), bray_symbols::DependencyReferenceKind::Address));

    state.stage_empty_reference(call, Target::storage(result.clone()));
    state.complete_transition(call);

    assert!(state.value_targets(&Target::storage(result.clone())).is_none(),
        "a nullable address cannot establish one definite location");

    assert_eq!(state.referenced_regions(&Target::storage(result.clone())), Some(vec![target]));

    assert_eq!(state.exit_references(result.root, &Default::default()).empty_slots().collect::<Vec<_>>(), vec![&result],
        "an empty alternative survives alongside its possible nonempty target");

    state.begin_transition(call);
    state.stage_slot(call, &result, result.clone());
    state.bound_staged_reference(call, &Target::storage(result.clone()));
    state.stage_returned_references(call, [(result.clone(), error_type())], |_| []);
    state.complete_transition(call);

    assert!(state.value_targets(&Target::storage(result.clone())).is_none());

    assert_eq!(state.exit_references(result.root, &Default::default()).empty_slots().collect::<Vec<_>>(), vec![&result],
        "capturing an imprecise returned pointer retains its known empty alternative");
}

#[test]
fn bounded_returned_references_preserve_input_exclusivity() {

    let ([left, right, result, raw], expressions) = places();

    let call = expressions[0].into();

    for (shared, opaque, mixed, previous, disjoint) in [
        (false, false, false, false, true),
        (true, false, false, false, false),
        (false, true, false, false, false),
        (false, false, true, false, false),
        (false, false, false, true, false),
    ] {
        let mut state = State {
            entry_borrows: Arc::new(std::collections::BTreeMap::from([
                (left.root, if shared { BorrowKind::Shared } else { BorrowKind::Mutable }),
                (right.root, BorrowKind::Shared),
            ])),
            ..State::default()
        };

        let left = Target { previous: false, origin: StorageOrigin::entry(left.clone(), bray_symbols::DependencyReferenceKind::Address) }
            .project(&[StorageProjection::OwnedTarget, StorageProjection::Element(expressions[0])]);

        let right = Target { previous: false, origin: StorageOrigin::entry(right.clone(), bray_symbols::DependencyReferenceKind::Address) };
        let destination = Target::storage(result.clone());

        if opaque {
            state.stage_returned_reference(call, destination.clone(), result.clone(), error_type(), [left.clone()]);
        } else {
            for target in std::iter::once(left.clone()).chain(mixed.then(|| right.clone())) {
                state.stage_address(call, target, destination.clone(),
                    &addressed_reference(error_type(), bray_symbols::DependencyReferenceKind::Borrow));
            }

            state.bound_staged_reference(call, &destination);
            state.stage_returned_references(call, [(result.clone(), error_type())], |_| [left.clone()]);
        }

        state.complete_transition(call);

        let mut returned = state.value_target(&destination, error_type()).expect("a returned view has a physical identity");

        if previous {
            state.stage_returned_reference(call, destination.clone(), result.clone(), error_type(), [left.clone()]);
            state.complete_transition(call);
            returned.previous = true;
        }

        let returned = returned.project(&[StorageProjection::UninitPayload]);

        assert_eq!(state.effect_origins(&returned).1, opaque || previous);
        assert!(state.effect_origins(&returned.project(&[StorageProjection::OwnedTarget])).1);

        let expected = if disjoint { StorageRelationship::Disjoint } else { StorageRelationship::PotentiallyOverlapping };

        for (left, right) in [(&returned, &right), (&right, &returned)] {
            assert_eq!(state.target_relationship(left, right), expected,
                "shared={shared}, opaque={opaque}, mixed={mixed}, previous={previous}");
        }

        assert_ne!(state.target_relationship(&returned.project(&[StorageProjection::OwnedTarget]), &right),
            StorageRelationship::Disjoint, "a reached owner needs its own physical bound");

        assert_ne!(state.target_relationship(&returned, &left), StorageRelationship::Disjoint);

        let raw = Target { previous: false, origin: StorageOrigin::entry(raw.clone(), bray_symbols::DependencyReferenceKind::Address) };

        assert_ne!(state.target_relationship(&returned, &raw), StorageRelationship::Disjoint);
    }
}

#[test]
fn input_exclusivity_follows_mutable_ownership_and_preserves_shared_aliases() {

    let ([left, right, raw, _], _) = places();

    let state = State {
        entry_borrows: Arc::new(std::collections::BTreeMap::from([
            (left.root, BorrowKind::Mutable),
            (right.root, BorrowKind::Shared),
        ])),
        ..State::default()
    };

    let target = |path: Place| Target {
        previous: false,
        origin: StorageOrigin::entry(path, bray_symbols::DependencyReferenceKind::Address),
    };

    let left = target(left);
    let right = target(right);
    let raw = target(raw);

    assert_eq!(
        state.target_relationship(&left, &right),
        StorageRelationship::Disjoint
    );

    assert_eq!(
        state.target_relationship(&left, &left),
        StorageRelationship::Identical
    );

    assert_eq!(
        state.target_relationship(&left, &raw),
        StorageRelationship::PotentiallyOverlapping
    );

    assert_eq!(
        state.target_relationship(&left.project(&[StorageProjection::OwnedTarget]), &right),
        StorageRelationship::Disjoint,
    );

    assert_eq!(
        state.target_relationship(&left.project(&[StorageProjection::UninitPayload]), &right),
        StorageRelationship::Disjoint,
    );

    assert_eq!(
        state.target_relationship(&right.project(&[StorageProjection::OwnedTarget]), &left),
        StorageRelationship::PotentiallyOverlapping,
    );

    assert_eq!(
        state.target_relationship(&right.project(&[StorageProjection::UninitPayload]), &left),
        StorageRelationship::PotentiallyOverlapping,
    );

    assert_eq!(
        state.target_relationship(&left.project(&[StorageProjection::OwnedTarget]), &raw),
        StorageRelationship::PotentiallyOverlapping,
    );

    let mut nested = left.clone();

    if let StorageOrigin::Entry { layer, .. } = &mut nested.origin {
        *layer = 2;
    }

    assert_eq!(
        state.target_relationship(&nested, &right),
        StorageRelationship::PotentiallyOverlapping
    );
}

#[test]
fn deferred_input_references_survive_frame_moves_without_following_replaced_input_slots() {
    let ([input, frame, payload, moved], expressions) = places();

    let producer = expressions[0].into();
    let movement = expressions[1].into();
    let root = Target::storage(input.clone());
    let path = bray_bound_tree::StorageReferencePath::new([].into(), [].into());

    for target in [
        Target::storage(payload.clone()),
        Target::storage(frame.clone()).project(&[StorageProjection::OwnedTarget]),
        Target {
            previous: false,
            origin: Origin::Returned { reference: frame.clone(), projections: [].into() },
        },
    ] {
        let mut state = State::default();

        state.point_at(target.clone(), input.clone(), error_type(), bray_symbols::DependencyReferenceKind::Borrow);

        let captured = state.capture_reached_value(&root);
        let value = captured_frame(producer, Arc::new(bray_bound_tree::StorageDeferredTransition::default()));
        let mut frame_value = Arc::clone(value.deferred_value().unwrap());
        let producer = &mut Arc::make_mut(&mut frame_value).producers.get_mut(&producer.into()).unwrap().entry;
        let ordinal = bray_symbols::SymbolOrdinal::new(0);

        Arc::make_mut(producer).arguments.insert(
            crate::ExecutionPlace::argument(ordinal),
            crate::ExecutionCondition::unknown().captured_value(captured),
        );

        state.clear(&input);
        state.point_at(Target::storage(moved.clone()), input.clone(), error_type(), bray_symbols::DependencyReferenceKind::Borrow);
        state.begin_transition(movement);
        state.stage_slot(movement, &frame, moved.clone());
        state.clear(&frame);
        state.complete_transition(movement);

        let captured = frame_value.producers.values().next().unwrap().entry.argument_storage(ordinal).unwrap();
        let sources = state.captured_reference_sources(semantic_values(), captured, &path).unwrap();

        let [actual] = sources.as_slice() else { panic!("the frame retains one captured input"); };

        assert_eq!(actual.targets(&state), Some(vec![target.clone()]),
            "output instantiation must preserve the captured physical input");

        state.retire_region(&target, Retirement::Storage);

        let retired = captured.retired(&[(target, Retirement::Storage)])
            .expect("the captured descriptor reaches the retired storage");

        assert!(retired.known_address().is_none(), "retired referents cannot become current input observations");
    }
}

#[test]
fn staged_future_moves_retain_completion_effects_and_references() {
    let ([frame, moved, payload, output], expressions) = places();

    let producer = expressions[0].into();
    let movement = expressions[1].into();
    let observation = expressions[2].into();
    let effect = super::Effect::unbounded(bray_symbols::DependencyMutationKind::Dispose(error_type()), None, false);
    let mut state = State::default();

    state.address(payload.clone(), frame.clone(), error_type(), expressions[0]);

    let operand = super::Source::Value(Target::storage(frame.clone())).mutation_targets(&state)
        .expect("deferred entry captures its evaluated pointer").into_iter().next().expect("one pointer alternative");

    let retirement = Effect::through(operand, DependencyMutationKind::Retire, None);

    let outcomes = super::EffectOutcomes::new([effect.clone(), retirement]).with_normal([]);
    let captured_cleanup = super::Effect::at(Target::storage(payload.clone()), bray_symbols::DependencyMutationKind::Clear, None);
    let result_cleanup = super::Effect::at(Target::storage(payload.clone()), bray_symbols::DependencyMutationKind::Escape, None);
    let captured = state.capture_reached_value(&Target::storage(frame.clone()));

    // A loop can leave another frame from the same site in an owned capture.
    let child = captured_frame(producer, Arc::new(bray_bound_tree::StorageDeferredTransition {
        effects: Some(super::EffectOutcomes::new([Effect::Escape(None)])),
        ..Default::default()
    }));

    let value = captured_frame(producer, Arc::new(bray_bound_tree::StorageDeferredTransition {
        effects: Some(outcomes.clone()),
        captured_cleanup: Some([captured_cleanup.clone()].into()),
        result_cleanup: Some([result_cleanup.clone()].into()),
        ..Default::default()
    }));

    let mut outer = Arc::clone(value.deferred_value().unwrap());
    let entry = &mut Arc::make_mut(&mut outer).producers.get_mut(&producer.into()).unwrap().entry;
    let ordinal = bray_symbols::SymbolOrdinal::new(0);

    Arc::make_mut(entry).arguments.insert(crate::ExecutionPlace::argument(ordinal), child.captured_value(captured));
    Arc::make_mut(entry).owned_inputs = [ordinal].into();

    let value = value.with_deferred_value(outer);

    state.begin_transition(movement);
    state.stage_slot(movement, &frame, moved.clone());
    state.clear(&frame);
    state.complete_transition(movement);

    let assumptions = Default::default();
    let proof = crate::execution_guarantees::ExecutionProof::new(&assumptions, &assumptions);

    assert_eq!(
        value.deferred_value().unwrap().effects(&proof),
        Some(outcomes)
    );

    assert_eq!(value.deferred_value().unwrap().cleanup_effects(false, &proof), Some(vec![captured_cleanup.clone()]));

    let mut discarded = vec![captured_cleanup, result_cleanup];

    discarded.sort_unstable();

    assert_eq!(value.deferred_value().unwrap().cleanup_effects(true, &proof), Some(discarded));

    let captured = value.deferred_value().unwrap().producers[&producer.into()].entry.argument_storage(ordinal).unwrap();

    state.begin_transition(observation);
    super::Source::Captured(captured.clone()).stage(&mut state, observation, Target::storage(output.clone()));

    assert!(!state.values.contains_key(&Target::storage(output.clone())));

    state.complete_transition(observation);

    assert!(
        state.values[&Target::storage(output)]
            .address.as_ref().unwrap().targets
            .contains(&Target::storage(payload))
    );
}

#[test]
fn fresh_allocation_publication_follows_only_the_references_stored_in_it() {
    let ([input, owner, _, _], origins) = places();

    let mut memory = State::default();

    memory.entry_reference(input.clone());

    let entry = memory
        .value_targets(&Target::storage(input.clone()))
        .unwrap()[0]
        .clone();

    let allocation = memory.fresh(owner, origins[0]);

    assert_eq!(
        allocation.relationship(&entry),
        bray_bound_tree::StorageRelationship::Disjoint
    );

    assert_eq!(
        entry.relationship(&allocation),
        bray_bound_tree::StorageRelationship::Disjoint
    );

    assert_eq!(
        memory.escaped_regions([allocation.clone()]),
        BTreeSet::from([allocation.clone()])
    );

    let call = origins[1].into();

    memory.stage_value(
        call,
        &Target::storage(input),
        allocation.project(&[StorageProjection::TupleElement(
            bray_symbols::SymbolOrdinal::new(0),
        )]),
    );

    memory.complete_transition(call);

    assert_eq!(
        memory.escaped_regions([allocation.clone()]),
        BTreeSet::from([allocation, entry])
    );
}

#[test]
fn typed_scalar_publication_does_not_load_aliased_pointer_slots() {
    let ([left, right, owner, raw], origins) = places();

    let scalar = error_type();
    let container = crate::test_support::nullable_type(scalar);
    let mut memory = State::default();

    memory.complete_reference_types = Arc::new([(scalar, true), (container, false)].into());

    memory.entry_reference_at(
        left.clone(),
        1,
        Target::storage(left.clone()),
        Some(container),
     bray_symbols::DependencyReferenceKind::Address);

    memory.entry_reference_at(
        right.clone(),
        1,
        Target::storage(right.clone()),
        Some(container),
     bray_symbols::DependencyReferenceKind::Address);

    let left = memory.value_targets(&Target::storage(left)).unwrap()[0].clone();
    let right = memory.value_targets(&Target::storage(right)).unwrap()[0].clone();
    let allocation = memory.fresh(owner.clone(), origins[0]);
    let call = origins[1].into();

    memory.stage_value(
        call,
        &Target::storage(owner.clone()),
        left.project(&[StorageProjection::TupleElement(
            bray_symbols::SymbolOrdinal::new(0),
        )]),
    );

    memory.complete_transition(call);

    // Aliased pointer-containing inputs still expose newly stored allocations.
    assert!(
        memory
            .escaped_regions([right.clone()])
            .contains(&allocation)
    );

    let word = right.project(&[StorageProjection::TupleElement(
        bray_symbols::SymbolOrdinal::new(1),
    )]);

    memory
        .contents
        .entry(word.clone())
        .or_default()
        .storage_type = Some(scalar);

    assert_eq!(
        memory.escaped_regions([word.clone()]),
        BTreeSet::from([word.clone()])
    );

    // Known stored references remain authoritative across typed layout views.
    let call = origins[2].into();

    memory.stage_value(call, &Target::storage(owner), word.clone());
    memory.complete_transition(call);

    memory
        .contents
        .entry(word.clone())
        .or_default()
        .storage_type = Some(scalar);

    assert!(memory.escaped_regions([word]).contains(&allocation));

    // Explicit raw exposure retains conservative allocation reachability.
    let other = right.project(&[StorageProjection::TupleElement(
        bray_symbols::SymbolOrdinal::new(2),
    )]);

    memory
        .contents
        .entry(other.clone())
        .or_default()
        .storage_type = Some(scalar);

    assert_eq!(
        memory.escaped_regions([other.clone()]),
        BTreeSet::from([other.clone()])
    );

    memory.point_at(
        other.clone(),
        raw,
        scalar,
        bray_symbols::DependencyReferenceKind::Address,
    );

    assert!(memory.escaped_regions([other]).contains(&allocation));
}

#[test]
fn mutation_summaries_preserve_initialization_only_for_complete_typed_cells() {

    let ([cell, alias, ..], origins) = places();

    let ty = error_type();
    let other = crate::test_support::nullable_type(ty);

    let target = Target {
        previous: false,
        origin: Origin::Entry {
            reference: cell,
            layer: 1,
            kind: bray_symbols::DependencyReferenceKind::Address,
            projections: Arc::from([]),
        },
    };

    let alias = Target {
        previous: false,
        origin: Origin::Entry {
            reference: alias,
            layer: 1,
            kind: bray_symbols::DependencyReferenceKind::Address,
            projections: Arc::from([]),
        },
    };

    let invalidation = bray_bound_tree::StorageInvalidation::unknown();

    // Identical types do not imply that two possible aliases name a complete cell.
    for (effect, valid) in [
        (Effect::at(target.clone(), DependencyMutationKind::Replace(ty), None), true),
        (Effect::bounded(target.clone(), DependencyMutationKind::Replace(ty), None), false),
        (Effect::at(alias.clone(), DependencyMutationKind::Replace(ty), None), false),
        (Effect::at(target.clone(), DependencyMutationKind::Replace(ty), Some(other)), false),
        (Effect::at(target.clone(), DependencyMutationKind::Replace(other), Some(other)), false),
        (Effect::UnboundedMutation { kind: DependencyMutationKind::Replace(ty), access_type: Some(other), ambient: false }, false),
        (Effect::UnboundedMutation { kind: DependencyMutationKind::Replace(other), access_type: Some(other), ambient: true }, false),
        (Effect::at(target.clone(), DependencyMutationKind::Consume, None), false),
        (Effect::Atomic, true),
    ] {
        let mut state = State::default();
        let contents = state.contents.entry(target.clone()).or_default();

        contents.storage_type = Some(ty);
        contents.observe_initialized(ty);
        state.escape_regions([target.clone()]);

        assert_eq!(state.target_relationship(&target, &alias), bray_bound_tree::StorageRelationship::PotentiallyOverlapping);
        state.apply_effects(Some(&[effect.clone()]), &invalidation, origins[0].into());

        assert_eq!(
            state.read_value(target.clone(), ty, MemoryReadKind::Copy, None, None),
            if valid { MemoryOperationStatus::Valid } else { MemoryOperationStatus::UninitializedRawStorage },
            "a read without a new proof must respect the complete update: {effect:?}",
        );

        // A fresh checked proof can establish initialization after a raw write.
        assert_eq!(state.read_value(target.clone(), ty, MemoryReadKind::Copy, Some(origins[0]), None), MemoryOperationStatus::Valid);
    }
}

#[test]
fn unresolved_deallocation_distinguishes_inline_slots_from_backing() {
    let ([owner, backing, ..], expressions) = places();

    let ty = error_type();
    let local = Target::storage(owner.clone());

    let allocation = Target {
        previous: false,
        origin: Origin::Allocation { allocation: backing, projections: Arc::from([]) },
    };

    for (access_type, ambient, retains_local) in [(Some(ty), false, true), (Some(ty), true, true), (None, false, false)] {
        let mut state = State { owned_storage: [owner.root].into(), ..State::default() };

        for target in [&local, &allocation] {
            state.contents.entry(target.clone()).or_default().record_write(ty);
        }

        state.escape_regions([local.clone(), allocation.clone()]);

        state.apply_effects(Some(&[Effect::unbounded(DependencyMutationKind::Retire, access_type, ambient)]),
            &bray_bound_tree::StorageInvalidation::unknown(), expressions[0].into());

        assert_eq!(state.contents[&local].is_initialized(ty), retains_local);
        assert!(!state.contents[&allocation].is_initialized(ty));
    }
}

#[test]
fn produced_buffer_allocation_survives_transport_without_publishing_early() {
    let ([owner, moved, exposed, raw], expressions) = places();

    let call = expressions[0].into();
    let pointer = Place { root: owner.root, projections: [BUFFER_POINTER].into() };
    let mut state = State { owned_storage: [owner.root, moved.root, exposed.root, raw.root].into(), ..State::default() };

    state.escape_regions([Target::storage(exposed.clone())]);
    state.stage_allocation_reference(call, Target::storage(pointer.clone()), owner.clone(), &[]);
    assert!(state.referenced_regions(&Target::storage(pointer.clone())).is_none(), "preparation publishes no result");
    state.complete_transition(call);
    state.stage_slot(call, &owner, moved.clone());
    state.complete_transition(call);
    state.clear(&owner);

    let pointer = state.value_targets(&Target::storage(Place { root: moved.root, projections: [BUFFER_POINTER].into() }))
        .and_then(|targets| targets.into_iter().next()).expect("moved buffer retains its pointer");

    assert_eq!(state.target_relationship(&pointer, &Target::storage(exposed.clone())), bray_bound_tree::StorageRelationship::Disjoint);
    assert!(!state.initialized(&pointer, error_type()), "allocation alone supplies no initialization proof");

    let nested = pointer.project(&[StorageProjection::AnyElement]);

    state.declare_owned_buffer(nested.clone(), nested.project(&[BUFFER_POINTER]), error_type());

    let backing = state.owned_regions(&nested, BUFFER_POINTER).unwrap().remove(0);
    let produced_pointer = Place { root: owner.root, projections: [BUFFER_POINTER].into() };

    state.stage_allocation_reference(call, Target::storage(produced_pointer.clone()), owner, &[]);
    state.complete_transition(call);

    assert!(state.referenced_regions(&Target::storage(Place { root: moved.root, projections: [BUFFER_POINTER].into() }))
        .unwrap().iter().all(|target| target.previous), "a repeated producer cannot revive its previous result");

    assert!(state.contents.contains_key(&Target { previous: true, ..backing }),
        "producer publication advances prior nested backing coordinates too");

    assert!(state.referenced_regions(&Target::storage(produced_pointer)).unwrap().iter().all(|target| !target.previous));

    state.stage_returned_reference(call, Target::storage(raw.clone()), raw.clone(), error_type(), []);
    state.complete_transition(call);

    let raw = state.value_target(&Target::storage(raw), error_type()).expect("ordinary returned address remains represented");

    assert_ne!(state.target_relationship(&raw, &Target::storage(exposed)), bray_bound_tree::StorageRelationship::Disjoint,
        "an ordinary returned raw address can still reach previously published inline storage");
}

#[test]
fn escape_roots_keep_stored_values_without_exporting_possible_aliases() {
    let ([parameter, local, ..], _) = places();

    let field = StorageProjection::TupleElement(bray_symbols::SymbolOrdinal::new(0));
    let pointer_field = StorageProjection::TupleElement(bray_symbols::SymbolOrdinal::new(1));
    let owner = Target { previous: false, origin: super::Origin::entry(parameter.clone(), bray_symbols::DependencyReferenceKind::Borrow) };
    let stored = Target { previous: false, origin: super::Origin::entry(Place { root: parameter.root, projections: [field].into() }, bray_symbols::DependencyReferenceKind::Borrow) };
    let pointer = Target { previous: false, origin: super::Origin::entry(Place { root: parameter.root, projections: [pointer_field].into() }, bray_symbols::DependencyReferenceKind::Address) };
    let mut state = State { owned_storage: [local.root].into(), ..State::default() };

    state.values.insert(owner.project(&[field]), super::model::AddressValue::one(stored.clone()).into());
    assert!(state.escaped_regions([pointer.clone()]).contains(&stored), "body-local checks retain possible aliases");
    assert_eq!(state.escape_roots([pointer.clone()]), BTreeSet::from([pointer.clone()]), "possible aliases are resolved again at the caller");

    let incoming_field = Target { previous: false, origin: super::Origin::entry(
        Place { root: parameter.root, projections: [pointer_field, StorageProjection::AddressTarget, field].into() },
        bray_symbols::DependencyReferenceKind::Address) };

    state.values.insert(pointer.project(&[field]), super::model::AddressValue::one(incoming_field.clone()).into());

    assert_eq!(state.escape_roots([pointer.clone()]), BTreeSet::from([pointer.clone()]),
        "an unchanged incoming pointee field is already reachable from its escaped raw-address root");

    assert!(state.escape_roots([incoming_field.clone()]).contains(&incoming_field),
        "an explicitly escaped field pointer remains an operand");

    state.values.insert(Target::storage(local.clone()).project(&[pointer_field]), super::model::AddressValue::one(incoming_field.clone()).into());

    let slot = pointer.project(&[field]);
    let bounds = state.values.remove(&slot).unwrap().into_dependency_bounds();

    state.values.insert(slot, bounds);

    assert!(state.escape_roots([pointer.clone(), Target::storage(local.clone())]).contains(&incoming_field),
        "possible origins do not prove the source field still holds its evaluated entry value");

    state.values.insert(pointer.project(&[field]), super::model::AddressValue::one(stored.clone()).into());

    let escaped = state.escape_roots([pointer.clone(), Target::storage(local.clone())]);

    assert!(escaped.contains(&incoming_field), "a captured old field pointer survives replacement of its original slot");
    assert!(escaped.contains(&stored), "a replacement's actual incoming pointer remains an escape root");

    let local = Target::storage(local);

    state.values.insert(local.project(&[field]), super::model::AddressValue::one(stored.clone()).into());

    assert!(state.escape_roots([local.project(&[pointer_field])]).contains(&stored),
        "an escaped local allocation retains its actual stored references even through a field address");
}
