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
        let lowered = self.lower_expression(expression.operand(), current)?;

        let Some(current) = lowered.block else {
            return Ok(lowered);
        };

        let Some(frame) = lowered.value else {
            panic!(
                "lowering contract violation: MissingOperationResult {value:?}",
                value = expression.operand()
            );
        };

        let source = self.source(expression.origin());

        let suspension = self
            .input
            .suspension(id)
            .expect("checked await retains its suspension");

        assert_eq!(
            suspension.kind(),
            AsyncSuspensionKind::Await {
                operand: expression.operand()
            },
            "checked await retains its exact operand"
        );

        let completion = self.expression_type(id);

        let awaited = self.lower_awaited_frame(
            id.into(),
            frame,
            completion,
            current,
            Self::retained_source(&source),
        )?;

        let (block, value) = self.finish_typed_call_panic_check(
            id,
            awaited.block.expect("await resumes to its completion"),
            &source,
            &awaited.value.expect("await publishes its completion value"),
            completion,
            None,
        )?;

        // Take the child's outcome before observing a new cancellation request.
        // A completed value is an owned temporary until the enclosing expression
        // accepts it, so cancellation uses the same cleanup as other inputs.
        let retained = self.input_temporaries.len();

        let value = if self.input_cleanup_phases(completion).is_some() {
            self.materialize_owned_input(id, block, &source, value, completion)?
        } else {
            value
        };

        let cancellation = self.suspension_cleanup_edge(&source, id.into())?;

        let resume = self.builder.push_block(
            Self::retained_source(&source),
            self.builder.block_kind(block),
        )?;

        self.branch_frame_cancellation(block, &source, resume, cancellation)?;
        self.input_temporaries.truncate(retained);

        Ok(LoweredExpression::continuing(resume, Some(value), source))
    }

    pub(super) fn lower_awaited_frame(
        &mut self,
        occurrence: bray_bound_tree::BoundExecutionSite,
        frame_value: MirOperand,
        completion: bray_symbols::TypeId,
        current: MirBlockId,
        source: MirSourceAnchor,
    ) -> Result<LoweredExpression, LoweringError> {
        let Some(parent) = self.input.unit_kind().protected_frame() else {
            panic!(
                "lowering contract violation: AwaitOutsideProtectedFrame {value:?}",
                value = occurrence
            );
        };

        let suspension = self.input.suspension(occurrence).unwrap_or_else(|| {
            panic!(
                "lowering contract violation: MissingSuspensionPoint {value:?}",
                value = occurrence
            )
        });

        let child = MirFrameReference::Erased;

        self.push_operation(
            current,
            Self::retained_source(&source),
            MirOperationKind::Async(MirAsyncOperation::ComposeAwaitedFrame {
                parent,
                child,
                frame: frame_value,
            }),
            None,
        )?;

        let resume = self.builder.push_block(
            Self::retained_source(&source),
            self.builder.block_kind(current),
        )?;

        let cancellation = match occurrence {
            bray_bound_tree::BoundExecutionSite::ScopedExit(_) => {
                // The exit dispatcher establishes the cleanup shield before driving this child.
                let unreachable = self.builder.push_block(
                    Self::retained_source(&source),
                    MirBlockKind::CleanupBroadcast,
                )?;

                self.set_terminator(
                    unreachable,
                    Self::retained_source(&source),
                    MirTerminatorKind::Unreachable,
                )?;

                bray_ir::MirCleanupEdge::new(
                    bray_ir::MirCleanupPhase::TaskCancellation,
                    MirEdge::new(unreachable, []),
                )
            }
            _ => self.suspension_cleanup_edge(&source, occurrence.node())?,
        };

        // Ordinary await observes cancellation after taking the terminal outcome.
        // Selected asynchronous exits already execute under their cleanup shield.
        let dispatch = resume;

        let broadcast = self.frame_broadcast(&source, occurrence, false)?;
        let state = self.next_frame_state()?;

        self.set_terminator(
            current,
            Self::retained_source(&source),
            MirTerminatorKind::Suspend {
                kind: MirSuspensionKind::Awaited,
                payload: None,
                resume_state: state,
                resume: MirEdge::new(dispatch, []),
                cancellation,
                registration: self.runtime_reference(RuntimeAbiRole::SuspensionRegistration),
                wake: self.runtime_reference(RuntimeAbiRole::Wake),
            },
        )?;

        let initialized_storages = self.retained_storages(suspension.retained_subjects())?;

        self.frame_states.push(
            MirFrameState::new(
                state,
                dispatch,
                broadcast,
                self.execution_lane_requirements(),
                initialized_storages,
            )
            .with_affinity(self.frame_affinity()),
        );

        let value = self
            .push_operation(
                resume,
                Self::retained_source(&source),
                MirOperationKind::Async(MirAsyncOperation::CommitAwaitedCompletion { child }),
                Some(completion),
            )?
            .result()
            .expect("awaited completion publishes its checked result");

        Ok(LoweredExpression::continuing(
            resume,
            Some(MirOperand::Value(value)),
            source,
        ))
    }

    pub(in crate::lowering) fn branch_frame_cancellation(
        &mut self,
        current: MirBlockId,
        source: &MirSourceAnchor,
        resume: MirBlockId,
        cancellation: bray_ir::MirCleanupEdge,
    ) -> Result<(), LoweringError> {
        let cancelled = self
            .builder
            .push_block(Self::retained_source(source), MirBlockKind::Ordinary)?;

        self.set_terminator(
            cancelled,
            Self::retained_source(source),
            MirTerminatorKind::CancelCurrentRun {
                cleanup: cancellation,
            },
        )?;

        let boolean =
            self.representation_type(bray_compiler_known::RepresentationRole::ScalarBool)?;

        let requested = self
            .push_operation(
                current,
                Self::retained_source(source),
                MirOperationKind::Async(MirAsyncOperation::ObserveFrameCancellation),
                Some(boolean),
            )?
            .result()
            .expect("frame cancellation observation returns a boolean");

        self.set_terminator(
            current,
            Self::retained_source(source),
            MirTerminatorKind::Branch {
                condition: MirOperand::Value(requested),
                then_edge: MirEdge::new(cancelled, []),
                else_edge: MirEdge::new(resume, []),
            },
        )?;

        Ok(())
    }

    pub(super) fn lower_call_operation(
        &mut self,
        expression: BoundExpressionId,
        block: MirBlockId,
        source: MirSourceAnchor,
        call: MirCall,
    ) -> Result<(MirBlockId, MirOperand), LoweringError> {
        let task_operation = self.input.task_operation(expression);

        let operation = match task_operation {
            Some(AsyncTaskOperationKind::Start) => {
                let frame = call_receiver(&call, expression);

                MirOperationKind::Async(MirAsyncOperation::StartTask {
                    frame: MirFrameReference::Erased,
                    value: frame,
                    allocation: self.runtime_reference(RuntimeAbiRole::TaskAllocation),
                    start: self.runtime_reference(RuntimeAbiRole::TaskStart),
                })
            }
            Some(kind @ (AsyncTaskOperationKind::Join | AsyncTaskOperationKind::Cancel)) => {
                let task = call_receiver(&call, expression);
                let variants = self.run_result_representation();

                let BoundCallResult::LazyFuture(result) = call.result() else {
                    panic!(
                        "lowering contract violation: InvalidTaskOperation {value:?}",
                        value = expression
                    );
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

        let result_type = self.expression_type(expression);

        let admission = match &operation {
            MirOperationKind::Async(MirAsyncOperation::CreateFrame {
                initializer: MirFrameInitializer::Callable(call),
                ..
            }) => match call.target() {
                bray_ir::MirCallTarget::Direct(callable) => {
                    Some(bray_ir::MirOutgoingOwner::Invocation {
                        callable: *callable,
                        result: call.result(),
                    })
                }
                bray_ir::MirCallTarget::Indirect { callee, .. } => {
                    Some(bray_ir::MirOutgoingOwner::FrameFactory(callee.clone()))
                }
                target => panic!(
                    "checked asynchronous construction retains a direct or indirect factory, got {target:?}"
                ),
            },
            _ => None,
        };

        let block = match &admission {
            // Admission and discharge refer to the same evaluated factory or selected declaration.
            Some(owner) => self.admit_outgoing(expression, block, &source, owner.clone())?,
            None => block,
        };

        let (block, value) = self.push_checked_value_operation(
            expression,
            block,
            Self::retained_source(&source),
            operation,
            result_type,
        )?;

        if let Some(owner) = admission {
            self.discharge_outgoing(block, &source, owner)?;
        }

        Ok((block, value))
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
        // State zero describes initial captures and is assembled after body lowering.
        let state = self
            .frame_states
            .len()
            .checked_add(1)
            .and_then(|state| u32::try_from(state).ok())
            .ok_or(LoweringError::MirCapacity(
                bray_ir::MirCapacityError::IdentityCapacityExceeded,
            ))?;

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
                    .unwrap_or_else(|| {
                        panic!(
                            "lowering contract violation: MissingStorageIdentity {value:?}",
                            value = *access
                        )
                    }),
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
                .unwrap_or_else(|| {
                    panic!(
                        "lowering contract violation: MissingStorageIdentityRecord {value:?}",
                        value = identity
                    )
                });

            // Static dependencies retain their product or thread owner, not frame-local storage.
            if matches!(storage, bray_bound_tree::StorageIdentity::Static(_)) {
                continue;
            }

            let ty = self.storage_identity_type(identity);
            let origin = bray_bound_tree::BoundNodeOrigin::source(self.input.unit().key().source());

            storages.push(self.place_for_identity(identity, ty, origin)?.storage());
        }

        // Inputs already evaluated for a pending call or construction survive later awaits.
        storages.extend(
            self.input_temporaries
                .iter()
                .map(|input| input.place.storage()),
        );

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
        frame_affinity(self.input.frame_dependencies())
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

fn call_receiver(call: &MirCall, expression: BoundExpressionId) -> MirOperand {
    call.arguments()
        .iter()
        .find_map(|argument| match argument {
            MirCallArgument::Receiver { value, .. } => {
                // The async operation owns the same immutable operand independently of the call.
                Some(value.clone())
            }
            MirCallArgument::Explicit { .. } => None,
        })
        .unwrap_or_else(|| {
            panic!(
                "lowering contract violation: InvalidTaskOperation {value:?}",
                value = expression
            )
        })
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
