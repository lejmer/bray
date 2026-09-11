use bray_ir::{
    MirBlockId, MirFrameExecutionState, MirOperationCommit, MirOperationKind, MirSourceAnchor,
    MirStorageId,
};

use crate::lowering::LoweringError;
use crate::lowering::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(in crate::lowering) fn cleanup_execution(
        &mut self,
        additional: impl IntoIterator<Item = MirStorageId>,
    ) -> Result<MirFrameExecutionState, LoweringError> {
        let mut storages =
            self.retained_storages(self.input.lowering_plans().frame_dependencies())?;

        storages.extend(self.cleanup_retained_storages.iter().copied());
        storages.extend(additional);

        if let Some(outcome) = &self.cleanup_outcome {
            storages.extend(outcome.retained_storages());
        }

        Ok(
            MirFrameExecutionState::new(self.execution_lane_requirements(), storages)
                .with_affinity(self.frame_affinity()),
        )
    }

    pub(in crate::lowering) fn push_cleanup_operation(
        &mut self,
        block: MirBlockId,
        source: MirSourceAnchor,
        kind: MirOperationKind,
        retained: impl IntoIterator<Item = MirStorageId>,
    ) -> Result<MirOperationCommit, LoweringError> {
        let commit = self.push_operation(block, source, kind, None)?;

        // A synchronous destructor template can acquire suspended remainder cleanup during
        // concrete adaptation, so checked context cannot depend on already owning a frame.
        let execution = self.cleanup_execution(retained)?;

        self.builder
            .set_cleanup_execution(commit.operation(), Some(execution))?;

        Ok(commit)
    }
}
