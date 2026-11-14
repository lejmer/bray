use super::ExecutionCondition;

use crate::ExecutionPlace;
use bray_symbols::{ConstantProjectionKind, SymbolOrdinal};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

fn guard_facts(values: &BTreeMap<ExecutionCondition, ExecutionCondition>) -> super::substitution::GuardSubstitution {
    super::substitution::GuardSubstitution::new(&Default::default(), values.iter().map(|(before, after)| (before, after.clone())))
}

#[test]
fn nonidentity_rewrites_keep_unchanged_structure_and_selected_subtrees_shared() {
    let input = |ordinal| ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(ordinal)));
    let claim = ExecutionCondition::Trusted(Arc::new(input(1)));

    let choice = ExecutionCondition::operation(bray_bound_tree::BoundOperator::LogicalOr,
        vec![input(0), claim.clone()]);

    let unrelated = BTreeMap::from([(input(99), ExecutionCondition::Boolean(false))]);

    let ExecutionCondition::Operation(_, operands) = &choice else { unreachable!(); };

    for unchanged in [
        choice.with_snapshots(&unrelated, &Default::default()),
        choice.with_copy_origins(&unrelated),
        guard_facts(&unrelated).apply(&choice),
        choice.with_equalities(&unrelated),
        choice.with_joined_values(&unrelated, &unrelated),
        choice.pointer_observations(&BTreeMap::new()),
        choice.capture_entry(&|place| ExecutionCondition::Input(place.clone()),
            &BTreeSet::new(), &BTreeSet::new(), &mut { ExecutionCondition::WORK_LIMIT }),
    ] {
        assert_eq!(unchanged, choice);

        let ExecutionCondition::Operation(_, current) = unchanged else { unreachable!(); };

        assert!(std::ptr::eq(operands.as_ref(), current.as_ref()), "unrelated rewriting must retain the existing operand forest");
    }

    let selected = BTreeMap::from([(input(0), ExecutionCondition::Boolean(false))]);

    let ExecutionCondition::Trusted(original) = &claim else { unreachable!(); };

    for rewritten in [
        choice.with_snapshots(&selected, &Default::default()),
        choice.with_copy_origins(&selected),
        guard_facts(&selected).apply(&choice),
        choice.with_equalities(&selected),
        choice.with_joined_values(&selected, &selected),
    ] {
        assert_eq!(rewritten, claim);

        let ExecutionCondition::Trusted(current) = rewritten else { unreachable!(); };

        assert!(Arc::ptr_eq(original, &current), "normalization must keep the selected unchanged claim shared");
    }
}

#[test]
fn observation_walks_visit_shared_nodes_without_expanding_the_tree() {
    use bray_bound_tree::BoundOperator::LogicalAnd;
    use std::cell::Cell;

    let mut condition = ExecutionCondition::Boolean(false);

    for _ in 0..12 {
        condition = ExecutionCondition::Operation(LogicalAnd, vec![condition.clone(), condition].into());
    }

    assert!(condition.observations().count() <= 25);

    let visits = Cell::new(0);

    assert!(condition.observes_only(|_| { visits.set(visits.get() + 1); false }));
    assert!(visits.get() <= 25, "stable-value queries scale with the shared graph");
    visits.set(0);
    assert!(!condition.observes_live_where(|_| true, |_| { visits.set(visits.get() + 1); false }));
    assert!(visits.get() <= 25, "live-value queries scale with the shared graph");

    let input = |ordinal| ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(ordinal)));

    let owned = || {
        let mut value = input(0);

        for _ in 0..18 {
            value = ExecutionCondition::Constructed(Arc::new(input(0)), Arc::from([
                (bray_bound_tree::StorageProjection::TupleElement(SymbolOrdinal::new(0)), value.clone()),
                (bray_bound_tree::StorageProjection::TupleElement(SymbolOrdinal::new(1)), value),
            ]));
        }

        value
    };

    let snapshots = [1, 2].into_iter().map(|ordinal| ExecutionCondition::Observation {
        owner: Arc::new(input(ordinal)), contents: Arc::new(owned()), kind: super::ExecutionObservationKind::Owned,
    }).collect();

    assert!(!super::ObservationSet::retains_owner(&snapshots, &input(99)),
        "separately allocated equal DAGs must not expand into structural comparison trees");

}

#[test]
fn a_shared_snapshot_reached_again_through_a_loan_still_observes_live_storage() {
    let referent = Arc::new(ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0))));

    let owned = ExecutionCondition::Observation {
        owner: Arc::new(ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)))),
        contents: Arc::clone(&referent),
        kind: super::ExecutionObservationKind::Owned,
    };

    // The owned contents are inspected first. The same node is live through the other edge.
    let condition = ExecutionCondition::Operation(bray_bound_tree::BoundOperator::LogicalAnd,
        vec![ExecutionCondition::Borrowed(Arc::clone(&referent)), owned].into());

    assert!(condition.observes_live_where(|value| value == referent.as_ref(), |_| false));
}

#[test]
fn identity_substitutions_preserve_the_existing_shared_value_and_qualifiers() {
    use bray_bound_tree::BoundOperator::{LogicalAnd, LogicalOr};

    let mut value = ExecutionCondition::Trusted(Arc::new(ExecutionCondition::Input(
        ExecutionPlace::argument(SymbolOrdinal::new(0)),
    )));

    // Shared repeated subexpressions are one existing value, regardless of expanded tree size.
    for index in 0..9 {
        value = ExecutionCondition::Operation(
            if index % 2 == 0 { LogicalAnd } else { LogicalOr },
            vec![value.clone(), value].into(),
        );
    }

    let ExecutionCondition::Operation(_, original) = &value else { unreachable!(); };

    let unrelated = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(99)));

    for replacements in [BTreeMap::new(), BTreeMap::from([(unrelated.clone(), unrelated)])] {
        for unchanged in [
            value.with_snapshots(&replacements, &Default::default()),
            value.with_copy_origins(&replacements),
            guard_facts(&replacements).apply(&value),
            value.with_equalities(&replacements),
            value.with_joined_values(&replacements, &replacements),
        ] {
            let ExecutionCondition::Operation(_, operands) = unchanged else {
                panic!("an identity substitution cannot erase an existing qualified value");
            };

            assert!(std::ptr::eq(original.as_ref(), operands.as_ref()));
        }
    }
}

#[test]
fn observed_storage_bindings_read_the_same_current_value_as_their_producer() {
    use bray_bound_tree::{BoundUnitId, BoundUnitKind, StorageAccess, StorageAccessRoot, StorageBinding, StorageBindingTarget, StorageIdentity, StoragePlanBuilder};
    use bray_symbols::{LocalScopeBoundary, LocalSymbolRegionId, LocalSymbolRegionKey, LocalSymbolRegionRole, LocalSymbolSnapshotBuilder, SymbolName};

    let (unit, expressions) = crate::test_support::expression_unit(BoundUnitId::new(0), |tree, origin| {
        vec![crate::test_support::push_expression(tree, crate::test_support::integer_literal_expression(origin, None))]
    });

    let syntax = unit.key().source().syntax();
    let key = LocalSymbolRegionKey::try_new(unit.key().declared_owner().clone(), LocalSymbolRegionRole::CallableBody, [syntax], None).expect("local region key");
    let mut symbols = LocalSymbolSnapshotBuilder::new(LocalSymbolRegionId::new(unit.unit().raw()), key);
    let scope = symbols.push_scope(None, LocalScopeBoundary::Root, syntax, syntax.full_range().start()).expect("root scope");
    let binding = symbols.push_binding(scope, SymbolName::try_new("entered").expect("binding name"), [syntax], None, false).expect("observed binding");
    let mut builder = StoragePlanBuilder::new(unit.unit(), BoundUnitKind::CallableBody);
    let identity = builder.push_identity(StorageIdentity::Temporary(expressions[0])).expect("produced storage");
    let access = builder.push_access(StorageAccess::new(StorageAccessRoot::Storage(identity), [], crate::test_support::error_type(), unit.key().source(), false)).expect("whole produced value");

    builder.bind(StorageBindingTarget::Local(binding), StorageBinding::Access(access)).expect("observed alias");

    let storage = builder.finish();
    let produced = ExecutionPlace::storage_identity(&storage, identity).expect("producer's current place");
    let observed = ExecutionPlace::binding(&storage, StorageBindingTarget::Local(binding)).expect("binding's current place");
    let field = bray_bound_tree::StorageProjection::TupleElement(SymbolOrdinal::new(0));
    let value = ExecutionCondition::constructed(ExecutionCondition::Expression(expressions[0]), [(field, ExecutionCondition::Boolean(true))].into());
    let current = BTreeMap::from([(produced, value)]);

    assert_eq!(observed.value_in(&current).map(|value| value.project(field)), Some(ExecutionCondition::Boolean(true)), "an observed name reads its producer's actual current tree");
}

#[test]
fn snapshot_substitution_keeps_one_current_component_overlay() {
    let input = |id| ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(id)));
    let field = |id| bray_bound_tree::StorageProjection::TupleElement(SymbolOrdinal::new(id));
    let source = ExecutionCondition::constructed(input(0), [(field(0), ExecutionCondition::Boolean(true)), (field(1), input(1))].into());
    let joined = ExecutionCondition::constructed(input(0), [(field(0), ExecutionCondition::Boolean(true)), (field(1), input(2))].into());
    let snapshots = BTreeMap::from([(input(0), source.clone()), (input(2), input(1))]);

    assert_eq!(joined.with_snapshots(&snapshots, &Default::default()), source,
        "a predecessor snapshot replaces the current overlay without retaining nested copies");

    assert_eq!(joined.borrowed().with_snapshots(&snapshots, &Default::default()), source.clone().borrowed());

    let changed = ExecutionCondition::constructed(source.clone(), [(field(1), ExecutionCondition::Boolean(false))].into());

    let expected = ExecutionCondition::constructed(input(0), [(field(0), ExecutionCondition::Boolean(true)),
        (field(1), ExecutionCondition::Boolean(false))].into());

    assert_eq!(changed, expected, "the outer overlay replaces overwritten contents and keeps untouched siblings");

    assert_eq!(ExecutionCondition::constructed(source.borrowed(), [(field(1), ExecutionCondition::Boolean(false))].into()),
        expected.borrowed(), "overlay composition preserves reference capability without acquiring ownership");
}

#[test]
fn predicate_reference_arguments_keep_referents_owners_and_changed_contents_distinct() {
    let input = |ordinal| ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(ordinal)));
    let predicate = bray_symbols::PredicateSymbolId::from_symbol_id(bray_symbols::SymbolId::new(950));
    let substitution = crate::test_support::empty_substitution(predicate.into());
    let condition = |value| ExecutionCondition::predicate(predicate.into(), substitution, vec![value]);
    let contents = ExecutionCondition::owned_value(input(0), input(1));
    let loan = |ordinal, value| ExecutionCondition::reference_value(input(ordinal).borrowed(), value);

    assert_eq!(condition(loan(2, contents.clone())), condition(contents.clone().borrowed()));
    assert_eq!(condition(loan(2, contents.clone())), condition(loan(3, contents.clone())));
    assert_ne!(condition(loan(2, contents.clone())), condition(loan(2, ExecutionCondition::owned_value(input(4), input(1)))));
    assert_ne!(condition(loan(2, contents)), condition(loan(2, ExecutionCondition::owned_value(input(0), input(5)))));

    let copied = |ordinal| ExecutionCondition::scalar_observation(input(ordinal), ExecutionCondition::Boolean(true));

    assert_ne!(condition(loan(2, copied(6))), condition(loan(2, copied(7))));

    let callable = crate::test_support::callable_instance(
        bray_symbols::FunctionSymbolId::from_symbol_id(bray_symbols::SymbolId::new(960)).into());

    let reference_type = crate::test_support::semantic_values().intern_type(bray_symbols::TypeData::Borrow {
        kind: bray_symbols::BorrowKind::Shared, target: crate::test_support::error_type(),
    }).expect("checked reference type");

    let computed = |reference_type| ExecutionCondition::call(callable, None, [], vec![input(0)], reference_type);

    assert_eq!(condition(loan(2, computed(Some(reference_type)))), condition(loan(3, computed(Some(reference_type)))));
    assert_ne!(condition(loan(2, computed(None))), condition(loan(3, computed(None))));
    assert_eq!(computed(Some(reference_type)).ownership_identity(), None);
    assert_eq!(computed(Some(reference_type)).borrowed().ownership_identity(), None);

    let symbolic = condition(loan(2, computed(Some(reference_type))));

    assert_eq!(symbolic.substitute(&|place| ExecutionCondition::Input(place.clone()),
        &ExecutionCondition::Unknown, &mut { ExecutionCondition::WORK_LIMIT }), symbolic);

    assert!(condition(loan(2, ExecutionCondition::Unknown)).is_unknown());
    assert!(condition(loan(3, ExecutionCondition::Unknown)).is_unknown());

    let ExecutionCondition::Predicate(_, _, arguments) = condition(loan(2, input(0))) else { unreachable!(); };

    assert_eq!(arguments[0].ownership_identity().map(|(kind, _)| kind), Some(super::ExecutionObservationKind::Reference));
}

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

    let qualified = ExecutionCondition::Trusted(Arc::new(input(1)));
    let untrusted_alternative = ExecutionCondition::operation(BoundOperator::LogicalOr, vec![input(0), qualified]);
    let assumptions = BTreeSet::from([(untrusted_alternative, true), (input(0), false)]);

    assert!(input(1).can_assume(true, &assumptions));

    assert!(input(1).can_assume(false, &assumptions),
        "ordinary qualified clauses cannot establish their predicate's truth");
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

    let mut stable = crate::execution_guarantees::ObservationSet::from_iter([
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

    let other = input(2);

    assert!(!stable.preserves(&other));
    stable.preserve_scalar_data(&ExecutionCondition::scalar_observation(input(3), other.clone()));
    assert!(stable.preserves(&other), "adding retained scalar data invalidates an earlier negative query");
    assert!(!stable.contains(&other), "retained scalar data still grants no storage authority");
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

        assert_eq!(ExecutionCondition::call(callable, None, [], vec![observed], None),
            ExecutionCondition::call(callable, None, [], vec![value.clone()], None));
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
    let copied = ExecutionCondition::scalar_observation(copy_identity.clone(), pointer.clone());
    let owned = ExecutionCondition::owned_value(copy_identity, pointer.clone());
    let count = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(2)));

    let condition = |value| ExecutionCondition::predicate(predicate.into(), callable.substitution(),
        vec![value, count.clone()]);

    let spatial = BTreeMap::from([(predicate.into(), &[0][..])]);

    for observed in [copied.clone(), owned.clone(), pointer.clone().borrowed()] {
        assert_eq!(condition(observed).pointer_observations(&spatial),
            condition(pointer.clone()).pointer_observations(&spatial));
    }

    assert_ne!(condition(copied.clone().borrowed()).pointer_observations(&spatial),
        condition(pointer.clone()).pointer_observations(&spatial));

    assert_ne!(condition(owned.clone().borrowed()).pointer_observations(&spatial),
        condition(pointer.clone()).pointer_observations(&spatial));

    let replaced = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(3)));

    assert_eq!(condition(owned.with_current_contents(replaced.clone())).pointer_observations(&spatial),
        condition(replaced).pointer_observations(&spatial));

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
    let call = |value| ExecutionCondition::call(callable, None, [], vec![value], None);
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

    let call = |value| ExecutionCondition::call(callable, None, [], vec![value], None);

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
fn data_aliases_preserve_identity_without_assuming_numeric_equality() {
    let input = |id| ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(id)));
    let alias = ExecutionCondition::value_alias(input(0), input(1));
    let numeric = ExecutionCondition::operation(bray_bound_tree::BoundOperator::Equal, vec![input(0), input(1)]);
    let mut ordinary = BTreeSet::new();

    numeric.clone().assume(true, &mut ordinary);

    assert_ne!(alias.prove_trusted(&BTreeSet::new(), &ordinary), Some(true));
    assert_eq!(ExecutionCondition::value_alias(input(0), input(0)), ExecutionCondition::Boolean(true));
    assert_eq!(ExecutionCondition::value_alias(input(1), input(0)), alias);
    assert!(alias.ownership_identity().is_none());

    for condition in [alias.clone(), numeric] {
        let selected = input(2);

        let guarded = ExecutionCondition::operation(bray_bound_tree::BoundOperator::LogicalOr, vec![
            ExecutionCondition::operation(bray_bound_tree::BoundOperator::LogicalNot, vec![selected.clone()]), condition.clone(),
        ]);

        ordinary.clear();
        guarded.assume(true, &mut ordinary);
        assert!(ExecutionCondition::equalities(&ordinary, None).is_empty());
        selected.assume(true, &mut ordinary);

        let bindings = ExecutionCondition::equalities(&ordinary, None);

        assert_eq!(input(0).with_equalities(&bindings), input(1).with_equalities(&bindings));
        assert_eq!(alias.prove_trusted(&BTreeSet::new(), &ordinary), (condition == alias).then_some(true));
    }

}

#[test]
fn structural_rewrites_preserve_initialized_nullable_contents() {
    let input = |id| ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(id)));
    let nullable = bray_bound_tree::StorageProjection::NullableValue;
    let source = input(0).present();

    let captured = crate::execution_guarantees::owned_snapshot(
        &source, input(1), &mut { ExecutionCondition::WORK_LIMIT },
    );

    let moved = crate::execution_guarantees::owned_snapshot(
        &source, input(2), &mut { ExecutionCondition::WORK_LIMIT },
    );

    let values = [(input(1), input(2))].into();

    assert_eq!(captured.with_snapshots(&Default::default(), &Default::default()), captured);
    assert_eq!(captured.with_joined_values(&Default::default(), &Default::default()), captured);
    assert_eq!(captured.with_joined_values(&values, &values), moved);

    let mapped = crate::map_execution_condition_references(
        &captured,
        &|value| Ok::<_, std::convert::Infallible>(value),
        &|place| Ok(ExecutionCondition::Input(place.clone())),
        &|literal| Ok(Arc::clone(literal)),
        &mut |_, _, _, _, _| unreachable!("the snapshot fixture contains no callable"),
    ).expect("identity remapping is infallible");

    assert_eq!(mapped, captured);
    assert!(captured.owned_components().any(|value| matches!(value, ExecutionCondition::Present(_))));
    assert_eq!(captured.project(nullable), input(1).project(nullable));

    let predicate = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>("ValidWrite");
    let substitution = crate::test_support::empty_substitution(predicate.into());
    let held = |value| ExecutionCondition::predicate(predicate.into(), substitution, vec![value]);
    let republished = ExecutionCondition::Present(Arc::new(input(1).project(nullable)));

    assert_eq!(held(republished.clone()), held(input(1)), "payload extraction retains the whole nullable predicate argument");

    let values = crate::test_support::semantic_values();

    let argument = values.intern_constant_term(bray_symbols::ConstantTermData::CallableArgument(SymbolOrdinal::new(1)))
        .expect("the formal argument belongs to the test store");

    let payload = values.intern_constant_term(bray_symbols::ConstantTermData::Projection(
        bray_symbols::ConstantProjection::new(argument, ConstantProjectionKind::NullableValue),
    )).expect("the payload belongs to the test store");

    let term = values.intern_constant_term(bray_symbols::ConstantTermData::NullablePresent(payload))
        .expect("the present term belongs to the test store");

    assert_eq!(crate::execution_condition_from_type_term(values, term, 2)
        .expect("the portable snapshot is valid"), republished);
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
        let other = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)));
        let decisive = ExecutionCondition::operation(operator, vec![known.clone(), other]);
        let evidence = BTreeSet::from([(known.clone(), truth)]);
        let mut budget = 3;

        assert_eq!(decisive.prove(&evidence, &mut budget), Some(truth));
        assert_eq!(budget, 1, "a decisive operand preserves work for subsequent proofs");

        let mut budget = 3;

        assert_eq!(decisive.prove_mixed(&evidence, &evidence, &mut budget), Some(truth));
        assert_eq!(budget, 1);

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

    assert_eq!(anonymous.owner_identity(), &ExecutionCondition::Unknown,
        "anonymous contents cannot become a substitute for missing backing identity");

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

    assert_eq!(owner.clone().with_snapshots(&snapshots, &Default::default()), snapshot);

    assert_eq!(
        owner.component(first).with_snapshots(&snapshots, &Default::default()),
        ExecutionCondition::Boolean(true)
    );

    assert_eq!(unchanged.with_snapshots(&snapshots, &Default::default()), unchanged);
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

    let empty = BTreeSet::new();
    let evidence = BTreeSet::from([(published.clone(), true)]);
    let proof = super::ExecutionProof::new(&evidence, &empty);

    for _ in 0..2 {
        assert_eq!(proof.prove_trusted(&declared), Some(true));
        assert_eq!(proof.prove_trusted(&published), Some(true));
        assert_eq!(proof.prove_trusted(&trusted), None, "unknown guard cannot release its evidence");
        assert_eq!(proof.prove_trusted(&ExecutionCondition::Unknown), None);
    }

    let ordinary_only = BTreeSet::from([(ordinary.clone(), true)]);
    let replaced = super::ExecutionProof::new(&empty, &ordinary_only);

    assert_eq!(replaced.prove_trusted(&trusted), None, "another context cannot retain prior authority");
    assert_eq!(replaced.prove_trusted(&declared), None);

    let unresolved = or(ordinary, ExecutionCondition::Unknown);

    assert!(!unresolved.implies(&unresolved));
}

#[test]
fn boolean_publication_forgets_local_atoms_under_both_polarities() {
    use bray_bound_tree::BoundOperator::{LogicalAnd, LogicalNot, LogicalOr};

    let input = |ordinal| ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(ordinal)));
    let left = input(0);
    let right = input(1);

    let local = ExecutionCondition::InitializedSnapshot(ExecutionPlace::argument(SymbolOrdinal::new(2)),
        Arc::new(ExecutionCondition::Boolean(true)), Arc::new(input(2)));

    let not = |condition| ExecutionCondition::operation(LogicalNot, vec![condition]);
    let and = |left, right| ExecutionCondition::operation(LogicalAnd, vec![left, right]);
    let or = |left, right| ExecutionCondition::operation(LogicalOr, vec![left, right]);
    let relation = or(and(left.clone(), local.clone()), and(right.clone(), not(local.clone())));
    let retain = |condition: &ExecutionCondition| matches!(condition, ExecutionCondition::Input(_)).then(|| condition.clone());

    for (condition, holds, expected) in [
        (relation.clone(), true, or(left.clone(), right.clone())),
        (relation.clone(), false, ExecutionCondition::Boolean(true)),
        (not(relation.clone()), true, ExecutionCondition::Boolean(true)),
        (not(relation), false, or(left.clone(), right.clone())),
        (or(left.clone(), local.clone()), true, ExecutionCondition::Boolean(true)),
        (and(left.clone(), local.clone()), false, ExecutionCondition::Boolean(true)),
        (local.clone(), true, ExecutionCondition::Boolean(true)),
        (not(local.clone()), true, ExecutionCondition::Boolean(true)),
    ] {
        let projected = condition.boolean_consequence(holds, &mut { retain }, &mut { ExecutionCondition::WORK_LIMIT });

        assert_eq!(projected, expected);

        let premise = if holds { condition } else { not(condition) };

        assert!(premise.implies(&projected), "{premise:?} must imply {projected:?}");
        assert_eq!(projected.prove_trusted(&BTreeSet::new(), &BTreeSet::from([(premise, true)])), Some(true));
    }

    assert_eq!(left.boolean_consequence(true, &mut { retain }, &mut 0), ExecutionCondition::Boolean(true));
}

#[test]
fn boolean_publication_bounds_unique_terms_instead_of_repeated_occurrences() {
    use bray_bound_tree::BoundOperator::{LogicalAnd, LogicalOr};

    let atom = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));

    let local = ExecutionCondition::InitializedSnapshot(ExecutionPlace::argument(SymbolOrdinal::new(1)),
        Arc::new(ExecutionCondition::Boolean(true)), Arc::new(atom.clone()));

    let mut relation = ExecutionCondition::Operation(LogicalAnd, vec![atom.clone(), local].into());

    for index in 0..10 {
        relation = ExecutionCondition::Operation(if index % 2 == 0 { LogicalOr } else { LogicalAnd },
            vec![relation.clone(), relation].into());
    }

    let mut budget = ExecutionCondition::WORK_LIMIT;

    let projected = relation.boolean_consequence(true, &mut |condition|
        matches!(condition, ExecutionCondition::Input(_)).then(|| condition.clone()), &mut budget);

    assert!(budget > 0, "repeated subformulas cannot exhaust the unique-term bound");
    assert_eq!(projected, atom, "forgetting the local conjunct retains the portable consequence");
}
