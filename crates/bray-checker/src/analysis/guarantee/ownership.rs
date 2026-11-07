use super::state::ExecutionState;
use crate::ExecutionCondition;
use bray_bound_tree::StorageProjection;
use bray_symbols::{DependencyOwnedResult, DependencyOwnedTransfer};

impl ExecutionState {
    /// Recovers exact formal ownership through snapshots and previously joined layouts.
    pub(super) fn owned_result(
        &self,
        value: &ExecutionCondition,
    ) -> DependencyOwnedResult<StorageProjection, ExecutionCondition> {
        if let Some(result) = self.owned_results.get(value) {
            return result.clone();
        }

        match value {
            ExecutionCondition::CurrentValue(_, contents) => self.owned_result(contents),
            ExecutionCondition::Present(value) => {
                self.prefixed_owned_result(value, StorageProjection::NullableValue)
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

                    Some(self.prefixed_owned_result(value, *field))
                }))
            }
            ExecutionCondition::Projection(field, parent) => self
                .owned_result(parent)
                .try_map(
                    &mut |transfer| {
                        let Some((first, rest)) = transfer.output().split_first() else {
                            return Ok::<_, std::convert::Infallible>(
                                DependencyOwnedResult::Transfer(DependencyOwnedTransfer::new(
                                    transfer.input().component(*field),
                                    [],
                                )),
                            );
                        };

                        Ok(
                            if crate::ExecutionPlace::projection_kind(*first) == Some(*field) {
                                DependencyOwnedResult::Transfer(
                                    DependencyOwnedTransfer::new(
                                        transfer.input(),
                                        rest.iter().copied(),
                                    )
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
        }
    }

    fn prefixed_owned_result(
        &self,
        value: &ExecutionCondition,
        field: StorageProjection,
    ) -> DependencyOwnedResult<StorageProjection, ExecutionCondition> {
        self.owned_result(value)
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
                        .with_condition(transfer.condition()),
                    ))
                },
                &mut |condition| Ok(Some(condition.clone())),
            )
            .expect("ownership prefix is infallible")
    }
}
