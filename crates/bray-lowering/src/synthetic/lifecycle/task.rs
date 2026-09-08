use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirAsyncOperation, MirEdge, MirOperand, MirOperationKind, MirPlace, MirProjectionKind,
    MirRuntimeReference, MirSourceAnchor, MirTerminatorKind, MirUnitBuilder,
};
use bray_runtime_interface::RuntimeAbiRole;

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
        runtime_abi: bray_runtime_interface::RuntimeAbiVersion,
    ) -> Result<bray_ir::MirBlockId, C::Error> {
        let completion = self.task_completion_type(task.ty())?;
        let values = self.context.semantic_values();

        let pointer = values
            .intern_type(bray_symbols::TypeData::Borrow {
                kind: bray_symbols::BorrowKind::Mutable,
                target: completion,
            })
            .map_err(SyntheticLoweringError::SemanticValue)?;

        let nullable = values
            .intern_type(bray_symbols::TypeData::Nullable(pointer))
            .map_err(SyntheticLoweringError::SemanticValue)?;

        let outcome = self.cleanup_outcome(builder, block, source)?;
        let state = self.next_lifecycle_state(builder, source)?;

        let resumed = crate::cleanup_await::suspend_cleanup(
            builder,
            block,
            source,
            state,
            bray_ir::MirSuspensionKind::TaskCompletion,
            Some(MirOperand::Copy(task.clone())),
        )
        .map_err(|cause| self.mir_error(source, cause))?;

        let borrowed = builder
            .push_operation(
                resumed,
                source.clone(),
                MirOperationKind::Async(MirAsyncOperation::BorrowTaskCompletion {
                    task: MirOperand::Copy(task.clone()),
                    runtime: MirRuntimeReference::new(
                        RuntimeAbiRole::TaskCompletionBorrow,
                        runtime_abi,
                    ),
                }),
                Some(nullable),
            )
            .map_err(|cause| self.mir_error(source, cause))?;

        let value =
            borrowed
                .result()
                .ok_or_else(|| SyntheticLoweringError::MissingOperationResult {
                    source: source.clone(),
                    operation: borrowed.operation(),
                })?;

        let storage = builder
            .push_storage(source.clone(), bray_ir::MirStorageKind::Temporary, nullable)
            .map_err(|cause| self.mir_error(source, cause))?;

        let borrowed = MirPlace::new(storage, [], nullable);

        self.push_lifecycle_operation(
            builder,
            resumed,
            source,
            MirOperationKind::Store {
                kind: bray_ir::MirStoreKind::Initialize,
                destination: borrowed.clone(),
                value: MirOperand::Value(value),
            },
        )?;

        let completed = builder
            .push_block(source.clone(), bray_ir::MirBlockKind::LifecycleResolution)
            .map_err(|cause| self.mir_error(source, cause))?;

        let finished = builder
            .push_block(source.clone(), bray_ir::MirBlockKind::LifecycleResolution)
            .map_err(|cause| self.mir_error(source, cause))?;

        builder
            .set_terminator(
                resumed,
                source.clone(),
                MirTerminatorKind::PatternBranch {
                    subject: MirOperand::Copy(borrowed.clone()),
                    predicate: bray_ir::MirPatternPredicate::NullablePresent,
                    matched: MirEdge::new(completed, []),
                    unmatched: MirEdge::new(finished, []),
                },
            )
            .map_err(|cause| self.mir_error(source, cause))?;

        let completion = borrowed
            .project(MirProjectionKind::NullableValue, pointer)
            .project(MirProjectionKind::Dereference, completion);

        let completed = self.resolve_lifecycle_action(
            builder,
            completed,
            source,
            MirOperationKind::Abandon {
                action: bray_ir::MirAbandonmentAction::Quiesce,
                place: completion,
            },
            &outcome,
        )?;

        // Every source outcome rejoins here before the borrowed owner's outcome propagates.
        self.push_lifecycle_operation(
            builder,
            completed,
            source,
            MirOperationKind::Async(MirAsyncOperation::ReleaseTaskCompletionBorrow {
                task: MirOperand::Copy(task),
                runtime: MirRuntimeReference::new(
                    RuntimeAbiRole::TaskCompletionBorrowRelease,
                    runtime_abi,
                ),
            }),
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

        // The outcome branch consumes the selected tag while payload cleanup retains its projection.
        let (completed, finished) = outcome
            .resolve_run_result(
                builder,
                resumed,
                source,
                result_place.clone(),
                (
                    variants,
                    crate::cleanup_outcome::CleanupCancellation::Propagate,
                ),
            )
            .map_err(|cause| self.mir_error(source, cause))?;

        let completion_place = result_place.project(
            MirProjectionKind::ActiveUnionPayloadElement {
                variant: variants.completed(),
                ordinal: bray_symbols::SymbolOrdinal::new(0),
            },
            completion,
        );

        let completed = self.resolve_lifecycle_action(
            builder,
            completed,
            source,
            MirOperationKind::Cleanup {
                phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                place: completion_place,
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
