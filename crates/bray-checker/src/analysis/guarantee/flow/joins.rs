use std::collections::{BTreeMap, BTreeSet};

use super::super::state::ExecutionState;
use super::merge::result_replacements;
use crate::ExecutionCondition;
use crate::execution_guarantees::common_components;

pub(in crate::analysis::guarantee) fn returned_condition(
    condition: &ExecutionCondition,
    result: &ExecutionCondition,
) -> ExecutionCondition {
    let mut values = BTreeMap::new();
    let mut identities = BTreeMap::new();

    result_replacements(
        result,
        &ExecutionCondition::Result,
        &Default::default(),
        &Default::default(),
        &mut values,
        &mut identities,
    );

    condition.with_joined_values(&values, &identities)
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
        (&target.assumptions, &incoming.assumptions),
        &mut facts,
        &mut { ExecutionCondition::WORK_LIMIT },
    );

    let mut left = BTreeMap::new();
    let mut right = BTreeMap::new();
    let mut left_ids = BTreeMap::new();
    let mut right_ids = BTreeMap::new();
    let mut stable_values = BTreeSet::new();
    let mut stable_identities = BTreeSet::new();

    for (place, value) in &target.current {
        let Some(other) = place.value_in(&incoming.current) else {
            continue;
        };

        if value.owner_identity() == other.owner_identity() {
            stable_identities.insert(value.owner_identity().clone());
        }

        if value == &other {
            stable_values.insert(value.clone());
            stable_values.insert(value.clone().borrowed());
        }
    }

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

    let conditions = super::witnesses::conditional_guarantees(
        (target, &left, &left_ids),
        (&incoming, &right, &right_ids),
        &BTreeMap::new(),
        &[(expression, joined.clone())],
    );

    let ownership = super::witnesses::owned_alternatives(
        (target, left_value),
        (&incoming, right_value),
        &joined,
    );

    target.join_values(&left, &left_ids);
    incoming.join_values(&right, &right_ids);
    super::witnesses::retain_conditional_guarantees(target, &mut incoming, &conditions);

    for (condition, holds) in facts {
        condition.clone().assume(holds, &mut target.assumptions);
        condition.assume(holds, &mut incoming.assumptions);
    }

    let mut completed = Some(target.clone());

    super::merge::merge(&mut completed, &Some(incoming), None);
    *target = completed.expect("reachable normal return retains a state");
    target.pending_results.remove(&expression);

    if ownership.is_empty() {
        target.owned_results.remove(&joined);
    } else {
        target.owned_results.insert(joined.clone(), ownership);
    }

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
    fn refreshing_and_joining_a_loan_preserves_its_context_without_owning_contents() {
        let owner = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
        let field = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0));
        let original = owner.with_component(&[field], ExecutionCondition::Boolean(true), None);
        let loan = original.clone().borrowed();
        let changed = original.with_component(&[field], ExecutionCondition::Boolean(false), None);
        let refreshed = loan.with_referent(changed.clone());

        assert_eq!(refreshed.owner_identity(), loan.owner_identity());
        assert_eq!(refreshed.clone().component(field), ExecutionCondition::Boolean(false));
        assert_eq!(loan.clone().component(field), ExecutionCondition::Boolean(true));
        assert_eq!(refreshed.clone().borrowed(), refreshed);
        assert_eq!(refreshed.owned_components().count(), 0);
        assert_eq!(refreshed.clone().into_referent(), changed);

        let observation = |value| ExecutionCondition::operation(
            bray_bound_tree::BoundOperator::Equal,
            vec![value, ExecutionCondition::Result],
        );

        assert_eq!(observation(refreshed.clone()), observation(changed.borrowed()));
        assert_ne!(observation(refreshed.clone()), observation(loan.clone()));

        let joined = join_components(&loan, &refreshed);

        assert_eq!(joined.owner_identity(), loan.owner_identity());
        assert_eq!(joined.owned_components().count(), 0);
        assert_ne!(joined.clone().component(field), ExecutionCondition::Boolean(true));
        assert_ne!(joined.component(field), ExecutionCondition::Boolean(false));
    }

    #[test]
    fn joins_keep_matching_nested_observations_without_inventing_identity() {
        let field = StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let component = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0));
        let place = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let identity = ExecutionCondition::Input(place.clone());
        let marker = ExecutionCondition::Boolean(true);

        let nested = |version| {
            ExecutionCondition::constructed(ExecutionCondition::Joined(
                    BoundUnitId::new(0),
                    version,
                    place.clone(),
                ),
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
            joined.clone().component(component).component(component),
            marker
        );

        let different = owner(
            ExecutionCondition::constructed(ExecutionCondition::Unknown,
                [(field, ExecutionCondition::Boolean(false))].into(),
            )
            .borrowed(),
        );

        assert_eq!(
            join_components(&joined, &different)
                .component(component)
                .component(component),
            ExecutionCondition::Unknown
        );
    }

    #[test]
    fn joined_contents_keep_the_descriptor_and_a_distinct_current_identity() {
        use crate::analysis::guarantee::state::ExecutionState;
        use crate::analysis::id::AnalysisBlockId;

        let place = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let descriptor = ExecutionCondition::Input(place.clone());
        let field = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0));
        let current = ExecutionCondition::Joined(BoundUnitId::new(0), 2, place.clone());
        let conditions = Default::default();
        let symbol = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>("ValidRead");
        let substitution = crate::test_support::empty_substitution(symbol.into());

        let guarantee = |value: ExecutionCondition| ExecutionCondition::Trusted(Arc::new(
            ExecutionCondition::predicate(symbol.into(), substitution, vec![value.component(field).borrowed()]),
        ));

        let branch = |version| ExecutionCondition::current_value(
            descriptor.clone(),
            ExecutionCondition::Joined(BoundUnitId::new(0), version, place.clone()),
        );

        for (borrowed, unchanged, component_carrier) in [false, true].into_iter()
            .flat_map(|borrowed| [false, true].into_iter().flat_map(move |unchanged|
                [false, true].map(move |component| (borrowed, unchanged, component))))
        {
            let context = |value: ExecutionCondition| if borrowed {
                descriptor.clone().borrowed().with_referent(value)
            } else { value };

            let left = context(if unchanged { descriptor.clone() } else { branch(0) });
            let right = context(branch(1));

            let joined = common_components(
                &left, &right, Some(current.clone()), (&conditions, &conditions),
                &mut Vec::new(), &mut { ExecutionCondition::WORK_LIMIT },
            );

            assert_eq!(joined.owner_identity(), left.owner_identity());
            assert_eq!(joined.clone().component(field), current.clone().component(field));
            assert_ne!(joined.clone().component(field), descriptor.clone().component(field));

            if borrowed { assert_eq!(joined.owned_components().count(), 0); }

            let state = |value: ExecutionCondition| {
                let mut state = ExecutionState::default();
                let condition = guarantee(value.clone());

                state.current.insert(place.clone(), value.clone());
                condition.clone().assume(true, &mut state.trusted_assumptions);

                let carrier = if component_carrier {
                    value.clone().component(field)
                } else { value.clone().into_referent() };

                state.carry_guarantee(carrier, condition);

                state
            };

            let mut merged = Some(state(left));

            super::super::merge::merge(
                &mut merged, &Some(state(right)), Some(AnalysisBlockId::from_slot(BoundUnitId::new(0), 2)),
            );

            let merged = merged.expect("both current observations remain reachable");
            let condition = guarantee(merged.current[&place].clone());

            assert_eq!(condition.prove_trusted(&merged.trusted_assumptions, &merged.assumptions), Some(true),
                "borrowed={borrowed} unchanged={unchanged} component={component_carrier} facts={:?} witnesses={:?}",
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
        let value = ExecutionCondition::Input(original.clone());
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
            Some(AnalysisBlockId::from_slot(BoundUnitId::new(0), 0)),
        );

        let merged = merged.expect("both predecessors are reachable");

        assert_eq!(merged.current[&original], value);
        assert_eq!(merged.current[&alias], ExecutionCondition::Unknown);
        assert!(merged.assumptions.contains(&(value, true)));
    }
    #[test]
    fn returned_branches_keep_shared_fields_in_the_whole_snapshot() {
        use crate::analysis::guarantee::state::ExecutionState;

        let owner = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let marker = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0));
        let marker_place = owner.clone().component(marker);
        let shared = ExecutionCondition::Input(marker_place.clone());
        let mut left = ExecutionState::default();
        let mut right = ExecutionState::default();

        left.assign(
            owner.clone(),
            ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1))),
            None,
        );

        left.assign(marker_place.clone(), shared.clone(), None);

        right.assign(
            owner.clone(),
            ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(2))),
            None,
        );

        right.assign(marker_place.clone(), shared.clone(), None);

        let mut merged = Some(left);

        super::super::merge::merge(&mut merged, &Some(right), None);

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

        let value = ExecutionCondition::Input(place.clone());

        let guard = ExecutionCondition::Input(
            bray_bound_tree::BoundReferenceTarget::Surface(
                bray_symbols::CallableParameterSymbolId::from_symbol_id(
                    bray_symbols::SymbolId::new(1),
                )
                .into(),
            )
            .into(),
        );

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
                Some(AnalysisBlockId::from_slot(BoundUnitId::new(0), 0)),
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

        let guard = ExecutionCondition::Input(
            bray_bound_tree::BoundReferenceTarget::Surface(
                bray_symbols::CallableParameterSymbolId::from_symbol_id(
                    bray_symbols::SymbolId::new(1),
                )
                .into(),
            )
            .into(),
        );

        let mut state = ExecutionState::default();

        state
            .current
            .insert(place.clone(), ExecutionCondition::Boolean(false));

        let mut target = Some(state);

        for _ in 0..16 {
            let mut incoming = target.clone().expect("reachable loop header");

            incoming
                .current
                .insert(place.clone(), ExecutionCondition::Boolean(true));

            incoming.assumptions.insert((guard.clone(), true));

            super::super::merge::merge(
                &mut target,
                &Some(incoming),
                Some(AnalysisBlockId::from_slot(BoundUnitId::new(0), 0)),
            );

            assert!(
                target
                    .as_ref()
                    .expect("reachable loop header")
                    .owned_results
                    .is_empty()
            );
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
    fn nested_payload_authority_includes_the_outer_active_case() {
        let variant = |id| UnionVariantSymbolId::from_symbol_id(SymbolId::new(id));
        let field = |id| UnionPayloadFieldSymbolId::from_symbol_id(SymbolId::new(id));
        let input = |id| ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(id)));
        let outer = variant(900);
        let rejected = variant(901);
        let inner = variant(902);
        let outer_field = StorageProjection::ActiveUnionPayloadField { variant: outer, field: field(903) };
        let inner_field = StorageProjection::ActiveUnionPayloadField { variant: inner, field: field(904) };
        let definition = bray_symbols::FunctionSymbolId::from_symbol_id(SymbolId::new(905));
        let callable = crate::test_support::callable_instance(definition.into());
        let predicate = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>("ValidWrite");

        let held = |owner: ExecutionCondition| ExecutionCondition::Trusted(Arc::new(
            ExecutionCondition::predicate(predicate.into(), callable.substitution(), vec![owner.borrowed()]),
        ));

        let (_, expressions) = expression_unit(BoundUnitId::new(0), |tree, origin| vec![
            push_expression(tree, BoundExpression::Error(BoundErrorExpression::new(origin, error_type()))),
        ]);

        for valid in [true, false] {
            let marker = input(0);

            let payload = ExecutionCondition::constructed(ExecutionCondition::Tagged(inner, Arc::new(input(1))),
                [(inner_field, marker.clone())].into(),
            );

            let accepted = ExecutionCondition::constructed(ExecutionCondition::Tagged(outer, Arc::new(input(2))),
                [(outer_field, payload)].into(),
            );

            let failure = ExecutionCondition::Tagged(rejected, Arc::new(input(3)));
            let mut left = ExecutionState::default();
            let owner = if valid { marker } else { input(4) };
            let guarantee = held(owner.clone());

            left.carry_guarantee(owner, guarantee.clone());
            guarantee.assume(true, &mut left.trusted_assumptions);

            let joined = merge_returned(
                &mut left, &ExecutionState::default(), &accepted, &failure,
                &ExecutionCondition::Result, expressions[0],
            );

            let active = ExecutionCondition::operation(bray_bound_tree::BoundOperator::LogicalAnd, vec![
                joined.clone().matches_variant(outer), joined.clone().project(outer_field).matches_variant(inner),
            ]);

            active.assume(true, &mut left.assumptions);

            let required = held(joined.project(outer_field).project(inner_field));

            assert_eq!(required.prove_trusted(&left.trusted_assumptions, &left.assumptions) == Some(true), valid);
        }
    }
}
