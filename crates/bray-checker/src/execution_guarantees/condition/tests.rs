use super::ExecutionCondition;
use crate::ExecutionPlace;
use bray_symbols::{ConstantProjectionKind, SymbolOrdinal};
use std::collections::BTreeSet;
use std::sync::Arc;

#[test]
fn const_calls_observe_current_values_without_loan_identity() {
    let definition = bray_symbols::FunctionSymbolId::from_symbol_id(bray_symbols::SymbolId::new(900));
    let callable = crate::test_support::callable_instance(definition.into());
    let place = ExecutionPlace::argument(SymbolOrdinal::new(0));
    let input = ExecutionCondition::Input(place.clone());
    let call = |value| ExecutionCondition::call(callable, None, [], vec![value]);
    let loan = input.clone().borrowed();
    let contents = ExecutionCondition::Joined(bray_bound_tree::BoundUnitId::new(0), 1, place);
    let current = loan.with_referent(contents.clone());

    assert_eq!(call(loan.clone()), call(input));
    assert_eq!(call(current.clone()), call(contents));
    assert_ne!(call(current.clone()), call(loan.clone()));
    assert_eq!(current.owner_identity(), loan.owner_identity());
    assert_eq!(current.owned_components().count(), 0);
}

#[test]
fn borrowed_scalar_contents_retain_storage_authority_and_const_value_semantics() {
    let definition =
        bray_symbols::FunctionSymbolId::from_symbol_id(bray_symbols::SymbolId::new(900));

    let callable = crate::test_support::callable_instance(definition.into());

    let symbol =
        crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>("ValidWrite");

    let condition =
        |value| ExecutionCondition::predicate(symbol.into(), callable.substitution(), vec![value]);

    let call = |value| ExecutionCondition::call(callable, None, [], vec![value]);

    let stored = |ordinal, value| {
        ExecutionCondition::scalar_observation(
            ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(ordinal))),
            value,
        ).borrowed()
    };

    for value in [
        ExecutionCondition::Boolean(true),
        ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(2))),
    ] {
        let loan = stored(0, value.clone());
        let unchanged = loan.with_referent(value.clone());
        let changed = loan.with_referent(ExecutionCondition::Boolean(false));
        let forwarded = loan.with_referent(changed.clone().into_referent());

        assert_eq!(forwarded, changed);

        let unrelated = stored(1, value);

        assert_eq!(condition(unchanged), condition(loan.clone()));

        assert_eq!(
            condition(changed.clone()),
            condition(stored(0, ExecutionCondition::Boolean(false)))
        );

        assert_ne!(condition(loan.clone()), condition(unrelated.clone()));
        assert_eq!(call(loan.clone()), call(unrelated));
        assert_ne!(call(loan.clone()), call(changed.clone()));
        assert_eq!(changed.owner_identity(), loan.owner_identity());
        assert_eq!(changed.owned_components().count(), 0);
    }
}

#[test]
fn aggregate_contents_refresh_does_not_introduce_a_scalar_storage_context() {
    let owner = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
    let current = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)));
    let changed = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(2)));
    let aggregate = ExecutionCondition::owned_value(owner, current);

    assert_eq!(aggregate.storage_contents(), None);

    let inline = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(3)));
    let backing = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(4)));

    let allocated = ExecutionCondition::constructed(inline.clone(),
        [(bray_bound_tree::StorageProjection::OwnedTarget, backing.clone())].into());

    let field = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0));

    assert_eq!(allocated.clone().project(bray_bound_tree::StorageProjection::OwnedTarget), backing);
    assert_eq!(allocated.component(field), inline.component(field));

    let definition = bray_symbols::FunctionSymbolId::from_symbol_id(bray_symbols::SymbolId::new(900));
    let callable = crate::test_support::callable_instance(definition.into());
    let predicate = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>("ValidWrite");
    let observe = |value| ExecutionCondition::predicate(predicate.into(), callable.substitution(), vec![value]);

    let alias = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(5)))
        .borrowed().with_referent(aggregate.clone());

    assert_eq!(observe(alias), observe(aggregate.clone().borrowed()));

    let loan = aggregate.borrowed();
    let refreshed = loan.with_referent(changed.clone());

    assert_eq!(refreshed.owner_identity(), loan.owner_identity());
    assert_eq!(refreshed.storage_contents(), None);
    assert_ne!(observe(refreshed), observe(changed.borrowed()));
}

#[test]
fn nullable_extraction_keeps_the_payload_owner_across_snapshots() {
    let identity = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));

    let payload = identity
        .clone()
        .component(ConstantProjectionKind::NullableValue);

    let snapshot = ExecutionCondition::constructed(identity.clone(),
        [(
            bray_bound_tree::StorageProjection::NullableValue,
            payload.clone(),
        )]
        .into(),
    );

    let extraction = ExecutionCondition::Projection(
        ConstantProjectionKind::NullableValue,
        Arc::new(snapshot.clone()),
    );

    assert_eq!(snapshot.owner_identity(), &identity);
    assert_eq!(extraction.owner_identity(), payload.owner_identity());
}

#[test]
fn changed_contents_do_not_reuse_unlisted_components_from_carrier_identity() {
    let identity = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
    let contents = ExecutionCondition::Result;
    let first = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0));
    let marker = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)));

    let value = ExecutionCondition::owned_value(
        identity.clone(),
        ExecutionCondition::constructed(contents.clone(),
            [(
                bray_bound_tree::StorageProjection::TupleElement(SymbolOrdinal::new(1)),
                marker.clone(),
            )].into(),
        ),
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

    let value = ExecutionCondition::constructed(root.clone(),
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
fn retained_borrow_paths_do_not_retain_the_referents_ownership() {
    let owner = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));

    let field = owner
        .clone()
        .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0)));

    let mut retained = super::ObservationSet::default();

    retained.insert(owner.clone().borrowed());
    assert!(retained.contains(&owner.clone().borrowed()));
    assert!(retained.contains(&field.clone().borrowed()));
    assert!(!retained.contains(&owner));
    assert!(!retained.contains(&field));
    assert!(!retained.overlaps(&owner));
    assert!(!retained.overlaps(&field));
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
    let nested = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1));

    for first in [
        ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0)),
        ConstantProjectionKind::NullableValue,
    ] {
        let path = [first, nested];
        let value = original.clone().component(first).component(nested);

        assert_eq!(original.with_component(&path, value, None), original);

        let changed = original.with_component(&path, ExecutionCondition::Boolean(true), None);

        assert_ne!(changed, original);

        assert_eq!(
            changed.clone().component(first).component(nested),
            ExecutionCondition::Boolean(true)
        );

        let updated = changed.with_component(&path, ExecutionCondition::Boolean(false), None);
        let expected = original.with_component(&path, ExecutionCondition::Boolean(false), None);

        assert_eq!(updated, expected);

        if first == ConstantProjectionKind::NullableValue {
            let payload = changed.component(first);

            let updated =
                payload.with_component(&[nested], ExecutionCondition::Boolean(false), None);

            assert_eq!(updated.present(), expected);
        }
    }
}

#[test]
fn anonymous_aggregate_contents_do_not_establish_an_observation_identity() {
    let anonymous = ExecutionCondition::constructed(ExecutionCondition::Unknown,
        [(
            bray_bound_tree::StorageProjection::OwnedTarget,
            ExecutionCondition::Boolean(true),
        )]
        .into(),
    );

    let changed = ExecutionCondition::constructed(anonymous.clone(),
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

    let snapshot = ExecutionCondition::constructed(owner.clone(),
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

#[test]
fn carrier_implication_preserves_trusted_provenance_through_negated_guards() {
    use bray_bound_tree::BoundOperator;

    let tag = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
    let present = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)));
    let ordinary = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(2)));
    let trusted = ExecutionCondition::Trusted(Arc::new(ordinary.clone()));
    let not = |value| ExecutionCondition::operation(BoundOperator::LogicalNot, vec![value]);

    let or =
        |left, right| ExecutionCondition::operation(BoundOperator::LogicalOr, vec![left, right]);

    let active = ExecutionCondition::operation(
        BoundOperator::LogicalAnd,
        vec![tag.clone(), present.clone()],
    );

    let published = or(not(active), trusted.clone());
    let declared = or(not(tag), or(not(present), trusted.clone()));

    assert!(published.implies(&declared));
    assert!(declared.implies(&published));
    assert!(trusted.implies(&ordinary));
    assert!(!ordinary.implies(&trusted));

    let unresolved = or(ordinary, ExecutionCondition::Unknown);

    assert!(!unresolved.implies(&unresolved));
}
