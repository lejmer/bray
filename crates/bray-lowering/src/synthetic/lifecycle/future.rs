use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirCleanupPhase, MirEdge, MirFrameEntry, MirOperand, MirOperationKind, MirPlace,
    MirProjectionKind, MirSourceAnchor, MirTerminatorKind, MirUnitBuilder,
};
use bray_symbols::SymbolOrdinal;

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
        let completion = self.context.representation_type(RepresentationRole::Unit)?;
        let outcome = self.cleanup_outcome(builder, block, source)?;

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

        self.finish_cleanup_outcome(builder, finished, source, &outcome)
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

        let outcome = self.cleanup_outcome(builder, block, source)?;

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

        // A task-observation future can own a terminal value even when its body was never driven.
        // Retain the result path for payload cleanup after the outcome branch consumes its tag.
        let (completed, finished) = outcome
            .resolve_run_result(
                builder,
                block,
                source,
                result.clone(),
                (variants, CleanupCancellation::Resolved),
            )
            .map_err(|cause| self.mir_error(source, cause))?;

        let payload = result.project(
            MirProjectionKind::ActiveUnionPayloadElement {
                variant: variants.completed(),
                ordinal: SymbolOrdinal::new(0),
            },
            completion,
        );

        let completed = self.resolve_lifecycle_action(
            builder,
            completed,
            source,
            MirOperationKind::Cleanup {
                phase: MirCleanupPhase::LifecycleResolution,
                place: payload,
            },
            &outcome,
        )?;

        builder
            .set_terminator(
                completed,
                source.clone(),
                MirTerminatorKind::Goto(MirEdge::new(finished, [])),
            )
            .map_err(|cause| self.mir_error(source, cause))?;

        self.finish_cleanup_outcome(builder, finished, source, &outcome)
    }
}
