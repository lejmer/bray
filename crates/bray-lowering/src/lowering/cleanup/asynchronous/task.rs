use bray_ir::{
    MirBlockId, MirEdge, MirGeneratedLifecycleRole, MirOperand, MirPlace, MirSourceAnchor,
    MirTerminatorKind,
};
use bray_symbols::TypeId;

use crate::lowering::LoweringError;
use crate::lowering::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(super) fn quiesce_task(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        task: &MirPlace,
        completion: TypeId,
    ) -> Result<MirBlockId, LoweringError> {
        let types = crate::cleanup_await::task_completion_borrow_types(
            self.input.semantic_values(),
            completion,
        )?;

        let state = self.next_frame_state()?;
        self.cleanup_retained_storages.push(task.storage());

        let resumed = crate::cleanup_await::suspend_cleanup(
            &mut self.builder,
            block,
            source,
            state,
            bray_ir::MirSuspensionKind::TaskCompletion,
            Some(MirOperand::Copy(Self::retained_place(task))),
        )?;

        self.retain_cleanup_state(state, resumed)?;

        let (completed, finished, payload) = crate::cleanup_await::borrow_task_completion(
            &mut self.builder,
            resumed,
            source,
            MirOperand::Copy(Self::retained_place(task)),
            completion,
            types,
        )?;

        let cleanup = self
            .input
            .lowering_plans()
            .cleanup_type(completion)
            .ok_or(LoweringError::MissingCleanupExecution(payload.storage()))?;

        let outcome = self
            .cleanup_outcome
            .as_ref()
            .ok_or(LoweringError::SemanticValueUnavailable)?;

        let completed = crate::cleanup_payload::broadcast_cancellation(
            &mut self.builder,
            completed,
            source,
            Self::retained_place(&payload),
            cleanup.cleanup(),
            outcome,
        )?;

        self.cleanup_retained_storages.push(payload.storage());

        let (completed, _) = self.push_lifecycle_cleanup(
            completed,
            source,
            MirGeneratedLifecycleRole::Abandon(bray_ir::MirAbandonmentAction::Quiesce),
            payload,
            None,
            false,
        )?;

        self.cleanup_retained_storages.pop();
        self.cleanup_retained_storages.pop();

        crate::cleanup_await::release_task_completion_borrow(
            &mut self.builder,
            completed,
            source,
            MirOperand::Copy(Self::retained_place(task)),
        )?;

        self.set_terminator(
            completed,
            Self::retained_source(source),
            MirTerminatorKind::Goto(MirEdge::new(finished, [])),
        )?;

        Ok(finished)
    }
}
