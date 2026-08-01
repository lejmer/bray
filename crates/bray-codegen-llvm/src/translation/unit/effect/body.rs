use super::super::core::UnitTranslator;
use super::super::support::{
    aggregate_value_element, extract_value, insert_value, int_value, llvm, next_helper,
    pointer_field_index,
};
use bray_codegen::{CodegenFailure, CodegenSymbolKey, CodegenTypeKind};
use bray_ir::{
    BoundUnitKey, MirAsyncOperation, MirFrameInitializer, MirHelperReference, MirOperation,
    MirOperationKind, MirPlace,
};
use inkwell::values::BasicValueEnum;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(in crate::translation::unit) fn translate_operation(
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
            MirOperationKind::Borrow { place, .. } => {
                let pointer = self.place(place)?;
                let result = self.operation_result_type(operation)?;

                let source = self
                    .request
                    .mappings()
                    .ty(place.ty())
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                if matches!(
                    source.kind(),
                    CodegenTypeKind::UnsizedSlice { .. } | CodegenTypeKind::UnsizedTraitView
                ) {
                    Some(llvm(self.builder.build_load(
                        self.types.map(result)?,
                        pointer,
                        "borrow.metadata",
                    ))?)
                } else {
                    Some(pointer.into())
                }
            }
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
            MirOperationKind::Memory(memory) => self.translate_memory(operation, memory)?,
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
            MirOperationKind::PanicReport(cause) => self.translate_panic_report(_id, cause)?,
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
            MirOperationKind::Async(asynchronous) => {
                self.translate_async_operation(_id, operation, asynchronous)?
            }
            MirOperationKind::Host(operation) => self.translate_host_operation(_id, operation)?,
        };

        if let Some(id) = operation.result() {
            let value = result.ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            self.values.insert(id, value);
        } else if result.is_some() {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        Ok(())
    }

    fn translate_panic_report(
        &mut self,
        operation: bray_ir::MirOperationId,
        cause: &bray_ir::MirPanicCause,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let message = match cause {
            bray_ir::MirPanicCause::Message(message)
            | bray_ir::MirPanicCause::Assertion(Some(message)) => {
                self.native_string_view(message)?
            }
            bray_ir::MirPanicCause::Assertion(None) => {
                crate::native::string_view_type(self.types.context(), self.request.target())
                    .const_zero()
                    .into()
            }
        };

        let helpers = self.operation_helpers(operation)?;

        let [helper] = helpers.as_slice() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        if helper.reference() != &MirHelperReference::PanicReport {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let Some(CodegenSymbolKey::Runtime(runtime)) = helper.symbol() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        if runtime.role() != bray_runtime_interface::RuntimeAbiRole::PanicReportConstruction {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        self.invoke_native_runtime(*runtime, &[message])
    }

    fn native_string_view(
        &mut self,
        message: &bray_ir::MirOperand,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let message_type = self.operand_type(message)?;
        let message = self.operand(message)?;

        let fields = self
            .request
            .mappings()
            .ty(message_type)
            .and_then(|mapping| match mapping.kind() {
                CodegenTypeKind::Aggregate(fields) if fields.len() == 2 => Some(fields.clone()),
                _ => None,
            })
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let pointer = pointer_field_index(self.request.mappings(), &fields)?;

        let length = 1_usize
            .checked_sub(pointer)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let pointer_element = u32::try_from(aggregate_value_element(
            self.request.mappings(),
            &fields,
            pointer,
        )?)
        .map_err(|_| CodegenFailure::UnsupportedTarget)?;

        let pointer = extract_value(&self.builder, message, pointer_element)?;

        let length_element = u32::try_from(aggregate_value_element(
            self.request.mappings(),
            &fields,
            length,
        )?)
        .map_err(|_| CodegenFailure::UnsupportedTarget)?;

        let length = extract_value(&self.builder, message, length_element)?;

        let mut view = crate::native::string_view_type(self.types.context(), self.request.target())
            .const_zero()
            .into();

        view = insert_value(&self.builder, view, pointer, 0)?;

        insert_value(&self.builder, view, length, 1)
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

        self.helper_address(helper)?
            .ok_or(CodegenFailure::GeneratedModuleInvariant)
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

        if helper.symbol().is_none() {
            return Ok(());
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
        operation: &MirOperation,
        asynchronous: &MirAsyncOperation,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        // Keep this exhaustive so every async operation requires an explicit translation.
        match asynchronous {
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
                let frame = self.native_inactive_frame(frame)?;

                let runtime = self.operation_runtime_helper(
                    operation_id,
                    MirHelperReference::ComposeAwaitedFrame(*child),
                )?;

                self.invoke_native_runtime(runtime, &[frame])
            }
            MirAsyncOperation::CommitAwaitedCompletion { child } => {
                let runtime = self.operation_runtime_helper(
                    operation_id,
                    MirHelperReference::CommitAwaitedCompletion(*child),
                )?;

                let address = self
                    .invoke_native_runtime(runtime, &[])?
                    .and_then(int_value)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let result_type = self.types.map(self.operation_result_type(operation)?)?;

                let pointer = llvm(
                    self.builder.build_int_to_ptr(
                        address,
                        self.types
                            .context()
                            .ptr_type(inkwell::AddressSpace::default()),
                        "awaited.completion",
                    ),
                )?;

                llvm(
                    self.builder
                        .build_load(result_type, pointer, "awaited.result"),
                )
                .map(Some)
            }
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
                if let Some(frame_context) = self.frame_context {
                    let context = self.frame_context_argument()?;

                    let context = llvm(
                        self.builder.build_int_to_ptr(
                            context,
                            self.types
                                .context()
                                .ptr_type(inkwell::AddressSpace::default()),
                            "frame.context",
                        ),
                    )?;

                    let cancellation = llvm(self.builder.build_struct_gep(
                        frame_context,
                        context,
                        2,
                        "frame.cancellation.pointer",
                    ))?;

                    let cancellation = llvm(self.builder.build_load(
                        self.types.context().i8_type(),
                        cancellation,
                        "frame.cancellation",
                    ))?
                    .into_int_value();

                    return llvm(self.builder.build_int_truncate(
                        cancellation,
                        self.types.context().bool_type(),
                        "frame.cancellation",
                    ))
                    .map(|value| Some(value.into()));
                }

                self.invoke_runtime(*runtime, &[])
            }
            MirAsyncOperation::PublishTerminalState { state, runtime } => {
                if self.frame_context.is_some() {
                    self.translate_frame_terminal_state(state)?;

                    return Ok(None);
                }

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

    fn native_inactive_frame(
        &mut self,
        frame: &bray_ir::MirOperand,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let frame_type = self.operand_type(frame)?;
        let frame = self.operand(frame)?;

        let fields = self
            .request
            .mappings()
            .ty(frame_type)
            .and_then(|mapping| match mapping.kind() {
                CodegenTypeKind::Aggregate(fields) if fields.len() == 2 => Some(fields.clone()),
                _ => None,
            })
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let mut native = crate::native::inactive_frame_type(self.types.context())
            .const_zero()
            .into();

        for (index, _) in fields.iter().enumerate() {
            let element = u32::try_from(aggregate_value_element(
                self.request.mappings(),
                &fields,
                index,
            )?)
            .map_err(|_| CodegenFailure::UnsupportedTarget)?;

            let value = extract_value(&self.builder, frame, element)?;

            if !value.is_pointer_value() {
                return Err(CodegenFailure::GeneratedModuleInvariant);
            }

            native = insert_value(&self.builder, native, value, index)?;
        }

        Ok(native)
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

    fn operation_runtime_helper(
        &self,
        operation: bray_ir::MirOperationId,
        expected: MirHelperReference,
    ) -> Result<bray_ir::MirRuntimeReference, CodegenFailure> {
        let helpers = self.operation_helpers(operation)?;

        let [helper] = helpers.as_slice() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        if helper.reference() != &expected {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let Some(CodegenSymbolKey::Runtime(runtime)) = helper.symbol() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        Ok(*runtime)
    }
}
