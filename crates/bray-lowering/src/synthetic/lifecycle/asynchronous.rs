use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirAsyncOperation, MirBlockId, MirBlockKind, MirCleanupEdge, MirCleanupPhase, MirEdge,
    MirFrameStateId, MirImmediateValue, MirOperand, MirOperationKind, MirPlace,
    MirRunResultVariants, MirRuntimeReference, MirSourceAnchor, MirTaskTerminalState,
    MirTerminatorKind, MirUnitBuildError, MirUnitBuilder,
};
use bray_runtime_interface::{ProtectedAsyncFrameId, RuntimeAbiRole};
use bray_symbols::TypeId;

use super::super::{SyntheticLowerer, SyntheticLoweringContext, SyntheticLoweringError};

impl<C: SyntheticLoweringContext + ?Sized> SyntheticLowerer<'_, C> {
    pub(super) fn resolve_lifecycle_action(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        operation: MirOperationKind,
        outcome: &crate::cleanup_outcome::CleanupOutcome,
    ) -> Result<MirBlockId, C::Error> {
        let (role, place) = match &operation {
            MirOperationKind::Finalize(place) => {
                (bray_ir::MirGeneratedLifecycleRole::Finalize, place)
            }
            MirOperationKind::Destroy(place) => {
                (bray_ir::MirGeneratedLifecycleRole::Destroy, place)
            }
            MirOperationKind::Abandon { action, place } => {
                (bray_ir::MirGeneratedLifecycleRole::Abandon(*action), place)
            }
            MirOperationKind::Cleanup { phase, place } => {
                (bray_ir::MirGeneratedLifecycleRole::Cleanup(*phase), place)
            }
            _ => {
                self.push_lifecycle_operation(builder, block, source, operation)?;

                return outcome
                    .check(builder, block, source)
                    .map_err(|cause| self.mir_error(source, cause));
            }
        };

        let ty = place.ty();

        if role == bray_ir::MirGeneratedLifecycleRole::Finalize
            && self.context.finalization_complete(ty)?
        {
            return Ok(block);
        }

        let cleanup = self.context.cleanup_type_execution(ty)?;

        let execution = role
            .execution(&cleanup)
            .ok_or(SyntheticLoweringError::UnresolvedType(ty))?;

        if execution == bray_symbols::CallableExecution::Synchronous {
            self.push_lifecycle_operation(builder, block, source, operation)?;

            return outcome
                .check(builder, block, source)
                .map_err(|cause| self.mir_error(source, cause));
        }

        let (block, rejected, value) =
            self.create_lifecycle_frame(builder, block, source, role, place.clone())?;

        let completion = self.context.representation_type(RepresentationRole::Unit)?;

        let (block, result, variants) = self.await_lifecycle_result(
            builder,
            block,
            source,
            crate::cleanup_await::CleanupAwait::Frame(
                MirOperand::Move(value),
                bray_ir::MirFrameEntry::Body,
            ),
            completion,
        )?;

        let (completed, finished) = outcome
            .resolve_run_result(
                builder,
                block,
                source,
                result,
                (
                    variants,
                    crate::cleanup_outcome::CleanupCancellation::Propagate,
                ),
            )
            .map_err(|cause| self.mir_error(source, cause))?;

        builder
            .set_terminator(
                completed,
                source.clone(),
                MirTerminatorKind::Goto(MirEdge::new(finished, [])),
            )
            .map_err(|cause| self.mir_error(source, cause))?;

        outcome
            .retain_allocation_failure(builder, rejected, source, finished)
            .map_err(|cause| self.mir_error(source, cause))?;

        Ok(finished)
    }

    pub(super) fn create_lifecycle_frame(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        role: bray_ir::MirGeneratedLifecycleRole,
        place: MirPlace,
    ) -> Result<(MirBlockId, MirBlockId, MirPlace), C::Error> {
        let ty = place.ty();

        let pointer = self
            .context
            .semantic_values()
            .intern_type(bray_symbols::TypeData::Borrow {
                kind: bray_symbols::BorrowKind::Mutable,
                target: ty,
            })
            .map_err(SyntheticLoweringError::SemanticValue)?;

        let receiver = self.lifecycle_receiver_operand(builder, block, source, place, pointer)?;
        let completion = self.context.representation_type(RepresentationRole::Unit)?;

        let boolean = self
            .context
            .representation_type(RepresentationRole::ScalarBool)?;

        let future = self
            .context
            .compiler_known_symbols()
            .unary_representation_type(
                self.context.semantic_values(),
                RepresentationRole::Future,
                completion,
            )
            .map_err(SyntheticLoweringError::SemanticValue)?
            .ok_or(SyntheticLoweringError::MissingRepresentation {
                role: RepresentationRole::Future,
                argument: Some(completion),
            })?;

        crate::cleanup_await::create_lifecycle_frame(
            builder,
            block,
            source,
            role,
            ty,
            receiver,
            bray_bound_tree::BoundFutureConstruction::new(completion, future),
            boolean,
        )
        .map_err(|cause| self.mir_error(source, cause))
    }

    pub(super) fn finish_lifecycle_body(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
    ) -> Result<(), C::Error> {
        if builder.protected_frame().is_some() {
            let ty = self.context.representation_type(RepresentationRole::Unit)?;

            let operation = MirAsyncOperation::PublishTerminalState {
                state: MirTaskTerminalState::Completed(MirOperand::Immediate {
                    value: MirImmediateValue::Unit,
                    ty,
                }),
                runtime: MirRuntimeReference::new(
                    RuntimeAbiRole::TerminalPublication,
                    builder.target().runtime_abi(),
                ),
            };

            self.push_lifecycle_operation(
                builder,
                block,
                source,
                MirOperationKind::Async(operation),
            )?;
        }

        builder
            .set_terminator(block, source.clone(), MirTerminatorKind::Return(None))
            .map_err(|cause| self.mir_error(source, cause))
    }

    pub(crate) fn attach_lifecycle_frame(
        &self,
        builder: &mut MirUnitBuilder,
        frame: ProtectedAsyncFrameId,
        entry: MirBlockId,
        receiver: MirPlace,
        source: &MirSourceAnchor,
    ) -> Result<(), C::Error> {
        if builder.protected_frame() != Some(frame) {
            return Err(self.mir_error(source, MirUnitBuildError::ProtectedFrameMismatch));
        }

        let abi = builder.target().runtime_abi();
        let result = self.context.representation_type(RepresentationRole::Unit)?;

        // Generated lifecycle frames capture a borrowed receiver. Inactive disposal releases
        // that borrow without invoking the lifecycle operation on the original owner's value.
        let captures = builder
            .push_block(source.clone(), MirBlockKind::CleanupBroadcast)
            .map_err(|cause| self.mir_error(source, cause))?;

        let completed = builder
            .push_block(source.clone(), MirBlockKind::LifecycleResolution)
            .map_err(|cause| self.mir_error(source, cause))?;

        builder
            .set_terminator(
                captures,
                source.clone(),
                MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                    MirCleanupPhase::LifecycleResolution,
                    MirEdge::new(completed, []),
                )),
            )
            .map_err(|cause| self.mir_error(source, cause))?;

        self.push_lifecycle_operation(
            builder,
            completed,
            source,
            MirOperationKind::Async(MirAsyncOperation::PublishTerminalState {
                state: MirTaskTerminalState::Cancelled,
                runtime: MirRuntimeReference::new(RuntimeAbiRole::TerminalPublication, abi),
            }),
        )?;

        builder
            .set_terminator(completed, source.clone(), MirTerminatorKind::Return(None))
            .map_err(|cause| self.mir_error(source, cause))?;

        self.attach_frame_descriptor(builder, entry, receiver, result, captures, source)
    }

    pub(super) fn await_lifecycle_result(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        awaited: crate::cleanup_await::CleanupAwait,
        completion: TypeId,
    ) -> Result<(MirBlockId, MirPlace, MirRunResultVariants), C::Error> {
        let block = self.lifecycle_resolution_block(builder, block, source)?;

        let next = self.next_lifecycle_state(builder, source)?;

        let (result, variants) = self.run_result(completion)?;

        let (resume, place) = crate::cleanup_await::await_cleanup(
            builder, block, source, next, awaited, result, variants,
        )
        .map_err(|cause| self.mir_error(source, cause))?;

        Ok((resume, place, variants))
    }

    pub(super) fn next_lifecycle_state(
        &self,
        builder: &MirUnitBuilder,
        source: &MirSourceAnchor,
    ) -> Result<MirFrameStateId, C::Error> {
        let next = builder
            .suspension_states()
            .map(|(state, _)| state.raw())
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| self.mir_error(source, MirUnitBuildError::IdentityCapacityExceeded))?;

        Ok(MirFrameStateId::new(next))
    }

    pub(in crate::synthetic) fn lifecycle_resolution_block(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
    ) -> Result<MirBlockId, C::Error> {
        let kind = builder
            .block_kind(block)
            .map_err(|cause| self.mir_error(source, cause))?;

        if kind == MirBlockKind::LifecycleResolution {
            return Ok(block);
        }

        if kind == MirBlockKind::CleanupBroadcast {
            return Err(SyntheticLoweringError::UnsupportedLifecycleRole(
                bray_ir::MirGeneratedLifecycleRole::Cleanup(MirCleanupPhase::TaskCancellation),
            )
            .into());
        }

        let broadcast = builder
            .push_block(source.clone(), MirBlockKind::CleanupBroadcast)
            .map_err(|cause| self.mir_error(source, cause))?;

        let lifecycle = builder
            .push_block(source.clone(), MirBlockKind::LifecycleResolution)
            .map_err(|cause| self.mir_error(source, cause))?;

        builder
            .set_terminator(
                block,
                source.clone(),
                MirTerminatorKind::BeginCleanup(MirCleanupEdge::new(
                    MirCleanupPhase::TaskCancellation,
                    MirEdge::new(broadcast, []),
                )),
            )
            .map_err(|cause| self.mir_error(source, cause))?;

        builder
            .set_terminator(
                broadcast,
                source.clone(),
                MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                    MirCleanupPhase::LifecycleResolution,
                    MirEdge::new(lifecycle, []),
                )),
            )
            .map_err(|cause| self.mir_error(source, cause))?;

        Ok(lifecycle)
    }
}
