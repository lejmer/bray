use crate::execution_guarantees::ExecutionSubstitution;
use std::collections::{BTreeMap, BTreeSet};

use super::super::state::ExecutionState;
use super::merge::result_replacements;
use crate::ExecutionCondition;
use crate::execution_guarantees::common_components;

/// Join actual predecessor constraints without inventing an unevaluated operand.
/// Only immutable observations participate. A missing constraint contributes true,
/// so forgetting a bounded formula cannot reintroduce it on the next iteration.
pub(super) fn ordinary_alternatives(
    left: &BTreeSet<(ExecutionCondition, bool)>,
    right: &BTreeSet<(ExecutionCondition, bool)>,
) -> ExecutionCondition {
    use bray_bound_tree::BoundOperator;

    let branch = |source: &BTreeSet<(ExecutionCondition, bool)>, other: &BTreeSet<(ExecutionCondition, bool)>| {
        ExecutionCondition::operation(BoundOperator::LogicalAnd, source.difference(other)
            .filter(|(condition, _)| !condition.contains_trusted() && !condition.observes_live(None))
            .take(ExecutionCondition::WORK_LIMIT)
            .map(|(condition, holds)| if *holds { condition.clone() } else {
                ExecutionCondition::operation(BoundOperator::LogicalNot, vec![condition.clone()])
            }).collect())
    };

    ExecutionCondition::operation(BoundOperator::LogicalOr, vec![branch(left, right), branch(right, left)])
}

pub(super) fn common_current_owner(
    left: &ExecutionCondition,
    right: &ExecutionCondition,
) -> Option<(crate::execution_guarantees::ExecutionObservationKind, ExecutionCondition)> {
    let role = |value: &ExecutionCondition| value.observation_kind()
        .or_else(|| value.ownership_identity().map(|(role, _)| role));

    let kind = role(left)?;

    if role(right) != Some(kind) || left.owner_identity() != right.owner_identity() {
        return None;
    }

    Some((kind, left.owner_identity().clone().into_referent().owner_identity().clone()))
}

pub(super) fn current_content_identities(
    left: &ExecutionState,
    right: &ExecutionState,
    places: &BTreeSet<&crate::ExecutionPlace>,
    block: Option<crate::analysis::id::AnalysisBlockId>,
) -> BTreeMap<ExecutionCondition, ExecutionCondition> {
    let Some(block) = block else { return BTreeMap::new(); };

    let mut owners = BTreeMap::new();

    for place in places {
        if super::merge::resolved_availability(left, place) != super::super::current::ValueAvailability::Present
            || super::merge::resolved_availability(right, place) != super::super::current::ValueAvailability::Present { continue; }

        let (Some(left), Some(right)) = (place.value_in(&left.current), place.value_in(&right.current)) else { continue; };

        collect_current_content_identities(&left, &right, super::merge::joined_observation(block, place), &mut owners);
    }

    owners.iter().map(|(owner, (_, contents))| {
        let mut identity = contents.clone();

        if let Some((root, path)) = owner.observation_path() {
            for length in 0..path.len() {
                let parent = path[..length].iter().fold(root.clone(), |value, projection| value.component(*projection));

                if let Some((_, parent)) = owners.get(&parent) {
                    identity = path[length..].iter().fold(parent.clone(),
                        |value, projection| value.component(*projection));

                    break;
                }
            }
        }

        (owner.clone(), identity)
    }).collect()
}

fn collect_current_content_identities(
    left: &ExecutionCondition,
    right: &ExecutionCondition,
    contents: ExecutionCondition,
    owners: &mut BTreeMap<ExecutionCondition, (crate::execution_guarantees::ExecutionObservationKind, ExecutionCondition)>,
) {
    use crate::execution_guarantees::ExecutionObservationKind;

    let Some((kind, owner)) = common_current_owner(left, right) else { return; };

    let entry = owners.entry(owner).or_insert_with(|| (kind, contents.clone()));

    // A loan observes its actual owner's contents, including owners moved into
    // aggregate fields. The reference slot does not create another snapshot.
    if entry.0 == ExecutionObservationKind::Reference && kind != ExecutionObservationKind::Reference {
        *entry = (kind, contents.clone());
    }

    if kind == ExecutionObservationKind::Reference { return; }

    for projection in left.component_projections().intersection(&right.component_projections()) {
        collect_current_content_identities(
            &left.clone().project(*projection), &right.clone().project(*projection),
            contents.clone().project(*projection), owners,
        );
    }
}

pub(in crate::analysis::guarantee) fn return_substitution(
    result: &ExecutionCondition,
) -> impl Fn(&ExecutionCondition) -> ExecutionCondition {
    let mut values = BTreeMap::new();
    let mut identities = BTreeMap::new();

    result_replacements(
        result,
        &ExecutionCondition::result(),
        &Default::default(),
        &Default::default(),
        &mut values,
        &mut identities,
    );

    move |condition| condition.with_joined_values(&values, &identities)
}

pub(in crate::analysis::guarantee) fn merge_returned(
    target: &mut ExecutionState,
    incoming: &ExecutionState,
    left_value: &ExecutionCondition,
    right_value: &ExecutionCondition,
    root: &ExecutionCondition,
    expression: bray_bound_tree::BoundExpressionId,
) -> ExecutionCondition {
    let mut incoming = incoming.clone();
    let mut facts = Vec::new();

    let joined = common_components(
        left_value,
        right_value,
        Some(root.clone()),
        (&target.proofs.ordinary_proof(&target.assumptions), &incoming.proofs.ordinary_proof(&incoming.assumptions)),
        &mut facts,
        &mut { ExecutionCondition::WORK_LIMIT },
    );

    let mut left = BTreeMap::new();
    let mut right = BTreeMap::new();
    let mut left_ids = BTreeMap::new();
    let mut right_ids = BTreeMap::new();

    let (stable_values, stable_identities) = super::merge::stable_observations(target, &incoming);

    for (source, replacements, identities) in [
        (left_value, &mut left, &mut left_ids),
        (right_value, &mut right, &mut right_ids),
    ] {
        result_replacements(
            source,
            &joined,
            &stable_values,
            &stable_identities,
            replacements,
            identities,
        );
    }

    // Pin independently retained observations before renaming their containing owner.
    for value in stable_values {
        left.insert(value.clone(), value.clone());
        right.insert(value.clone(), value);
    }

    for value in stable_identities {
        left_ids.insert(value.clone(), value.clone());
        right_ids.insert(value.clone(), value);
    }

    target
        .pending_results
        .insert(expression, left_value.clone());

    incoming
        .pending_results
        .insert(expression, right_value.clone());

    let left_substitution = ExecutionSubstitution::new(&left, &left_ids);
    let right_substitution = ExecutionSubstitution::new(&right, &right_ids);

    let conditions = super::witnesses::conditional_facts(
        (target, &left_substitution),
        (&incoming, &right_substitution),
        &BTreeMap::new(),
        &[(expression, joined.clone())],
        &BTreeSet::new(),
        None,
        &mut facts,
    );

    let ownership = super::witnesses::owned_alternatives(
        (target, left_value),
        (&incoming, right_value),
        &joined,
    );

    target.join_values(&left_substitution);
    incoming.join_values(&right_substitution);
    super::witnesses::retain_conditional_guarantees(target, &mut incoming, &conditions);

    for (condition, holds) in facts {
        condition.clone().assume(holds, &mut target.assumptions);
        condition.assume(holds, &mut incoming.assumptions);
    }

    let mut completed = Some(target.clone());

    super::merge::merge(&mut completed, &Some(incoming), super::merge::ExecutionJoin::Result(expression));
    *target = completed.expect("reachable normal return retains a state");
    target.pending_results.remove(&expression);

    target.owned_results.insert(joined.clone(), ownership);

    joined
}

#[cfg(test)]
mod tests {
    use super::common_components;
    use crate::{ExecutionCondition, ExecutionPlace};
    use bray_bound_tree::{BoundUnitId, StorageProjection};
    use bray_symbols::{ConstantProjectionKind, SymbolOrdinal};
    use std::sync::Arc;

    fn join_components(
        left: &ExecutionCondition,
        right: &ExecutionCondition,
    ) -> ExecutionCondition {
        let conditions = Default::default();
        let cache = crate::execution_guarantees::ExecutionProofCache::default();
        let conditions = cache.ordinary_proof(&conditions);

        common_components(
            left,
            right,
            None,
            (&conditions, &conditions),
            &mut Vec::new(),
            &mut { ExecutionCondition::WORK_LIMIT },
        )
    }

    #[test]
    fn moving_scalar_storage_preserves_its_data_without_keeping_the_old_place() {
        let input = |ordinal| ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(ordinal))));
        let data = ExecutionCondition::new(crate::ExecutionConditionKind::ScalarValue(Arc::new(input(2))));
        let source = ExecutionCondition::scalar_observation(input(0), data.clone());
        let destination = input(1);

        let moved = crate::execution_guarantees::owned_snapshot(&source, destination.clone(), &mut {
            ExecutionCondition::WORK_LIMIT
        });

        assert_eq!(moved.storage_contents().map(|(place, _)| place), Some(&destination));
        assert_eq!(moved.copied_value(), &data);
        assert_eq!(moved.ownership_identity(), None);
    }

    #[test]
    fn nullable_joins_preserve_payloads_and_normalization_makes_progress() {
        use crate::execution_guarantees::ExecutionObservationKind;

        let input = |ordinal| ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(ordinal))));
        let payload = StorageProjection::NullableValue;
        let field = StorageProjection::TupleElement(SymbolOrdinal::new(0));

        let absent = ExecutionCondition::new(crate::ExecutionConditionKind::Literal(Arc::new(bray_symbols::ConstantValueData::new(
            crate::test_support::error_type(), bray_symbols::ConstantValueKind::NullableAbsent,
        ))));

        let identity = input(9);
        let conditions = Default::default();
        let cache = crate::execution_guarantees::ExecutionProofCache::default();
        let conditions = cache.ordinary_proof(&conditions);

        let join = |left: &ExecutionCondition, right: &ExecutionCondition| {
            let mut budget = 64;

            let value = common_components(left, right, Some(identity.clone()), (&conditions, &conditions),
                &mut Vec::new(), &mut budget);

            assert!(budget > 0, "normalization must remove a wrapper or descend into a component");

            value
        };

        for borrowed in [false, true] {
            let owner = ExecutionCondition::constructed(input(0), [(field, ExecutionCondition::boolean(true))].into());
            let value = if borrowed { owner.borrowed() } else { owner };
            let present = value.present();

            for wrapped in [false, true] {
                let present = if wrapped { ExecutionCondition::owned_value(input(1), present.clone()) } else { present.clone() };
                let joined = join(&present, &absent);
                let selected = joined.clone().project(payload);

                assert_eq!(selected.clone().project(field).into_referent(), ExecutionCondition::boolean(true));

                assert_eq!(selected.ownership_identity().map(|(kind, _)| kind), Some(if borrowed {
                    ExecutionObservationKind::Reference
                } else { ExecutionObservationKind::Owned }));

                assert_eq!(join(&absent, &present), joined);
                assert_eq!(join(&joined, &absent), joined, "nullable back edges must stabilize");
            }
        }

        let reference = |ordinal| ExecutionCondition::constructed(input(ordinal),
            [(field, ExecutionCondition::boolean(true))].into()).borrowed().present();

        let joined = join(&reference(0), &reference(1));

        assert_eq!(joined.clone().project(payload).project(field).into_referent(), ExecutionCondition::boolean(true));

        assert_eq!(joined.project(payload).ownership_identity().map(|(kind, _)| kind),
            Some(ExecutionObservationKind::Reference), "joining nullable references cannot grant ownership");

        assert_eq!(join(&ExecutionCondition::unknown(), &absent), ExecutionCondition::unknown());
    }

    #[test]
    fn scalar_joins_produce_usable_stable_data_without_owner_authority() {
        let literal = |value| ExecutionCondition::new(crate::ExecutionConditionKind::Literal(Arc::new(bray_symbols::ConstantValueData::new(
            crate::test_support::error_type(),
            bray_symbols::ConstantValueKind::Integer(bray_symbols::IntegerConstant::from_u64(value)),
        ))));

        let place = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let identity = |version| ExecutionCondition::new(crate::ExecutionConditionKind::Joined(BoundUnitId::new(0), version, place.clone()));
        let conditions = Default::default();
        let cache = crate::execution_guarantees::ExecutionProofCache::default();
        let conditions = cache.ordinary_proof(&conditions);

        let join = |left: &ExecutionCondition, right: &ExecutionCondition, version| {
            common_components(left, right, Some(identity(version)), (&conditions, &conditions),
                &mut Vec::new(), &mut { ExecutionCondition::WORK_LIMIT })
        };

        for (copy_left, copy_right, same_context) in [(false, false, false), (true, true, false), (true, false, false), (true, true, true)] {
            let left = if copy_left { ExecutionCondition::scalar_observation(identity(10), literal(0)) } else { literal(0) };
            let right = if copy_right { ExecutionCondition::scalar_observation(identity(if same_context { 10 } else { 11 }), literal(1)) } else { literal(1) };
            let joined = join(&left, &right, 1);

            assert_ne!(joined.computed_value(), &ExecutionCondition::unknown(),
                "two produced scalar alternatives need a guard subject: copied={copy_left}/{copy_right}");

            assert!(joined.ownership_identity().is_none());
            assert_eq!(join(&right, &left, 1), joined, "predecessor order cannot change the produced datum");

            assert_ne!(ExecutionCondition::operation(bray_bound_tree::BoundOperator::Less,
                vec![joined.clone(), literal(8)]), ExecutionCondition::unknown());

            assert_eq!(join(&joined, &right, 1), joined, "repeated analysis must stabilize");

            assert_ne!(join(&left, &right, 2).computed_value(), joined.computed_value(),
                "another contents epoch cannot reuse the old scalar datum");
        }

        assert_eq!(join(&ExecutionCondition::unknown(), &literal(1), 1).computed_value(), &ExecutionCondition::unknown(),
            "a missing predecessor value is not a produced opaque scalar");
    }

    #[test]
    fn short_circuit_join_keeps_the_produced_boolean_meaning() {
        for (operator, shortcut, compound) in [
            (bray_bound_tree::BoundOperator::LogicalAnd, false, false),
            (bray_bound_tree::BoundOperator::LogicalOr, true, false),
            (bray_bound_tree::BoundOperator::LogicalAnd, false, true),
            (bray_bound_tree::BoundOperator::LogicalOr, true, true),
        ] {
            let data = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(0))));

            let data = if compound {
                ExecutionCondition::operation(operator, vec![data,
                    ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(2))))])
            } else { data };

            let left = ExecutionCondition::scalar_observation(ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(9)))),
                data);

            let right = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(1))));
            let formula = ExecutionCondition::operation(operator, vec![left.clone(), right]);
            let produced = ExecutionCondition::scalar_observation(ExecutionCondition::result(), formula);
            let mut skipped = Default::default();
            let mut evaluated = Default::default();

            left.clone().assume(shortcut, &mut skipped);
            left.assume(!shortcut, &mut evaluated);

            let cache = crate::execution_guarantees::ExecutionProofCache::default();
            let skipped = cache.ordinary_proof(&skipped);
            let evaluated = cache.ordinary_proof(&evaluated);

            for reversed in [false, true] {
                let constant = ExecutionCondition::new(crate::ExecutionConditionKind::Boolean(shortcut));

                let (first, second, conditions) = if reversed {
                    (&produced, &constant, (&evaluated, &skipped))
                } else {
                    (&constant, &produced, (&skipped, &evaluated))
                };

                let joined = common_components(first, second, Some(ExecutionCondition::result()), conditions,
                    &mut Vec::new(), &mut { ExecutionCondition::WORK_LIMIT });

                assert_eq!(joined, produced);
                assert!(joined.ownership_identity().is_none(), "equal Boolean data cannot supply owner authority");
            }

            let incompatible = common_components(&ExecutionCondition::new(crate::ExecutionConditionKind::Boolean(!shortcut)), &produced,
                Some(ExecutionCondition::result()), (&skipped, &evaluated), &mut Vec::new(),
                &mut { ExecutionCondition::WORK_LIMIT });

            assert_ne!(incompatible, produced, "a selected value must agree with both predecessors");
        }
    }

    #[test]
    fn repeated_loan_join_stabilizes_without_acquiring_ownership() {
        let owner = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(0))));
        let field = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0));
        let before = owner.with_component(&[field], ExecutionCondition::boolean(true), None).borrowed();
        let after = before.with_referent(owner.with_component(&[field], ExecutionCondition::boolean(false), None));
        let joined = join_components(&before, &after);

        assert_eq!(join_components(&joined, &after), joined);
        assert_eq!(join_components(&after, &joined), joined);
        assert_eq!(joined.owned_components().count(), 0);
    }

    #[test]
    fn refreshing_and_joining_a_loan_preserves_its_context_without_owning_contents() {
        let owner = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(0))));
        let field = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0));
        let original = owner.with_component(&[field], ExecutionCondition::boolean(true), None);
        let loan = original.clone().borrowed();
        let changed = original.with_component(&[field], ExecutionCondition::boolean(false), None);
        let refreshed = loan.with_referent(changed.clone());

        assert_eq!(refreshed.owner_identity(), loan.owner_identity());
        assert_eq!(refreshed.clone().component(field).into_referent(), ExecutionCondition::boolean(false));
        assert_eq!(loan.clone().component(field).into_referent(), ExecutionCondition::boolean(true));
        assert_eq!(refreshed.clone().borrowed(), refreshed);
        assert_eq!(refreshed.owned_components().count(), 0);
        assert_eq!(refreshed.clone().into_referent(), changed);

        let observation = |value| ExecutionCondition::operation(
            bray_bound_tree::BoundOperator::Equal,
            vec![value, ExecutionCondition::result()],
        );

        assert_eq!(observation(refreshed.clone()), observation(changed.borrowed()));
        assert_ne!(observation(refreshed.clone()), observation(loan.clone()));

        let joined = join_components(&loan, &refreshed);

        assert_eq!(joined.owner_identity(), loan.owner_identity());
        assert_eq!(joined.owned_components().count(), 0);
        assert_ne!(joined.clone().component(field), ExecutionCondition::boolean(true));
        assert_ne!(joined.component(field), ExecutionCondition::boolean(false));
    }

    #[test]
    fn joins_keep_matching_nested_observations_without_inventing_identity() {
        let field = StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let component = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0));
        let place = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let identity = ExecutionCondition::new(crate::ExecutionConditionKind::Input(place.clone()));
        let marker = ExecutionCondition::boolean(true);

        let nested = |version| {
            ExecutionCondition::constructed(ExecutionCondition::new(crate::ExecutionConditionKind::Joined(
                    BoundUnitId::new(0),
                    version,
                    place.clone(),
                )),
                [(field, marker.clone())].into(),
            )
            .borrowed()
        };

        let owner = |value| {
            ExecutionCondition::constructed(identity.clone(), [(field, value)].into())
        };

        let joined = join_components(&owner(nested(1)), &owner(nested(2)));

        assert_eq!(joined.owner_identity(), &identity);

        assert_eq!(
            joined.clone().component(component).component(component).into_referent(),
            marker
        );

        let different = owner(
            ExecutionCondition::constructed(ExecutionCondition::unknown(),
                [(field, ExecutionCondition::boolean(false))].into(),
            )
            .borrowed(),
        );

        assert_eq!(
            join_components(&joined, &different)
                .component(component)
                .component(component).into_referent(),
            ExecutionCondition::unknown()
        );
    }

    #[test]
    fn joined_contents_keep_the_descriptor_and_a_distinct_current_identity() {
        use crate::analysis::guarantee::state::ExecutionState;
        use crate::analysis::id::AnalysisBlockId;

        let place = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let descriptor = ExecutionCondition::new(crate::ExecutionConditionKind::Input(place.clone()));
        let field = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0));
        let current = ExecutionCondition::new(crate::ExecutionConditionKind::Joined(BoundUnitId::new(0), 2, place.clone()));
        let conditions = Default::default();
        let cache = crate::execution_guarantees::ExecutionProofCache::default();
        let conditions = cache.ordinary_proof(&conditions);
        let symbol = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>("ValidRead");
        let substitution = crate::test_support::empty_substitution(symbol.into());

        let guarantee = |value: ExecutionCondition| ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(
            ExecutionCondition::predicate(symbol.into(), substitution, vec![value.component(field).borrowed()]),
        )));

        let whole_guarantee = |value: ExecutionCondition| ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(
            ExecutionCondition::predicate(symbol.into(), substitution, vec![value.borrowed()]),
        )));

        #[derive(Clone, Copy, Debug)]
        enum Context { Owned, Inline, Loan, Reference }

        for (context, unchanged, component_carrier) in [Context::Owned, Context::Inline, Context::Loan, Context::Reference].into_iter()
            .flat_map(|context| [false, true].into_iter().flat_map(move |unchanged|
                [false, true].map(move |component| (context, unchanged, component))))
        {
            let branch = |version| {
                let data = ExecutionCondition::new(crate::ExecutionConditionKind::Joined(BoundUnitId::new(0), version, place.clone()));

                match context {
                    Context::Owned => ExecutionCondition::owned_value(descriptor.clone(), data),
                    Context::Inline => ExecutionCondition::constructed(descriptor.clone(),
                        [(StorageProjection::TupleElement(SymbolOrdinal::new(0)), data.component(field))].into()),
                    Context::Loan => descriptor.clone().borrowed().with_referent(
                        ExecutionCondition::owned_value(descriptor.clone(), data),
                    ),
                    Context::Reference => descriptor.with_referent(data),
                }
            };

            let initial = match context {
                Context::Loan => descriptor.clone().borrowed(),
                Context::Owned => descriptor.clone(),
                Context::Inline => ExecutionCondition::constructed(descriptor.clone(),
                    [(StorageProjection::TupleElement(SymbolOrdinal::new(0)), descriptor.clone().component(field))].into()),
                Context::Reference => ExecutionCondition::reference_value(descriptor.clone(), descriptor.clone()),
            };

            let left = if unchanged { initial } else { branch(0) };
            let right = branch(1);

            let joined = common_components(
                &left, &right, Some(current.clone()), (&conditions, &conditions),
                &mut Vec::new(), &mut { ExecutionCondition::WORK_LIMIT },
            );

            assert_eq!(joined.owner_identity(), left.owner_identity());
            assert_eq!(joined.clone().component(field).into_referent(), current.clone().component(field));
            assert_ne!(joined.clone().component(field), descriptor.clone().component(field));

            if !matches!(context, Context::Owned | Context::Inline) {
                assert_eq!(joined.owned_components().count(), 0);
                assert_eq!(joined.clone().borrowed(), joined);

                let substituted = joined.with_snapshots(&Default::default());

                assert_eq!(substituted, joined);
                assert_eq!(substituted.owned_components().count(), 0);
            }

            let state = |value: ExecutionCondition, previous: ExecutionCondition| {
                let mut state = ExecutionState::default();
                let condition = guarantee(value.clone());

                state.current.insert(place.clone(), value.clone());
                state.current.insert(ExecutionPlace::argument(SymbolOrdinal::new(99)), value.clone().borrowed());
                state.current.insert(ExecutionPlace::argument(SymbolOrdinal::new(98)), value.clone().borrowed().component(field));

                let retired = ExecutionPlace::argument(SymbolOrdinal::new(97));

                state.current.insert(retired.clone(), previous);
                state.current.retire(&retired);
                condition.clone().assume(true, &mut state.trusted_assumptions);

                let carrier = if component_carrier {
                    value.clone().component(field)
                } else { value.clone().into_referent() };

                state.carry_guarantee(carrier, condition);

                let whole = whole_guarantee(value.clone());

                state.carry_guarantee(value.into_referent(), whole.clone());
                whole.assume(true, &mut state.trusted_assumptions);

                state
            };

            let previous = ExecutionCondition::constructed(descriptor.clone(),
                [(StorageProjection::TupleElement(SymbolOrdinal::new(0)),
                    ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(96)))))].into()).borrowed();

            let left_state = state(left.clone(), previous);
            let right_state = state(right.clone(), descriptor.clone().borrowed());
            let mut left_values = Default::default();
            let mut left_owners = Default::default();
            let mut right_values = Default::default();
            let mut right_owners = Default::default();

            for (source, values, owners) in [(&left, &mut left_values, &mut left_owners),
                (&right, &mut right_values, &mut right_owners)] {
                super::super::merge::result_replacements(source, &joined, &Default::default(), &Default::default(), values, owners);
            }

            let retired = ExecutionPlace::argument(SymbolOrdinal::new(97));

            let snapshots = [(place.clone(), joined.clone()),
                (retired.clone(), right_state.current.get(&retired).expect("retained terminal data").clone())].into();

            let retained = super::super::witnesses::conditional_facts(
                (&left_state, &crate::execution_guarantees::ExecutionSubstitution::new(&left_values, &left_owners)), (&right_state, &crate::execution_guarantees::ExecutionSubstitution::new(&right_values, &right_owners)),
                &snapshots, &[], &Default::default(), None, &mut Vec::new());

            let mut unknown = left_state.clone();

            unknown.current.insert(place.clone(), ExecutionCondition::unknown());

            let rejected = super::super::witnesses::conditional_facts(
                (&unknown, &crate::execution_guarantees::ExecutionSubstitution::new(&left_values, &left_owners)), (&right_state, &crate::execution_guarantees::ExecutionSubstitution::new(&right_values, &right_owners)),
                &snapshots, &[], &Default::default(), None, &mut Vec::new());

            assert!(!rejected.iter().any(|witness| witness.available && witness.clauses.iter().any(|(_, condition)| *condition == whole_guarantee(joined.clone()))),
                "a live Unknown observation cannot supply the missing whole-object evidence");

            if matches!(context, Context::Reference) {
                assert!(!retained.iter().any(|witness| &witness.owner == joined.clone().into_referent().owner_identity()),
                    "a reference descriptor cannot manufacture a joined pointee owner");
            } else {
                assert!(retained.iter().any(|witness| witness.available && witness.clauses.iter().any(|(_, condition)| *condition == whole_guarantee(joined.clone()))),
                    "the proof collector must ignore retired terminal views: context={context:?} retained={retained:?}");
            }

            let mut merged = Some(left_state);

            super::super::merge::merge(
                &mut merged, &Some(right_state),
                Some(AnalysisBlockId::from_slot(BoundUnitId::new(0), 2)).into(),
            );

            let merged = merged.expect("both current observations remain reachable");

            assert_eq!(merged.current.get(&ExecutionPlace::argument(SymbolOrdinal::new(99)))
                .expect("retained readonly alias").clone().into_referent(),
                merged.current.get(&place).expect("joined owner").clone().into_referent(),
                "an alias and its owner must observe the same joined contents");

            assert_eq!(merged.current.get(&ExecutionPlace::argument(SymbolOrdinal::new(98)))
                .expect("retained readonly field alias").clone().into_referent(),
                merged.current.get(&place).expect("joined owner").clone().component(field).into_referent(),
                "a field alias must observe its containing owner's joined component");

            let condition = guarantee(merged.current.get(&place).expect("joined observation").clone());
            let whole = whole_guarantee(merged.current.get(&place).expect("joined observation").clone());

            assert_eq!(whole.prove_trusted(&merged.trusted_assumptions, &merged.assumptions), Some(true),
                "a retired temporary cannot replace the live whole-object snapshot: context={context:?}");

            assert_eq!(condition.prove_trusted(&merged.trusted_assumptions, &merged.assumptions), Some(true),
                "context={context:?} unchanged={unchanged} component={component_carrier} facts={:?} witnesses={:?}",
                merged.trusted_assumptions, merged.witnesses);

            assert_ne!(condition, guarantee(descriptor.clone()));
        }
    }

    #[test]
    fn disappearing_alias_does_not_rename_a_surviving_observation() {
        use crate::analysis::guarantee::state::ExecutionState;
        use crate::analysis::id::AnalysisBlockId;

        let original = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let alias = ExecutionPlace::argument(SymbolOrdinal::new(1));
        let value = ExecutionCondition::new(crate::ExecutionConditionKind::Input(original.clone()));
        let mut left = ExecutionState::default();

        left.current.insert(original.clone(), value.clone());
        left.current.insert(alias.clone(), value.clone());
        left.assumptions.insert((value.clone(), true));

        let mut right = ExecutionState::default();

        right.current.insert(original.clone(), value.clone());
        right.assumptions.insert((value.clone(), true));

        let mut merged = Some(left);

        super::super::merge::merge(
            &mut merged,
            &Some(right),
            Some(AnalysisBlockId::from_slot(BoundUnitId::new(0), 0)).into(),
        );

        let merged = merged.expect("both predecessors are reachable");

        assert_eq!(*merged.current.get(&original).expect("original observation"), value);
        assert_eq!(*merged.current.get(&alias).expect("alias observation"), ExecutionCondition::unknown());
        assert!(merged.assumptions.contains(&(value, true)));
    }
    #[test]
    fn returned_branches_keep_shared_fields_in_the_whole_snapshot() {
        use crate::analysis::guarantee::state::ExecutionState;

        let owner = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let marker = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0));
        let marker_place = owner.clone().component(marker);
        let shared = ExecutionCondition::new(crate::ExecutionConditionKind::Input(marker_place.clone()));
        let mut left = ExecutionState::default();
        let mut right = ExecutionState::default();

        left.assign(
            owner.clone(),
            ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)))),
            None,
        );

        left.assign(marker_place.clone(), shared.clone(), None);

        right.assign(
            owner.clone(),
            ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(2)))),
            None,
        );

        right.assign(marker_place.clone(), shared.clone(), None);

        let mut merged = Some(left);

        super::super::merge::merge(&mut merged, &Some(right), None.into());

        let merged = merged.expect("both returned branches are reachable");
        let whole = owner.value_in(&merged.current).expect("retained owner");

        assert_eq!(whole.component(marker), shared);
        assert_eq!(marker_place.value_in(&merged.current), Some(shared));
    }

    #[test]
    fn stable_owned_values_converge_across_guarded_back_edges() {
        use crate::analysis::guarantee::state::ExecutionState;
        use crate::analysis::id::AnalysisBlockId;

        let place = ExecutionPlace::from(bray_bound_tree::BoundReferenceTarget::Surface(
            bray_symbols::CallableParameterSymbolId::from_symbol_id(bray_symbols::SymbolId::new(0))
                .into(),
        ));

        let value = ExecutionCondition::new(crate::ExecutionConditionKind::Input(place.clone()));

        let guard = ExecutionCondition::new(crate::ExecutionConditionKind::Input(
            bray_bound_tree::BoundReferenceTarget::Surface(
                bray_symbols::CallableParameterSymbolId::from_symbol_id(
                    bray_symbols::SymbolId::new(1),
                )
                .into(),
            )
            .into(),
        ));

        let mut state = ExecutionState::default();

        state.current.insert(place, value.clone());

        state.owned_results.insert(value.clone(), bray_symbols::DependencyOwnedResult::Transfer(
            bray_symbols::DependencyOwnedTransfer::new(value.clone(), []),
        ));

        let mut target = Some(state);

        for _ in 0..16 {
            let mut incoming = target.clone().expect("reachable loop header");

            incoming.assumptions.insert((guard.clone(), true));

            assert!(!super::super::merge::merge(
                &mut target,
                &Some(incoming),
                Some(AnalysisBlockId::from_slot(BoundUnitId::new(0), 0)).into(),
            ));
        }

        let state = target.expect("reachable loop header");

        assert_eq!(state.owned_results.len(), 1);

        assert_eq!(
            state.owned_result(&value),
            bray_symbols::DependencyOwnedResult::Transfer(
                bray_symbols::DependencyOwnedTransfer::new(value, [])
            )
        );
    }

    #[test]
    fn empty_layouts_do_not_accumulate_in_loop_observations() {
        use crate::analysis::guarantee::state::ExecutionState;
        use crate::analysis::id::AnalysisBlockId;

        let place = ExecutionPlace::from(bray_bound_tree::BoundReferenceTarget::Surface(
            bray_symbols::CallableParameterSymbolId::from_symbol_id(bray_symbols::SymbolId::new(0))
                .into(),
        ));

        let guard = ExecutionCondition::new(crate::ExecutionConditionKind::Input(
            bray_bound_tree::BoundReferenceTarget::Surface(
                bray_symbols::CallableParameterSymbolId::from_symbol_id(
                    bray_symbols::SymbolId::new(1),
                )
                .into(),
            )
            .into(),
        ));

        let mut state = ExecutionState::default();

        state
            .current
            .insert(place.clone(), ExecutionCondition::boolean(false));

        let mut target = Some(state);
        let mut previous_keys = None;

        for _ in 0..16 {
            let mut incoming = target.clone().expect("reachable loop header");

            incoming
                .current
                .insert(place.clone(), ExecutionCondition::boolean(true));

            incoming.assumptions.insert((guard.clone(), true));

            super::super::merge::merge(
                &mut target,
                &Some(incoming),
                Some(AnalysisBlockId::from_slot(BoundUnitId::new(0), 0)).into(),
            );

            let layouts = &target.as_ref().expect("reachable loop header").owned_results;

            assert!(layouts.values().all(bray_symbols::DependencyOwnedResult::is_empty));

            let keys = layouts.keys().cloned().collect::<std::collections::BTreeSet<_>>();

            if let Some(previous) = &previous_keys { assert_eq!(&keys, previous, "loop ownership observations must stabilize"); }
            previous_keys = Some(keys);
        }
    }
}

#[cfg(test)]
mod nested_guard_tests {
    use super::merge_returned;
    use crate::analysis::guarantee::state::ExecutionState;
    use crate::{ExecutionCondition, ExecutionPlace};
    use bray_bound_tree::{BoundErrorExpression, BoundExpression, BoundUnitId, StorageProjection};
    use crate::test_support::{error_type, expression_unit, push_expression};
    use bray_symbols::{SymbolId, SymbolOrdinal, UnionPayloadFieldSymbolId, UnionVariantSymbolId};
    use std::sync::Arc;

    #[test]
    fn nullable_payload_witnesses_follow_the_selected_owner_only() {
        let input = |id| ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(id))));
        let marker = StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let predicate = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>("ValidWrite");
        let substitution = crate::test_support::empty_substitution(predicate.into());

        let held = |owner: ExecutionCondition| ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(
            ExecutionCondition::predicate(predicate.into(), substitution, vec![owner.project(marker).borrowed()]),
        )));

        let absent = ExecutionCondition::new(crate::ExecutionConditionKind::Literal(Arc::new(bray_symbols::ConstantValueData::new(
            error_type(), bray_symbols::ConstantValueKind::NullableAbsent,
        ))));

        let (_, expressions) = expression_unit(BoundUnitId::new(0), |tree, origin| (0..2).map(|_| {
            push_expression(tree, BoundExpression::Error(BoundErrorExpression::new(origin, error_type())))
        }).collect());

        for (depth, valid) in [(1, true), (2, true), (1, false), (2, false)] {
            let mut left = ExecutionState::default();
            let witness_owner = input(if valid { 0 } else { 1 });
            let guarantee = held(witness_owner.clone());

            left.carry_guarantee(witness_owner, guarantee.clone());
            guarantee.assume(true, &mut left.trusted_assumptions);

            let present = (0..depth).fold(input(0), |value, _| value.present());

            let joined = merge_returned(&mut left, &ExecutionState::default(), &present, &absent,
                &ExecutionCondition::result(), expressions[0]);

            let mut selected = joined;

            for _ in 0..depth {
                ExecutionCondition::operation(bray_bound_tree::BoundOperator::NotEqual,
                    vec![selected.clone(), absent.clone()]).assume(true, &mut left.assumptions);

                selected = selected.project(StorageProjection::NullableValue);
            }

            let required = held(selected);

            assert_eq!(required.prove_trusted(&left.trusted_assumptions, &left.assumptions) == Some(true), valid,
                "depth={depth} valid={valid} required={required:?} facts={:?} witnesses={:?}",
                left.trusted_assumptions, left.witnesses);
        }

        for parent in [input(2), ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expressions[1]))] {
        for detailed in [None, Some(4), Some(6)] {
        let support = parent.clone().project(marker);

        let condition = |owner: ExecutionCondition| ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(
            ExecutionCondition::predicate(predicate.into(), substitution, vec![owner.borrowed(), support.clone().project(marker).borrowed()]),
        )));

        let mut left = ExecutionState::default();

        left.assign(ExecutionPlace::argument(SymbolOrdinal::new(2)),
            ExecutionCondition::constructed(parent.clone(), [(marker, ExecutionCondition::unknown())].into()), None);

        left.assign(ExecutionPlace::argument(SymbolOrdinal::new(5)), support.clone(), None);

        if let Some(alias) = detailed {
            let field = StorageProjection::TupleElement(SymbolOrdinal::new(1));
            let data = ExecutionCondition::constructed(support.clone(), [(field, support.clone().project(field))].into());

            left.assign(ExecutionPlace::argument(SymbolOrdinal::new(alias)),
                ExecutionCondition::reference_value(support.clone().borrowed(), data), None);
        }

        let mut right = left.clone();
        let guarantee = condition(input(0));

        left.carry_guarantee(input(0), guarantee.clone());
        guarantee.assume(true, &mut left.trusted_assumptions);

        let place = ExecutionPlace::argument(SymbolOrdinal::new(7));

        left.assign(place.clone(), input(0).present(), None);
        right.assign(place.clone(), absent.clone(), None);

        let mut merged = Some(left);

        super::super::merge::merge(&mut merged, &Some(right),
            Some(crate::analysis::id::AnalysisBlockId::from_slot(BoundUnitId::new(0), 0)).into());

        let mut merged = merged.expect("both nullable predecessors are reachable");
        let joined = place.value_in(&merged.current).expect("joined value retains its slot");

        ExecutionCondition::operation(bray_bound_tree::BoundOperator::NotEqual,
            vec![joined.clone(), absent.clone()]).assume(true, &mut merged.assumptions);

        let required = condition(joined.project(StorageProjection::NullableValue));

        assert_eq!(required.prove_trusted(&merged.trusted_assumptions, &merged.assumptions), Some(true),
            "a live extracted child must shield its observation from its partially moved parent snapshot: detailed={detailed:?}");
        }
        }
    }

    #[test]
    fn conditional_returns_keep_scalar_data_and_retire_its_authority() {
        let input = |id| ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(id))));
        let field = StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let predicate = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>("ValidWrite");
        let substitution = crate::test_support::empty_substitution(predicate.into());

        let permission = |value| ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(
            ExecutionCondition::predicate(predicate.into(), substitution, vec![value]),
        )));

        let absent = ExecutionCondition::new(crate::ExecutionConditionKind::Literal(Arc::new(bray_symbols::ConstantValueData::new(
            error_type(), bray_symbols::ConstantValueKind::NullableAbsent,
        ))));

        let (_, expressions) = expression_unit(BoundUnitId::new(0), |tree, origin| vec![
            push_expression(tree, BoundExpression::Error(BoundErrorExpression::new(origin, error_type()))),
        ]);

        for (guarded, materialized) in [(false, false), (true, false), (false, true), (true, true)] {
            for valid in [false, true] {
                let choice = input(3);

                let value = if materialized { ExecutionCondition::constructed(input(0), [(field,
                    ExecutionCondition::opaque_scalar(input(0).project(field)))].into()) } else { input(0) };

                let copied = ExecutionCondition::scalar_observation(input(4), value.clone().project(field));
                let mut present = ExecutionState::default();
                let mut missing = ExecutionState::default();

                for state in [&mut present, &mut missing] {
                    state.current.insert(ExecutionPlace::argument(SymbolOrdinal::new(4)), copied.clone());
                }

                choice.clone().assume(true, &mut present.assumptions);
                choice.clone().assume(false, &mut missing.assumptions);

                let owner = input(u32::from(!valid));
                let claim = permission(if valid { value.clone().project(field) } else { owner.clone().project(field) });

                let claim = if guarded { ExecutionCondition::operation(
                    bray_bound_tree::BoundOperator::LogicalOr,
                    vec![ExecutionCondition::operation(bray_bound_tree::BoundOperator::LogicalNot, vec![choice]), claim],
                ) } else { claim };

                present.carry_guarantee(owner, claim.clone());
                claim.assume(true, &mut present.trusted_assumptions);

                let joined = merge_returned(&mut present, &missing, &value.present(), &absent,
                    &ExecutionCondition::result(), expressions[0]);

                let required = permission(copied);

                let prove = |state: &ExecutionState| {
                    let aliases = state.proofs.equalities(&state.assumptions, None);

                    let trusted = state.trusted_assumptions.iter().map(|(condition, holds)|
                        (condition.with_equalities(&aliases), *holds)).collect();

                    required.with_equalities(&aliases).prove_trusted(&trusted, &state.assumptions)
                };

                assert_ne!(prove(&present), Some(true));

                ExecutionCondition::operation(bray_bound_tree::BoundOperator::NotEqual,
                    vec![joined.clone(), absent.clone()]).assume(true, &mut present.assumptions);

                assert_eq!(prove(&present) == Some(true), valid,
                    "guarded={guarded} valid={valid} facts={:?} ordinary={:?}", present.trusted_assumptions, present.assumptions);

                present.invalidate_trusted(&joined.project(StorageProjection::NullableValue));
                assert_ne!(prove(&present), Some(true));
            }
        }
    }

    #[test]
    fn nullable_whole_carrier_claims_survive_only_the_present_branch() {
        let input = |id| ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(id))));
        let predicate = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>("ValidWrite");
        let substitution = crate::test_support::empty_substitution(predicate.into());

        let held = |owner: ExecutionCondition| ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(
            ExecutionCondition::predicate(predicate.into(), substitution, vec![owner.borrowed()]),
        )));

        let absent = ExecutionCondition::new(crate::ExecutionConditionKind::Literal(Arc::new(bray_symbols::ConstantValueData::new(
            error_type(), bray_symbols::ConstantValueKind::NullableAbsent,
        ))));

        let (_, expressions) = expression_unit(BoundUnitId::new(0), |tree, origin| vec![
            push_expression(tree, BoundExpression::Error(BoundErrorExpression::new(origin, error_type()))),
        ]);

        for (valid, known_present) in [true, false].into_iter().flat_map(|valid|
            [true, false].map(move |known| (valid, known))) {
            let nullable = |owner: ExecutionCondition| if known_present { owner.present() } else { owner };
            let present = nullable(input(0));
            let claim = held(if valid { present.clone() } else { nullable(input(1)) });
            let mut left = ExecutionState::default();

            left.carry_guarantee(present.clone(), claim.clone());
            claim.assume(true, &mut left.trusted_assumptions);

            let inactive = ExecutionCondition::constructed(absent.clone(),
                [(StorageProjection::NullableValue, ExecutionCondition::unknown())].into());

            let joined = merge_returned(&mut left, &ExecutionState::default(), &present, &inactive,
                &ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expressions[0])), expressions[0]);

            let required = held(joined.clone());

            assert_ne!(required.prove_trusted(&left.trusted_assumptions, &left.assumptions), Some(true));

            ExecutionCondition::operation(bray_bound_tree::BoundOperator::NotEqual,
                vec![joined, absent.clone()]).assume(true, &mut left.assumptions);

            assert_eq!(required.prove_trusted(&left.trusted_assumptions, &left.assumptions) == Some(true), valid,
                "valid={valid} known_present={known_present} required={required:?} facts={:?} witnesses={:?}", left.trusted_assumptions, left.witnesses);
        }
    }

    #[test]
    fn nested_variant_witness_joins_are_independent_of_predecessor_order() {
        let input = |id| ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(id))));
        let variant = |id| UnionVariantSymbolId::from_symbol_id(SymbolId::new(id));
        let active = variant(920);
        let inactive = variant(921);

        let field = StorageProjection::ActiveUnionPayloadField {
            variant: active, field: UnionPayloadFieldSymbolId::from_symbol_id(SymbolId::new(922)),
        };

        let marker = StorageProjection::TupleElement(SymbolOrdinal::new(0));

        let empty = ExecutionCondition::new(crate::ExecutionConditionKind::Literal(Arc::new(bray_symbols::ConstantValueData::new(
            error_type(), bray_symbols::ConstantValueKind::Union { variant: inactive, fields: [].into() },
        ))));

        let predicate = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>("ValidWrite");
        let substitution = crate::test_support::empty_substitution(predicate.into());

        let held = |owner: ExecutionCondition| ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(
            ExecutionCondition::predicate(predicate.into(), substitution, vec![owner.project(marker).borrowed()]),
        )));

        let absent = ExecutionCondition::new(crate::ExecutionConditionKind::Literal(Arc::new(bray_symbols::ConstantValueData::new(
            error_type(), bray_symbols::ConstantValueKind::NullableAbsent,
        ))));

        let (_, expressions) = expression_unit(BoundUnitId::new(0), |tree, origin| (0..2).map(|_| {
            push_expression(tree, BoundExpression::Error(BoundErrorExpression::new(origin, error_type())))
        }).collect());

        for nullable_outer in [true, false] {
        let alternatives = if nullable_outer { [
            ExecutionCondition::constructed(ExecutionCondition::new(crate::ExecutionConditionKind::Tagged(active, Arc::new(input(1)))),
                [(field, input(0))].into()).present(),
            empty.clone().present(),
            absent.clone(),
        ] } else { [
            ExecutionCondition::constructed(ExecutionCondition::new(crate::ExecutionConditionKind::Tagged(active, Arc::new(input(1)))),
                [(field, input(0).present())].into()),
            ExecutionCondition::constructed(ExecutionCondition::new(crate::ExecutionConditionKind::Tagged(active, Arc::new(input(1)))),
                [(field, absent.clone())].into()),
            empty.clone(),
        ] };

        for (order, valid, same_region) in [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]]
            .into_iter().flat_map(|order| [true, false].into_iter().flat_map(move |valid|
                [false, true].map(move |same_region| (order, valid, same_region))))
        {
            let mut states = [ExecutionState::default(), ExecutionState::default(), ExecutionState::default()];
            let owner = input(if valid { 0 } else { 3 });
            let guarantee = held(owner.clone());

            states[0].carry_guarantee(owner, guarantee.clone());
            guarantee.assume(true, &mut states[0].trusted_assumptions);

            let [first, second, third] = order;

            let mut inner = states[second].clone();

            let inner_value = merge_returned(&mut inner, &states[third], &alternatives[second], &alternatives[third],
                &ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expressions[0])), expressions[0]);

            let mut outer = states[first].clone();
            let expression = expressions[usize::from(!same_region)];

            let joined = merge_returned(&mut outer, &inner, &alternatives[first], &inner_value,
                &ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expression)), expression);

            let (nullable, tagged, payload) = if nullable_outer {
                let tagged = joined.clone().project(StorageProjection::NullableValue);

                (joined.clone(), tagged.clone(), tagged.project(field))
            } else {
                let nullable = joined.clone().project(field);

                (nullable.clone(), joined.clone(), nullable.project(StorageProjection::NullableValue))
            };

            let required = held(payload);

            assert_ne!(required.prove_trusted(&outer.trusted_assumptions, &outer.assumptions), Some(true),
                "an unselected payload cannot expose authority");

            ExecutionCondition::operation(bray_bound_tree::BoundOperator::NotEqual,
                vec![nullable, absent.clone()]).assume(true, &mut outer.assumptions);

            tagged.matches_variant(active).assume(true, &mut outer.assumptions);

            assert_eq!(required.prove_trusted(&outer.trusted_assumptions, &outer.assumptions) == Some(true), valid,
                "nullable_outer={nullable_outer} order={order:?} valid={valid} same_region={same_region} required={required:?} facts={:?} witnesses={:?}",
                outer.trusted_assumptions, outer.witnesses);
        }
        }
    }

    #[test]
    fn mixed_copy_and_owned_predecessors_keep_only_guarded_join_authority() {
        let owner = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(0))));
        let guard = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(1))));
        let copied = ExecutionCondition::scalar_observation(owner.clone(), owner.clone());
        let predicate = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>("ValidWrite");
        let substitution = crate::test_support::empty_substitution(predicate.into());

        let held = |owner: ExecutionCondition| ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(
            ExecutionCondition::predicate(predicate.into(), substitution, vec![owner.borrowed()]),
        )));

        let (_, expressions) = expression_unit(BoundUnitId::new(0), |tree, origin| vec![
            push_expression(tree, BoundExpression::Error(BoundErrorExpression::new(origin, error_type()))),
        ]);

        let destination = ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expressions[0]));
        let mut left = ExecutionState::default();
        let condition = held(owner.clone());

        left.carry_guarantee(owner.clone(), condition.clone());
        condition.assume(true, &mut left.trusted_assumptions);

        let mut right = left.clone();

        guard.clone().assume(true, &mut left.assumptions);
        guard.clone().assume(false, &mut right.assumptions);

        let joined = merge_returned(&mut left, &right, &owner, &copied, &destination, expressions[0]);

        assert_eq!(joined.ownership_identity(), Some((crate::execution_guarantees::ExecutionObservationKind::Owned, &destination)));
        assert!(!owner.is_owned_projection_of(&joined));

        let required = held(joined);

        assert_ne!(required.prove_trusted(&left.trusted_assumptions, &left.assumptions), Some(true));

        guard.assume(true, &mut left.assumptions);

        assert_eq!(required.prove_trusted(&left.trusted_assumptions, &left.assumptions), Some(true));
    }

    #[test]
    fn nested_payload_authority_includes_the_outer_active_case() {
        let variant = |id| UnionVariantSymbolId::from_symbol_id(SymbolId::new(id));
        let field = |id| UnionPayloadFieldSymbolId::from_symbol_id(SymbolId::new(id));
        let input = |id| ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(id))));
        let outer = variant(900);
        let rejected = variant(901);
        let inner = variant(902);
        let outer_field = StorageProjection::ActiveUnionPayloadField { variant: outer, field: field(903) };
        let inner_field = StorageProjection::ActiveUnionPayloadField { variant: inner, field: field(904) };
        let definition = bray_symbols::FunctionSymbolId::from_symbol_id(SymbolId::new(905));
        let callable = crate::test_support::callable_instance(definition.into());
        let predicate = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>("ValidWrite");

        let held = |owner: ExecutionCondition| ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(
            ExecutionCondition::predicate(predicate.into(), callable.substitution(), vec![owner.borrowed()]),
        )));

        let (_, expressions) = expression_unit(BoundUnitId::new(0), |tree, origin| vec![
            push_expression(tree, BoundExpression::Error(BoundErrorExpression::new(origin, error_type()))),
        ]);

        for valid in [true, false] {
            let marker = input(0);

            let payload = ExecutionCondition::constructed(ExecutionCondition::new(crate::ExecutionConditionKind::Tagged(inner, Arc::new(input(1)))),
                [(inner_field, marker.clone())].into(),
            );

            let accepted = ExecutionCondition::constructed(ExecutionCondition::new(crate::ExecutionConditionKind::Tagged(outer, Arc::new(input(2)))),
                [(outer_field, payload)].into(),
            );

            let failure = ExecutionCondition::new(crate::ExecutionConditionKind::Tagged(rejected, Arc::new(input(3))));
            let mut left = ExecutionState::default();
            let owner = if valid { marker } else { input(4) };
            let guarantee = held(owner.clone());

            left.carry_guarantee(owner, guarantee.clone());
            guarantee.assume(true, &mut left.trusted_assumptions);

            let joined = merge_returned(
                &mut left, &ExecutionState::default(), &accepted, &failure,
                &ExecutionCondition::result(), expressions[0],
            );

            let active = ExecutionCondition::operation(bray_bound_tree::BoundOperator::LogicalAnd, vec![
                joined.clone().matches_variant(outer), joined.clone().project(outer_field).matches_variant(inner),
            ]);

            active.assume(true, &mut left.assumptions);

            let required = held(joined.project(outer_field).project(inner_field));

            assert_eq!(required.prove_trusted(&left.trusted_assumptions, &left.assumptions) == Some(true), valid);
        }
    }

    #[test]
    fn retired_source_snapshot_does_not_pin_a_moved_payload_owner() {
        let variant = |id| UnionVariantSymbolId::from_symbol_id(SymbolId::new(id));
        let active = variant(910);
        let empty = variant(911);

        let field = StorageProjection::ActiveUnionPayloadField {
            variant: active,
            field: UnionPayloadFieldSymbolId::from_symbol_id(SymbolId::new(912)),
        };

        let source_place = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let source = ExecutionCondition::new(crate::ExecutionConditionKind::Input(source_place.clone()));
        let predicate = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>("ValidWrite");
        let substitution = crate::test_support::empty_substitution(predicate.into());

        let held = |owner: ExecutionCondition| ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(
            ExecutionCondition::predicate(predicate.into(), substitution, vec![owner.borrowed()]),
        )));

        let (_, expressions) = expression_unit(BoundUnitId::new(0), |tree, origin| vec![
            push_expression(tree, BoundExpression::Error(BoundErrorExpression::new(origin, error_type()))),
        ]);

        let mut left = ExecutionState::default();

        left.current.insert(source_place.clone(), source.clone());
        left.current.retire(&source_place);

        let right = left.clone();
        let condition = held(source.clone());

        left.carry_guarantee(source.clone(), condition.clone());
        condition.assume(true, &mut left.trusted_assumptions);

        let accepted = ExecutionCondition::constructed(
            ExecutionCondition::new(crate::ExecutionConditionKind::Tagged(active, Arc::new(ExecutionCondition::new(crate::ExecutionConditionKind::Input(
                ExecutionPlace::argument(SymbolOrdinal::new(1)),
            ))))),
            [(field, source)].into(),
        );

        let rejected = ExecutionCondition::new(crate::ExecutionConditionKind::Tagged(empty, Arc::new(ExecutionCondition::new(crate::ExecutionConditionKind::Input(
            ExecutionPlace::argument(SymbolOrdinal::new(2)),
        )))));

        let joined = merge_returned(&mut left, &right, &accepted, &rejected,
            &ExecutionCondition::result(), expressions[0]);

        joined.clone().matches_variant(active).assume(true, &mut left.assumptions);

        assert_eq!(held(joined.project(field)).prove_trusted(
            &left.trusted_assumptions, &left.assumptions,
        ), Some(true));
    }

    #[test]
    fn unchanged_owned_components_shield_ancestor_renaming() {
        let root = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(0))));
        let changed = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(1))));
        let component = root.clone().project(StorageProjection::TupleElement(SymbolOrdinal::new(0)));
        let mut values = [(root.clone(), changed.clone())].into_iter().collect();
        let mut identities = [(root, changed)].into_iter().collect();

        super::super::merge::result_replacements(&component, &component,
            &Default::default(), &Default::default(), &mut values, &mut identities);

        assert_eq!(component.with_joined_values(&values, &identities), component);

        let field = StorageProjection::TupleElement(SymbolOrdinal::new(1));
        let nested = component.clone().project(field);

        for unchanged in [true, false] {
            let changed = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(2))));
            let current = if unchanged { nested.clone() } else { changed.clone().project(field) };

            let joined = ExecutionCondition::owned_value(component.clone(), ExecutionCondition::constructed(
                changed, [(field, current)].into(),
            ));

            let mut values = Default::default();
            let mut identities = Default::default();

            super::super::merge::result_replacements(&component, &joined,
                &Default::default(), &Default::default(), &mut values, &mut identities);

            assert_eq!(nested.with_joined_values(&values, &identities) == nested, unchanged,
                "implicit retained field unchanged={unchanged}");
        }
    }
}
