use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirCleanupPhase, MirEdge, MirFrameEntry, MirOperand, MirOperationKind, MirPlace,
    MirSourceAnchor, MirTerminatorKind, MirUnitBuilder,
};

use super::super::{SyntheticLowerer, SyntheticLoweringContext, SyntheticLoweringError};
use crate::cleanup_outcome::CleanupCancellation;

impl<C: SyntheticLoweringContext + ?Sized> SyntheticLowerer<'_, C> {
    pub(super) fn quiesce_inactive_future(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        future: MirPlace,
    ) -> Result<MirBlockId, C::Error> {
        let outcome = self.cleanup_outcome(builder, block, source)?;
        let finished = self.await_future_quiescence(builder, block, source, future, &outcome)?;

        self.finish_cleanup_outcome(builder, finished, source, &outcome)
    }

    pub(super) fn await_future_quiescence(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        future: MirPlace,
        outcome: &crate::cleanup_outcome::CleanupOutcome,
    ) -> Result<MirBlockId, C::Error> {
        let completion = self.context.representation_type(RepresentationRole::Unit)?;

        let (block, result, variants) = self.await_lifecycle_result(
            builder,
            block,
            source,
            crate::cleanup_await::CleanupAwait::Frame(
                MirOperand::Copy(future),
                MirFrameEntry::CaptureQuiescence,
            ),
            completion,
        )?;

        let (completed, finished) = outcome
            .resolve_run_result(
                builder,
                block,
                source,
                result,
                (variants, CleanupCancellation::Propagate),
            )
            .map_err(|cause| self.mir_error(source, cause))?;

        builder
            .set_terminator(
                completed,
                source.clone(),
                MirTerminatorKind::Goto(MirEdge::new(finished, [])),
            )
            .map_err(|cause| self.mir_error(source, cause))?;

        Ok(finished)
    }

    pub(super) fn destroy_inactive_captures(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        future: MirPlace,
    ) -> Result<MirBlockId, C::Error> {
        let runtime = bray_ir::MirRuntimeReference::new(
            bray_runtime_interface::RuntimeAbiRole::InactiveCaptureDestruction,
            builder.target().runtime_abi(),
        );

        self.resolve_lifecycle_sequence(
            builder,
            block,
            source,
            [MirOperationKind::Async(
                bray_ir::MirAsyncOperation::DestroyInactiveCaptures {
                    frame: MirOperand::Move(future),
                    runtime,
                },
            )],
        )
    }

    pub(super) fn resolve_inactive_future(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        future: MirPlace,
    ) -> Result<MirBlockId, C::Error> {
        let outcome = self.cleanup_outcome(builder, block, source)?;
        let finished = self.await_future_cleanup(builder, block, source, future, &outcome)?;

        self.finish_cleanup_outcome(builder, finished, source, &outcome)
    }

    pub(super) fn await_future_cleanup(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        future: MirPlace,
        outcome: &crate::cleanup_outcome::CleanupOutcome,
    ) -> Result<MirBlockId, C::Error> {
        let completion = self
            .context
            .compiler_known_symbols()
            .unary_representation_argument(
                self.context.semantic_values(),
                RepresentationRole::Future,
                future.ty(),
            )
            .map_err(SyntheticLoweringError::SemanticValue)?
            .ok_or(SyntheticLoweringError::MissingRepresentation {
                role: RepresentationRole::Future,
                argument: Some(future.ty()),
            })?;

        let (block, result, variants) = self.await_lifecycle_result(
            builder,
            block,
            source,
            crate::cleanup_await::CleanupAwait::Frame(
                MirOperand::Move(future),
                MirFrameEntry::CaptureCleanup,
            ),
            completion,
        )?;

        let (completed, finished, payload) = outcome
            .resolve_inactive_completion(builder, block, source, result, (variants, completion))
            .map_err(|cause| self.mir_error(source, cause))?;

        let completed = self.resolve_lifecycle_action(
            builder,
            completed,
            source,
            MirOperationKind::Cleanup {
                phase: MirCleanupPhase::LifecycleResolution,
                place: payload,
            },
            outcome,
        )?;

        builder
            .set_terminator(
                completed,
                source.clone(),
                MirTerminatorKind::Goto(MirEdge::new(finished, [])),
            )
            .map_err(|cause| self.mir_error(source, cause))?;

        Ok(finished)
    }
}
