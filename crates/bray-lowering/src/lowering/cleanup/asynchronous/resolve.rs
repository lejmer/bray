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
            let retained = [place.storage()];

            self.push_cleanup_operation(
                block,
                Self::retained_source(source),
                MirOperationKind::DestructorRemainder { role, place },
                retained,
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
            let retained = [place.storage()];
            let operation = cleanup_operation(role, place)?;
            self.push_cleanup_operation(block, Self::retained_source(source), operation, retained)?;

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

        // Realization selects the closed execution path while source lowering retains guards,
        // pending values, and the exact lexical execution requirements on the semantic action.
        if role != MirGeneratedLifecycleRole::Abandon(bray_ir::MirAbandonmentAction::Quiesce) {
            self.set_storage_initialized(block, source, &place, false)?;
        }

        let retained =
            std::iter::once(place.storage()).chain(pending.as_ref().map(MirPlace::storage));

        let operation = cleanup_operation(role, place)?;
        self.push_cleanup_operation(block, Self::retained_source(source), operation, retained)?;
        let finished = self.check_cleanup_action_outcome(block, source)?;

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

    pub(in crate::lowering::cleanup) fn resolve_cleanup_payload(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        storage: bray_ir::MirStorageId,
        payload: MirPlace,
    ) -> Result<MirBlockId, LoweringError> {
        let cleanup = self
            .input
            .lowering_plans()
            .cleanup_type(payload.ty())
            .ok_or(LoweringError::MissingCleanupExecution(payload.storage()))?;

        let outcome = self
            .cleanup_outcome
            .as_ref()
            .ok_or(LoweringError::SemanticValueUnavailable)?;

        let block = crate::cleanup_payload::broadcast_cancellation(
            &mut self.builder,
            block,
            source,
            Self::retained_place(&payload),
            cleanup.cleanup(),
            outcome,
        )?;

        self.cleanup_retained_storages.push(storage);

        let result = self.push_lifecycle_cleanup(
            block,
            source,
            MirGeneratedLifecycleRole::Cleanup(bray_ir::MirCleanupPhase::LifecycleResolution),
            payload,
            None,
            false,
        );

        self.cleanup_retained_storages.pop();

        result.map(|(block, _)| block)
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
    let storage = place.storage();

    if matches!(
        role,
        MirGeneratedLifecycleRole::Finalize | MirGeneratedLifecycleRole::StaticFinalize
    ) {
        return Err(LoweringError::MissingCleanupExecution(storage));
    }

    role.operation(place)
        .ok_or(LoweringError::MissingCleanupExecution(storage))
}
