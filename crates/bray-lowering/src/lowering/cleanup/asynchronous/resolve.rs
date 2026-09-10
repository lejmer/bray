use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirBlockKind, MirEdge, MirGeneratedLifecycleRole, MirOperand, MirOperationKind,
    MirPlace, MirSourceAnchor, MirStorageKind, MirStoreKind, MirTerminatorKind, MirValueId,
};
use bray_symbols::{CallableExecution, TypeId};

use crate::lowering::LoweringError;
use crate::lowering::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(in crate::lowering::cleanup) fn push_lifecycle_cleanup(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        role: MirGeneratedLifecycleRole,
        place: MirPlace,
        value: Option<(MirValueId, TypeId)>,
        synchronous_destruction: bool,
    ) -> Result<(MirBlockId, Option<(MirValueId, TypeId)>), LoweringError> {
        if self.future_has_cleanup_free_captures(&place) {
            let consumes = matches!(
                role,
                MirGeneratedLifecycleRole::Destroy
                    | MirGeneratedLifecycleRole::Abandon(bray_ir::MirAbandonmentAction::Destroy)
                    | MirGeneratedLifecycleRole::Cleanup(
                        bray_ir::MirCleanupPhase::LifecycleResolution
                    )
            );

            if consumes {
                self.set_storage_initialized(block, source, &place, false)?;

                let runtime = self.runtime_reference(
                    bray_runtime_interface::RuntimeAbiRole::InactiveCaptureDestruction,
                );

                self.push_operation(
                    block,
                    Self::retained_source(source),
                    MirOperationKind::Async(bray_ir::MirAsyncOperation::DestroyInactiveCaptures {
                        frame: MirOperand::Move(place),
                        runtime,
                    }),
                    None,
                )?;

                return Ok((self.check_cleanup_action_outcome(block, source)?, value));
            }

            // An unstarted ordinary call with inert captures has no owned run to quiesce.
            return Ok((block, value));
        }

        if self.destructor_remainder
            && matches!(
                role,
                MirGeneratedLifecycleRole::Destroy
                    | MirGeneratedLifecycleRole::Cleanup(
                        bray_ir::MirCleanupPhase::LifecycleResolution
                    )
            )
        {
            self.push_operation(
                block,
                Self::retained_source(source),
                MirOperationKind::DestructorRemainder { role, place },
                None,
            )?;

            return Ok((self.check_cleanup_action_outcome(block, source)?, value));
        }

        let shape = self
            .input
            .lowering_plans()
            .cleanup_type(place.ty())
            .ok_or(LoweringError::MissingCleanupExecution(place.storage()))?;

        let execution = if synchronous_destruction {
            match role {
                MirGeneratedLifecycleRole::Destroy => Some(CallableExecution::Synchronous),
                MirGeneratedLifecycleRole::Cleanup(
                    bray_ir::MirCleanupPhase::LifecycleResolution,
                ) => shape.finalization_execution(),
                _ => role.execution(shape),
            }
        } else {
            role.execution(shape)
        };

        if execution == Some(CallableExecution::Synchronous)
            || (execution.is_none() && self.builder.protected_frame().is_none())
        {
            let operation = cleanup_operation(role, place)?;
            self.push_operation(block, Self::retained_source(source), operation, None)?;

            return Ok((self.check_cleanup_action_outcome(block, source)?, value));
        }

        let pending = value
            .map(|(value, ty)| {
                let storage = self.builder.push_storage(
                    Self::retained_source(source),
                    MirStorageKind::Temporary,
                    ty,
                )?;

                let place = MirPlace::new(storage, [], ty);

                self.push_operation(
                    block,
                    Self::retained_source(source),
                    MirOperationKind::Store {
                        kind: MirStoreKind::Initialize,
                        destination: Self::retained_place(&place),
                        value: MirOperand::Value(value),
                    },
                    None,
                )?;

                Ok::<_, LoweringError>(place)
            })
            .transpose()?;

        let own_outcome = self.cleanup_outcome.is_none();

        if own_outcome {
            self.cleanup_outcome = Some(self.create_cleanup_outcome(block, source)?);
        }

        let finished = if execution.is_none() {
            // Open templates retain the semantic action. Concrete lowering selects its execution
            // mode after substitution, with the same guarded ownership and saved continuation value.
            if role != MirGeneratedLifecycleRole::Abandon(bray_ir::MirAbandonmentAction::Quiesce) {
                self.set_storage_initialized(block, source, &place, false)?;
            }

            let operation = cleanup_operation(role, place)?;
            self.push_operation(block, Self::retained_source(source), operation, None)?;

            self.check_cleanup_action_outcome(block, source)?
        } else {
            self.resolve_async_cleanup(block, source, role, &place, pending.as_ref())?
        };

        let finished = if own_outcome {
            self.finish_ordinary_cleanup_await(finished, source)?
        } else {
            finished
        };

        let Some(pending) = pending else {
            return Ok((finished, None));
        };

        let continuation = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::LifecycleResolution,
        )?;

        let value = self.builder.push_block_parameter(
            continuation,
            Self::retained_source(source),
            pending.ty(),
        )?;

        let ty = pending.ty();

        self.set_terminator(
            finished,
            Self::retained_source(source),
            MirTerminatorKind::Goto(MirEdge::new(continuation, [MirOperand::Move(pending)])),
        )?;

        Ok((continuation, Some((value, ty))))
    }

    fn resolve_async_cleanup(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        role: MirGeneratedLifecycleRole,
        place: &MirPlace,
        pending: Option<&MirPlace>,
    ) -> Result<MirBlockId, LoweringError> {
        if role == MirGeneratedLifecycleRole::Abandon(bray_ir::MirAbandonmentAction::Quiesce) {
            let completion = self
                .input
                .available_compiler_known_symbols()
                .unary_representation_argument(
                    self.input.semantic_values(),
                    RepresentationRole::Task,
                    place.ty(),
                )?;

            if let Some(completion) = completion {
                if let Some(pending) = pending {
                    self.cleanup_retained_storages.push(pending.storage());
                }

                let finished = self.quiesce_task(block, source, place, completion)?;

                if pending.is_some() {
                    self.cleanup_retained_storages.pop();
                }

                return Ok(finished);
            }
        }

        let (block, rejected, awaited, completion) =
            self.prepare_cleanup_await(block, source, role, place)?;

        let resolves_future = matches!(
            &awaited,
            crate::cleanup_await::CleanupAwait::Frame(_, bray_ir::MirFrameEntry::CaptureCleanup)
        );

        let resolves_task = matches!(&awaited, crate::cleanup_await::CleanupAwait::Task(_));

        if role != MirGeneratedLifecycleRole::Abandon(bray_ir::MirAbandonmentAction::Quiesce) {
            self.set_storage_initialized(block, source, place, false)?;
        }

        if let Some(pending) = pending {
            self.cleanup_retained_storages.push(pending.storage());
        }

        let (block, result, variants) =
            self.await_cleanup_frame(block, source, awaited, completion)?;

        let outcome = self
            .cleanup_outcome
            .as_ref()
            .ok_or(LoweringError::SemanticValueUnavailable)?;

        let result_storage = result.storage();

        let (completed, finished, payload) = if resolves_future || resolves_task {
            let (completed, finished, payload) = outcome.resolve_completion(
                &mut self.builder,
                block,
                source,
                result,
                (variants, completion),
            )?;

            (completed, finished, Some(payload))
        } else {
            let (completed, finished) = outcome.resolve_run_result(
                &mut self.builder,
                block,
                source,
                result,
                (
                    variants,
                    crate::cleanup_outcome::CleanupCancellation::Propagate,
                ),
            )?;

            (completed, finished, None)
        };

        if let Some(rejected) = rejected {
            outcome.retain_allocation_failure(&mut self.builder, rejected, source, finished)?;
        }

        let completed = if let Some(payload) = payload {
            self.cleanup_retained_storages.push(result_storage);

            let (completed, _) = self.push_lifecycle_cleanup(
                completed,
                source,
                MirGeneratedLifecycleRole::Cleanup(bray_ir::MirCleanupPhase::LifecycleResolution),
                payload,
                None,
                false,
            )?;

            self.cleanup_retained_storages.pop();

            completed
        } else {
            completed
        };

        if pending.is_some() {
            self.cleanup_retained_storages.pop();
        }

        self.set_terminator(
            completed,
            Self::retained_source(source),
            MirTerminatorKind::Goto(MirEdge::new(finished, [])),
        )?;

        Ok(finished)
    }

    fn future_has_cleanup_free_captures(&self, place: &MirPlace) -> bool {
        place.projections().is_empty()
            && self.storages.iter().any(|(identity, storage)| {
                *storage == place.storage()
                    && self
                        .input
                        .lowering_plans()
                        .future_has_cleanup_free_captures(*identity)
            })
    }

    fn finish_ordinary_cleanup_await(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
    ) -> Result<MirBlockId, LoweringError> {
        let outcome = self
            .cleanup_outcome
            .take()
            .ok_or(LoweringError::SemanticValueUnavailable)?;

        let (panicked, cancelled, _) = self
            .cleanup_failure_targets
            .ok_or(LoweringError::SemanticValueUnavailable)?;

        let bridge = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::LifecycleResolution,
        )?;

        let completed = outcome.dispatch(&mut self.builder, block, source, bridge, cancelled)?;

        self.set_terminator(
            bridge,
            Self::retained_source(source),
            MirTerminatorKind::Goto(MirEdge::new(panicked, [outcome.report()])),
        )?;

        Ok(completed)
    }
}

fn cleanup_operation(
    role: MirGeneratedLifecycleRole,
    place: MirPlace,
) -> Result<MirOperationKind, LoweringError> {
    match role {
        MirGeneratedLifecycleRole::Destroy => Ok(MirOperationKind::Destroy(place)),
        MirGeneratedLifecycleRole::Cleanup(phase) => Ok(MirOperationKind::Cleanup { phase, place }),
        MirGeneratedLifecycleRole::Abandon(action) => {
            Ok(MirOperationKind::Abandon { action, place })
        }
        _ => Err(LoweringError::MissingCleanupExecution(place.storage())),
    }
}
