use std::collections::BTreeMap;

use bray_bound_tree::{StorageMutation, StorageProjection};

use crate::{ExecutionCondition, ExecutionPlace};

/// The checked physical-survival mask for one actual storage value. It holds
/// no object data or evidence and is applied to the retained source once.
#[derive(Default)]
pub(super) struct ValuePreservation {
    pub(super) value: bool,
    pub(super) descriptor: bool,
    pub(super) discriminant: bool,
    children: BTreeMap<StorageProjection, Self>,
}

impl ValuePreservation {
    pub(super) fn at_mut(&mut self, path: &[StorageProjection]) -> &mut Self {
        path.iter().fold(self, |node, projection| node.children.entry(*projection).or_default())
    }

    pub(super) fn at(&self, path: &[StorageProjection]) -> Option<&Self> {
        let mut node = self;

        for projection in path {
            if node.value { return Some(node); }
            node = node.children.get(projection)?;
        }

        Some(node)
    }

    /// A mutation produces unknown new contents and carries only actual source
    /// nodes selected by physical survival. It never recovers data from witnesses,
    /// source syntax, another slot or a marker constructed after the operation.
    pub(super) fn apply(
        &self,
        source: &ExecutionCondition,
        place: Option<&ExecutionPlace>,
        mutation: StorageMutation,
    ) -> ExecutionCondition {
        self.apply_bounded(source, place, mutation, &mut { ExecutionCondition::WORK_LIMIT })
    }

    fn apply_bounded(
        &self,
        source: &ExecutionCondition,
        place: Option<&ExecutionPlace>,
        mutation: StorageMutation,
        budget: &mut usize,
    ) -> ExecutionCondition {
        let Some(remaining) = budget.checked_sub(1) else {
            return ExecutionCondition::Unknown;
        };

        *budget = remaining;

        if self.value { return source.clone(); }

        if self.descriptor
            && let Some(referent) = match source {
                ExecutionCondition::Borrowed(referent)
                | ExecutionCondition::Observation { contents: referent, kind: crate::execution_guarantees::ExecutionObservationKind::Reference, .. } => Some(referent.as_ref()),
                _ => None,
            }
        {
            // Frame reached data before retaining the loan descriptor. Its
            // fields keep their actual values rather than projected field loans.
            return source.with_referent(self.apply_bounded(referent, place, mutation, budget));
        }

        if self.descriptor && let Some((context, copied)) = source.storage_contents() {
            // The private copy context is not the descriptor it contains. Update
            // the copied descriptor's reached data without replacing its origin.
            return ExecutionCondition::scalar_observation(
                context.clone(),
                self.apply_bounded(copied, place, mutation, budget),
            );
        }

        let reached = source.clone().into_referent();
        let identity = if reached.ownership_identity().is_some() || reached.storage_contents().is_some() {
            reached.owner_identity().clone()
        } else {
            place.cloned().map_or(ExecutionCondition::Unknown, ExecutionCondition::Input)
        };
        let mut contents = if self.discriminant {
            source.discriminant()
        } else { ExecutionCondition::post_state(mutation, identity) };

        for (projection, preservation) in &self.children {
            let child = source.clone().project(*projection);
            let child_place = place.and_then(|place| place.clone().project(&[*projection]));
            let next = preservation.apply_bounded(&child, child_place.as_ref(), mutation, budget);

            contents = contents.with_storage_component(&[*projection], next);
        }

        // An in-place contents update retains its existing owned context.
        if place.is_some() && (self.descriptor
            || source.observation_kind() == Some(crate::execution_guarantees::ExecutionObservationKind::Owned)) {
            return source.with_current_contents(contents);
        }

        contents
    }
}

#[cfg(test)]
mod tests {
    use super::ValuePreservation;
    use crate::test_support::{expression_unit, integer_literal_expression, push_expression};
    use crate::{ExecutionCondition, ExecutionPlace};
    use bray_bound_tree::{BoundExecutionSite, BoundUnitId, StorageMutation, StorageProjection};
    use bray_symbols::SymbolOrdinal;

    #[test]
    fn retained_discriminants_keep_tags_without_payload_data_or_authority() {
        use bray_bound_tree::BoundOperator;
        use bray_symbols::{ConstantValueData, ConstantValueKind};
        use std::sync::Arc;

        let place = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let owner = ExecutionCondition::Input(place.clone());
        let absent = ExecutionCondition::Literal(Arc::new(ConstantValueData::new(crate::test_support::error_type(), ConstantValueKind::NullableAbsent)));
        let payload = bray_bound_tree::StorageProjection::NullableValue;
        let (_, sites) = expression_unit(BoundUnitId::new(0), |tree, origin| {
            vec![push_expression(tree, integer_literal_expression(origin, None))]
        });
        let mutation = StorageMutation::Operation(sites[0].into());
        let mask = ValuePreservation { discriminant: true, ..Default::default() };
        let presence = |value| ExecutionCondition::operation(BoundOperator::NotEqual, vec![value, absent.clone()]);

        for source in [absent.clone(), owner.clone().present(), owner.clone()] {
            let updated = mask.apply(&source, Some(&place), mutation);

            assert_eq!(presence(updated.clone()), presence(source.clone()));
            assert_ne!(updated.clone().project(payload), owner);
            assert!(super::super::super::state::ExecutionState::default().owned_values(&updated)
                .all(|value| value.ownership_identity().is_none()));
            assert_eq!(mask.apply(&updated, Some(&place), mutation), updated);
            assert_ne!(presence(ValuePreservation::default().apply(&source, Some(&place), mutation)), presence(source));
        }

        let variant = bray_symbols::UnionVariantSymbolId::from_symbol_id(bray_symbols::SymbolId::new(902));

        for source in [ExecutionCondition::Tagged(variant, Arc::new(owner.clone())), owner] {
            let updated = mask.apply(&source, Some(&place), mutation);

            assert_eq!(updated.clone().matches_variant(variant), source.clone().matches_variant(variant));
            assert_eq!(updated.known_variant(), source.known_variant());
            assert!(super::super::super::state::ExecutionState::default().owned_values(&updated)
                .all(|value| value.ownership_identity().is_none()));
            assert_eq!(mask.apply(&updated, Some(&place), mutation), updated);
        }

    }

    #[test]
    fn framing_a_lifted_payload_does_not_claim_the_whole_nullable_input() {
        use bray_symbols::{DependencyOwnedResult, DependencyOwnedTransfer};

        let place = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let input = ExecutionCondition::Input(place.clone());
        let mut state = super::super::super::state::ExecutionState::default();
        state.owned_results.insert(input.clone(), DependencyOwnedResult::Transfer(
            DependencyOwnedTransfer::new(input.clone(), []),
        ));
        let (_, sites) = expression_unit(BoundUnitId::new(0), |tree, origin| {
            vec![push_expression(tree, integer_literal_expression(origin, None))]
        });
        let mutation = StorageMutation::Operation(sites[0].into());
        let mut mask = ValuePreservation { descriptor: true, discriminant: true, ..Default::default() };
        mask.at_mut(&[StorageProjection::NullableValue]).value = true;
        let lifted = mask.apply(&input.clone().present(), Some(&place), mutation);
        let transfers = state.owned_result(&lifted);
        let transfers = transfers.transfers().collect::<Vec<_>>();

        assert_eq!(transfers.len(), 1);
        assert_eq!(transfers[0].input(), input);
        assert_eq!(transfers[0].output(), &[StorageProjection::NullableValue]);

        let retained = mask.apply(&input, Some(&place), mutation);
        assert!(state.owned_result(&retained).transfers().any(|transfer| transfer.output().is_empty()),
            "an actual nullable input keeps its existing whole-owner context");
    }

    #[test]
    fn mutation_retains_an_existing_owned_context_without_retaining_joined_contents() {
        let place = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let owner = ExecutionCondition::Input(place.clone());
        let previous = ExecutionCondition::Joined(BoundUnitId::new(0), 1, place.clone());
        let source = ExecutionCondition::owned_value(owner.clone(), previous.clone());
        let (_, sites) = expression_unit(BoundUnitId::new(0), |tree, origin| {
            vec![push_expression(tree, integer_literal_expression(origin, None))]
        });
        let mutation = StorageMutation::Operation(sites[0].into());
        let mask = ValuePreservation::default();
        let updated = mask.apply(&source, Some(&place), mutation);

        assert_eq!(updated.ownership_identity(), source.ownership_identity());
        assert_eq!(updated.computed_value(), &ExecutionCondition::post_state(mutation, owner));
        assert_ne!(updated.computed_value(), &previous);
        assert_eq!(mask.apply(&updated, Some(&place), mutation), updated);
    }

    #[test]
    fn same_owner_mutation_observations_agree_across_alias_places_without_retaining_old_contents() {
        let input = |ordinal| ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(ordinal)));
        let owner = input(0);
        let source = owner.clone().borrowed();
        let aliases = [1, 2].map(|ordinal| ExecutionPlace::argument(SymbolOrdinal::new(ordinal)));
        let (_, sites) = expression_unit(BoundUnitId::new(0), |tree, origin| {
            (0..2).map(|_| push_expression(tree, integer_literal_expression(origin, None))).collect()
        });
        let mutation = StorageMutation::Operation(sites[0].into());
        let mask = ValuePreservation { descriptor: true, ..Default::default() };
        let predicate = bray_symbols::PredicateSymbolId::from_symbol_id(bray_symbols::SymbolId::new(900));
        let substitution = crate::test_support::empty_substitution(predicate.into());
        let condition = |value| ExecutionCondition::predicate(predicate.into(), substitution, vec![value]);
        let left = mask.apply(&source, Some(&aliases[0]), mutation);
        let right = mask.apply(&source, Some(&aliases[1]), mutation);

        assert_eq!(left.ownership_identity(), source.ownership_identity());
        assert_eq!(right.ownership_identity(), source.ownership_identity());
        assert_ne!(condition(left.clone()), condition(source.clone()), "owner retention cannot retain old contents");
        assert_ne!(condition(left.clone()), condition(mask.apply(&input(3).borrowed(), Some(&aliases[0]), mutation)),
            "distinct owners do not share a mutation observation");
        assert_ne!(condition(left.clone()), condition(mask.apply(&source, Some(&aliases[0]), StorageMutation::Operation(sites[1].into()))),
            "distinct mutations produce distinct contents");
        assert_eq!(mask.apply(&left, Some(&aliases[0]), mutation), left, "replay must stabilize");
        assert_eq!(condition(left), condition(right), "one mutated owner has one contents observation through both aliases");
    }

    #[test]
    fn preserving_a_copied_descriptor_keeps_its_value_origin_without_old_contents() {
        let place = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let origin = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)));
        let context = ExecutionCondition::Input(place.clone());
        let copied = ExecutionCondition::scalar_observation(context.clone(), origin.clone());
        let (_, sites) = expression_unit(BoundUnitId::new(0), |tree, origin| {
            vec![push_expression(tree, integer_literal_expression(origin, None))]
        });
        let mutation = StorageMutation::Operation(sites[0].into());
        let mask = ValuePreservation { descriptor: true, ..Default::default() };

        let updated = mask.apply(&copied, Some(&place), mutation);

        assert_eq!(updated.storage_contents().map(|(owner, _)| owner), Some(&context));
        assert_eq!(updated.copied_value().owner_identity(), &origin);
        assert_eq!(updated.computed_value(), &ExecutionCondition::post_state(mutation, origin));
        assert!(super::super::super::state::ExecutionState::default().owned_values(&updated)
            .all(|value| value.ownership_identity().is_none()));
    }

    #[test]
    fn repeated_effects_keep_actual_nested_owners_and_only_unchanged_data() {
        let root = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let owner = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)));
        let marker = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(2)));
        let child = StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let progress = StorageProjection::TupleElement(SymbolOrdinal::new(1));

        let mut value = ExecutionCondition::constructed(ExecutionCondition::Input(root.clone()), [
            (child, ExecutionCondition::constructed(owner.clone(), [(child, marker.clone())].into())),
            (progress, ExecutionCondition::Boolean(true)),
        ].into());

        let mut mask = ValuePreservation::default();

        mask.descriptor = true;
        mask.at_mut(&[child]).descriptor = true;
        mask.at_mut(&[child, child]).value = true;

        let (_, sites) = expression_unit(BoundUnitId::new(0), |tree, origin| {
            (0..2).map(|_| push_expression(tree, integer_literal_expression(origin, None))).collect()
        });

        for site in sites {

            value = mask.apply(&value, Some(&root), StorageMutation::Operation(BoundExecutionSite::from(site)));

            let retained = value.clone().project(child);

            assert_eq!(retained.owner_identity(), &owner);
            assert_eq!(retained.project(child), marker);
            assert_ne!(value.clone().project(progress), ExecutionCondition::Boolean(true));
        }
    }
    #[test]
    fn framing_a_loan_retains_field_data_without_storing_a_field_loan() {
        let root = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let owner = ExecutionCondition::Input(root.clone());
        let field = StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let source = owner.clone().borrowed();
        let mut mask = ValuePreservation { descriptor: true, ..Default::default() };

        mask.at_mut(&[field]).value = true;

        let (_, sites) = expression_unit(BoundUnitId::new(0), |tree, origin| {
            vec![push_expression(tree, integer_literal_expression(origin, None))]
        });
        let updated = mask.apply(&source, Some(&root), StorageMutation::Operation(sites[0].into()));
        let data = updated.clone().into_referent().project(field);

        assert_eq!(data, owner.project(field));
        assert_eq!(updated.ownership_identity(), source.ownership_identity());
    }

}
