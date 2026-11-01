use super::ExecutionCondition;
use crate::ExecutionPlace;
use bray_symbols::{ConstantProjectionKind, SymbolOrdinal};
use std::collections::BTreeSet;
use std::sync::Arc;

#[test]
fn changed_contents_do_not_reuse_unlisted_components_from_carrier_identity() {
    let identity = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
    let contents = ExecutionCondition::Result;
    let first = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0));
    let marker = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)));

    let value = ExecutionCondition::Constructed(
        Arc::new(identity.clone()),
        [
            (
                bray_bound_tree::StorageProjection::OwnedTarget,
                contents.clone(),
            ),
            (
                bray_bound_tree::StorageProjection::TupleElement(SymbolOrdinal::new(1)),
                marker.clone(),
            ),
        ]
        .into(),
    );

    assert_eq!(value.owner_identity(), &identity);

    assert_eq!(
        value.clone().component(first),
        contents.clone().component(first)
    );

    assert_eq!(
        value
            .clone()
            .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1))),
        marker
    );

    assert_eq!(
        value
            .component(ConstantProjectionKind::NullableValue)
            .component(first),
        contents
            .component(ConstantProjectionKind::NullableValue)
            .component(first)
    );
}

#[test]
fn partial_aggregate_snapshots_keep_unlisted_component_identity() {
    let root = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
    let first = bray_bound_tree::StorageProjection::TupleElement(SymbolOrdinal::new(0));
    let second = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1));

    let value = ExecutionCondition::Constructed(
        Arc::new(root.clone()),
        [(first, ExecutionCondition::Unknown)].into(),
    );

    assert_eq!(value.clone().project(first), ExecutionCondition::Unknown);
    assert_eq!(value.component(second), root.component(second));
}

#[test]
fn qualified_unknown_conditions_cannot_be_assumed_or_proven() {
    for condition in [
        ExecutionCondition::Unknown,
        ExecutionCondition::Trusted(Arc::new(ExecutionCondition::Unknown)),
        ExecutionCondition::Entry {
            condition: Arc::new(ExecutionCondition::Unknown),
            captured: true,
        },
    ] {
        let mut facts = BTreeSet::new();

        condition.clone().assume(true, &mut facts);
        assert!(facts.is_empty());

        // Even a supplied set containing a recovery term cannot turn it into evidence.
        facts.insert((condition.clone(), true));
        assert_eq!(condition.prove_trusted(&facts, &BTreeSet::new()), None);

        assert_eq!(
            condition.prove(&facts, &mut { ExecutionCondition::WORK_LIMIT }),
            None
        );
    }
}

#[test]
fn observation_paths_distinguish_contents_from_containing_identity() {
    for root in [
        ExecutionCondition::Result,
        ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0))),
    ] {
        let left = root
            .clone()
            .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0)));

        let right = root
            .clone()
            .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1)));

        let child = left
            .clone()
            .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0)));

        let mut changed = super::ObservationSet::default();

        changed.insert(left.clone());
        assert!(changed.overlaps(&root));
        assert!(changed.overlaps(&left));
        assert!(changed.overlaps(&child));
        assert!(!changed.overlaps(&right));
        assert!(!changed.contains(&root));
        assert!(changed.contains(&child));
        assert!(!right.clone().borrowed().observes(&left));
        assert!(root.clone().borrowed().observes(&left));
        assert!(left.clone().borrowed().observes(&root));

        assert!(
            !ExecutionCondition::Entry {
                condition: Arc::new(left),
                captured: true
            }
            .observes(&root)
        );
    }
}

#[test]
fn boolean_proofs_keep_decisive_operands_without_equating_unknowns() {
    use bray_bound_tree::BoundOperator;

    for left in [None, Some(false), Some(true)] {
        for right in [None, Some(false), Some(true)] {
            for operator in [BoundOperator::LogicalAnd, BoundOperator::LogicalOr] {
                let condition = ExecutionCondition::operation(
                    operator,
                    vec![
                        left.map_or(ExecutionCondition::Unknown, ExecutionCondition::Boolean),
                        right.map_or(ExecutionCondition::Unknown, ExecutionCondition::Boolean),
                    ],
                );

                let decisive = operator == BoundOperator::LogicalOr;

                let expected = if left == Some(decisive) || right == Some(decisive) {
                    Some(decisive)
                } else if left.is_some() && right.is_some() {
                    Some(!decisive)
                } else {
                    None
                };

                let facts = BTreeSet::new();

                assert_eq!(
                    condition.prove(&facts, &mut { ExecutionCondition::WORK_LIMIT }),
                    expected
                );

                assert_eq!(
                    condition.prove_mixed(&facts, &facts, &mut { ExecutionCondition::WORK_LIMIT }),
                    expected
                );
            }
        }
    }

    let known = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));

    for (operator, truth) in [
        (BoundOperator::LogicalOr, true),
        (BoundOperator::LogicalAnd, false),
    ] {
        let condition = ExecutionCondition::operation(
            operator,
            vec![ExecutionCondition::Unknown, known.clone()],
        );

        let mut facts = BTreeSet::new();

        condition.clone().assume(truth, &mut facts);
        assert!(facts.is_empty());
        facts.insert((condition.clone(), truth));

        assert_eq!(
            condition.prove(&facts, &mut { ExecutionCondition::WORK_LIMIT }),
            None
        );

        assert_eq!(
            condition.prove_mixed(&facts, &facts, &mut { ExecutionCondition::WORK_LIMIT }),
            None
        );
    }
}

#[test]
fn repeated_component_observation_keeps_the_original_snapshot() {
    let original = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
    let first = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0));
    let nested = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1));
    let value = original.clone().component(first).component(nested);

    assert_eq!(original.with_component(&[first, nested], value), original);

    let changed = original.with_component(&[first, nested], ExecutionCondition::Boolean(true));

    assert_ne!(changed, original);

    assert_eq!(
        changed.clone().component(first).component(nested),
        ExecutionCondition::Boolean(true)
    );
}

#[test]
fn anonymous_aggregate_contents_do_not_establish_an_observation_identity() {
    let anonymous = ExecutionCondition::Constructed(
        Arc::new(ExecutionCondition::Unknown),
        [(
            bray_bound_tree::StorageProjection::OwnedTarget,
            ExecutionCondition::Boolean(true),
        )]
        .into(),
    );

    let changed = ExecutionCondition::Constructed(
        Arc::new(anonymous.clone()),
        [(
            bray_bound_tree::StorageProjection::OwnedTarget,
            ExecutionCondition::Boolean(false),
        )]
        .into(),
    );

    assert_eq!(anonymous.owner_identity(), changed.owner_identity());
    assert!(anonymous.owner_identity().observation_path().is_none());
    assert!(changed.owner_identity().observation_path().is_none());
}

#[test]
fn ordinary_compound_evidence_cannot_supply_a_trusted_occurrence() {
    let left = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
    let right = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)));

    for qualified in [false, true] {
        let right = if qualified {
            ExecutionCondition::Trusted(Arc::new(right.clone()))
        } else {
            right.clone()
        };

        let condition = ExecutionCondition::operation(
            bray_bound_tree::BoundOperator::LogicalOr,
            vec![left.clone(), right],
        );

        let ordinary = BTreeSet::from([(condition.clone(), true)]);

        assert_eq!(
            condition.prove_mixed(&BTreeSet::new(), &ordinary, &mut {
                ExecutionCondition::WORK_LIMIT
            }),
            (!qualified).then_some(true)
        );
    }
}

#[test]
fn captured_value_evidence_matches_unwrapped_proof_inputs() {
    let input = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));

    let captured = ExecutionCondition::Entry {
        condition: Arc::new(input.clone()),
        captured: true,
    };

    for value in [true, false] {
        let mut assumptions = BTreeSet::new();

        captured.clone().assume(value, &mut assumptions);

        assert_eq!(assumptions, BTreeSet::from([(input.clone(), value)]));

        assert_eq!(
            captured.prove(&assumptions, &mut { ExecutionCondition::WORK_LIMIT }),
            Some(value)
        );
    }
}

#[test]
fn entry_evidence_preserves_authority_and_live_observation_boundaries() {
    let input = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));

    for (condition, captured) in [
        (input.clone(), false),
        (ExecutionCondition::Trusted(Arc::new(input.clone())), true),
        (ExecutionCondition::Borrowed(Arc::new(input.clone())), true),
    ] {
        let entry = ExecutionCondition::Entry {
            condition: Arc::new(condition.clone()),
            captured,
        };

        let mut assumptions = BTreeSet::new();

        entry.clone().assume(true, &mut assumptions);

        assert_eq!(assumptions, BTreeSet::from([(entry, true)]));

        assert_eq!(
            condition.prove(&assumptions, &mut { ExecutionCondition::WORK_LIMIT }),
            None
        );
    }
}

#[test]
fn boolean_constants_normalize_symbolic_completion_implications() {
    use bray_bound_tree::BoundOperator;

    let subject = ExecutionCondition::Result;

    for (operator, identity, absorbing) in [
        (BoundOperator::LogicalAnd, true, false),
        (BoundOperator::LogicalOr, false, true),
    ] {
        for reversed in [false, true] {
            let mut operands = vec![subject.clone(), ExecutionCondition::Boolean(identity)];

            if reversed {
                operands.reverse();
            }

            assert_eq!(ExecutionCondition::operation(operator, operands), subject);

            let mut operands = vec![subject.clone(), ExecutionCondition::Boolean(absorbing)];

            if reversed {
                operands.reverse();
            }

            assert_eq!(
                ExecutionCondition::operation(operator, operands),
                ExecutionCondition::Boolean(absorbing)
            );
        }
    }
}

#[test]
fn owned_renaming_reaches_compact_input_fields_without_equating_borrows() {
    let root = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
    let first = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0));
    let second = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1));
    let replacement = ExecutionCondition::Result.component(first);
    let specific = ExecutionCondition::Result.component(second);

    let replacements = [
        (root.clone(), replacement.clone()),
        (root.clone().component(first), specific.clone()),
    ]
    .into();

    for (subject, expected) in [
        (
            root.clone().component(second).borrowed(),
            replacement.component(second).borrowed(),
        ),
        (
            root.component(first).component(second).borrowed(),
            specific.component(second).borrowed(),
        ),
    ] {
        assert_eq!(
            subject.with_joined_values(&replacements, &replacements),
            expected
        );

        assert_eq!(subject.with_equalities(&replacements), subject);

        let entry = ExecutionCondition::Entry {
            condition: Arc::new(subject),
            captured: true,
        };

        assert_eq!(
            entry.with_joined_values(&replacements, &replacements),
            entry
        );
    }
}

#[test]
fn predecessor_snapshots_do_not_expand_their_surviving_owner_recursively() {
    let owner = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
    let first = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0));
    let second = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1));
    let unchanged = owner.clone().component(second);

    let snapshot = ExecutionCondition::Constructed(
        Arc::new(owner.clone()),
        [
            (
                bray_bound_tree::StorageProjection::TupleElement(SymbolOrdinal::new(0)),
                ExecutionCondition::Boolean(true),
            ),
            (
                bray_bound_tree::StorageProjection::TupleElement(SymbolOrdinal::new(1)),
                unchanged.clone(),
            ),
        ]
        .into(),
    );

    let snapshots = [(owner.clone(), snapshot.clone())].into();

    assert_eq!(owner.clone().with_snapshots(&snapshots), snapshot);

    assert_eq!(
        owner.component(first).with_snapshots(&snapshots),
        ExecutionCondition::Boolean(true)
    );

    assert_eq!(unchanged.with_snapshots(&snapshots), unchanged);
}
