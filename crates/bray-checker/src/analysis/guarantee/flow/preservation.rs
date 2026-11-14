use std::collections::BTreeMap;

use bray_bound_tree::{StorageMutation, StorageProjection};

use crate::{ExecutionCondition, ExecutionPlace};

/// The checked physical-survival mask for one actual storage value. It holds
/// no object data or evidence and is applied to the retained source once.
#[derive(Default)]
pub(in crate::analysis::guarantee) struct ValuePreservation {
    pub(super) value: bool,
    inline: bool,
    pub(super) descriptor: bool,
    pub(super) discriminant: bool,
    children: BTreeMap<StorageProjection, Self>,
}

impl ValuePreservation {
    pub(super) fn at_mut(&mut self, path: &[StorageProjection]) -> &mut Self {
        let mut node = self;

        for projection in path {
            if node.value { return node; }
            node = node.children.entry(*projection).or_default();
        }

        node
    }

    pub(super) fn at(&self, path: &[StorageProjection]) -> Option<&Self> {
        let mut node = self;

        for projection in path {
            if node.value { return Some(node); }
            node = node.children.get(projection)?;
        }

        Some(node)
    }

    pub(super) fn record(&mut self, kind: bray_bound_tree::StorageObservationKind) {
        use bray_bound_tree::StorageObservationKind as Kind;

        match kind {
            Kind::Identity | Kind::Reference(_) => self.descriptor = true,
            Kind::Discriminant => self.discriminant = true,
            Kind::InlineBits => self.inline = true,
            Kind::Mutable => {},
            _ => {
                self.value = true;
                self.children.clear();
            },
        }
    }

    /// Resolve portable inline preservation against the checked actual source shape.
    pub(in crate::analysis::guarantee) fn returned(
        preservation: &bray_symbols::DependencyValuePreservation<StorageProjection>,
        observations: &[(std::sync::Arc<[StorageProjection]>, bray_bound_tree::StorageObservationKind)],
    ) -> Self {
        let mut mask = Self::default();

        for (path, kind) in observations { mask.at_mut(path).record(*kind); }
        mask.restrict_contents(preservation, &mut { ExecutionCondition::WORK_LIMIT });

        mask
    }

    fn restrict_contents(
        &mut self,
        preservation: &bray_symbols::DependencyValuePreservation<StorageProjection>,
        budget: &mut usize,
    ) {
        let Some(remaining) = budget.checked_sub(1) else { *self = Self::default(); return; };

        *budget = remaining;

        for projection in preservation.children().keys() {
            // A complete ordinary value supplies unchanged children by default;
            // explicit portable overrides still replace their contents.
            self.children.entry(*projection).or_insert(Self {
                value: self.value, inline: self.inline, ..Default::default()
            });
        }

        if !preservation.preserves_value() {
            self.value = false;
            self.inline = false;
            self.descriptor = false;
            self.discriminant = false;
        }

        for (projection, child) in &mut self.children {
            child.restrict_contents(&preservation.projected(projection), budget);
        }
    }

    /// Frame the actual value and its existing ownership layout together.
    pub(super) fn frame(
        &mut self,
        state: &mut super::super::state::ExecutionState,
        source: &ExecutionCondition,
        path: &[StorageProjection],
        place: Option<&ExecutionPlace>,
        mutation: StorageMutation,
    ) -> ExecutionCondition {
        self.retain_inline_descriptors(&source.clone().into_referent(), &mut { ExecutionCondition::WORK_LIMIT });

        let source = path.iter().copied().fold(source.clone(), ExecutionCondition::project);
        let mask = self.at_mut(path);

        // Another alias can still retain the original snapshot and layout.
        let ownership = state.owned_result(&source);

        // Selection applies to this branch's existing snapshot too. An explicit
        // empty layout cannot later be inferred again from an inactive child.
        state.owned_results.insert(source.clone(), ownership.clone());

        // Current ownership survives constructor normalization independently of
        // formal input transfers. Fresh local owners have no input mapping.
        let owned_context = source.ownership_identity()
            .is_some_and(|(role, _)| role == crate::execution_guarantees::ExecutionObservationKind::Owned);

        let empty = Default::default();

        let moved = state.entries.get(&mutation.occurrence())
            .map_or(&empty, |entry| &entry.extracted_values);

        let next = mask.apply_bounded(&source, place, mutation, owned_context, moved, &mut { ExecutionCondition::WORK_LIMIT });

        if source == next { return next; }

        let ownership = ownership.try_map(
            &mut |transfer| {
                let before = transfer.output().iter().fold(source.clone(), |value, field| value.project(*field));
                let after = transfer.output().iter().fold(next.clone(), |value, field| value.project(*field));

                if before.ownership_identity().is_none()
                    || before.ownership_identity() != after.ownership_identity()
                {
                    return Ok::<_, std::convert::Infallible>(bray_symbols::DependencyOwnedResult::default());
                }

                let preservation = mask.at(transfer.output()).map_or_else(
                    bray_symbols::DependencyValuePreservation::none,
                    Self::contents_preservation,
                );

                Ok(bray_symbols::DependencyOwnedResult::Transfer(transfer.clone().with_preservation(
                    transfer.preservation().intersection(&preservation),
                )))
            },
            &mut |condition| Ok(Some(condition.clone())),
        ).expect("physical ownership preservation is infallible");

        state.owned_results.insert(next.clone(), ownership);

        next
    }

    fn retain_inline_descriptors(&mut self, source: &ExecutionCondition, budget: &mut usize) {
        let Some(remaining) = budget.checked_sub(1) else { return; };

        *budget = remaining;

        if self.value { return; }

        for projection in source.component_projections() {
            if !bray_bound_tree::StoragePath::inline_projections(&[projection]) { continue; }

            let child = source.clone().project(projection);

            if child.is_reference() {
                // A retained owner's inline reference slot keeps its descriptor.
                // Its reached contents and nested owners need separate preservation.
                if self.descriptor { self.children.entry(projection).or_default().descriptor = true; }
            } else if let Some(mask) = self.children.get_mut(&projection) {
                mask.retain_inline_descriptors(&child, budget);
            }
        }
    }

    fn contents_preservation(&self) -> bray_symbols::DependencyValuePreservation<StorageProjection> {
        bray_symbols::DependencyValuePreservation::new(self.value || self.inline, self.children.iter()
            .map(|(field, child)| (*field, child.contents_preservation())).collect())
    }

    /// A mutation produces unknown new contents and carries only actual source
    /// nodes selected by physical survival. It never recovers data from witnesses,
    /// source syntax, another slot or a marker constructed after the operation.
    pub(in crate::analysis::guarantee) fn apply(
        &self,
        source: &ExecutionCondition,
        place: Option<&ExecutionPlace>,
        mutation: StorageMutation,
    ) -> ExecutionCondition {
        self.apply_bounded(source, place, mutation, false, &Default::default(), &mut { ExecutionCondition::WORK_LIMIT })
    }

    fn apply_bounded(
        &self,
        source: &ExecutionCondition,
        place: Option<&ExecutionPlace>,
        mutation: StorageMutation,
        owned_context: bool,
        moved: &std::collections::BTreeSet<ExecutionCondition>,
        budget: &mut usize,
    ) -> ExecutionCondition {
        let Some(remaining) = budget.checked_sub(1) else {
            return ExecutionCondition::Unknown;
        };

        *budget = remaining;

        let consumed = crate::execution_guarantees::ObservationSet::retains_owner(moved, source);
        let retained = self.value && !consumed;

        if !retained && self.descriptor
            && let Some(referent) = match source {
                ExecutionCondition::Borrowed(referent)
                | ExecutionCondition::Observation { contents: referent, kind: crate::execution_guarantees::ExecutionObservationKind::Reference, .. } => Some(referent.as_ref()),
                _ => None,
            }
        {
            // Frame reached data before retaining the loan descriptor. Its
            // fields keep their actual values rather than projected field loans.
            return source.with_referent(self.apply_bounded(referent, place, mutation, false, moved, budget));
        }

        if !retained && self.descriptor && let Some((context, copied)) = source.storage_contents() {
            // The private copy context is not the descriptor it contains. Update
            // the copied descriptor's reached data without replacing its origin.
            return ExecutionCondition::scalar_observation(
                context.clone(),
                self.apply_bounded(copied, place, mutation, false, moved, budget),
            );
        }

        let reached = source.clone().into_referent();

        let identity = if reached.ownership_identity().is_some() || reached.storage_contents().is_some() {
            reached.owner_identity().clone()
        } else {
            place.cloned().map_or(ExecutionCondition::Unknown, ExecutionCondition::Input)
        };

        let mut contents = if retained { source.clone() }
        else if self.discriminant && !consumed {
            source.discriminant()
        } else { ExecutionCondition::post_state(mutation, identity) };

        for (projection, preservation) in &self.children {
            let child = source.clone().project(*projection);
            let child_place = place.and_then(|place| place.clone().project(&[*projection]));
            let next = preservation.apply_bounded(&child, child_place.as_ref(), mutation, false, moved, budget);

            contents = contents.with_storage_component(&[*projection], next);
        }

        // An in-place contents update retains its existing owned context.
        // The physical slot can survive extraction, but its moved owner cannot
        // remain the identity of the replacement value.
        if !retained && place.is_some() && !consumed && (self.descriptor || owned_context
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
    fn anonymous_nullable_wrappers_and_payloads_retain_their_recorded_layout() {
        use bray_bound_tree::StorageObservationKind as Kind;
        use bray_symbols::{DependencyOwnedResult as Layout, DependencyOwnedTransfer as Transfer, DependencyValuePreservation as Mask};
        use super::super::super::state::ExecutionState;

        let place = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let input = ExecutionCondition::Input(place.clone());
        let layout = Layout::Transfer(Transfer::new(input.clone(), []).with_preservation(Mask::all()));
        let source = ExecutionCondition::Present(std::sync::Arc::new(input.clone()));
        let mut state = ExecutionState::default();

        state.owned_results.insert(input.clone(), layout.clone());

        let (_, sites) = expression_unit(BoundUnitId::new(0), |tree, origin| {
            vec![push_expression(tree, integer_literal_expression(origin, None))]
        });

        let observations: [(std::sync::Arc<[StorageProjection]>, Kind); 3] = [
            (std::sync::Arc::from([]), Kind::Discriminant),
            (std::sync::Arc::from([StorageProjection::NullableValue]), Kind::Identity),
            (std::sync::Arc::from([StorageProjection::NullableValue]), Kind::InlineBits),
        ];

        let mut mask = ValuePreservation::returned(&Mask::all(), &observations);
        let framed = mask.frame(&mut state, &source, &[], Some(&place), StorageMutation::Operation(sites[0].into()));

        assert_eq!(framed.ownership_identity(), None,
            "the nullable wrapper has no independent owner after framing its opaque payload");

        let whole = state.owned_result(&framed);
        let transfers = whole.transfers().collect::<Vec<_>>();

        assert_eq!(transfers.len(), 1);
        assert_eq!(transfers[0].input(), input);
        assert_eq!(transfers[0].output(), &[StorageProjection::NullableValue]);
        assert_eq!(transfers[0].preservation(), &Mask::all());

        let payload = framed.clone().project(StorageProjection::NullableValue);

        assert_eq!(state.owned_result(&payload), layout,
            "payload selection projects the same recorded wrapper layout");

        state.owned_results.insert(framed.clone(), Layout::default());
        assert!(state.owned_result(&framed).is_empty());

        assert!(state.owned_result(&payload).is_empty(),
            "an explicit no-transfer wrapper cannot recover an earlier payload mapping");

        let borrowed = input.borrowed();

        state.owned_results.insert(borrowed.clone(), layout);
        assert!(state.owned_result(&borrowed).is_empty());
        assert!(state.owned_result(&ExecutionCondition::Unknown).is_empty());
    }

    #[test]
    fn repeated_identity_returns_keep_current_data_and_finite_snapshots() {
        use bray_symbols::DependencyValuePreservation as Mask;

        let place = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let owner = ExecutionCondition::Input(place.clone());
        let retained = StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let changed = StorageProjection::TupleElement(SymbolOrdinal::new(1));

        let source = ExecutionCondition::constructed(owner.clone(), [
            (retained, ExecutionCondition::Boolean(true)),
            (changed, ExecutionCondition::Boolean(false)),
        ].into());

        let (_, sites) = expression_unit(BoundUnitId::new(0), |tree, origin| {
            (0..2).map(|_| push_expression(tree, integer_literal_expression(origin, None))).collect()
        });

        let changed_mask = Mask::new(true, [(changed, Mask::none())].into());

        let observations = [(std::sync::Arc::from([]), bray_bound_tree::StorageObservationKind::Value)];

        let contents = ValuePreservation::returned(&changed_mask, &observations)
            .apply(&source, Some(&place), StorageMutation::Operation(sites[0].into()));

        let mut current = ExecutionCondition::owned_value(owner.clone(), contents);
        let expected = current.clone();

        for _ in 0..16 {
            let contents = ValuePreservation::returned(&Mask::all(), &observations)
                .apply(&current, Some(&place), StorageMutation::Operation(sites[1].into()));

            current = ExecutionCondition::owned_value(owner.clone(), contents);
            assert_eq!(current, expected);
        }

        assert_eq!(current.clone().project(retained), ExecutionCondition::Boolean(true));
        assert_ne!(current.project(changed), ExecutionCondition::Boolean(false));
    }
    #[test]
    fn shallow_transport_keeps_ordinary_bits_without_freezing_protected_or_reached_data() {
        use bray_bound_tree::StorageObservationKind as Kind;
        use bray_symbols::DependencyValuePreservation as Mask;

        let root = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let input = ExecutionCondition::Input(root.clone());
        let fields = [0, 1, 2, 3].map(|index| StorageProjection::TupleElement(SymbolOrdinal::new(index)));
        let referent = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)));

        let source = ExecutionCondition::constructed(input.clone(), [
            (fields[0], ExecutionCondition::Boolean(true)),
            (fields[1], ExecutionCondition::Boolean(true)),
            (fields[2], ExecutionCondition::reference_value(referent.clone().borrowed(), ExecutionCondition::Boolean(true))),
            (fields[3], ExecutionCondition::Boolean(true)),
        ].into());

        let (_, sites) = expression_unit(BoundUnitId::new(0), |tree, origin| {
            vec![push_expression(tree, integer_literal_expression(origin, None))]
        });

        let mutation = StorageMutation::Operation(sites[0].into());
        let mut complete = ValuePreservation::default();

        complete.at_mut(&[fields[0]]).record(Kind::Identity);
        complete.record(Kind::Value);

        assert_eq!(complete.apply(&source, Some(&root), mutation), source,
            "whole-value preservation subsumes earlier child descriptor masks");

        let observations: [(std::sync::Arc<[StorageProjection]>, Kind); 5] = [
            (std::sync::Arc::from([]), Kind::Identity),
            (std::sync::Arc::from([fields[0]]), Kind::Value),
            (std::sync::Arc::from([fields[1]]), Kind::Mutable),
            (std::sync::Arc::from([fields[2]]), Kind::Reference(crate::test_support::error_type())),
            (std::sync::Arc::from([fields[3]]), Kind::Address),
        ];

        let mask = ValuePreservation::returned(&Mask::all(), &observations);
        let actual = mask.apply(&source, Some(&root), mutation);

        assert_eq!(actual.clone().project(fields[0]), ExecutionCondition::Boolean(true));
        assert_ne!(actual.clone().project(fields[1]).computed_value(), &ExecutionCondition::Boolean(true));
        assert_eq!(actual.clone().project(fields[2]).owner_identity(), source.clone().project(fields[2]).owner_identity());
        assert_ne!(actual.clone().project(fields[2]).into_referent().computed_value(), &ExecutionCondition::Boolean(true));
        assert_eq!(actual.project(fields[3]), ExecutionCondition::Boolean(true));

        let opaque = [(std::sync::Arc::from([]), Kind::InlineBits)];
        let mask = ValuePreservation::returned(&Mask::all(), &opaque);

        assert_eq!(mask.contents_preservation(), Mask::all());
        assert_ne!(mask.apply(&input, Some(&root), mutation).computed_value(), input.computed_value());

        let ordinary = [(std::sync::Arc::from([]), Kind::Value)];
        let partial = Mask::new(true, [(fields[0], Mask::none())].into());
        let actual = ValuePreservation::returned(&partial, &ordinary).apply(&source, Some(&root), mutation);

        assert_ne!(actual.clone().project(fields[0]).computed_value(), &ExecutionCondition::Boolean(true));
        assert_eq!(actual.project(fields[3]), ExecutionCondition::Boolean(true));
    }

    #[test]
    fn retained_owner_descriptors_keep_inline_reference_slots_without_freezing_referents() {
        let place = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let owner = ExecutionCondition::Input(place.clone());
        let referent = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)));
        let other = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(2)));
        let field = StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let data = StorageProjection::TupleElement(SymbolOrdinal::new(1));

        let reference = ExecutionCondition::reference_value(referent.clone().borrowed(),
            ExecutionCondition::constructed(referent, [(field, other.clone().borrowed())].into()));

        let source = ExecutionCondition::owned_value(owner.clone(), ExecutionCondition::constructed(owner,
            [(field, reference.clone()), (data, ExecutionCondition::Boolean(true))].into()));

        let (_, sites) = expression_unit(BoundUnitId::new(0), |tree, origin| {
            vec![push_expression(tree, integer_literal_expression(origin, None))]
        });

        let mutation = StorageMutation::Operation(sites[0].into());
        let mut mask = ValuePreservation { descriptor: true, ..Default::default() };
        let mut state = super::super::super::state::ExecutionState::default();
        let child_place = place.clone().project(&[field]).expect("inline reference field");
        let selected = mask.frame(&mut state, &source, &[field], Some(&child_place), mutation);

        assert_eq!(selected.ownership_identity(), reference.ownership_identity(),
            "a projected update inherits the same owner's descriptor before selecting its field");

        assert_ne!(selected.into_referent(), reference.clone().into_referent());

        for whole in [true, false] {
            let mut mask = ValuePreservation { descriptor: whole, ..Default::default() };

            if !whole { mask.at_mut(&[field]).descriptor = true; }

            let mut state = super::super::super::state::ExecutionState::default();
            let updated = mask.frame(&mut state, &source, &[], Some(&place), mutation);
            let retained = updated.clone().project(field);

            assert_eq!(retained.ownership_identity(), reference.ownership_identity(),
                "an unchanged inline reference slot keeps its actual descriptor");

            assert_ne!(retained.clone().into_referent(), reference.clone().into_referent(),
                "retaining a reference slot does not retain reached contents");

            assert_ne!(retained.into_referent().project(field).ownership_identity(), other.clone().borrowed().ownership_identity(),
                "descriptor preservation stops at the stored reference boundary");

            assert_ne!(updated.clone().project(data).computed_value(), &ExecutionCondition::Boolean(true));

            assert_eq!(mask.frame(&mut state, &updated, &[], Some(&place), mutation), updated,
                "repeated framing must stabilize");
        }
    }

    #[test]
    fn nested_owner_and_shared_alias_join_one_contents_snapshot() {
        use super::super::super::state::ExecutionState;

        let (_, sites) = expression_unit(BoundUnitId::new(0), |tree, origin| {
            (0..3).map(|_| push_expression(tree, integer_literal_expression(origin, None))).collect()
        });

        let scope = StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let marker = StorageProjection::TupleElement(SymbolOrdinal::new(1));
        let operation = StorageProjection::TupleElement(SymbolOrdinal::new(2));
        let payload = StorageProjection::NullableValue;
        let nested = ExecutionCondition::Expression(sites[0]);
        let retained_marker = nested.clone().project(operation).project(payload).project(marker);
        let container = ExecutionCondition::Expression(sites[1]);
        let source = ExecutionCondition::owned_value(container.clone(), ExecutionCondition::constructed(container, [(scope, nested)].into()));

        assert_eq!(crate::execution_guarantees::owned_snapshot(&source, source.owner_identity().clone(),
            &mut { ExecutionCondition::WORK_LIMIT }), source, "retaining an owner does not move its existing nested producers");

        let place = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let alias_place = ExecutionPlace::argument(SymbolOrdinal::new(1));
        let mut mask = ValuePreservation { descriptor: true, ..Default::default() };

        mask.at_mut(&[scope]).descriptor = true;
        mask.at_mut(&[scope, marker]).value = true;
        mask.at_mut(&[scope, operation]).discriminant = true;
        mask.at_mut(&[scope, operation]).descriptor = true;
        mask.at_mut(&[scope, operation, payload]).descriptor = true;
        mask.at_mut(&[scope, operation, payload, marker]).value = true;

        let framed = mask.apply(&source, Some(&place), StorageMutation::Operation(sites[2].into()));
        let alias = source.clone().project(scope).borrowed();
        let framed_alias = mask.children[&scope].apply(&alias, Some(&alias_place), StorageMutation::Operation(sites[2].into()));
        let predicate = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>("ValidWrite");
        let substitution = crate::test_support::empty_substitution(predicate.into());

        let absent = ExecutionCondition::Literal(std::sync::Arc::new(bray_symbols::ConstantValueData::new(
            crate::test_support::error_type(), bray_symbols::ConstantValueKind::NullableAbsent,
        )));

        let held = |value: ExecutionCondition| {
            let selected = value.project(operation);

            ExecutionCondition::operation(bray_bound_tree::BoundOperator::LogicalOr, vec![
                ExecutionCondition::operation(bray_bound_tree::BoundOperator::Equal, vec![selected.clone(), absent.clone()]),
                ExecutionCondition::Trusted(std::sync::Arc::new(ExecutionCondition::predicate(predicate.into(), substitution,
                    vec![selected.project(payload).project(marker).borrowed()]))),
            ])
        };

        let mut left = ExecutionState::default();
        let mut right = ExecutionState::default();

        for (state, value, alias) in [(&mut left, source, alias), (&mut right, framed, framed_alias)] {
            let guarantee = held(value.clone().project(scope));

            state.carry_guarantee(value.clone().project(scope).project(operation).project(payload), guarantee.clone());
            guarantee.assume(true, &mut state.trusted_assumptions);
            state.assign(place.clone(), value.clone(), None);
            state.assign(alias_place.clone(), alias, None);
            state.assign(ExecutionPlace::argument(SymbolOrdinal::new(2)), retained_marker.clone(), None);
        }

        for (state, is_absent) in [(&mut left, true), (&mut right, false)] {
            let selected = place.value_in(&state.current).expect("current predecessor").project(scope).project(operation);

            ExecutionCondition::operation(bray_bound_tree::BoundOperator::Equal, vec![selected, absent.clone()])
                .assume(is_absent, &mut state.assumptions);
        }

        let untouched = Some(left.clone());
        let mut joined = Some(left);
        let boundary = super::super::merge::ExecutionJoin::Block(crate::analysis::id::AnalysisBlockId::from_slot(BoundUnitId::new(0), 1));

        super::super::merge::merge(&mut joined, &Some(right), boundary);

        let state = joined.expect("reachable owner join");
        let owned = place.value_in(&state.current).expect("current owner").project(scope);
        let borrowed = alias_place.value_in(&state.current).expect("current shared alias").into_referent();

        assert_eq!(owned.clone().project(marker).computed_value(), borrowed.clone().project(marker).computed_value());
        assert_eq!(owned.computed_value(), borrowed.computed_value(), "the nested object has one joined contents snapshot through both views");

        assert_eq!(owned.clone().project(operation).project(payload).project(marker).computed_value(),
            retained_marker.computed_value(), "joining an inactive path does not move the retained active payload");

        assert_eq!(held(owned).prove_trusted(&state.trusted_assumptions, &state.assumptions), Some(true),
            "joining both views must preserve the complete owner's guarantee");

        let mut joined = Some(state);
        let boundary = super::super::merge::ExecutionJoin::Block(crate::analysis::id::AnalysisBlockId::from_slot(BoundUnitId::new(0), 2));

        super::super::merge::merge(&mut joined, &untouched, boundary);

        let state = joined.expect("reachable outer join");
        let required = held(place.value_in(&state.current).expect("current owner").project(scope));

        assert_eq!(required.prove_trusted(&state.trusted_assumptions, &state.assumptions), Some(true),
            "the next join must preserve the same conditional guarantee");
    }

    #[test]
    fn selected_empty_ownership_survives_framing_and_both_join_boundaries() {
        use bray_symbols::{DependencyOwnedResult as Layout, DependencyOwnedTransfer as Transfer, DependencyValuePreservation as Mask};
        use super::super::super::state::ExecutionState;

        let (_, sites) = expression_unit(BoundUnitId::new(0), |tree, origin| {
            (0..3).map(|_| push_expression(tree, integer_literal_expression(origin, None))).collect()
        });

        let place = ExecutionPlace::argument(SymbolOrdinal::new(2));
        let input = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
        let guard = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)));
        let field = StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let origin = ExecutionCondition::Expression(sites[0]);

        let source = ExecutionCondition::owned_value(origin.clone(), ExecutionCondition::constructed(origin,
            [(field, input.clone())].into()));

        let conditional = Layout::case(guard.clone(), Layout::Transfer(Transfer::new(input.clone(), [field])
            .with_preservation(Mask::all())));

        let mut states = Vec::new();

        for (index, unchanged) in [true, false].into_iter().enumerate() {
            let mut state = ExecutionState::default();

            state.owned_results.insert(input.clone(), Layout::Transfer(Transfer::new(input.clone(), []).with_preservation(Mask::all())));
            state.owned_results.insert(source.clone(), conditional.clone());
            guard.clone().assume(false, &mut state.assumptions);

            let mut mask = ValuePreservation { descriptor: true, value: unchanged, ..Default::default() };

            mask.at_mut(&[field]).descriptor = true;

            let value = mask.frame(&mut state, &source, &[], Some(&place), StorageMutation::Operation(sites[index].into()));

            state.assumptions.clear();
            assert!(state.owned_result(&source).is_empty(), "the original selected snapshot cannot revive after its selector expires");
            assert!(state.owned_result(&value).is_empty(), "a retained inactive child is no ownership proof");
            state.assign(place.clone(), value.clone(), None);
            states.push((state, value));
        }

        let mut joined = Some(states[0].0.clone());

        super::super::merge::merge(&mut joined, &Some(states[1].0.clone()), super::super::merge::ExecutionJoin::Boundary);

        let mut current = joined.expect("reachable current-value join");
        let value = place.value_in(&current.current).expect("joined value");

        assert!(current.owned_result(&value).is_empty(), "current-value join retains explicit no-transfer information");

        let returned = super::super::joins::merge_returned(&mut current, &states[1].0, &value, &states[1].1,
            &ExecutionCondition::Expression(sites[2]), sites[2]);

        assert!(current.owned_result(&returned).is_empty(), "result publication retains explicit no-transfer information");
    }

    #[test]
    fn framing_joined_ownership_keeps_only_retained_actual_components() {
        use bray_symbols::{DependencyOwnedResult as Layout, DependencyOwnedTransfer as Transfer, DependencyValuePreservation as Mask};

        let input = |ordinal| ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(ordinal)));
        let kept = StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let changed = StorageProjection::TupleElement(SymbolOrdinal::new(1));
        let origin = input(3);

        let source = ExecutionCondition::constructed(origin.clone(), [
            (kept, origin.clone().project(kept)),
            (changed, origin.project(changed)),
        ].into());

        let ownership = Layout::all([
            Layout::Transfer(Transfer::new(input(0), []).with_preservation(Mask::all())),
            Layout::Transfer(Transfer::new(input(1), [kept]).with_preservation(Mask::all())),
            Layout::Transfer(Transfer::new(input(2), [changed]).with_preservation(Mask::all())),
        ]);

        let mut state = super::super::super::state::ExecutionState::default();

        state.owned_results.insert(source.clone(), ownership.clone());

        let mut mask = ValuePreservation { descriptor: true, ..Default::default() };

        mask.at_mut(&[kept]).value = true;

        let (_, sites) = expression_unit(BoundUnitId::new(0), |tree, origin| {
            vec![push_expression(tree, integer_literal_expression(origin, None))]
        });

        let mutation = StorageMutation::Operation(sites[0].into());
        let place = ExecutionPlace::argument(SymbolOrdinal::new(3));
        let next = mask.frame(&mut state, &source, &[], Some(&place), mutation);
        let retained = state.owned_result(&next);

        assert_eq!(retained.transfers().count(), 2);

        let root = retained.transfers().find(|transfer| transfer.output().is_empty()).expect("retained aggregate identity");

        assert!(!root.preservation().preserves_value());
        assert!(root.preservation().projected(&kept).preserves_value());
        assert!(!root.preservation().projected(&changed).preserves_value());
        assert!(retained.transfers().any(|transfer| transfer.input() == input(1) && transfer.preservation().preserves_value()));
        assert_eq!(state.owned_result(&source), ownership, "framing cannot rewrite a prior snapshot");

        let replay = mask.frame(&mut state, &next, &[], Some(&place), mutation);

        assert_eq!(state.owned_result(&replay), retained, "repeated physical framing stabilizes");

        let mut overwritten = ValuePreservation { descriptor: true, ..Default::default() };
        let final_value = overwritten.frame(&mut state, &next, &[], Some(&place), mutation);

        assert_eq!(state.owned_result(&final_value).transfers().count(), 1, "a later overwrite retires the old child authority");
    }

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

        let mut payload_mask = ValuePreservation { discriminant: true, ..Default::default() };

        payload_mask.at_mut(&[payload, StorageProjection::TupleElement(SymbolOrdinal::new(0))]).value = true;

        let framed = payload_mask.apply(&owner, Some(&place), mutation);
        let conditions = Default::default();
        let cache = crate::execution_guarantees::ExecutionProofCache::default();
        let conditions = cache.ordinary_proof(&conditions);

        let joined = crate::execution_guarantees::common_components(&owner, &framed,
            Some(ExecutionCondition::Result), (&conditions, &conditions), &mut Vec::new(),
            &mut { ExecutionCondition::WORK_LIMIT });

        let moved = crate::execution_guarantees::owned_snapshot(&framed,
            ExecutionCondition::Result, &mut { ExecutionCondition::WORK_LIMIT });

        for value in [joined, moved] {
            assert_eq!(presence(value.clone()), presence(owner.clone()), "retained nullable tags survive joins and owner transfer");

            let marker = StorageProjection::TupleElement(SymbolOrdinal::new(0));

            assert_ne!(value.clone().project(payload).project(marker), ExecutionCondition::Unknown, "joining a framed nullable owner preserves its retained payload marker");
            assert_ne!(value.project(payload), owner.clone().project(payload), "retaining a tag does not restore old payload contents");
        }

        for source in [owner.clone(), ExecutionCondition::owned_value(owner.clone(), owner.clone())] {
            let join = |left: &ExecutionCondition, right: &ExecutionCondition| crate::execution_guarantees::common_components(
                left, right, Some(ExecutionCondition::Result), (&conditions, &conditions), &mut Vec::new(),
                &mut { ExecutionCondition::WORK_LIMIT });

            let mut joined = join(&source, &payload_mask.apply(&source, Some(&place), mutation));

            for _ in 0..3 {
                let updated = payload_mask.apply(&joined, Some(&place), mutation);

                joined = join(&joined, &updated);
                joined = join(&joined, &source);

                let marker = joined.clone().project(payload).project(StorageProjection::TupleElement(SymbolOrdinal::new(0)));

                assert_ne!(marker, ExecutionCondition::Unknown, "repeated framing and joining cannot erase a physically retained payload marker: {joined:?}");
            }

            assert_eq!(join(&joined, &payload_mask.apply(&joined, Some(&place), mutation)), joined, "retained payload analysis stabilizes");
        }

        let variant = bray_symbols::UnionVariantSymbolId::from_symbol_id(bray_symbols::SymbolId::new(902));

        for source in [ExecutionCondition::Tagged(variant, Arc::new(owner.clone())), owner.clone()] {
            let updated = mask.apply(&source, Some(&place), mutation);

            assert_eq!(updated.clone().matches_variant(variant), source.clone().matches_variant(variant));
            assert_eq!(updated.known_variant(), source.known_variant());

            assert!(super::super::super::state::ExecutionState::default().owned_values(&updated)
                .all(|value| value.ownership_identity().is_none()));

            assert_eq!(mask.apply(&updated, Some(&place), mutation), updated);

            let moved = crate::execution_guarantees::owned_snapshot(&updated,
                ExecutionCondition::Result, &mut { ExecutionCondition::WORK_LIMIT });

            assert_eq!(moved.matches_variant(variant), source.clone().matches_variant(variant));

            if matches!(updated, ExecutionCondition::Discriminant(_)) {
                let joined = crate::execution_guarantees::common_components(&source, &updated,
                    Some(ExecutionCondition::Result), (&conditions, &conditions), &mut Vec::new(),
                    &mut { ExecutionCondition::WORK_LIMIT });

                assert_eq!(joined.matches_variant(variant), source.clone().matches_variant(variant),
                    "a physically retained union tag survives a join");
            }
        }

        let replaced = ExecutionCondition::PostState(mutation, Arc::new(owner.clone()));

        assert_eq!(owner.common_retained_discriminant(&replaced.discriminant()), None,
            "a different snapshot cannot preserve an older tag");

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

        let (_, sites) = expression_unit(BoundUnitId::new(0), |tree, origin| {
            (0..2).map(|_| push_expression(tree, integer_literal_expression(origin, None))).collect()
        });

        let mutation = StorageMutation::Operation(sites[0].into());
        let mut mask = ValuePreservation::default();
        let fresh = ExecutionCondition::Expression(sites[1]);

        for source in [owner.clone(), ExecutionCondition::owned_value(owner.clone(), previous.clone()), fresh] {
            let mut state = super::super::super::state::ExecutionState::default();

            let layout = if matches!(source, ExecutionCondition::Expression(_)) {
                bray_symbols::DependencyOwnedResult::default()
            } else {
                bray_symbols::DependencyOwnedResult::Transfer(
                    bray_symbols::DependencyOwnedTransfer::new(owner.clone(), []),
                )
            };

            state.owned_results.insert(source.clone(), layout.clone());

            let updated = mask.frame(&mut state, &source, &[], Some(&place), mutation);

            assert_eq!(updated.ownership_identity(), source.ownership_identity());
            assert_eq!(updated.computed_value(), &ExecutionCondition::post_state(mutation, source.owner_identity().clone()));
            assert_ne!(updated.computed_value(), &previous);
            assert_ne!(updated.computed_value(), &owner);
            assert_eq!(mask.frame(&mut state, &updated, &[], Some(&place), mutation), updated);
            assert_eq!(state.owned_result(&source), layout, "framing leaves the prior snapshot intact");

            let mut extraction = state.clone();

            extraction.entries.insert(sites[0].into(), crate::ExecutionCallEvidence {
                extracted_values: [source.clone()].into(),
                ..Default::default()
            });

            let vacated = mask.frame(&mut extraction, &source, &[], Some(&place), mutation);

            assert_ne!(vacated.ownership_identity(), source.ownership_identity(),
                "a vacated slot cannot keep the moved owner's authority");

            assert_eq!(vacated.computed_value(), &ExecutionCondition::post_state(mutation, source.owner_identity().clone()));

            assert_eq!(mask.frame(&mut extraction, &vacated, &[], Some(&place), mutation), vacated,
                "replaying extraction must stabilize without restoring the prior owner");

            assert!(extraction.owned_result(&vacated).is_empty(), "the vacated value cannot inherit the moved formal lineage");
            assert_eq!(extraction.owned_result(&source), layout, "the returned snapshot keeps its own lineage");
        }
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
