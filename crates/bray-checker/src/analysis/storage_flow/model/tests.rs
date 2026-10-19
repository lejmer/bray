use bray_bound_tree::{
    BoundErrorExpression, BoundExpression, BoundUnitId, BoundUnitKind, StorageIdentity,
    StoragePlanBuilder,
};

use super::StorageFlowState;
use crate::test_support::{error_type, expression_unit, push_expression};

#[test]
fn raw_expression_indexes_preserve_first_result_and_later_alias_order() {
    use bray_bound_tree::{StorageAccess, StorageAccessPurpose, StorageAccessRoot};

    let (unit, expressions) = expression_unit(BoundUnitId::new(92), |tree, origin| {
        (0..2)
            .map(|_| {
                push_expression(
                    tree,
                    BoundExpression::Error(BoundErrorExpression::new(origin, error_type())),
                )
            })
            .collect()
    });

    let mut builder = StoragePlanBuilder::new(unit.unit(), unit.key().kind());

    let first = builder
        .push_identity(StorageIdentity::Temporary(expressions[0]))
        .expect("first result must build");

    let second = builder
        .push_identity(StorageIdentity::Allocation(expressions[0]))
        .expect("second result must build");

    let source = unit
        .tree()
        .expression(expressions[1])
        .expect("test expression must exist")
        .origin()
        .source_anchor();

    for (identity, recovered) in [(first, true), (first, false), (second, false)] {
        let access = builder
            .push_access(StorageAccess::new(
                StorageAccessRoot::Storage(identity),
                [],
                error_type(),
                source,
                recovered,
            ))
            .expect("test access must build");

        builder
            .plan_access(
                expressions[1].into(),
                expressions[1],
                StorageAccessPurpose::Read,
                access,
            )
            .expect("test access plan must build");
    }

    let storage = builder.finish();

    let mut input = super::StorageFlowInput::default();
    input.prepare_locations(&storage, &crate::analysis::guarantee::CleanupPlan::default());

    assert_eq!(input.definitions(expressions[0].into()), &[first, second]);

    assert_eq!(
        input.expression_places(expressions[1]),
        &[
            super::super::memory::Place::root(first),
            super::super::memory::Place::root(second)
        ]
    );

    let mut state = reachable_state();

    state
        .memory
        .fresh(super::super::memory::Place::root(second), expressions[0]);

    assert_eq!(
        input
            .expression_places(expressions[1])
            .iter()
            .find(|place| state.memory.has_stored_value(place)),
        Some(&super::super::memory::Place::root(second))
    );
}

#[test]
fn guard_observations_do_not_create_ownership_and_join_by_intersection() {
    let (identity, _) = storage_and_expressions(78);

    let mut observed = reachable_state();

    observed.observed_pattern_bindings.insert(identity);

    assert!(observed.live.is_empty());
    assert!(observed.initialized.is_empty());

    let mut merged = observed.clone();

    assert!(!merged.merge(&observed, None));
    assert!(merged.observed_pattern_bindings.contains(&identity));
    assert!(merged.merge(&reachable_state(), None));
    assert!(merged.observed_pattern_bindings.is_empty());
    assert!(!merged.merge(&observed, None));
}

#[test]
fn fully_moved_storage_survives_only_when_every_incoming_path_moves_it() {
    let (identity, _) = storage_and_expressions(79);

    let mut left = reachable_state();
    let mut moved = reachable_state();

    left.fully_moved.insert(identity);
    moved.fully_moved.insert(identity);

    assert!(!left.merge(&moved, None));
    assert_eq!(left.fully_moved, [identity].into_iter().collect());

    assert!(left.merge(&reachable_state(), None));
    assert!(left.fully_moved.is_empty());
}

#[test]
fn complete_moves_end_definite_initialization() {
    let (identity, _) = storage_and_expressions(81);

    let mut state = reachable_state();

    state.initialized.insert(identity);
    state.move_complete_storage(identity);

    assert_eq!(state.fully_moved, [identity].into_iter().collect());
    assert!(state.initialized.is_empty());
}

#[test]
fn control_flow_joins_retain_maybe_live_storage_and_intersect_initialization() {
    let (identity, _) = storage_and_expressions(80);

    let mut initialized = reachable_state();

    initialized.live.insert(identity);
    initialized.initialized.insert(identity);

    assert!(initialized.merge(&reachable_state(), None));
    assert_eq!(initialized.live, [identity].into_iter().collect());
    assert!(initialized.initialized.is_empty());
}

#[test]
fn absence_provenance_survives_mutation_and_joins_conservatively() {
    use super::StorageAbsence;
    use bray_bound_tree::{StorageAccess, StorageAccessRoot, StorageInvalidation};

    let (unit, expressions) = expression_unit(BoundUnitId::new(82), |tree, origin| {
        (0..2)
            .map(|_| {
                push_expression(
                    tree,
                    BoundExpression::Error(BoundErrorExpression::new(origin, error_type())),
                )
            })
            .collect::<Vec<_>>()
    });

    let mut builder = StoragePlanBuilder::new(unit.unit(), unit.key().kind());

    let accesses = expressions
        .iter()
        .map(|expression| {
            let identity = builder
                .push_identity(StorageIdentity::Allocation(*expression))
                .unwrap();

            let origin = unit
                .tree()
                .expression(*expression)
                .unwrap()
                .origin()
                .source_anchor();

            builder
                .push_access(StorageAccess::new(
                    StorageAccessRoot::Storage(identity),
                    [],
                    error_type(),
                    origin,
                    false,
                ))
                .unwrap()
        })
        .collect::<Vec<_>>();

    let [moved, observed] = accesses.as_slice() else {
        panic!("test builds two accesses");
    };

    let (moved, observed) = (*moved, *observed);

    let storage = builder.finish();
    let moved = storage.logical_path(moved).cloned().unwrap();
    let observed = storage.logical_path(observed).cloned().unwrap();
    let mut state = reachable_state();

    state.definitely_absent.extend([
        (moved.clone(), StorageAbsence::Moved),
        (observed, StorageAbsence::NoPayload),
    ]);

    state.invalidate_observed_absence(
        &StorageInvalidation::unknown(),
        &storage,
        &Default::default(),
    );

    assert_eq!(
        state.definitely_absent.keys().cloned().collect::<Vec<_>>(),
        [moved.clone()]
    );

    let mut incoming = reachable_state();

    incoming
        .definitely_absent
        .insert(moved, StorageAbsence::NoPayload);

    assert!(state.merge(&incoming, None));
    assert!(!state.merge(&incoming, None));

    state.invalidate_observed_absence(
        &StorageInvalidation::unknown(),
        &storage,
        &Default::default(),
    );

    assert!(state.definitely_absent.is_empty());
}

#[test]
fn uncertain_writes_do_not_reconstruct_a_definitely_moved_component() {
    let (root, expressions) = storage_and_expressions(125);

    let exact = bray_bound_tree::StoragePath {
        root,
        projections: [bray_bound_tree::StorageProjection::ElementFromStart(
            bray_symbols::SymbolOrdinal::new(0),
        )]
        .into(),
    };

    let dynamic = bray_bound_tree::StoragePath {
        root,
        projections: [bray_bound_tree::StorageProjection::Element(expressions[1])].into(),
    };

    let mut state = reachable_state();

    state.live.insert(root);
    state.initialized.insert(root);

    state.record_move(
        exact.clone(),
        expressions[0],
        crate::test_support::error_type(),
        true,
    );

    state.initialize_path(&dynamic);
    assert!(state.moved.contains_key(&exact));
    assert!(state.definitely_absent.is_empty());
    state.initialize_path(&exact);
    assert!(state.moved.is_empty());
}

fn reachable_state() -> StorageFlowState {
    StorageFlowState {
        reachable: true,
        ..StorageFlowState::default()
    }
}

fn storage_and_expressions(
    raw_unit: u32,
) -> (
    bray_bound_tree::StorageIdentityId,
    Vec<bray_bound_tree::BoundExpressionId>,
) {
    let unit = BoundUnitId::new(raw_unit);

    let (_, expressions) = expression_unit(unit, |tree, origin| {
        (0..3)
            .map(|_| {
                push_expression(
                    tree,
                    BoundExpression::Error(BoundErrorExpression::new(origin, error_type())),
                )
            })
            .collect::<Vec<_>>()
    });

    let mut storage = StoragePlanBuilder::new(unit, BoundUnitKind::CallableBody);

    let identity = storage
        .push_identity(StorageIdentity::Allocation(expressions[0]))
        .unwrap_or_else(|error| panic!("allocation identity must validate: {error:?}"));

    (identity, expressions)
}

#[test]
fn payload_absence_survives_contents_changes_but_not_discriminant_replacement() {
    use super::StorageAbsence;

    use bray_bound_tree::{
        StorageAccess, StorageAccessRoot, StorageInvalidation, StoragePath, StorageProjection,
    };

    use std::collections::{BTreeMap, BTreeSet};

    let (unit, expressions) = expression_unit(BoundUnitId::new(126), |tree, origin| {
        vec![push_expression(
            tree,
            BoundExpression::Error(BoundErrorExpression::new(origin, error_type())),
        )]
    });

    let source = unit
        .tree()
        .expression(expressions[0])
        .unwrap()
        .origin()
        .source_anchor();

    let mut builder = StoragePlanBuilder::new(unit.unit(), BoundUnitKind::CallableBody);

    let root = builder
        .push_identity(StorageIdentity::Temporary(expressions[0]))
        .unwrap();

    let access = builder
        .push_access(StorageAccess::new(
            StorageAccessRoot::Storage(root),
            [],
            error_type(),
            source,
            false,
        ))
        .unwrap();

    let storage = builder.finish();
    let parent = StoragePath::root(root);

    let payload = StoragePath {
        root,
        projections: [StorageProjection::NullableValue].into(),
    };

    let discriminants = BTreeMap::from([(root, vec![parent.clone()])]);

    let observed = |changed| StorageInvalidation::Observed {
        accesses: BTreeMap::from([(access, vec![changed])]),
        snapshots: BTreeMap::new(),
        referents: BTreeSet::new(),
        stable_descriptors: BTreeSet::new(),
    };

    let opaque = StorageInvalidation::All {
        stable_descriptors: [parent.clone()].into(),
        stable_values: BTreeSet::new(),
    };

    for (invalidation, inline, retained) in [
        (observed(payload.clone()), true, true),
        (observed(parent), true, false),
        (opaque.clone(), true, true),
        (opaque, false, false),
        (StorageInvalidation::unknown(), true, false),
    ] {
        let mut state = reachable_state();

        state
            .definitely_absent
            .insert(payload.clone(), StorageAbsence::NoPayload);

        let no_discriminants = BTreeMap::new();

        state.invalidate_observed_absence(
            &invalidation,
            &storage,
            if inline {
                &discriminants
            } else {
                &no_discriminants
            },
        );

        assert_eq!(state.definitely_absent.contains_key(&payload), retained);
    }
}
