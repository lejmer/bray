use super::core::UnitTranslator;
use super::support::{llvm, next_helper};
use bray_codegen::CodegenFailure;
use bray_ir::{
    BoundUnitKey, MirAsyncOperation, MirFrameInitializer, MirGeneratorOperation,
    MirHelperReference, MirHostOperation, MirOperation, MirOperationKind, MirPlace,
};
use inkwell::values::BasicValueEnum;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn translate_operation(
        &mut self,
        _id: bray_ir::MirOperationId,
        operation: &MirOperation,
    ) -> Result<(), CodegenFailure> {
        // Keep this exhaustive so every MIR operation requires an explicit translation.
        let result = match operation.kind() {
            MirOperationKind::Store {
                destination, value, ..
            } => {
                let destination = self.place(destination)?;
                let value = self.operand(value)?;

                llvm(self.builder.build_store(destination, value))?;

                None
            }
            MirOperationKind::Borrow { place, .. } => Some(self.place(place)?.into()),
            MirOperationKind::Unary { operator, operand } => {
                Some(self.translate_unary(*operator, operand)?)
            }
            MirOperationKind::Binary {
                operator,
                left,
                right,
            } => Some(self.translate_binary(*operator, left, right)?),
            MirOperationKind::Aggregate(aggregate) => {
                Some(self.translate_aggregate(operation, aggregate)?)
            }
            MirOperationKind::Construct(construction) => {
                Some(self.translate_construction(_id, operation, construction)?)
            }
            MirOperationKind::Convert {
                operand,
                conversion,
            } => {
                let operand = self.operand(operand)?;
                let helpers = self.operation_helpers(_id)?;
                let mut helpers = helpers.iter();

                let converted =
                    self.translate_conversion_plan(operand, conversion, &mut helpers)?;

                if helpers.next().is_some() {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                }

                Some(converted)
            }
            MirOperationKind::Call(call) => self.translate_call(_id, call)?,
            MirOperationKind::PatternProjection {
                subject,
                projection,
                operation: pattern_operation,
            } => {
                let result = self.operation_result_type(operation)?;
                let subject_type = self.operand_type(subject)?;
                let subject = self.operand(subject)?;

                Some(self.translate_pattern_projection(
                    subject,
                    subject_type,
                    result,
                    *projection,
                    *pattern_operation,
                )?)
            }
            MirOperationKind::PanicReport(cause) => match cause {
                bray_ir::MirPanicCause::Message(message) => Some(self.operand(message)?),
                bray_ir::MirPanicCause::Assertion(Some(message)) => Some(self.operand(message)?),
                bray_ir::MirPanicCause::Assertion(None) => {
                    Some(self.operation_result_zero(operation)?)
                }
            },
            MirOperationKind::AnonymousCallable(unit) => {
                Some(self.translate_anonymous_callable(_id, unit)?)
            }
            MirOperationKind::Generator(generator) => self.translate_generator(_id, generator)?,
            MirOperationKind::Finalize(place) => {
                self.translate_lifecycle_helper(
                    _id,
                    MirHelperReference::Finalize(place.ty()),
                    place,
                )?;

                None
            }
            MirOperationKind::Destroy(place) => {
                self.translate_lifecycle_helper(
                    _id,
                    MirHelperReference::Destroy(place.ty()),
                    place,
                )?;

                None
            }
            MirOperationKind::Cleanup { phase, place } => {
                self.translate_lifecycle_helper(
                    _id,
                    MirHelperReference::Cleanup {
                        phase: *phase,
                        ty: place.ty(),
                    },
                    place,
                )?;

                None
            }
            MirOperationKind::Async(operation) => self.translate_async_operation(_id, operation)?,
            MirOperationKind::Host(operation) => self.translate_host_operation(operation)?,
        };

        if let Some(id) = operation.result() {
            let value = result.ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            self.values.insert(id, value);
        } else if result.is_some() {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        Ok(())
    }

    pub(super) fn translate_anonymous_callable(
        &self,
        operation: bray_ir::MirOperationId,
        unit: &BoundUnitKey,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let helpers = self.operation_helpers(operation)?;

        let [helper] = helpers.as_slice() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        if !matches!(
            helper.reference(),
            MirHelperReference::AnonymousCallable(helper_unit) if helper_unit == unit
        ) {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let symbol = self
            .request
            .mappings()
            .symbol(helper.symbol())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let function = self
            .module
            .get_function(symbol.name().as_str())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        Ok(function.as_global_value().as_pointer_value().into())
    }

    pub(super) fn translate_generator(
        &mut self,
        operation: bray_ir::MirOperationId,
        generator: &MirGeneratorOperation,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let helpers = self.operation_helpers(operation)?;

        let [helper] = helpers.as_slice() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let (expected, arguments) = match generator {
            MirGeneratorOperation::Begin {
                destination,
                exact_count,
                ..
            } => {
                let mut arguments = vec![self.place(destination)?.into()];

                if let Some(term) = exact_count {
                    let value = self
                        .request
                        .mappings()
                        .constant_term(self.instance.key(), *term)
                        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                    arguments.push(self.constant(value)?);
                }

                (MirHelperReference::BeginGenerator, arguments)
            }
            MirGeneratorOperation::Push { destination, value } => (
                MirHelperReference::PushGenerator,
                vec![self.place(destination)?.into(), self.operand(value)?],
            ),
            MirGeneratorOperation::Finish { destination } => (
                MirHelperReference::FinishGenerator,
                vec![self.place(destination)?.into()],
            ),
        };

        if helper.reference() != &expected {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        self.invoke_helper(helper, &arguments)
    }

    pub(super) fn translate_lifecycle_helper(
        &mut self,
        operation: bray_ir::MirOperationId,
        expected: MirHelperReference,
        place: &MirPlace,
    ) -> Result<(), CodegenFailure> {
        let helpers = self.operation_helpers(operation)?;

        let [helper] = helpers.as_slice() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        if helper.reference() != &expected {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let place = self.place(place)?.into();

        if self.invoke_helper(helper, &[place])?.is_some() {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        Ok(())
    }

    pub(super) fn translate_async_operation(
        &mut self,
        operation_id: bray_ir::MirOperationId,
        operation: &MirAsyncOperation,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        // Keep this exhaustive so every async operation requires an explicit translation.
        match operation {
            MirAsyncOperation::CreateFrame { frame, initializer } => {
                self.translate_frame_creation(operation_id, *frame, initializer)
            }
            MirAsyncOperation::MoveInactiveFrame {
                frame,
                source,
                destination,
            } => {
                let helpers = self.operation_helpers(operation_id)?;

                let [helper] = helpers.as_slice() else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                if helper.reference() != &MirHelperReference::MoveInactiveFrame(*frame) {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                }

                let arguments = [self.place(source)?.into(), self.place(destination)?.into()];

                self.invoke_helper(helper, &arguments)
            }
            MirAsyncOperation::ResumeFrame {
                state,
                storage,
                runtime,
                ..
            } => {
                let state = self.runtime_integer_argument(*runtime, 1, u64::from(state.raw()))?;
                let storage = self.storage(*storage)?.into();

                self.invoke_runtime(*runtime, &[storage, state])
            }
            MirAsyncOperation::ComposeAwaitedFrame { child, frame, .. } => {
                let frame = self.operand(frame)?;

                self.invoke_single_operation_helper(
                    operation_id,
                    MirHelperReference::ComposeAwaitedFrame(*child),
                    &[frame],
                )
            }
            MirAsyncOperation::CommitAwaitedCompletion { child } => self
                .invoke_single_operation_helper(
                    operation_id,
                    MirHelperReference::CommitAwaitedCompletion(*child),
                    &[],
                ),
            MirAsyncOperation::StartTask {
                value,
                allocation,
                start,
                ..
            } => {
                let allocation = self
                    .invoke_runtime(*allocation, &[])?
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let frame = self.operand(value)?;

                self.invoke_runtime(*start, &[allocation, frame])
            }
            MirAsyncOperation::RequestTaskCancellation { task, runtime }
            | MirAsyncOperation::ResolveTask { task, runtime } => {
                let task = self.operand(task)?;

                self.invoke_runtime(*runtime, &[task])
            }
            MirAsyncOperation::ObserveCurrentRunCancellation { runtime } => {
                self.invoke_runtime(*runtime, &[])
            }
            MirAsyncOperation::PublishTerminalState { state, runtime } => {
                let arguments = self.terminal_state_arguments(*runtime, state)?;

                self.invoke_runtime(*runtime, &arguments)
            }
            MirAsyncOperation::ExecuteCleanupBroadcast { runtime, .. }
            | MirAsyncOperation::ExecuteLifecycleResolution { runtime, .. } => {
                self.invoke_runtime(*runtime, &[])
            }
            MirAsyncOperation::TransferCleanupIncident { incident, runtime } => {
                let incident = self.operand(incident)?;

                self.invoke_runtime(*runtime, &[incident])
            }
            MirAsyncOperation::DestroyTerminalTask { task } => {
                let task = self.operand(task)?;

                self.invoke_single_operation_helper(
                    operation_id,
                    MirHelperReference::DestroyTerminalTask,
                    &[task],
                )
            }
        }
    }

    pub(super) fn translate_frame_creation(
        &mut self,
        operation_id: bray_ir::MirOperationId,
        frame: bray_ir::MirFrameReference,
        initializer: &MirFrameInitializer,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let helpers = self.operation_helpers(operation_id)?;
        let mut helpers = helpers.iter();
        let mut arguments = Vec::new();

        match initializer {
            MirFrameInitializer::Callable(call) => {
                arguments = self.evaluate_call_arguments(call, &mut helpers)?;
            }
            MirFrameInitializer::TaskObservation {
                task,
                request_cancellation,
                ..
            } => {
                arguments.push(self.operand(task)?);

                arguments.push(
                    self.helper_boolean_argument(
                        helpers
                            .as_slice()
                            .first()
                            .ok_or(CodegenFailure::GeneratedModuleInvariant)?,
                        arguments.len(),
                        *request_cancellation,
                    )?,
                );
            }
        }

        let helper = next_helper(&mut helpers, &MirHelperReference::CreateFrame(frame))?;
        let result = self.invoke_helper(helper, &arguments)?;

        if helpers.next().is_some() {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        Ok(result)
    }

    pub(super) fn invoke_single_operation_helper(
        &mut self,
        operation: bray_ir::MirOperationId,
        expected: MirHelperReference,
        arguments: &[BasicValueEnum<'context>],
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let helpers = self.operation_helpers(operation)?;

        let [helper] = helpers.as_slice() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        if helper.reference() != &expected {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        self.invoke_helper(helper, arguments)
    }

    pub(super) fn translate_host_operation(
        &mut self,
        operation: &MirHostOperation,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        // Keep this exhaustive so every host operation requires an explicit translation.
        match operation {
            MirHostOperation::ExecuteRoot { runtime, .. }
            | MirHostOperation::RequestRootCancellation { runtime }
            | MirHostOperation::ObserveRootTerminal { runtime }
            | MirHostOperation::ReportCleanupIncidents { runtime }
            | MirHostOperation::StructuredShutdown { runtime } => {
                self.invoke_runtime(*runtime, &[])
            }
        }
    }
}
