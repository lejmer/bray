use bray_bound_tree::{
    AsyncTaskOperationKind, BoundAwaitExpression, BoundCallResult, BoundDependencySubject,
    BoundExpressionId,
};
use bray_compiler_known::ImplementationHook;
use bray_ir::{
    MirAsyncOperation, MirBlockId, MirBlockKind, MirCall, MirCallArgument, MirEdge,
    MirFrameInitializer, MirFrameReference, MirFrameStateFacts, MirFrameStateId, MirOperand,
    MirOperationKind, MirRuntimeReference, MirSourceAnchor, MirSuspensionKind, MirTerminatorKind,
};
use bray_runtime_interface::{ExecutionLaneRequirement, RuntimeAbiRole};

use super::LoweringError;
use super::block::LoweredExpression;
use super::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(super) fn lower_await(
        &mut self,
        id: BoundExpressionId,
        expression: BoundAwaitExpression,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let Some(parent) = self.input.unit_kind().protected_frame() else {
            return Err(LoweringError::AwaitOutsideProtectedFrame(id));
        };

        let suspension = self
            .input
            .async_facts()
            .suspensions()
            .iter()
            .find(|suspension| suspension.expression() == id)
            // Lowering mutates its builder while retaining this immutable checked fact.
            .cloned()
            .ok_or(LoweringError::MissingSuspensionPoint(id))?;

        let lowered = self.lower_expression(expression.operand(), current)?;

        let Some(current) = lowered.block else {
            return Ok(lowered);
        };

        let Some(frame) = lowered.value else {
            return Err(LoweringError::MissingOperationResult(expression.operand()));
        };

        let source = self.source(expression.origin());
        let child = MirFrameReference::Erased;

        self.builder.push_operation(
            current,
            Self::retained_source(&source),
            MirOperationKind::Async(MirAsyncOperation::ComposeAwaitedFrame {
                parent,
                child,
                frame,
            }),
            None,
        )?;

        let resume = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let cancellation = self.suspension_cleanup_edge(&source)?;
        let state = self.next_frame_state()?;

        self.builder.set_terminator(
            current,
            Self::retained_source(&source),
            MirTerminatorKind::Suspend {
                kind: MirSuspensionKind::Awaited,
                resume_state: state,
                resume: MirEdge::new(resume, []),
                cancellation,
                registration: self.runtime_reference(RuntimeAbiRole::SuspensionRegistration),
                wake: self.runtime_reference(RuntimeAbiRole::Wake),
            },
        )?;

        let initialized_storages = self.retained_storages(suspension.retained_subjects())?;

        self.frame_states.push(MirFrameStateFacts::new(
            state,
            resume,
            self.execution_lane_requirements(),
            initialized_storages,
        ));

        let value = self.push_value_operation(
            id,
            resume,
            Self::retained_source(&source),
            MirOperationKind::Async(MirAsyncOperation::CommitAwaitedCompletion { child }),
        )?;

        Ok(LoweredExpression::continuing(resume, Some(value), source))
    }

    pub(super) fn lower_call_operation(
        &mut self,
        expression: BoundExpressionId,
        block: MirBlockId,
        source: MirSourceAnchor,
        call: MirCall,
    ) -> Result<MirOperand, LoweringError> {
        let task_operation = self
            .input
            .async_facts()
            .task_operations()
            .iter()
            .find(|operation| operation.expression() == expression)
            .map(|operation| operation.kind());

        let operation = match task_operation {
            Some(AsyncTaskOperationKind::Start) => {
                let frame = call_receiver(&call, expression)?;

                MirOperationKind::Async(MirAsyncOperation::StartTask {
                    frame: MirFrameReference::Erased,
                    value: frame,
                    allocation: self.runtime_reference(RuntimeAbiRole::TaskAllocation),
                    start: self.runtime_reference(RuntimeAbiRole::TaskStart),
                })
            }
            Some(kind @ (AsyncTaskOperationKind::Join | AsyncTaskOperationKind::Cancel)) => {
                let task = call_receiver(&call, expression)?;

                let BoundCallResult::LazyFuture(result) = call.result() else {
                    return Err(LoweringError::InvalidTaskOperation(expression));
                };

                MirOperationKind::Async(MirAsyncOperation::CreateFrame {
                    frame: MirFrameReference::Erased,
                    initializer: MirFrameInitializer::TaskObservation {
                        task,
                        result,
                        request_cancellation: kind == AsyncTaskOperationKind::Cancel,
                    },
                })
            }
            None => match call.result() {
                BoundCallResult::Immediate(_) => MirOperationKind::Call(call),
                BoundCallResult::LazyFuture(_) => {
                    MirOperationKind::Async(MirAsyncOperation::CreateFrame {
                        frame: MirFrameReference::Erased,
                        initializer: MirFrameInitializer::Callable(call),
                    })
                }
            },
        };

        self.push_value_operation(expression, block, source, operation)
    }

    pub(super) fn runtime_reference(&self, role: RuntimeAbiRole) -> MirRuntimeReference {
        MirRuntimeReference::new(role, self.input.target().runtime_abi())
    }

    pub(super) fn execution_lane_requirements(&self) -> Vec<ExecutionLaneRequirement> {
        self.input
            .body_behavior()
            .execution_requirements()
            .iter()
            .filter_map(|requirement| {
                match self
                    .input
                    .available_compiler_known_symbols()
                    .symbol_implementation(requirement.declaration())
                {
                    Some(ImplementationHook::BlockingExecution) => {
                        Some(ExecutionLaneRequirement::Blocking)
                    }
                    Some(ImplementationHook::ComputeExecution) => {
                        Some(ExecutionLaneRequirement::Compute)
                    }
                    Some(ImplementationHook::MainThreadExecution) => {
                        Some(ExecutionLaneRequirement::MainThread)
                    }
                    _ => None,
                }
            })
            .collect()
    }

    pub(super) fn next_frame_state(&self) -> Result<MirFrameStateId, LoweringError> {
        let state = u32::try_from(self.frame_states.len()).map_err(|_| {
            LoweringError::Mir(bray_ir::MirUnitBuildError::IdentityCapacityExceeded)
        })?;

        Ok(MirFrameStateId::new(state))
    }

    fn retained_storages(
        &mut self,
        subjects: &[BoundDependencySubject],
    ) -> Result<Vec<bray_ir::MirStorageId>, LoweringError> {
        let mut storages = Vec::new();

        for subject in subjects {
            let storage = match subject {
                BoundDependencySubject::Storage(identity) => {
                    let ty = self.storage_identity_type(*identity)?;

                    let origin =
                        bray_bound_tree::BoundNodeOrigin::source(self.input.unit().key().source());

                    Some(self.place_for_identity(*identity, ty, origin)?.storage())
                }
                BoundDependencySubject::StorageAccess(access) => {
                    Some(self.place_for_access(*access, false)?.storage())
                }
                BoundDependencySubject::BorrowCapability(_)
                | BoundDependencySubject::ScopedCapability(_)
                | BoundDependencySubject::ImplementationWitness(_)
                | BoundDependencySubject::LifecycleObligation(_) => None,
            };

            if let Some(storage) = storage {
                storages.push(storage);
            }
        }

        storages.sort_unstable();
        storages.dedup();

        Ok(storages)
    }
}

fn call_receiver(
    call: &MirCall,
    expression: BoundExpressionId,
) -> Result<MirOperand, LoweringError> {
    call.arguments()
        .iter()
        .find_map(|argument| match argument {
            MirCallArgument::Receiver { value, .. } => {
                // The async operation owns the same immutable operand independently of the call.
                Some(value.clone())
            }
            MirCallArgument::Explicit { .. } | MirCallArgument::Default { .. } => None,
        })
        .ok_or(LoweringError::InvalidTaskOperation(expression))
}
