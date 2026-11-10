use super::ExecutionCondition;
use crate::ExecutionPlace;
use bray_symbols::{ConstantProjectionKind, SymbolOrdinal};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[test]
fn retained_disjunctions_reject_only_incompatible_branches() {
    use bray_bound_tree::BoundOperator;

    let input = |ordinal| ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(ordinal)));
    let variant = |ordinal| bray_symbols::UnionVariantSymbolId::from_symbol_id(bray_symbols::SymbolId::new(ordinal));
    let first = input(0).matches_variant(variant(910));
    let second = input(0).matches_variant(variant(911));
    let third = input(0).matches_variant(variant(912));
    let alternative = ExecutionCondition::operation(BoundOperator::LogicalOr, vec![first.clone(), second.clone()]);
    let assumptions = BTreeSet::from([(alternative, true)]);

    assert!(first.can_assume(true, &assumptions));
    assert!(second.can_assume(true, &assumptions));
    assert!(!third.can_assume(true, &assumptions));
    assert!(third.can_assume(false, &assumptions));
    assert!(input(1).matches_variant(variant(912)).can_assume(true, &assumptions));
    assert!(ExecutionCondition::Unknown.can_assume(true, &assumptions));
    assert!(ExecutionCondition::Unknown.can_assume(false, &assumptions));

    let unavailable = ExecutionCondition::operation(BoundOperator::LogicalAnd, vec![
        ExecutionCondition::operation(BoundOperator::LogicalNot, vec![first.clone()]),
        ExecutionCondition::operation(BoundOperator::LogicalNot, vec![second.clone()]),
    ]);
    let assumptions = BTreeSet::from([(unavailable, false)]);

    assert!(first.can_assume(true, &assumptions));
    assert!(!third.can_assume(true, &assumptions));
}

#[test]
fn projecting_a_stored_reference_preserves_its_independent_referent() {
    let input = |ordinal| ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(ordinal)));
    let field = bray_symbols::StructFieldSymbolId::from_symbol_id(bray_symbols::SymbolId::new(900));
    let physical = bray_bound_tree::StorageProjection::ProductField(field);
    let logical = ConstantProjectionKind::ProductField(field);

    for selected in [input(1).borrowed(), ExecutionCondition::reference_value(input(2).borrowed(), input(1))] {
        let contents = ExecutionCondition::constructed(input(0), Arc::from([(physical, selected.clone())]));
        let container = ExecutionCondition::reference_value(input(0).borrowed(), contents);

        assert_eq!(container.clone().project(physical), selected);
        assert_eq!(container.component(logical), selected);
        assert_eq!(selected.ownership_identity().map(|(kind, _)| kind), Some(crate::execution_guarantees::ExecutionObservationKind::Reference));
    }

    let owned = ExecutionCondition::constructed(input(0), Arc::from([(physical, input(1))]));
    let view = ExecutionCondition::reference_value(input(0).borrowed(), owned);

    assert_eq!(view.component(logical).ownership_identity().map(|(kind, _)| kind), Some(crate::execution_guarantees::ExecutionObservationKind::Reference));

    let scalar = ExecutionCondition::scalar_observation(input(3), input(1));
    let contents = ExecutionCondition::constructed(input(0), Arc::from([(physical, scalar)]));
    let view = ExecutionCondition::reference_value(input(0).borrowed(), contents);
    let projected = view.component(logical);

    assert_eq!(projected.observation_kind(), Some(crate::execution_guarantees::ExecutionObservationKind::Scalar));
    assert_eq!(projected.copied_value(), &input(1));
    assert_ne!(projected.borrowed().owner_identity(), input(1).borrowed().owner_identity());
}

#[test]
fn copy_proof_correlations_preserve_borrowed_storage_identity() {
    let definition = bray_symbols::FunctionSymbolId::from_symbol_id(bray_symbols::SymbolId::new(900));
    let substitution = crate::test_support::callable_instance(definition.into()).substitution();
    let predicate = bray_symbols::PredicateSymbolId::from_symbol_id(bray_symbols::SymbolId::new(901));
    let source = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
    let copied = ExecutionCondition::scalar_observation(
        ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1))), source.clone());
    let place = ExecutionPlace::argument(SymbolOrdinal::new(0));
    let evidence = crate::ExecutionCallEvidence {
        arguments: BTreeMap::from([(place.clone(), copied.clone())]),
        copy_sources: BTreeMap::from([(place, source.clone())]),
        ..Default::default()
    };
    let condition = |value| ExecutionCondition::predicate(predicate.into(), substitution, vec![value]);
    let facts = BTreeSet::from([
        (condition(source.clone()), true),
        (condition(source.clone().borrowed()), true),
    ]);
    let observed = evidence.proof_facts(&facts).collect::<BTreeSet<_>>();

    assert!(observed.contains(&(condition(copied.clone()), true)));
    assert!(observed.contains(&(condition(source.borrowed()), true)));
    assert!(!observed.contains(&(condition(copied.borrowed()), true)));
}

#[test]
fn retained_scalar_data_does_not_preserve_its_original_storage_loan_or_owner() {
    let input = |ordinal| ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(ordinal)));
    let data = input(0);
    let copied = ExecutionCondition::scalar_observation(input(1), data.clone());
    let stable = crate::execution_guarantees::ObservationSet::from_iter([
        copied.clone(), copied.clone().borrowed(),
    ]);
    let predicate = bray_symbols::PredicateSymbolId::from_symbol_id(bray_symbols::SymbolId::new(900));
    let substitution = crate::test_support::empty_substitution(predicate.into());
    let condition = |value| ExecutionCondition::predicate(predicate.into(), substitution, vec![value]);

    assert!(stable.preserves(&data));
    assert!(!stable.contains(&data));
    assert!(condition(data.clone()).observes_only(|value| stable.preserves(value)));
    assert!(!condition(data.clone().borrowed()).observes_only(|value| stable.preserves(value)));
    assert!(condition(copied.clone().borrowed()).observes_only(|value| stable.preserves(value)));
    assert!(!crate::execution_guarantees::ObservationSet::retains_owner(
        &BTreeSet::from([copied]), &data));
}

#[test]
fn predicate_substitution_distinguishes_value_reads_from_explicit_loans() {
    let input = |ordinal| ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(ordinal)));
    let field = bray_symbols::StructFieldSymbolId::from_symbol_id(bray_symbols::SymbolId::new(900));
    let projection = ConstantProjectionKind::ProductField(field);
    let definition = bray_symbols::FunctionSymbolId::from_symbol_id(bray_symbols::SymbolId::new(901));
    let callable = crate::test_support::callable_instance(definition.into());
    let predicate = bray_symbols::PredicateSymbolId::from_symbol_id(bray_symbols::SymbolId::new(902));
    let field_value = input(0).component(projection);
    let condition = ExecutionCondition::predicate(predicate.into(), callable.substitution(),
        vec![field_value.clone(), field_value.borrowed()]);
    let arguments = BTreeMap::from([(ExecutionPlace::argument(SymbolOrdinal::new(0)), input(1).borrowed())]);
    let instantiated = condition.substitute(&|place| place.value_in(&arguments).unwrap(),
        &ExecutionCondition::Unknown, &mut { ExecutionCondition::WORK_LIMIT });
    let expected = input(1).component(projection);

    assert_eq!(instantiated, ExecutionCondition::predicate(predicate.into(), callable.substitution(),
        vec![expected.clone(), expected.borrowed()]));

    let condition = ExecutionCondition::predicate(predicate.into(), callable.substitution(),
        vec![input(0), input(0).borrowed()]);
    let copied = ExecutionCondition::scalar_observation(input(2), input(1).borrowed());
    let instantiated = condition.substitute(&|_| copied.clone(),
        &ExecutionCondition::Unknown, &mut { ExecutionCondition::WORK_LIMIT });

    assert_eq!(instantiated, ExecutionCondition::predicate(predicate.into(), callable.substitution(),
        vec![input(1), copied.borrowed()]));
}

#[test]
fn arithmetic_and_const_calls_read_the_same_current_scalar_value() {
    let definition = bray_symbols::FunctionSymbolId::from_symbol_id(bray_symbols::SymbolId::new(900));
    let callable = crate::test_support::callable_instance(definition.into());
    let value = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
    let identity = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)));
    let copied = ExecutionCondition::scalar_observation(identity.clone(), value.clone());
    let loan = ExecutionCondition::reference_value(identity.borrowed(), copied.clone());

    for observed in [copied, loan, value.clone().borrowed()] {
        for operator in [bray_bound_tree::BoundOperator::Add, bray_bound_tree::BoundOperator::Subtract,
            bray_bound_tree::BoundOperator::LessEqual] {
            assert_eq!(ExecutionCondition::operation(operator, vec![observed.clone(), value.clone()]),
                ExecutionCondition::operation(operator, vec![value.clone(), value.clone()]));
        }

        assert_eq!(ExecutionCondition::call(callable, None, [], vec![observed]),
            ExecutionCondition::call(callable, None, [], vec![value.clone()]));
    }
}

#[test]
fn spatial_extents_observe_copied_values_without_owning_them() {
    let predicate = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>("ValidWrite");
    let definition = bray_symbols::FunctionSymbolId::from_symbol_id(bray_symbols::SymbolId::new(900));
    let callable = crate::test_support::callable_instance(definition.into());
    let pointer = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
    let count = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)));
    let copied = ExecutionCondition::scalar_observation(
        ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(2))), count.clone());
    let condition = |extent| ExecutionCondition::predicate(predicate.into(), callable.substitution(),
        vec![pointer.clone(), extent]);
    let spatial = BTreeMap::from([(predicate.into(), &[0][..])]);

    assert_eq!(condition(copied.clone()).pointer_observations(&spatial),
        condition(count).pointer_observations(&spatial));
    assert!(copied.ownership_identity().is_none());
}

#[test]
fn spatial_predicates_follow_copied_descriptors_without_copying_owner_authority() {
    let predicate = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>("ValidWrite");
    let definition = bray_symbols::FunctionSymbolId::from_symbol_id(bray_symbols::SymbolId::new(900));
    let callable = crate::test_support::callable_instance(definition.into());
    let pointer = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
    let copy_identity = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)));
    let copied = ExecutionCondition::scalar_observation(copy_identity, pointer.clone());
    let count = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(2)));
    let condition = |value| ExecutionCondition::predicate(predicate.into(), callable.substitution(),
        vec![value, count.clone()]);
    let spatial = BTreeMap::from([(predicate.into(), &[0][..])]);

    for observed in [copied.clone(), pointer.clone().borrowed()] {
        assert_eq!(condition(observed).pointer_observations(&spatial),
            condition(pointer.clone()).pointer_observations(&spatial));
    }
    assert_ne!(condition(copied.clone().borrowed()).pointer_observations(&spatial),
        condition(pointer.clone()).pointer_observations(&spatial));
    assert_ne!(copied.clone().borrowed().owner_identity(), pointer.borrowed().owner_identity());
    assert!(copied.ownership_identity().is_none());
}

#[test]
fn disjoint_ranges_observe_both_descriptors_and_borrowed_extents() {
    let predicate = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>("NonOverlapping");
    let definition = bray_symbols::FunctionSymbolId::from_symbol_id(bray_symbols::SymbolId::new(900));
    let callable = crate::test_support::callable_instance(definition.into());
    let values = (0..4).map(|ordinal| ExecutionCondition::Input(
        ExecutionPlace::argument(SymbolOrdinal::new(ordinal)))).collect::<Vec<_>>();
    let observed = values.iter().enumerate().map(|(index, value)| {
        ExecutionCondition::scalar_observation(
            ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(4 + index as u32))),
            value.clone(),
        ).borrowed()
    }).collect::<Vec<_>>();
    let condition = |arguments| ExecutionCondition::predicate(predicate.into(), callable.substitution(), arguments);
    let spatial = BTreeMap::from([(predicate.into(), &[0, 2][..])]);

    // Borrowing a copied scalar observes its separate storage identity.
    assert_ne!(condition(observed).pointer_observations(&spatial),
        condition(values.clone()).pointer_observations(&spatial));
    assert_eq!(condition(values.iter().cloned().map(ExecutionCondition::borrowed).collect()).pointer_observations(&spatial),
        condition(values.clone()).pointer_observations(&spatial));
    let mut materialized = values.clone();
    materialized[1] = ExecutionCondition::owned_value(values[3].clone(), values[1].clone());
    assert_eq!(condition(materialized).pointer_observations(&spatial),
        condition(values.clone()).pointer_observations(&spatial));
    let mut changed = values.clone();
    changed.swap(0, 2);
    assert_ne!(condition(changed).pointer_observations(&spatial),
        condition(values).pointer_observations(&spatial));
}

#[test]
fn contents_replacement_preserves_descriptor_role_without_old_data() {
    let owner = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
    let previous = ExecutionCondition::Boolean(true);
    let next = ExecutionCondition::Boolean(false);

    for source in [
        ExecutionCondition::owned_value(owner.clone(), previous.clone()),
        owner.clone().borrowed().with_referent(previous.clone()),
        ExecutionCondition::scalar_observation(owner, previous),
    ] {
        let changed = source.with_current_contents(next.clone());

        assert_eq!(source.ownership_identity(), changed.ownership_identity());
        assert_eq!(changed.computed_value(), &next);
    }
}

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
        ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0))),
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
fn republishing_a_nullable_payload_keeps_one_parent_snapshot() {
    let identity = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
    let payload = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)));
    let field = bray_bound_tree::StorageProjection::NullableValue;
    let snapshot = ExecutionCondition::constructed(identity, [(field, payload.clone())].into());

    let extraction = snapshot.clone().project(field);

    assert_eq!(extraction.owner_identity(), payload.owner_identity());
    assert_eq!(extraction.clone().present(), snapshot);
    assert_eq!(snapshot.with_storage_component(&[field], extraction), snapshot);
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
