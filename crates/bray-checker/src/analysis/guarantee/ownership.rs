use crate::execution_guarantees::ExecutionObservationKind as ObservationKind;
use super::state::ExecutionState;
use crate::ExecutionCondition;
use bray_bound_tree::StorageProjection;
use bray_symbols::{DependencyOwnedResult, DependencyOwnedTransfer, DependencyValuePreservation};

impl ExecutionState {
    /// Selects exact formal ownership from retained snapshots and joined layouts.
    pub(super) fn owned_result(
        &self,
        value: &ExecutionCondition,
    ) -> DependencyOwnedResult<StorageProjection, ExecutionCondition> {
        self.owned_result_bounded(value, false, &mut { ExecutionCondition::WORK_LIMIT })
    }

    /// Publishes current formal origins only after a checked move out of a borrowed slot.
    pub(super) fn extracted_result(
        &self,
        value: &ExecutionCondition,
    ) -> DependencyOwnedResult<StorageProjection, ExecutionCondition> {
        self.owned_result_bounded(value, true, &mut { ExecutionCondition::WORK_LIMIT })
    }

    /// Selects actual descendant mappings independently of the containing owner.
    pub(super) fn owned_children_result(
        &self,
        value: &ExecutionCondition,
    ) -> DependencyOwnedResult<StorageProjection, ExecutionCondition> {
        let mut budget = ExecutionCondition::WORK_LIMIT;

        let children = self.owned_result_bounded(value, false, &mut budget).try_map(
            &mut |transfer| Ok::<_, std::convert::Infallible>(
                if transfer.output().is_empty() { DependencyOwnedResult::default() }
                else { DependencyOwnedResult::Transfer(transfer.clone()) },
            ),
            &mut |condition| Ok(Some(condition.clone())),
        ).expect("actual descendant ownership selection is infallible");

        without_empty_guards(children, &mut budget)
    }

    fn owned_result_bounded(
        &self,
        value: &ExecutionCondition,
        extraction: bool,
        budget: &mut usize,
    ) -> DependencyOwnedResult<StorageProjection, ExecutionCondition> {
        let Some(remaining) = budget.checked_sub(1) else {
            return DependencyOwnedResult::default();
        };

        *budget = remaining;

        let role = value.ownership_identity().map(|(kind, _)| kind);

        if role == Some(ObservationKind::Reference)
            || matches!(value.kind(), crate::ExecutionConditionKind::Observation { kind: ObservationKind::Scalar, .. })
        {
            return DependencyOwnedResult::default();
        }

        if let Some(result) = self.recorded_owned_result(value, budget) {
            return result;
        }

        // A projected anonymous wrapper keeps its authoritative layout even when
        // its current payload data no longer exposes the original formal input.
        if let crate::ExecutionConditionKind::Projection(field, parent) = value.kind()
            && let Some(result) = self.recorded_owned_result(parent, budget)
        {
            return project_owned_result(result, *field);
        }

        if let Some(payload) = value.nullable_payload_snapshot() {
            return self.owned_result_bounded(payload, extraction, budget);
        }

        let contents = match value.kind() {
            crate::ExecutionConditionKind::Observation { contents, kind: ObservationKind::Owned, .. } => self.owned_result_bounded(contents, extraction, budget),
            crate::ExecutionConditionKind::Present(value) => {
                self.prefixed_owned_result(value, StorageProjection::NullableValue, extraction, budget)
            }
            crate::ExecutionConditionKind::Tagged(_, contents) => self.owned_result_bounded(contents, extraction, budget),
            crate::ExecutionConditionKind::Constructed(base, fields) => {
                let inherited = self.owned_result_bounded(base, extraction, budget).try_map(
                    &mut |transfer| {
                        if transfer.output().first().is_some_and(|first| fields.iter().any(|(field, _)| field == first)) {
                            let prior = selected_data(base, transfer.output(), budget);
                            let current = selected_data(value, transfer.output(), budget);

                            if !matches!(prior.as_ref().and_then(ExecutionCondition::ownership_identity), Some((ObservationKind::Owned, _)))
                                || prior != current
                            {
                                return Ok::<_, std::convert::Infallible>(DependencyOwnedResult::default());
                            }
                        }

                        let mut preservation = transfer.preservation().clone();

                        if transfer.output().is_empty() {
                            let mut children = preservation.children().clone();
                            let mut preserved = preservation.preserves_value();

                            for (field, value) in fields.iter() {
                                if crate::ExecutionPlace::projection_kind(*field).is_none() {
                                    preserved = false;
                                    continue;
                                }

                                let source = transfer.input().project(*field);

                                children.insert(*field, self.value_preservation(value, &source, budget));
                            }

                            preservation = DependencyValuePreservation::new(preserved, children);
                        }

                        Ok(DependencyOwnedResult::Transfer(transfer.clone().with_preservation(preservation)))
                    },
                    &mut |condition| Ok(Some(condition.clone())),
                ).expect("actual component ownership update is infallible");

                let inherited = without_empty_guards(inherited, budget);

                DependencyOwnedResult::all(std::iter::once(inherited).chain(fields.iter().filter_map(|(field, value)| {
                    if !matches!(
                        field,
                        StorageProjection::ProductField(_)
                            | StorageProjection::ActiveUnionPayloadField { .. }
                            | StorageProjection::TupleElement(_)
                            | StorageProjection::ElementFromStart(_)
                            | StorageProjection::NullableValue
                    ) {
                        return None;
                    }

                    Some(self.prefixed_owned_result(value, *field, extraction, budget))
                })))
            }
            crate::ExecutionConditionKind::Projection(field, parent) => project_owned_result(
                self.owned_result_bounded(parent, extraction, budget), *field,
            ),
            crate::ExecutionConditionKind::Input(place) if !place.projections.is_empty() => {
                let mut root = place.clone();

                root.projections = [].into();

                let ownership = self.owned_result_bounded(&ExecutionCondition::new(crate::ExecutionConditionKind::Input(root)), extraction, budget);

                if place.projections.len() > *budget { return DependencyOwnedResult::default(); }
                *budget -= place.projections.len();

                place.projections.iter().fold(ownership, |ownership, field| project_owned_result(ownership, *field))
            }
            crate::ExecutionConditionKind::Input(place) if extraction && place.is_callable_input() => {
                DependencyOwnedResult::Transfer(DependencyOwnedTransfer::new(value.clone(), [])
                    .with_preservation(DependencyValuePreservation::all()))
            }
            _ => DependencyOwnedResult::default(),
        };

        // Contents can change without changing this object's recorded identity.
        // Only its own mapping survives that way. Child mappings come from the
        // actual current children, never a previous version of the aggregate.
        let owner = matches!(value.kind(), crate::ExecutionConditionKind::Observation { kind: ObservationKind::Owned, .. })
            .then(|| value.ownership_identity()).flatten()
            .filter(|(kind, _)| *kind == ObservationKind::Owned)
            .and_then(|(_, owner)| self.recorded_owned_result(owner, budget))
            .map(|layout| layout.root_layout()).unwrap_or_default()
            .try_map(
                &mut |transfer| Ok::<_, std::convert::Infallible>(
                    if contents.transfers().any(|actual| actual.same_identity(transfer)) {
                        DependencyOwnedResult::default()
                    } else {
                        DependencyOwnedResult::Transfer(transfer.clone().with_preservation(
                            self.value_preservation(value, &transfer.input(), budget),
                        ))
                    },
                ),
                &mut |condition| Ok(Some(condition.clone())),
            ).expect("actual owner contents selection is infallible").root_layout();

        DependencyOwnedResult::all([owner, contents])
    }

    fn recorded_owned_result(
        &self,
        value: &ExecutionCondition,
        budget: &mut usize,
    ) -> Option<DependencyOwnedResult<StorageProjection, ExecutionCondition>> {
        self.owned_results.get(value).map(|layout| {
            applicable_owned_cases(layout, &self.proofs.ordinary_proof(&self.assumptions), budget).unwrap_or_default()
        })
    }

    fn value_preservation(
        &self,
        value: &ExecutionCondition,
        source: &ExecutionCondition,
        budget: &mut usize,
    ) -> DependencyValuePreservation<StorageProjection> {
        let Some(remaining) = budget.checked_sub(1) else {
            return DependencyValuePreservation::none();
        };

        *budget = remaining;

        if value == source && *value != ExecutionCondition::unknown() {
            return DependencyValuePreservation::all();
        }

        if let Some(layout) = self.recorded_owned_result(value, budget) {
            return layout_preservation(&layout, source, &self.proofs.ordinary_proof(&self.assumptions), budget).unwrap_or_default();
        }

        if let Some(payload) = value.nullable_payload_snapshot() {
            return self.value_preservation(payload, source, budget);
        }

        match value.kind() {
            crate::ExecutionConditionKind::Observation { contents, kind: ObservationKind::Owned | ObservationKind::Scalar, .. }
            | crate::ExecutionConditionKind::Tagged(_, contents) => self.value_preservation(contents, source, budget),
            crate::ExecutionConditionKind::Constructed(base, fields) => {
                let base = self.value_preservation(base, source, budget);
                let mut children = base.children().clone();
                let mut preserved = base.preserves_value();

                for (field, value) in fields.iter() {
                    if crate::ExecutionPlace::projection_kind(*field).is_none() {
                        preserved = false;
                        continue;
                    }

                    children.insert(*field, self.value_preservation(value, &source.clone().project(*field), budget));
                }

                DependencyValuePreservation::new(preserved, children)
            }
            crate::ExecutionConditionKind::Present(value) => DependencyValuePreservation::new(false, [(
                StorageProjection::NullableValue,
                self.value_preservation(value, &source.clone().project(StorageProjection::NullableValue), budget),
            )].into()),
            _ => DependencyValuePreservation::none(),
        }
    }

    fn prefixed_owned_result(
        &self,
        value: &ExecutionCondition,
        field: StorageProjection,
        extraction: bool,
        budget: &mut usize,
    ) -> DependencyOwnedResult<StorageProjection, ExecutionCondition> {
        self.owned_result_bounded(value, extraction, budget)
            .try_map(
                &mut |transfer| {
                    if transfer.output().len() >= ExecutionCondition::WORK_LIMIT {
                        return Ok(DependencyOwnedResult::default());
                    }

                    Ok::<_, std::convert::Infallible>(DependencyOwnedResult::Transfer(
                        DependencyOwnedTransfer::new(
                            transfer.input(),
                            std::iter::once(field).chain(transfer.output().iter().copied()),
                        )
                        .with_preservation(transfer.preservation().clone())
                        .with_condition(transfer.condition()),
                    ))
                },
                &mut |condition| Ok(Some(condition.clone())),
            )
            .expect("ownership prefix is infallible")
    }
}

/// An impossible alternative contributes no layout. A possible empty layout
/// remains an alternative and cannot establish an input transfer.
fn applicable_owned_cases(
    layout: &DependencyOwnedResult<StorageProjection, ExecutionCondition>,
    proof: &crate::execution_guarantees::ExecutionProof<'_>,
    budget: &mut usize,
) -> Option<DependencyOwnedResult<StorageProjection, ExecutionCondition>> {
    let Some(remaining) = budget.checked_sub(1) else { return Some(DependencyOwnedResult::default()); };

    *budget = remaining;

    match layout {
        DependencyOwnedResult::Case { condition, result } => {
            let known = proof.prove_mixed(condition, &mut { ExecutionCondition::WORK_LIMIT });

            if known == Some(false) { return None; }

            let selected = applicable_owned_cases(result, proof, budget)?;

            Some(if known == Some(true) { selected } else { DependencyOwnedResult::case(condition.clone(), selected) })
        }
        DependencyOwnedResult::All(values) => values.iter()
            .map(|value| applicable_owned_cases(value, proof, budget))
            .collect::<Option<Vec<_>>>().map(DependencyOwnedResult::all),
        DependencyOwnedResult::Choice(values) => {
            let selected = values.iter().filter_map(|value| applicable_owned_cases(value, proof, budget)).collect::<Vec<_>>();

            (!selected.is_empty()).then(|| DependencyOwnedResult::choice(selected))
        }
        DependencyOwnedResult::Transfer(_) => Some(layout.clone()),
    }
}

fn project_owned_result(
    ownership: DependencyOwnedResult<StorageProjection, ExecutionCondition>,
    field: bray_symbols::ConstantProjectionKind,
) -> DependencyOwnedResult<StorageProjection, ExecutionCondition> {
    ownership
                .try_map(
                    &mut |transfer| {
                        let Some((first, rest)) = transfer.output().split_first() else {
                            let preservation = transfer.preservation().children().iter()
                                .find(|(projection, _)| crate::ExecutionPlace::projection_kind(**projection) == Some(field))
                                .map(|(_, preservation)| preservation.clone())
                                .unwrap_or_else(|| bray_symbols::DependencyValuePreservation::new(
                                    transfer.preservation().preserves_value(), Default::default()));

                            return Ok::<_, std::convert::Infallible>(
                                DependencyOwnedResult::Transfer(DependencyOwnedTransfer::new(
                                    transfer.input().component(field),
                                    [],
                                ).with_preservation(preservation)
                                    .with_condition(transfer.condition())),
                            );
                        };

                        Ok(
                            if crate::ExecutionPlace::projection_kind(*first) == Some(field) {
                                DependencyOwnedResult::Transfer(
                                    DependencyOwnedTransfer::new(
                                        transfer.input(),
                                        rest.iter().copied(),
                                    )
                                    .with_preservation(transfer.preservation().clone())
                                    .with_condition(transfer.condition()),
                                )
                            } else {
                                DependencyOwnedResult::default()
                            },
                        )
                    },
                    &mut |condition| Ok(Some(condition.clone())),
                )
                .expect("ownership projection is infallible")
}

fn layout_preservation(
    layout: &DependencyOwnedResult<StorageProjection, ExecutionCondition>,
    source: &ExecutionCondition,
    proof: &crate::execution_guarantees::ExecutionProof<'_>,
    budget: &mut usize,
) -> Option<DependencyValuePreservation<StorageProjection>> {
    let remaining = budget.checked_sub(1)?;

    *budget = remaining;

    match layout {
        DependencyOwnedResult::Transfer(transfer) => (transfer.output().is_empty()
            && transfer.input() == *source
            && transfer.condition().is_none_or(|condition| {
                proof.prove_mixed(&condition, &mut { ExecutionCondition::WORK_LIMIT }) == Some(true)
            })).then(|| transfer.preservation().clone()),
        DependencyOwnedResult::All(values) => values.iter()
            .filter_map(|value| layout_preservation(value, source, proof, budget))
            .reduce(|left, right| left.intersection(&right)),
        DependencyOwnedResult::Choice(values) => {
            let mut preserved: Option<DependencyValuePreservation<StorageProjection>> = None;

            for value in values.iter() {
                let next = layout_preservation(value, source, proof, budget)?;

                preserved = Some(preserved.map_or_else(|| next.clone(), |previous| previous.intersection(&next)));
            }

            preserved
        }
        DependencyOwnedResult::Case { condition, result }
            if proof.prove_mixed(condition, &mut { ExecutionCondition::WORK_LIMIT }) == Some(true) =>
        {
            layout_preservation(result, source, proof, budget)
        }
        _ => None,
    }
}

fn selected_data(
    value: &ExecutionCondition,
    path: &[StorageProjection],
    budget: &mut usize,
) -> Option<ExecutionCondition> {
    let remaining = budget.checked_sub(path.len())?;

    *budget = remaining;

    Some(path.iter().fold(value.clone(), |value, field| {
        let selected = value.project(*field);

        selected.nullable_payload_snapshot().cloned().unwrap_or(selected)
    }))
}

fn without_empty_guards(
    layout: DependencyOwnedResult<StorageProjection, ExecutionCondition>,
    budget: &mut usize,
) -> DependencyOwnedResult<StorageProjection, ExecutionCondition> {
    let Some(remaining) = budget.checked_sub(1) else {
        return DependencyOwnedResult::default();
    };

    *budget = remaining;

    match layout {
        DependencyOwnedResult::Case { condition, result } => {
            let result = without_empty_guards(result.as_ref().clone(), budget);

            if result.is_empty() { DependencyOwnedResult::default() }
            else { DependencyOwnedResult::case(condition, result) }
        }
        DependencyOwnedResult::All(values) => DependencyOwnedResult::all(values.iter().cloned()
            .map(|value| without_empty_guards(value, budget))),
        DependencyOwnedResult::Choice(values) => DependencyOwnedResult::choice(values.iter().cloned()
            .map(|value| without_empty_guards(value, budget))),
        layout => layout,
    }
}

#[cfg(test)]
mod tests {
    use super::ExecutionState;
    use crate::{ExecutionCondition, ExecutionPlace};
    use bray_bound_tree::StorageProjection;
    use bray_symbols::{DependencyOwnedResult, DependencyOwnedTransfer, DependencyValuePreservation, SymbolOrdinal};

    #[test]
    fn initialized_alternatives_select_ownership_without_a_trusted_witness() {
        use bray_bound_tree::BoundOperator::{LogicalAnd, LogicalNot, LogicalOr};

        let place = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let value = ExecutionCondition::new(crate::ExecutionConditionKind::Input(place.clone()));
        let take = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(1))));
        let initialized = ExecutionCondition::new(crate::ExecutionConditionKind::InitializedSnapshot(place, std::sync::Arc::new(ExecutionCondition::boolean(true)), std::sync::Arc::new(value.clone())));
        let not = |condition| ExecutionCondition::operation(LogicalNot, vec![condition]);

        let relation = ExecutionCondition::operation(LogicalOr, vec![
            ExecutionCondition::operation(LogicalAnd, vec![initialized.clone(), not(take.clone())]),
            ExecutionCondition::operation(LogicalAnd, vec![not(initialized.clone()), take.clone()]),
        ]);

        let transfer = DependencyOwnedResult::Transfer(DependencyOwnedTransfer::new(value.clone(), [])
            .with_preservation(DependencyValuePreservation::all()));

        for selected in [None, Some(false), Some(true)] {
            let mut state = ExecutionState::default();

            relation.clone().assume(true, &mut state.assumptions);
            state.assumptions.insert((ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(std::sync::Arc::new(take.clone()))), true));
            if let Some(selected) = selected { initialized.clone().assume(selected, &mut state.assumptions); }

            state.owned_results.insert(value.clone(), DependencyOwnedResult::choice([
                DependencyOwnedResult::case(take.clone(), transfer.clone()),
                DependencyOwnedResult::case(not(take.clone()), DependencyOwnedResult::default()),
            ]));

            let assumptions = state.assumptions.clone();

            state.refine_witnesses();
            assert!(state.witnesses.is_empty());
            assert!(state.trusted_assumptions.is_empty());
            assert_eq!(state.assumptions, assumptions, "proof consequences must not become new flow assumptions");

            let actual = state.owned_result(&value);

            match selected {
                Some(false) => assert_eq!(actual, transfer),
                Some(true) => assert!(actual.is_empty()),
                None => {
                    assert!(matches!(actual, DependencyOwnedResult::Choice(_)), "unknown selection retains the possible empty alternative");
                    assert!(actual.guarded_transfers().all(|(_, guards)| !guards.is_empty()), "unknown guards cannot become unconditional transfers");
                }
            }
        }
    }

    #[test]
    fn canonical_input_projections_reuse_only_the_recorded_owner_layout() {
        let root = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(0))));
        let field = StorageProjection::NullableValue;
        let projection = bray_symbols::ConstantProjectionKind::NullableValue;
        let selected = root.clone().project(field);
        let explicit = ExecutionCondition::new(crate::ExecutionConditionKind::Projection(projection, std::sync::Arc::new(root.clone())));
        let mut state = ExecutionState::default();

        assert!(state.owned_result(&selected).is_empty());

        let preservation = DependencyValuePreservation::new(true, [(field, DependencyValuePreservation::none())].into());

        let layout = DependencyOwnedResult::case(
            ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)))),
            DependencyOwnedResult::Transfer(DependencyOwnedTransfer::new(root.clone(), []).with_preservation(preservation)),
        );

        state.owned_results.insert(root.clone(), layout);

        let actual = state.owned_result(&selected);

        assert_eq!(actual, state.owned_result(&explicit));

        let transfers = actual.transfers().collect::<Vec<_>>();

        assert_eq!(transfers.len(), 1);
        assert_eq!(transfers[0].input(), selected);
        assert!(!transfers[0].preservation().preserves_value());
        assert!(!actual.guarded_transfers().next().expect("guarded transfer").1.is_empty());
        assert!(state.owned_result(&root.borrowed().project(field)).is_empty());
    }

    #[test]
    fn checked_extraction_publishes_current_formal_contents_without_borrow_authority() {
        let original = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::from(bray_bound_tree::BoundReferenceTarget::Surface(
            bray_symbols::CallableParameterSymbolId::from_symbol_id(bray_symbols::SymbolId::new(0)).into()))));

        let state = ExecutionState::default();
        let field = StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let current = original.clone().with_storage_component(&[field], ExecutionCondition::unknown());

        assert!(state.owned_result(&original).is_empty(), "borrowing a formal never grants ownership");
        assert!(state.extracted_result(&original.clone().borrowed()).is_empty(), "returning a loan cannot extract its referent");
        assert!(state.extracted_result(&ExecutionCondition::new(crate::ExecutionConditionKind::ScalarValue(std::sync::Arc::new(original.clone())))).is_empty(), "copied data cannot carry owner authority");

        let moved = state.extracted_result(&original);

        assert!(moved.transfers().any(|transfer| transfer.input() == original && transfer.preservation().preserves_value()));

        let changed = state.extracted_result(&current);

        assert!(changed.transfers().any(|transfer| transfer.input() == original && transfer.output().is_empty()));

        assert!(changed.transfers().all(|transfer| transfer.preservation().children().get(&field)
            .is_some_and(|child| !child.preserves_value())), "retained identity cannot restore overwritten contents");

        let (_, sites) = crate::test_support::expression_unit(bray_bound_tree::BoundUnitId::new(0), |tree, origin| {
            vec![crate::test_support::push_expression(tree, crate::test_support::integer_literal_expression(origin, None))]
        });

        let opaque = ExecutionCondition::post_state(bray_bound_tree::StorageMutation::Operation(sites[0].into()), original);

        assert!(state.extracted_result(&opaque).is_empty(), "opaque replacement cannot resurrect the old formal value");
    }

    #[test]
    fn conditional_predecessor_masks_survive_joined_data_and_untouched_updates() {
        let original = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(0))));
        let condition = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(1))));
        let field = StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let other = StorageProjection::TupleElement(SymbolOrdinal::new(1));
        let mut state = ExecutionState::default();

        state.owned_results.insert(original.clone(), DependencyOwnedResult::Transfer(
            DependencyOwnedTransfer::new(original.clone(), []).with_preservation(DependencyValuePreservation::all()),
        ));

        let predecessor = ExecutionCondition::constructed(ExecutionCondition::unknown(), [(field, original)].into());
        let active = state.owned_result(&predecessor);

        assert!(active.transfers().all(|transfer| transfer.preservation().preserves_value()));

        let layout = DependencyOwnedResult::choice([
            DependencyOwnedResult::case(condition, active), DependencyOwnedResult::default(),
        ]);

        let joined = ExecutionCondition::result();

        state.owned_results.insert(joined.clone(), layout.clone());
        assert_eq!(state.owned_result(&joined), layout);

        let updated = ExecutionCondition::constructed(joined, [(other, ExecutionCondition::boolean(false))].into());
        let retained = state.owned_result(&updated);

        assert!(!retained.transfers().collect::<Vec<_>>().is_empty());

        assert!(retained.transfers().all(|transfer| {
            transfer.output() == [field] && transfer.preservation().preserves_value()
        }));

        assert!(retained.guaranteed_transfers().is_empty());
    }

    #[test]
    fn repeated_actual_updates_compose_masks_without_restoring_old_contents() {
        let original = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(0))));
        let first = StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let second = StorageProjection::TupleElement(SymbolOrdinal::new(1));
        let mut state = ExecutionState::default();

        state.owned_results.insert(original.clone(), DependencyOwnedResult::Transfer(
            DependencyOwnedTransfer::new(original.clone(), []).with_preservation(DependencyValuePreservation::all()),
        ));

        let mut current = original.clone();

        for field in [first, second].into_iter().cycle().take(16) {
            current = current.with_storage_component(&[field], ExecutionCondition::unknown());

            let captured = state.owned_result(&current);

            state.owned_results.insert(current.clone(), captured);
        }

        let layout = state.owned_result(&current);

        let root = layout.transfers().find(|transfer| transfer.output().is_empty())
            .expect("updated object retains its original top identity");

        assert_eq!(root.input(), original);
        assert!(root.preservation().preserves_value());
        assert_eq!(root.preservation().projected(&first), DependencyValuePreservation::none());
        assert_eq!(root.preservation().projected(&second), DependencyValuePreservation::none());
    }

    #[test]
    fn joins_keep_distinct_contents_snapshots_of_the_same_owner() {
        let original = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(0))));
        let field = StorageProjection::TupleElement(SymbolOrdinal::new(0));

        let original_layout = DependencyOwnedResult::Transfer(
            DependencyOwnedTransfer::new(original.clone(), []).with_preservation(DependencyValuePreservation::all()),
        );

        let mut state = ExecutionState::default();

        state.owned_results.insert(original.clone(), original_layout.clone());

        let changed = original.with_storage_component(&[field], ExecutionCondition::unknown());
        let changed_layout = state.owned_result(&changed);

        state.owned_results.insert(changed.clone(), changed_layout.clone());

        let unrelated = |ordinal| ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(ordinal))));

        state.join_values(&crate::execution_guarantees::ExecutionSubstitution::new(&[(unrelated(1), unrelated(2))].into(), &Default::default()));
        assert_eq!(state.owned_results.get(&original), Some(&original_layout), "a changed snapshot cannot alter the original value's preservation");
        assert_eq!(state.owned_results.get(&changed), Some(&changed_layout), "a snapshot's identity does not erase its changed contents");
        assert_eq!(state.owned_result(&changed), changed_layout);
    }

    #[test]
    fn mutated_formal_retains_its_top_lineage_and_current_child_lineage() {
        let original = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(0))));
        let old_child = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(1))));
        let new_child = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(2))));
        let field = StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let mut state = ExecutionState::default();

        state.owned_results.insert(original.clone(), DependencyOwnedResult::all([
            DependencyOwnedResult::Transfer(DependencyOwnedTransfer::new(original.clone(), [])),
            DependencyOwnedResult::Transfer(DependencyOwnedTransfer::new(old_child.clone(), [field])),
        ]));

        state.owned_results.insert(new_child.clone(), DependencyOwnedResult::Transfer(
            DependencyOwnedTransfer::new(new_child.clone(), []),
        ));

        let changed = ExecutionCondition::owned_value(original.clone(),
            ExecutionCondition::constructed(ExecutionCondition::unknown(), [(field, new_child.clone())].into()));

        let ownership = state.owned_result(&changed);

        let mappings = ownership.transfers().map(|transfer| (transfer.input(), transfer.output().to_vec()))
            .collect::<std::collections::BTreeSet<_>>();

        assert_eq!(mappings, [(original, vec![]), (new_child, vec![field])].into());
        assert!(!ownership.transfers().any(|transfer| transfer.input() == old_child));
    }
}
