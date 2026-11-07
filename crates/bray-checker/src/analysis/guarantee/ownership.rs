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
        self.owned_result_bounded(value, &mut { ExecutionCondition::WORK_LIMIT })
    }

    /// Selects actual descendant mappings independently of the containing owner.
    pub(super) fn owned_children_result(
        &self,
        value: &ExecutionCondition,
    ) -> DependencyOwnedResult<StorageProjection, ExecutionCondition> {
        let mut budget = ExecutionCondition::WORK_LIMIT;

        let children = self.owned_result_bounded(value, &mut budget).try_map(
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
        budget: &mut usize,
    ) -> DependencyOwnedResult<StorageProjection, ExecutionCondition> {
        let Some(remaining) = budget.checked_sub(1) else {
            return DependencyOwnedResult::default();
        };

        *budget = remaining;

        if let Some(payload) = value.nullable_payload_snapshot() {
            return self.owned_result_bounded(payload, budget);
        }

        let role = value.ownership_identity().map(|(kind, _)| kind);

        if role == Some(ObservationKind::Reference)
            || matches!(value, ExecutionCondition::Observation { kind: ObservationKind::Scalar, .. })
        {
            return DependencyOwnedResult::default();
        }

        if role == Some(ObservationKind::Owned)
            && let Some(result) = self.owned_results.get(value)
        {
            return result.clone();
        }

        let contents = match value {
            ExecutionCondition::Observation { contents, kind: ObservationKind::Owned, .. } => self.owned_result_bounded(contents, budget),
            ExecutionCondition::Present(value) => {
                self.prefixed_owned_result(value, StorageProjection::NullableValue, budget)
            }
            ExecutionCondition::Tagged(_, contents) => self.owned_result_bounded(contents, budget),
            ExecutionCondition::Constructed(base, fields) => {
                let inherited = self.owned_result_bounded(base, budget).try_map(
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

                    Some(self.prefixed_owned_result(value, *field, budget))
                })))
            }
            ExecutionCondition::Projection(field, parent) => self
                .owned_result_bounded(parent, budget)
                .try_map(
                    &mut |transfer| {
                        let Some((first, rest)) = transfer.output().split_first() else {
                            let preservation = transfer.preservation().children().iter()
                                .find(|(projection, _)| crate::ExecutionPlace::projection_kind(**projection) == Some(*field))
                                .map(|(_, preservation)| preservation.clone())
                                .unwrap_or_else(|| bray_symbols::DependencyValuePreservation::new(
                                    transfer.preservation().preserves_value(), Default::default()));

                            return Ok::<_, std::convert::Infallible>(
                                DependencyOwnedResult::Transfer(DependencyOwnedTransfer::new(
                                    transfer.input().component(*field),
                                    [],
                                ).with_preservation(preservation)
                                    .with_condition(transfer.condition())),
                            );
                        };

                        Ok(
                            if crate::ExecutionPlace::projection_kind(*first) == Some(*field) {
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
                .expect("ownership projection is infallible"),
            _ => DependencyOwnedResult::default(),
        };

        // Contents can change without changing this object's recorded identity.
        // Only its own mapping survives that way. Child mappings come from the
        // actual current children, never a previous version of the aggregate.
        let owner = matches!(value, ExecutionCondition::Observation { kind: ObservationKind::Owned, .. })
            .then(|| value.ownership_identity()).flatten()
            .filter(|(kind, _)| *kind == ObservationKind::Owned)
            .and_then(|(_, owner)| self.owned_results.get(owner))
            .map(DependencyOwnedResult::root_layout).unwrap_or_default()
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

        if value == source && *value != ExecutionCondition::Unknown {
            return DependencyValuePreservation::all();
        }

        if let Some(layout) = self.owned_results.get(value) {
            return layout_preservation(layout, source, &self.assumptions, budget).unwrap_or_default();
        }

        if let Some(payload) = value.nullable_payload_snapshot() {
            return self.value_preservation(payload, source, budget);
        }

        match value {
            ExecutionCondition::Observation { contents, kind: ObservationKind::Owned | ObservationKind::Scalar, .. }
            | ExecutionCondition::Tagged(_, contents) => self.value_preservation(contents, source, budget),
            ExecutionCondition::Constructed(base, fields) => {
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
            ExecutionCondition::Present(value) => DependencyValuePreservation::new(false, [(
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
        budget: &mut usize,
    ) -> DependencyOwnedResult<StorageProjection, ExecutionCondition> {
        self.owned_result_bounded(value, budget)
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

fn layout_preservation(
    layout: &DependencyOwnedResult<StorageProjection, ExecutionCondition>,
    source: &ExecutionCondition,
    assumptions: &std::collections::BTreeSet<(ExecutionCondition, bool)>,
    budget: &mut usize,
) -> Option<DependencyValuePreservation<StorageProjection>> {
    let remaining = budget.checked_sub(1)?;

    *budget = remaining;

    match layout {
        DependencyOwnedResult::Transfer(transfer) => (transfer.output().is_empty()
            && transfer.input() == *source
            && transfer.condition().is_none_or(|condition| {
                condition.prove(assumptions, &mut { ExecutionCondition::WORK_LIMIT }) == Some(true)
            })).then(|| transfer.preservation().clone()),
        DependencyOwnedResult::All(values) => values.iter()
            .filter_map(|value| layout_preservation(value, source, assumptions, budget))
            .reduce(|left, right| left.intersection(&right)),
        DependencyOwnedResult::Choice(values) => {
            let mut preserved: Option<DependencyValuePreservation<StorageProjection>> = None;

            for value in values.iter() {
                let next = layout_preservation(value, source, assumptions, budget)?;

                preserved = Some(preserved.map_or_else(|| next.clone(), |previous| previous.intersection(&next)));
            }

            preserved
        }
        DependencyOwnedResult::Case { condition, result }
            if condition.prove(assumptions, &mut { ExecutionCondition::WORK_LIMIT }) == Some(true) =>
        {
            layout_preservation(result, source, assumptions, budget)
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
    fn conditional_predecessor_masks_survive_joined_data_and_untouched_updates() {
        let original = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
        let condition = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)));
        let field = StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let other = StorageProjection::TupleElement(SymbolOrdinal::new(1));
        let mut state = ExecutionState::default();

        state.owned_results.insert(original.clone(), DependencyOwnedResult::Transfer(
            DependencyOwnedTransfer::new(original.clone(), []).with_preservation(DependencyValuePreservation::all()),
        ));

        let predecessor = ExecutionCondition::constructed(ExecutionCondition::Unknown, [(field, original)].into());
        let active = state.owned_result(&predecessor);

        assert!(active.transfers().all(|transfer| transfer.preservation().preserves_value()));

        let layout = DependencyOwnedResult::choice([
            DependencyOwnedResult::case(condition, active), DependencyOwnedResult::default(),
        ]);

        let joined = ExecutionCondition::Result;

        state.owned_results.insert(joined.clone(), layout.clone());
        assert_eq!(state.owned_result(&joined), layout);

        let updated = ExecutionCondition::constructed(joined, [(other, ExecutionCondition::Boolean(false))].into());
        let retained = state.owned_result(&updated);

        assert!(!retained.transfers().collect::<Vec<_>>().is_empty());

        assert!(retained.transfers().all(|transfer| {
            transfer.output() == [field] && transfer.preservation().preserves_value()
        }));

        assert!(retained.guaranteed_transfers().is_empty());
    }

    #[test]
    fn repeated_actual_updates_compose_masks_without_restoring_old_contents() {
        let original = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
        let first = StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let second = StorageProjection::TupleElement(SymbolOrdinal::new(1));
        let mut state = ExecutionState::default();

        state.owned_results.insert(original.clone(), DependencyOwnedResult::Transfer(
            DependencyOwnedTransfer::new(original.clone(), []).with_preservation(DependencyValuePreservation::all()),
        ));

        let mut current = original.clone();

        for field in [first, second].into_iter().cycle().take(16) {
            current = current.with_storage_component(&[field], ExecutionCondition::Unknown);

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
    fn mutated_formal_retains_its_top_lineage_and_current_child_lineage() {
        let original = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
        let old_child = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)));
        let new_child = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(2)));
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
            ExecutionCondition::constructed(ExecutionCondition::Unknown, [(field, new_child.clone())].into()));

        let ownership = state.owned_result(&changed);

        let mappings = ownership.transfers().map(|transfer| (transfer.input(), transfer.output().to_vec()))
            .collect::<std::collections::BTreeSet<_>>();

        assert_eq!(mappings, [(original, vec![]), (new_child, vec![field])].into());
        assert!(!ownership.transfers().any(|transfer| transfer.input() == old_child));
    }
}
