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

    input.prepare_locations(
        &storage,
        &crate::analysis::guarantee::CleanupPlan::default(),
    );

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

    left.live.insert(identity);
    moved.live.insert(identity);
    left.fully_moved.insert(identity);
    moved.fully_moved.insert(identity);

    assert!(!left.merge(&moved, None));
    assert_eq!(left.fully_moved, [identity].into_iter().collect());

    let mut owned = reachable_state();

    owned.live.insert(identity);
    owned.initialized.insert(identity);

    assert!(left.merge(&owned, None));
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
    use bray_bound_tree::StorageAbsence;
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
                .expect("fixture identity must fit its identity space");

            let origin = unit
                .tree()
                .expression(*expression)
                .expect("fixture expression must exist in its bound unit")
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
                .expect("fixture access must fit its identity space")
        })
        .collect::<Vec<_>>();

    let [moved, observed] = accesses.as_slice() else {
        panic!("test builds two accesses");
    };

    let (moved, observed) = (*moved, *observed);

    let storage = builder.finish();

    let moved = storage
        .logical_path(moved)
        .cloned()
        .expect("fixture cloned selection must contain its expected value");

    let observed = storage
        .logical_path(observed)
        .cloned()
        .expect("fixture cloned selection must contain its expected value");

    let mut state = reachable_state();

    std::sync::Arc::make_mut(&mut state.definitely_absent).extend([
        (moved.clone(), StorageAbsence::Moved),
        (observed, StorageAbsence::NoPayload),
    ]);

    state.invalidate_observed_absence(
        &StorageInvalidation::unknown(),
        &Default::default(),
    );

    assert_eq!(
        state.definitely_absent.keys().cloned().collect::<Vec<_>>(),
        std::slice::from_ref(&moved)
    );

    let mut incoming = reachable_state();

    std::sync::Arc::make_mut(&mut incoming.definitely_absent).insert(moved, StorageAbsence::NoPayload);

    assert!(state.merge(&incoming, None));
    assert!(!state.merge(&incoming, None));

    state.invalidate_observed_absence(
        &StorageInvalidation::unknown(),
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
    let mut state = StorageFlowState::default();

    state.reachable = true;

    state
}

#[test]
fn completed_call_payload_absence_reaches_cleanup_and_is_replaced_by_a_later_output() {
    use bray_bound_tree::{StoragePath, StorageProjection};

    let (identity, expressions) = storage_and_expressions(141);

    let payload = StoragePath {
        root: identity,
        projections: [StorageProjection::NullableValue].into(),
    };

    let mut state = reachable_state();

    state.memory.stage_no_payload_alternative(
        expressions[1].into(),
        super::super::memory::Target::storage(payload.clone()),
    );

    state.memory.complete_transition(expressions[1].into());

    assert!(state.absent_paths().any(|path| path == &payload));
    assert!(state.no_payload_paths().any(|path| path == &payload));
    assert!(state.definitely_absent.is_empty());

    state.memory.stage_unknown(
        expressions[2].into(),
        super::super::memory::Target::storage(payload.clone()),
    );

    state.memory.complete_transition(expressions[2].into());

    assert!(!state.absent_paths().any(|path| path == &payload));
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
    use bray_bound_tree::StorageAbsence;

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
        .expect("fixture expression must exist in its bound unit")
        .origin()
        .source_anchor();

    let mut builder = StoragePlanBuilder::new(unit.unit(), BoundUnitKind::CallableBody);

    let root = builder
        .push_identity(StorageIdentity::Temporary(expressions[0]))
        .expect("fixture identity must fit its identity space");

    let access = builder
        .push_access(StorageAccess::new(
            StorageAccessRoot::Storage(root),
            [],
            error_type(),
            source,
            false,
        ))
        .expect("fixture access must fit its identity space");

    let parent = StoragePath::root(root);

    let payload = StoragePath {
        root,
        projections: [StorageProjection::NullableValue].into(),
    };

    let discriminants = BTreeMap::from([(root, BTreeSet::from([parent.clone()]))]);

    let observed = |changed| StorageInvalidation::Observed {
        transition: Default::default(),
        scope: bray_bound_tree::StorageMutationScope::Unbounded,
        affected_pointees: None,
        accesses: BTreeMap::from([(access, vec![changed])]),
        snapshots: BTreeMap::new(),
        referents: BTreeSet::new(),
        stable_descriptors: BTreeSet::new(),
        stable_values: BTreeMap::new(),
        stable_reference_slots: Default::default(), normal_snapshots: Default::default(), stable_pointees: Default::default(),
    };

    let opaque = StorageInvalidation::All {
        stable_descriptors: [parent.clone()].into(),
        stable_values: BTreeMap::new(),
        stable_allocation_addresses: Default::default(),
        stable_reference_slots: Default::default(), normal_snapshots: Default::default(), stable_pointees: Default::default(),
    };

    let opaque_value = |kind| StorageInvalidation::All {
        stable_descriptors: Default::default(),
        stable_values: [(parent.clone(), kind)].into(),
        stable_allocation_addresses: Default::default(),
        stable_reference_slots: Default::default(), normal_snapshots: Default::default(), stable_pointees: Default::default(),
    };

    for (invalidation, inline, retained) in [
        (observed(payload.clone()), true, true),
        (observed(parent.clone()), true, false),
        (opaque.clone(), true, true),
        (opaque, false, false),
        (opaque_value(bray_bound_tree::StorageObservationKind::Discriminant), true, true),
        (opaque_value(bray_bound_tree::StorageObservationKind::Address), true, false),
        (StorageInvalidation::unknown(), true, false),
    ] {
        let mut state = reachable_state();

        std::sync::Arc::make_mut(&mut state.definitely_absent).insert(payload.clone(), StorageAbsence::NoPayload);

        let no_discriminants = BTreeMap::new();

        state.invalidate_observed_absence(
            &invalidation,
            if inline {
                &discriminants
            } else {
                &no_discriminants
            },
        );

        assert_eq!(state.definitely_absent.contains_key(&payload), retained);
    }
}

#[test]
fn abnormal_reports_retain_dependencies_without_requesting_pending_storage() {
    let (identity, expressions) = storage_and_expressions(94);

    let subject = bray_bound_tree::BoundDependencySubject::Storage(identity);
    let boundary = expressions[0];
    let mut abnormal = reachable_state();
    let mut empty = reachable_state();

    empty.retain_reports(Some(boundary), Default::default(), false);
    assert_eq!(empty.pending_report_boundaries().count(), 0);
    assert_eq!(empty.completed_report().as_deref(), Some(&[][..]));

    assert!(empty.transfer_report_boundary(boundary, None, false).is_none());
    assert_eq!(empty.completed_report().as_deref(), Some(&[][..]));

    let requirement = bray_symbols::DependencyRequirement::direct(
        bray_symbols::DependencySubject::root(bray_symbols::DependencySubjectRoot::Parameter(bray_symbols::SymbolOrdinal::new(0))),
        bray_symbols::DependencyRequirementKind::ValueDependencies,
    );

    let dependencies = bray_bound_tree::ReportDependencies {
        subjects: [subject].into(),
        requirements: [requirement.clone()].into(),
        ..Default::default()
    };

    abnormal.retain_reports(Some(boundary), std::sync::Arc::new(dependencies), false);
    assert!(abnormal.retained_report_value_requires(subject));
    assert_eq!(abnormal.pending_report_boundaries().count(), 0);

    let mut normal = reachable_state();

    normal.retain_reports(Some(boundary), Default::default(), true);
    assert!(abnormal.merge(&normal, None));
    assert!(!abnormal.merge(&normal, None));

    assert_eq!(
        abnormal.pending_report_boundaries().collect::<Vec<_>>(),
        [Some(boundary)]
    );

    assert!(abnormal.retained_report_value_requires(subject));

    let mut caught = abnormal.clone();

    let transferred = caught.transfer_report_boundary(boundary, None, true)
        .expect("catch owns the transferred report");

    assert!(transferred.subjects.contains(&subject));
    assert!(transferred.requirements.contains(&requirement));
    assert_eq!(caught.pending_report_boundaries().count(), 0);
    assert!(!caught.retained_report_value_requires(subject));
    assert!(caught.completed_report().is_none());

    assert!(abnormal.transfer_report_boundary(boundary, None, false).is_none());

    assert_eq!(
        abnormal.pending_report_boundaries().collect::<Vec<_>>(),
        [None]
    );

    assert!(abnormal.retained_report_value_requires(subject));
    assert_eq!(abnormal.completed_report().as_deref(), Some(&[requirement][..]));
}

#[test]
fn ownership_join_preserves_moves_when_other_paths_never_create_the_storage() {
    let (identity, _) = storage_and_expressions(98);

    let states = [None, Some(false), Some(true)];

    let make = |status: Option<bool>| {
        let mut state = reachable_state();

        if let Some(moved) = status {
            state.live.insert(identity);
            state.initialized.insert(identity);

            if moved {
                state.move_complete_storage(identity);
            }
        }

        state
    };

    for first in states {
        for second in states {
            let mut joined = make(first);

            joined.merge(&make(second), None);

            let live = first.is_some() || second.is_some();
            let owned = first == Some(false) || second == Some(false);

            assert_eq!(
                joined.live.contains(&identity),
                live,
                "{first:?}, {second:?}"
            );

            assert_eq!(
                joined.fully_moved.contains(&identity),
                live && !owned,
                "{first:?}, {second:?}"
            );

            assert!(
                !joined.merge(&make(second), None),
                "repeating a predecessor must be stable"
            );

            for third in states {
                let mut left = joined.clone();

                left.merge(&make(third), None);

                let mut tail = make(second);

                tail.merge(&make(third), None);

                let mut right = make(first);

                right.merge(&tail, None);

                assert_eq!(
                    (&left.live, &left.fully_moved, &left.initialized),
                    (&right.live, &right.fully_moved, &right.initialized),
                    "{first:?}, {second:?}, {third:?}"
                );
            }
        }
    }
}
