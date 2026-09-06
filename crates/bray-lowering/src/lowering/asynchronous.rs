use bray_bound_tree::{
    AsyncSuspensionKind, AsyncTaskOperationKind, BoundAwaitExpression, BoundCallResult,
    BoundDependencySubject, BoundExpressionId,
};
use bray_compiler_known::ImplementationHook;
use bray_ir::{
    MirAsyncOperation, MirBlockId, MirBlockKind, MirCall, MirCallArgument, MirEdge,
    MirFrameInitializer, MirFrameReference, MirFrameState, MirFrameStateId, MirOperand,
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
            .lowering_plans()
            .suspension(id)
            // Lowering mutates its builder while retaining this immutable checked decision.
            .cloned()
            .ok_or(LoweringError::MissingSuspensionPoint(id))?;

        if suspension.kind()
            != (AsyncSuspensionKind::Await {
                operand: expression.operand(),
            })
        {
            return Err(LoweringError::MissingSuspensionPoint(id));
        }

        let lowered = self.lower_expression(expression.operand(), current)?;

        let Some(current) = lowered.block else {
            return Ok(lowered);
        };

        let Some(frame) = lowered.value else {
            return Err(LoweringError::MissingOperationResult(expression.operand()));
        };

        let source = self.source(expression.origin());
        let child = MirFrameReference::Erased;

        self.push_operation(
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

        let cancellation = self.suspension_cleanup_edge(&source, id.into())?;
        let state = self.next_frame_state()?;

        self.set_terminator(
            current,
            Self::retained_source(&source),
            MirTerminatorKind::Suspend {
                kind: MirSuspensionKind::Awaited,
                payload: None,
                resume_state: state,
                resume: MirEdge::new(resume, []),
                cancellation,
                registration: self.runtime_reference(RuntimeAbiRole::SuspensionRegistration),
                wake: self.runtime_reference(RuntimeAbiRole::Wake),
            },
        )?;

        let initialized_storages = self.retained_storages(suspension.retained_subjects())?;

        self.frame_states.push(
            MirFrameState::new(
                state,
                resume,
                self.execution_lane_requirements(),
                initialized_storages,
            )
            .with_affinity(self.frame_affinity()),
        );

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
    ) -> Result<(MirBlockId, MirOperand), LoweringError> {
        let task_operation = self.input.lowering_plans().task_operation(expression);

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
                let variants = self.run_result_representation()?;

                let BoundCallResult::LazyFuture(result) = call.result() else {
                    return Err(LoweringError::InvalidTaskOperation(expression));
                };

                MirOperationKind::Async(MirAsyncOperation::CreateFrame {
                    frame: MirFrameReference::Erased,
                    initializer: MirFrameInitializer::TaskObservation {
                        task,
                        result,
                        variants: bray_ir::MirRunResultVariants::new(
                            variants.completed_variant,
                            variants.panicked_variant,
                            variants.cancelled_variant,
                        ),
                        runtime: self.runtime_reference(RuntimeAbiRole::TaskObservationCreation),
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

        let result_type = self.expression_type(expression)?;

        self.push_checked_value_operation(expression, block, source, operation, result_type)
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

    pub(super) fn retained_storages(
        &mut self,
        subjects: &[BoundDependencySubject],
    ) -> Result<Vec<bray_ir::MirStorageId>, LoweringError> {
        let mut storages = Vec::new();

        for subject in subjects {
            let identity = match subject {
                BoundDependencySubject::Storage(identity) => *identity,
                BoundDependencySubject::StorageAccess(access) => self
                    .input
                    .storage_plan()
                    .root_identity(*access)
                    .ok_or(LoweringError::MissingStorageIdentity(*access))?,
                BoundDependencySubject::BorrowCapability(_)
                | BoundDependencySubject::ScopedCapability(_)
                | BoundDependencySubject::ImplementationWitness(_)
                | BoundDependencySubject::ProductStatic(_)
                | BoundDependencySubject::ExactThreadStatic(_)
                | BoundDependencySubject::LifecycleObligation(_) => continue,
            };

            let storage = self
                .input
                .storage_plan()
                .identity(identity)
                .ok_or(LoweringError::MissingStorageIdentityRecord(identity))?;

            // Static dependencies retain their product or thread owner, not frame-local storage.
            if matches!(storage, bray_bound_tree::StorageIdentity::Static(_)) {
                continue;
            }

            let ty = self.storage_identity_type(identity)?;
            let origin = bray_bound_tree::BoundNodeOrigin::source(self.input.unit().key().source());
            storages.push(self.place_for_identity(identity, ty, origin)?.storage());
        }

        // Entry initializes every guard, including guards for values created after resumption.
        storages.extend(self.initialization_guards.values().flat_map(|state| {
            std::iter::once(state.guard.storage())
                .chain(state.parts.iter().map(|part| part.guard.storage()))
        }));

        storages.sort_unstable();
        storages.dedup();

        Ok(storages)
    }

    pub(super) fn frame_affinity(&self) -> bray_runtime_interface::ProtectedFrameAffinity {
        frame_affinity(self.input.lowering_plans().frame_dependencies())
    }
}

fn frame_affinity(
    dependencies: &[BoundDependencySubject],
) -> bray_runtime_interface::ProtectedFrameAffinity {
    if dependencies
        .iter()
        .any(|subject| matches!(subject, BoundDependencySubject::ExactThreadStatic(_)))
    {
        bray_runtime_interface::ProtectedFrameAffinity::OriginThread
    } else {
        bray_runtime_interface::ProtectedFrameAffinity::Movable
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

#[cfg(test)]
mod tests {
    use bray_bound_tree::{BoundDependencySubject, BoundUnitId, BoundUnitKind, CheckedAsync};
    use bray_runtime_interface::ProtectedFrameAffinity;
    use bray_symbols::{StaticSymbolId, SymbolId};

    use super::frame_affinity;

    #[test]
    fn exact_thread_static_dependencies_pin_the_protected_frame() {
        let unit = BoundUnitId::new(91);
        let root = StaticSymbolId::from_symbol_id(SymbolId::new(7));

        let pinned = CheckedAsync::try_new(
            unit,
            BoundUnitKind::CallableBody,
            [BoundDependencySubject::ExactThreadStatic(root)],
            [],
            [],
            [],
            [],
            [],
            false,
        )
        .unwrap_or_else(|error| panic!("pinned async analysis must validate: {error:?}"));

        let movable = CheckedAsync::try_new(
            unit,
            BoundUnitKind::CallableBody,
            [],
            [],
            [],
            [],
            [],
            [],
            false,
        )
        .unwrap_or_else(|error| panic!("movable async analysis must validate: {error:?}"));

        assert_eq!(
            frame_affinity(pinned.frame_dependencies()),
            ProtectedFrameAffinity::OriginThread
        );

        assert_eq!(
            frame_affinity(movable.frame_dependencies()),
            ProtectedFrameAffinity::Movable
        );
    }
}
