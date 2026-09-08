use bray_bound_tree::BoundFutureConstruction;
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirBlockKind, MirEdge, MirFrameState, MirGeneratedLifecycleRole, MirOperand,
    MirOperationKind, MirPlace, MirRunResultVariants, MirSourceAnchor, MirStorageKind,
    MirStoreKind, MirTerminatorKind, MirUnitBuildError, MirValueId,
};
use bray_symbols::{BorrowKind, CallableExecution, TypeData, TypeId};

use crate::lowering::LoweringError;
use crate::lowering::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(super) fn push_lifecycle_cleanup(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        role: MirGeneratedLifecycleRole,
        place: MirPlace,
        value: Option<(MirValueId, TypeId)>,
        synchronous_destruction: bool,
    ) -> Result<(MirBlockId, Option<(MirValueId, TypeId)>), LoweringError> {
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
            let future = self.create_cleanup_frame(block, source, role, &place)?;

            if role != MirGeneratedLifecycleRole::Abandon(bray_ir::MirAbandonmentAction::Quiesce) {
                self.set_storage_initialized(block, source, &place, false)?;
            }

            if let Some(pending) = &pending {
                self.cleanup_retained_storages.push(pending.storage());
            }

            let (block, result, variants) = self.await_cleanup_frame(block, source, future)?;

            if pending.is_some() {
                self.cleanup_retained_storages.pop();
            }

            let outcome = self
                .cleanup_outcome
                .as_ref()
                .ok_or(LoweringError::SemanticValueUnavailable)?;

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

            self.set_terminator(
                completed,
                Self::retained_source(source),
                MirTerminatorKind::Goto(MirEdge::new(finished, [])),
            )?;

            finished
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

    fn create_cleanup_frame(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        role: MirGeneratedLifecycleRole,
        place: &MirPlace,
    ) -> Result<MirOperand, LoweringError> {
        let ty = place.ty();

        let receiver = self.input.semantic_values().intern_type(TypeData::Borrow {
            kind: BorrowKind::Mutable,
            target: ty,
        })?;

        let borrow = self.push_operation(
            block,
            Self::retained_source(source),
            MirOperationKind::Borrow {
                kind: BorrowKind::Mutable,
                place: Self::retained_place(place),
            },
            Some(receiver),
        )?;

        let receiver = MirOperand::Value(borrow.result().ok_or(
            MirUnitBuildError::MissingOperationResult(borrow.operation()),
        )?);

        let completion = self.representation_type(RepresentationRole::Unit)?;
        let future = self.unary_representation_type(RepresentationRole::Future, completion)?;

        crate::cleanup_await::create_lifecycle_frame(
            &mut self.builder,
            block,
            source,
            role,
            ty,
            receiver,
            BoundFutureConstruction::new(completion, future),
        )
        .map_err(Into::into)
    }

    fn await_cleanup_frame(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        future: MirOperand,
    ) -> Result<(MirBlockId, MirPlace, MirRunResultVariants), LoweringError> {
        let completion = self.representation_type(RepresentationRole::Unit)?;
        let result = self.unary_representation_type(RepresentationRole::RunResult, completion)?;
        let representation = self.run_result_representation()?;

        let variants = MirRunResultVariants::new(
            representation.completed_variant,
            representation.panicked_variant,
            representation.cancelled_variant,
        );

        let state = self.next_frame_state()?;

        let (resume, result) = crate::cleanup_await::await_cleanup(
            &mut self.builder,
            block,
            source,
            state,
            crate::cleanup_await::CleanupAwait::Frame(future, bray_ir::MirFrameEntry::Body),
            result,
            variants,
        )?;

        let mut storages =
            self.retained_storages(self.input.lowering_plans().frame_dependencies())?;

        storages.extend(self.cleanup_retained_storages.iter().copied());
        storages.sort_unstable();
        storages.dedup();

        self.frame_states.push(
            MirFrameState::new(state, resume, self.execution_lane_requirements(), storages)
                .with_affinity(self.frame_affinity()),
        );

        Ok((resume, result, variants))
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
