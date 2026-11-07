use crate::execution_guarantees::ExecutionObservationKind as ObservationKind;
use super::state::ExecutionState;
use crate::ExecutionCondition;
use bray_bound_tree::StorageProjection;
use bray_symbols::{DependencyOwnedResult, DependencyOwnedTransfer};

impl ExecutionState {
    /// Selects exact formal ownership from retained snapshots and joined layouts.
    pub(super) fn owned_result(
        &self,
        value: &ExecutionCondition,
    ) -> DependencyOwnedResult<StorageProjection, ExecutionCondition> {
        self.owned_result_bounded(value, &mut { ExecutionCondition::WORK_LIMIT })
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

        if let Some(result) = self.owned_results.get(value) {
            return result.clone();
        }

        let contents = match value {
            ExecutionCondition::Observation { contents, kind: ObservationKind::Owned, .. } => self.owned_result_bounded(contents, budget),
            ExecutionCondition::Present(value) => {
                self.prefixed_owned_result(value, StorageProjection::NullableValue, budget)
            }
            ExecutionCondition::Constructed(_, fields) => {
                DependencyOwnedResult::all(fields.iter().filter_map(|(field, value)| {
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
                }))
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
                                ).with_preservation(preservation)),
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
        let owner = self.owned_results.get(value.owner_identity()).map(|result| {
            result.try_map(
                &mut |transfer| Ok::<_, std::convert::Infallible>(
                    if transfer.output().is_empty() {
                        DependencyOwnedResult::Transfer(transfer.clone())
                    } else {
                        DependencyOwnedResult::default()
                    }),
                &mut |condition| Ok(Some(condition.clone())),
            ).expect("recorded owner selection is infallible")
        }).unwrap_or_default();

        DependencyOwnedResult::all([owner, contents])
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

#[cfg(test)]
mod tests {
    use super::ExecutionState;
    use crate::{ExecutionCondition, ExecutionPlace};
    use bray_bound_tree::StorageProjection;
    use bray_symbols::{DependencyOwnedResult, DependencyOwnedTransfer, SymbolOrdinal};

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
