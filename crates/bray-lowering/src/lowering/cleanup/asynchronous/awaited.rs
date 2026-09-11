use bray_compiler_known::RepresentationRole;
use bray_ir::{MirBlockId, MirFrameState, MirPlace, MirRunResultVariants, MirSourceAnchor};
use bray_symbols::TypeId;

use crate::lowering::LoweringError;
use crate::lowering::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(in crate::lowering::cleanup) fn await_cleanup_frame(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        awaited: crate::cleanup_await::CleanupAwait,
        completion: TypeId,
    ) -> Result<(MirBlockId, MirPlace, MirRunResultVariants), LoweringError> {
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
            awaited,
            result,
            variants,
        )?;

        self.retain_cleanup_state(state, resume)?;

        Ok((resume, result, variants))
    }

    pub(super) fn retain_cleanup_state(
        &mut self,
        state: bray_ir::MirFrameStateId,
        resume: MirBlockId,
    ) -> Result<(), LoweringError> {
        let execution = self.cleanup_execution([])?;

        self.frame_states
            .push(MirFrameState::new(state, resume, execution));

        Ok(())
    }
}
