use bray_bound_tree::BoundCallResult;
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirCall, MirCallTarget, MirEdge, MirOperationKind, MirSourceAnchor,
    MirTerminatorKind,
};
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::TypeId;

use crate::lowering::LoweringError;
use crate::lowering::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(super) fn request_awaited_cancellation(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
    ) -> Result<(), LoweringError> {
        let unit = self.representation_type(RepresentationRole::Unit)?;

        self.push_operation(
            block,
            Self::retained_source(source),
            MirOperationKind::Call(MirCall::protocol(
                MirCallTarget::Runtime(
                    self.runtime_reference(RuntimeAbiRole::AwaitedFrameCancellationRequest),
                ),
                BoundCallResult::Immediate(unit),
                [],
                [],
            )),
            Some(unit),
        )?;

        Ok(())
    }

    pub(super) fn resolve_cancelled_await(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        completion: TypeId,
    ) -> Result<MirBlockId, LoweringError> {
        // Every owned task has received cancellation before this shielded wait begins.
        // Resolving the existing child releases the parent's slot before payload or owner finalizers.
        let (block, result, variants) = self.await_cleanup_frame(
            block,
            source,
            crate::cleanup_await::CleanupAwait::AttachedFrame,
            completion,
        )?;

        let storage = result.storage();

        let outcome = self
            .cleanup_outcome
            .as_ref()
            .ok_or(LoweringError::SemanticValueUnavailable)?;

        let (completed, finished, payload) = outcome.resolve_completion(
            &mut self.builder,
            block,
            source,
            result,
            (variants, completion),
        )?;

        let completed = self.resolve_cleanup_payload(completed, source, storage, payload)?;

        self.set_terminator(
            completed,
            Self::retained_source(source),
            MirTerminatorKind::Goto(MirEdge::new(finished, [])),
        )?;

        Ok(finished)
    }
}
