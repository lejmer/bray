use bray_ir::{
    MirAsyncOperation, MirBlockId, MirBlockKind, MirCleanupEdge, MirCleanupPhase, MirEdge,
    MirFrameStateId, MirOperand, MirOperationKind, MirPlace, MirRuntimeReference, MirSourceAnchor,
    MirStorageKind, MirSuspensionKind, MirTargetContract, MirTaskTerminalState, MirTerminatorKind,
    MirUnit, MirUnitBuilder, MirUnitId, MirUnitKey, MirUnitKind,
};
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::{CallableDefinitionId, TypeId};

use super::{SyntheticLowerer, SyntheticLoweringContext};

/// The selected consuming observation of an existing task.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskObservationMethod {
    /// Wait for the task's terminal state.
    Join,
    /// Request task cancellation before waiting for its terminal state.
    Cancel,
}

/// Lowers a compiler-provided task observation with owned capture cleanup.
pub fn lower_task_observation<C: SyntheticLoweringContext + ?Sized>(
    context: &C,
    unit: MirUnitId,
    definition: CallableDefinitionId,
    method: TaskObservationMethod,
    task: TypeId,
    target: &MirTargetContract,
) -> Result<MirUnit, C::Error> {
    SyntheticLowerer::new(context).lower_task_observation(unit, definition, method, task, target)
}

impl<C: SyntheticLoweringContext + ?Sized> SyntheticLowerer<'_, C> {
    fn lower_task_observation(
        &self,
        unit: MirUnitId,
        definition: CallableDefinitionId,
        method: TaskObservationMethod,
        task: TypeId,
        target: &MirTargetContract,
    ) -> Result<MirUnit, C::Error> {
        let completion = self.task_completion_type(task)?;

        let (result, variants) = self.run_result(completion)?;

        let key = MirUnitKey::CompilerProvidedCallable(definition);
        let frame = crate::identity::protected_frame_identity(&key, target);
        let source = MirSourceAnchor::CompilerProvidedCallable(definition);
        let invalid = |cause| self.mir_error(&source, cause);

        let mut builder = MirUnitBuilder::for_compiler_provided_callable(
            unit,
            definition,
            MirUnitKind::ProtectedAsyncFrame(frame),
            target.clone(),
        );

        let entry = builder
            .push_block(source.clone(), MirBlockKind::Ordinary)
            .map_err(invalid)?;

        let storage = builder
            .push_storage(source.clone(), MirStorageKind::Parameter(0), task)
            .map_err(invalid)?;

        let task = MirPlace::new(storage, [], task);

        let resumed = builder
            .push_block(source.clone(), MirBlockKind::Ordinary)
            .map_err(invalid)?;

        let cancelled = builder
            .push_block(source.clone(), MirBlockKind::CleanupBroadcast)
            .map_err(invalid)?;

        let abi = target.runtime_abi();

        if method == TaskObservationMethod::Cancel {
            builder
                .push_operation(
                    entry,
                    source.clone(),
                    MirOperationKind::Async(MirAsyncOperation::RequestTaskCancellation {
                        task: MirOperand::Copy(task.clone()),
                        runtime: MirRuntimeReference::new(
                            RuntimeAbiRole::TaskCancellationRequest,
                            abi,
                        ),
                    }),
                    None,
                )
                .map_err(invalid)?;
        }

        // Both the suspension and its continuations retain the same task owner until transfer.
        builder
            .set_terminator(
                entry,
                source.clone(),
                MirTerminatorKind::Suspend {
                    kind: MirSuspensionKind::TaskCompletion,
                    payload: Some(MirOperand::Copy(task.clone())),
                    resume_state: MirFrameStateId::new(1),
                    resume: MirEdge::new(resumed, []),
                    cancellation: Some(MirCleanupEdge::new(
                        MirCleanupPhase::TaskCancellation,
                        MirEdge::new(cancelled, []),
                    )),
                    registration: MirRuntimeReference::new(
                        RuntimeAbiRole::SuspensionRegistration,
                        abi,
                    ),
                    wake: MirRuntimeReference::new(RuntimeAbiRole::Wake, abi),
                },
            )
            .map_err(invalid)?;

        let transferred = builder
            .push_operation(
                resumed,
                source.clone(),
                MirOperationKind::Async(MirAsyncOperation::ResolveTask {
                    task: MirOperand::Copy(task.clone()),
                    variants,
                    runtime: MirRuntimeReference::new(RuntimeAbiRole::TaskResolution, abi),
                }),
                Some(result),
            )
            .map_err(invalid)?;

        let value = transferred.result().ok_or_else(|| {
            invalid(bray_ir::MirUnitBuildError::MissingOperationResult(
                transferred.operation(),
            ))
        })?;

        builder
            .push_operation(
                resumed,
                source.clone(),
                MirOperationKind::Async(MirAsyncOperation::DestroyTerminalTask {
                    task: MirOperand::Move(task.clone()),
                    completion: None,
                }),
                None,
            )
            .map_err(invalid)?;

        builder
            .push_operation(
                resumed,
                source.clone(),
                MirOperationKind::Async(MirAsyncOperation::PublishTerminalState {
                    state: MirTaskTerminalState::Completed(MirOperand::Value(value)),
                    runtime: MirRuntimeReference::new(RuntimeAbiRole::TerminalPublication, abi),
                }),
                None,
            )
            .map_err(invalid)?;

        builder
            .set_terminator(resumed, source.clone(), MirTerminatorKind::Return(None))
            .map_err(invalid)?;

        self.lower_task_observation_cleanup(&mut builder, cancelled, &source, task.clone())?;
        self.attach_frame_descriptor(&mut builder, entry, task, result, cancelled, &[], &source)?;

        builder.finish(entry).map_err(invalid)
    }

    fn lower_task_observation_cleanup(
        &self,
        builder: &mut MirUnitBuilder,
        entry: MirBlockId,
        source: &MirSourceAnchor,
        task: MirPlace,
    ) -> Result<(), C::Error> {
        let invalid = |cause| self.mir_error(source, cause);
        let outcome = self.cleanup_outcome(builder, entry, source)?;
        let abi = builder.target().runtime_abi();

        outcome
            .initialize_cancellation(builder, entry, source)
            .map_err(invalid)?;

        // Cancellation broadcast borrows the task that the shielded resolution later consumes.
        builder
            .push_operation(
                entry,
                source.clone(),
                MirOperationKind::Cleanup {
                    phase: MirCleanupPhase::TaskCancellation,
                    place: task.clone(),
                },
                None,
            )
            .map_err(invalid)?;

        let broadcast = outcome.check(builder, entry, source).map_err(invalid)?;

        let lifecycle = builder
            .push_block(source.clone(), MirBlockKind::LifecycleResolution)
            .map_err(invalid)?;

        builder
            .set_terminator(
                broadcast,
                source.clone(),
                MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                    MirCleanupPhase::LifecycleResolution,
                    MirEdge::new(lifecycle, []),
                )),
            )
            .map_err(invalid)?;

        // Result cleanup may suspend while the task allocation remains owned by this frame.
        let block =
            self.push_task_resolution(builder, lifecycle, source, task.clone(), &outcome)?;

        builder
            .push_operation(
                block,
                source.clone(),
                MirOperationKind::Async(MirAsyncOperation::DestroyTerminalTask {
                    task: MirOperand::Move(task),
                    completion: None,
                }),
                None,
            )
            .map_err(invalid)?;

        let block = self.finish_cleanup_outcome(builder, block, source, &outcome)?;

        builder
            .push_operation(
                block,
                source.clone(),
                MirOperationKind::Async(MirAsyncOperation::PublishTerminalState {
                    state: MirTaskTerminalState::Cancelled,
                    runtime: MirRuntimeReference::new(RuntimeAbiRole::TerminalPublication, abi),
                }),
                None,
            )
            .map_err(invalid)?;

        builder
            .set_terminator(block, source.clone(), MirTerminatorKind::Return(None))
            .map_err(invalid)
    }
}
