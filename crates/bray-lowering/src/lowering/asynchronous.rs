use bray_bound_tree::{
    AsyncSuspensionKind, AsyncTaskOperationKind, BoundAwaitExpression, BoundCallResult,
    BoundDependencySubject, BoundExpressionId,
};
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
                entry: bray_ir::MirFrameEntry::Body,
            }),
            None,
        )?;

        let resume = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let completion = self.expression_type(id)?;
        let cancellation = self.suspension_cleanup_edge(&source, id.into(), Some(completion))?;
        let state = self.next_frame_state()?;

        self.set_terminator(
            current,
            Self::retained_source(&source),
            MirTerminatorKind::Suspend {
                kind: MirSuspensionKind::Awaited,
                payload: None,
                resume_state: state,
                resume: MirEdge::new(resume, []),
                cancellation: Some(cancellation),
                registration: self.runtime_reference(RuntimeAbiRole::SuspensionRegistration),
                wake: self.runtime_reference(RuntimeAbiRole::Wake),
            },
        )?;

        let retained_storages = self.retained_storages(suspension.retained_subjects())?;

        self.frame_states.push(MirFrameState::new(
            state,
            resume,
            bray_ir::MirFrameExecutionState::new(
                self.execution_lane_requirements(),
                retained_storages,
            )
            .with_affinity(self.frame_affinity()),
        ));

        let result = self.unary_representation_type(
            bray_compiler_known::RepresentationRole::RunResult,
            completion,
        )?;

        let representation = self.run_result_representation()?;

        let variants = bray_ir::MirRunResultVariants::new(
            representation.completed_variant,
            representation.panicked_variant,
            representation.cancelled_variant,
        );

        let value = self.push_typed_value_operation(
            id,
            resume,
            Self::retained_source(&source),
            MirOperationKind::Async(MirAsyncOperation::ResolveAwaitedFrame {
                variants,
                runtime: self.runtime_reference(RuntimeAbiRole::AwaitedFrameResolution),
            }),
            result,
        )?;

        self.propagate_run_result(
            id,
            resume,
            source,
            value,
            result,
            super::expression::PropagationSource::Awaited,
        )
    }

    pub(super) fn lower_call_operation(
        &mut self,
        expression: BoundExpressionId,
        block: MirBlockId,
        source: MirSourceAnchor,
        call: MirCall,
    ) -> Result<(MirBlockId, MirOperand), LoweringError> {
        let task_operation = self.input.lowering_plans().task_operation(expression);

        match task_operation {
            Some(AsyncTaskOperationKind::Start) => {
                let frame = call_receiver(&call, expression)?;

                return self.lower_task_start(expression, block, source, frame);
            }
            Some(AsyncTaskOperationKind::Join | AsyncTaskOperationKind::Cancel) => {
                if !matches!(call.result(), BoundCallResult::LazyFuture(_)) {
                    return Err(LoweringError::InvalidTaskOperation(expression));
                }

                self.lower_frame_creation(expression, block, source, call)
            }
            None => match call.result() {
                BoundCallResult::Immediate(_) => {
                    let result_type = self.expression_type(expression)?;

                    self.push_checked_value_operation(
                        expression,
                        block,
                        source,
                        MirOperationKind::Call(call),
                        result_type,
                    )
                }
                BoundCallResult::LazyFuture(_) => {
                    self.lower_frame_creation(expression, block, source, call)
                }
            },
        }
    }

    fn lower_frame_creation(
        &mut self,
        expression: BoundExpressionId,
        block: MirBlockId,
        source: MirSourceAnchor,
        call: MirCall,
    ) -> Result<(MirBlockId, MirOperand), LoweringError> {
        let moved = call
            .arguments()
            .iter()
            .filter_map(|argument| match argument.value() {
                MirOperand::Move(place) => Some(Self::retained_place(place)),
                _ => None,
            })
            .collect::<Vec<_>>();

        let boolean =
            self.representation_type(bray_compiler_known::RepresentationRole::ScalarBool)?;

        let report_type =
            self.representation_type(bray_compiler_known::RepresentationRole::PanicReport)?;

        let (created, rejected, future, _) = crate::frame_creation::create_frame(
            &mut self.builder,
            block,
            &source,
            MirFrameInitializer::Callable(call),
            bray_ir::MirFrameStorageSource::Fresh,
            boolean,
        )?;

        let report = crate::frame_creation::allocation_panic(
            &mut self.builder,
            rejected,
            &source,
            report_type,
        )?;

        self.finish_panic_to_active_catch(
            expression,
            rejected,
            &source,
            report,
            report_type,
            None,
        )?;

        for place in moved {
            self.set_storage_initialized(created, &source, &place, false)?;
        }

        Ok((created, MirOperand::Move(future)))
    }

    fn lower_task_start(
        &mut self,
        expression: BoundExpressionId,
        block: MirBlockId,
        source: MirSourceAnchor,
        frame: MirOperand,
    ) -> Result<(MirBlockId, MirOperand), LoweringError> {
        let frame_type = self.builder.operand_type(&frame)?;
        let task_type = self.expression_type(expression)?;

        let boolean =
            self.representation_type(bray_compiler_known::RepresentationRole::ScalarBool)?;

        let frame_storage = self.builder.push_storage(
            Self::retained_source(&source),
            bray_ir::MirStorageKind::Temporary,
            frame_type,
        )?;

        let task_storage = self.builder.push_storage(
            Self::retained_source(&source),
            bray_ir::MirStorageKind::Temporary,
            task_type,
        )?;

        let retained = bray_ir::MirPlace::new(frame_storage, [], frame_type);
        let task = bray_ir::MirPlace::new(task_storage, [], task_type);

        self.push_operation(
            block,
            Self::retained_source(&source),
            MirOperationKind::Store {
                kind: bray_ir::MirStoreKind::Initialize,
                destination: Self::retained_place(&retained),
                value: frame,
            },
            None,
        )?;

        let admitted = self.push_typed_value_operation(
            expression,
            block,
            Self::retained_source(&source),
            MirOperationKind::Async(MirAsyncOperation::StartTask {
                frame: MirFrameReference::Erased,
                value: MirOperand::Copy(Self::retained_place(&retained)),
                destination: Self::retained_place(&task),
                allocation: self.runtime_reference(RuntimeAbiRole::TaskAllocation),
                start: self.runtime_reference(RuntimeAbiRole::TaskStart),
            }),
            boolean,
        )?;

        let completed = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let rejected = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        self.set_terminator(
            block,
            Self::retained_source(&source),
            MirTerminatorKind::Branch {
                condition: admitted,
                then_edge: MirEdge::new(completed, []),
                else_edge: MirEdge::new(rejected, []),
            },
        )?;

        let report_type =
            self.representation_type(bray_compiler_known::RepresentationRole::PanicReport)?;

        let report = self.push_panic_report(
            expression,
            rejected,
            &source,
            bray_ir::MirPanicCause::TaskAdmission,
            report_type,
        )?;

        self.finish_panic_to_active_catch(
            expression,
            rejected,
            &source,
            report,
            report_type,
            Some(&retained),
        )?;

        Ok((completed, MirOperand::Move(task)))
    }

    pub(super) fn runtime_reference(&self, role: RuntimeAbiRole) -> MirRuntimeReference {
        MirRuntimeReference::new(role, self.input.target().runtime_abi())
    }

    pub(super) fn execution_lane_requirements(&self) -> Vec<ExecutionLaneRequirement> {
        crate::execution::execution_lane_requirements(
            self.input.available_compiler_known_symbols(),
            self.input.body_behavior().execution_requirements(),
        )
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

        storages.extend(
            self.construction_temporaries
                .iter()
                .map(|temporary| temporary.place.storage()),
        );

        // Guards retain ownership decisions across suspension.
        storages.extend(self.initialization_guards.values().flat_map(|state| {
            std::iter::once(state.guard.storage())
                .chain(state.parts.iter().map(|part| part.guard.storage()))
        }));

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
            MirCallArgument::Explicit { .. } => None,
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
