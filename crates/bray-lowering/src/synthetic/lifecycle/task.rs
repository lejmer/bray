use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirAsyncOperation, MirEdge, MirOperand, MirOperationKind, MirPlace, MirSourceAnchor,
    MirTerminatorKind, MirUnitBuilder,
};

use super::super::{SyntheticLowerer, SyntheticLoweringContext, SyntheticLoweringError};
use crate::cleanup_outcome::CleanupOutcome;

impl<C: SyntheticLoweringContext + ?Sized> SyntheticLowerer<'_, C> {
    pub(in crate::synthetic) fn task_completion_type(
        &self,
        task: bray_symbols::TypeId,
    ) -> Result<bray_symbols::TypeId, C::Error> {
        self.context
            .compiler_known_symbols()
            .unary_representation_argument(
                self.context.semantic_values(),
                RepresentationRole::Task,
                task,
            )
            .map_err(SyntheticLoweringError::SemanticValue)?
            .ok_or_else(|| {
                SyntheticLoweringError::MissingRepresentation {
                    role: RepresentationRole::Task,
                    argument: Some(task),
                }
                .into()
            })
    }

    pub(super) fn push_task_quiescence(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        task: MirPlace,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let outcome = self.cleanup_outcome(builder, block, source)?;
        let finished = self.await_task_quiescence(builder, block, source, task, &outcome)?;

        self.finish_cleanup_outcome(builder, finished, source, &outcome)
    }

    pub(super) fn await_task_quiescence(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        task: MirPlace,
        outcome: &CleanupOutcome,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let completion = self.task_completion_type(task.ty())?;

        let state = self.next_lifecycle_state(builder, source)?;

        // Suspension, payload observation and borrow release each retain the same owner's path.
        let resumed = crate::cleanup_await::suspend_cleanup(
            builder,
            block,
            source,
            state,
            bray_ir::MirSuspensionKind::TaskCompletion,
            Some(MirOperand::Copy(task.clone())),
        )
        .map_err(|cause| self.mir_error(source, cause))?;

        self.quiesce_task_completion(builder, resumed, source, task, completion, outcome)
    }

    /// Quiesces a terminal task payload after the caller has waited, retaining its outer outcome.
    pub(crate) fn quiesce_task_completion(
        &self,
        builder: &mut MirUnitBuilder,
        resumed: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        task: MirPlace,
        completion: bray_symbols::TypeId,
        outcome: &CleanupOutcome,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let types = crate::cleanup_await::task_completion_borrow_types(
            self.context.semantic_values(),
            completion,
        )
        .map_err(SyntheticLoweringError::SemanticValue)?;

        // Completion borrowing and its release independently retain the owner path.
        let (completed, finished, payload) = crate::cleanup_await::borrow_task_completion(
            builder,
            resumed,
            source,
            MirOperand::Copy(task.clone()),
            completion,
            types,
        )
        .map_err(|cause| self.mir_error(source, cause))?;

        let cleanup = self.context.cleanup_type_execution(completion)?;

        // Borrowed completion tasks were not available during the enclosing owner's broadcast.
        let completed = crate::cleanup_payload::broadcast_cancellation(
            builder,
            completed,
            source,
            payload.clone(),
            cleanup.cleanup(),
            outcome,
        )
        .map_err(|cause| self.mir_error(source, cause))?;

        let completed = self.resolve_lifecycle_action(
            builder,
            completed,
            source,
            MirOperationKind::Abandon {
                action: bray_ir::MirAbandonmentAction::Quiesce,
                place: payload,
            },
            outcome,
        )?;

        crate::cleanup_await::release_task_completion_borrow(
            builder,
            completed,
            source,
            MirOperand::Copy(task),
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

    pub(super) fn push_abandoned_task_destruction(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        task: MirPlace,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let completion = self.task_completion_type(task.ty())?;

        self.push_lifecycle_operation(
            builder,
            block,
            source,
            MirOperationKind::Async(MirAsyncOperation::DestroyTerminalTask {
                task: MirOperand::Move(task),
                completion: Some(completion),
            }),
        )?;

        Ok(block)
    }

    pub(in crate::synthetic) fn push_task_resolution(
        &self,
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        task: MirPlace,
        outcome: &CleanupOutcome,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let completion = self.task_completion_type(task.ty())?;

        let (resumed, result_place, variants) = self.await_lifecycle_result(
            builder,
            block,
            source,
            crate::cleanup_await::CleanupAwait::Task(MirOperand::Copy(task)),
            completion,
        )?;

        self.resolve_owner_completion(
            builder,
            resumed,
            source,
            result_place,
            (variants, completion),
            outcome,
        )
    }
}
