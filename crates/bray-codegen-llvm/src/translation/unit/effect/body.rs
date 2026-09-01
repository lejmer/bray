use super::super::core::UnitTranslator;
use super::super::support::{
    extract_value, insert_value, int_value, llvm, next_helper, pointer_value,
};
use bray_codegen::{CodegenFailure, CodegenSymbolKey, CodegenTypeKind};
use bray_ir::{
    MirAsyncOperation, MirFrameInitializer, MirHelperReference, MirOperation, MirOperationKind,
    MirPlace, MirSourceAnchor,
};
use inkwell::values::BasicValueEnum;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(in crate::translation::unit) fn translate_operation(
        &mut self,
        id: bray_ir::MirOperationId,
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
                    .type_mapping(place.ty())
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
                Some(self.translate_construction(id, operation, construction)?)
            }
            MirOperationKind::Convert {
                operand,
                conversion,
            } => {
                let operand = self.operand(operand)?;
                let helpers = self.operation_helpers(id)?;
                let mut helpers = helpers.iter();

                let converted =
                    self.translate_conversion_plan(operand, conversion, &mut helpers)?;

                if helpers.next().is_some() {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                }

                Some(converted)
            }
            MirOperationKind::NumericConversion { operand, .. } => {
                let source = self.operand_type(operand)?;
                let target = self.operation_result_type(operation)?;
                let operand = self.operand(operand)?;

                Some(self.convert(operand, source, target)?)
            }
            MirOperationKind::NullableQuery(query) => {
                let operand = self.operand(query.operand())?;

                if !matches!(
                    self.type_mapping(query.result_type())
                        .map(bray_codegen::CodegenTypeMapping::kind),
                    Some(CodegenTypeKind::Boolean)
                ) {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                }

                let present = if query.operand_type() == query.nullable_type() {
                    self.nullable_present(operand, query.nullable_type())?
                } else {
                    let mapping = self
                        .type_mapping(query.operand_type())
                        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                    if !matches!(
                        mapping.kind(),
                        CodegenTypeKind::Pointer { target, .. }
                            if *target == query.nullable_type()
                    ) {
                        return Err(CodegenFailure::GeneratedModuleInvariant);
                    }

                    let pointer =
                        pointer_value(operand).ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                    self.nullable_present_at(pointer, query.nullable_type())?
                };

                let result = match query.kind() {
                    bray_ir::MirNullableQueryKind::IsPresent => present,
                    bray_ir::MirNullableQueryKind::IsAbsent => {
                        llvm(self.builder.build_not(present, "nullable.absent"))?
                    }
                };

                Some(result.into())
            }
            MirOperationKind::Call(call) => self.translate_call(id, call)?,
            MirOperationKind::Memory(memory) => self.translate_memory(id, operation, memory)?,
            MirOperationKind::Text(text) => self.translate_text(id, text)?,
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
            MirOperationKind::PanicReport(cause) => self.translate_panic_report(id, cause)?,
            MirOperationKind::AnonymousCallable(unit) => {
                Some(self.translate_anonymous_callable(id, unit)?)
            }
            MirOperationKind::DeclaredCallable(callable) => {
                Some(self.translate_declared_callable(id, *callable)?)
            }
            MirOperationKind::Generator(generator) => self.translate_generator(id, generator)?,
            MirOperationKind::Finalize(place) => {
                self.translate_lifecycle_helper(
                    id,
                    MirHelperReference::Finalize(place.ty()),
                    place,
                )?;

                None
            }
            MirOperationKind::Destroy(place) => {
                self.translate_lifecycle_helper(
                    id,
                    MirHelperReference::Destroy(place.ty()),
                    place,
                )?;

                None
            }
            MirOperationKind::Cleanup { phase, place } => {
                self.translate_lifecycle_helper(
                    id,
                    MirHelperReference::Cleanup {
                        phase: *phase,
                        ty: place.ty(),
                    },
                    place,
                )?;

                None
            }
            MirOperationKind::Async(asynchronous) => {
                self.translate_async_operation(id, operation, asynchronous)?
            }
            MirOperationKind::Host(operation) => self.translate_host_operation(id, operation)?,
        };

        self.clear_moved_places()?;

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
        let (cause, message) = match cause {
            bray_ir::MirPanicCause::Message(message) => (
                bray_runtime_abi::NativePanicCause::MESSAGE,
                self.native_string_view(message)?,
            ),
            bray_ir::MirPanicCause::Assertion(Some(message)) => (
                bray_runtime_abi::NativePanicCause::ASSERTION,
                self.native_string_view(message)?,
            ),
            bray_ir::MirPanicCause::Assertion(None) => (
                bray_runtime_abi::NativePanicCause::ASSERTION,
                crate::native::string_view_type(self.types.context(), self.request.target())
                    .const_zero()
                    .into(),
            ),
            bray_ir::MirPanicCause::ExplicitTestFailure(message) => (
                bray_runtime_abi::NativePanicCause::EXPLICIT_TEST_FAILURE,
                self.native_string_view(message)?,
            ),
        };

        let cause = self
            .types
            .context()
            .i32_type()
            .const_int(u64::from(cause.code()), false)
            .into();

        let source = self.native_source_anchor(operation)?;

        let source_fields = (0..5)
            .map(|index| extract_value(&self.builder, source, index))
            .collect::<Result<Vec<_>, _>>()?;

        let message_fields = (0..2)
            .map(|index| extract_value(&self.builder, message, index))
            .collect::<Result<Vec<_>, _>>()?;

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

        let arguments = std::iter::once(cause)
            .chain(source_fields)
            .chain(message_fields)
            .collect::<Vec<_>>();

        self.invoke_native_runtime(*runtime, &arguments)
    }

    fn native_source_anchor(
        &self,
        operation: bray_ir::MirOperationId,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let source = self
            .unit
            .operation(operation)
            .map(MirOperation::source)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let source = match source {
            MirSourceAnchor::Source(origin) => {
                let anchor = origin.source_anchor();
                let syntax = anchor.syntax();
                let range = syntax.full_range();

                bray_runtime_abi::NativeSourceAnchor::new(
                    syntax.source_id().raw(),
                    range.start().bytes(),
                    range.end().bytes(),
                    anchor.source_version().raw(),
                )
            }
            MirSourceAnchor::ExecutableHost(_)
            | MirSourceAnchor::GeneratedLifecycle(_)
            | MirSourceAnchor::ImportedExecutable(_) => {
                bray_runtime_abi::NativeSourceAnchor::unavailable()
            }
        };

        Ok(crate::native::source_anchor_value(self.types.context(), source).into())
    }

    fn native_string_view(
        &mut self,
        message: &bray_ir::MirOperand,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let message_type = self.operand_type(message)?;
        let message = self.operand(message)?;

        let (pointer, length, _) = self.string_view_parts(message, message_type)?;

        let mut view = crate::native::string_view_type(self.types.context(), self.request.target())
            .const_zero()
            .into();

        view = insert_value(&self.builder, view, pointer.into(), 0)?;

        insert_value(&self.builder, view, length.into(), 1)
    }

    pub(super) fn translate_anonymous_callable(
        &self,
        operation: bray_ir::MirOperationId,
        unit: &bray_ir::MirAnonymousCallableReference,
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

    pub(super) fn translate_declared_callable(
        &self,
        operation: bray_ir::MirOperationId,
        callable: bray_ir::MirCallableReference,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let helpers = self.operation_helpers(operation)?;

        let [helper] = helpers.as_slice() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        if helper.reference() != &MirHelperReference::DeclaredCallable(callable) {
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
                let frame = self.native_inactive_frame(value)?;
                let task = self.allocate_native_task(*allocation)?;

                self.start_native_task(*start, task, frame)?;

                Ok(Some(task.into()))
            }
            MirAsyncOperation::RequestTaskCancellation { task, runtime } => {
                let task = self.operand(task)?;

                let status = self
                    .invoke_runtime(*runtime, &[task])?
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                self.require_runtime_success(status, "task.cancellation")?;

                Ok(None)
            }
            MirAsyncOperation::ResolveTask {
                task,
                variants,
                runtime,
            } => {
                let task = int_value(self.operand(task)?)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let result = self.operation_result_type(operation)?;

                self.resolve_native_task(*runtime, task, result, *variants)
                    .map(Some)
            }
            MirAsyncOperation::ObserveCurrentRunCancellation { runtime } => {
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

                let status = self
                    .invoke_single_operation_helper(
                        operation_id,
                        MirHelperReference::DestroyTerminalTask,
                        &[task],
                    )?
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                self.require_runtime_success(status, "task.destruction")?;

                Ok(None)
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
            .type_mapping(frame_type)
            .and_then(|mapping| match mapping.kind() {
                CodegenTypeKind::Aggregate(fields) if fields.len() == 2 => Some(fields.clone()),
                _ => None,
            })
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let mut native = crate::native::inactive_frame_type(self.types.context())
            .const_zero()
            .into();

        for (index, _) in fields.iter().enumerate() {
            let element = u32::try_from(self.aggregate_value_element(&fields, index)?)
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

        match initializer {
            MirFrameInitializer::Callable(call) => {
                let arguments = self.evaluate_call_arguments(call, &mut helpers, None)?;
                let helper = next_helper(&mut helpers, &MirHelperReference::CreateFrame(frame))?;
                let result = self.invoke_helper(helper, arguments.values())?;

                if helpers.next().is_some() {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                }

                Ok(result)
            }
            MirFrameInitializer::TaskObservation {
                task,
                result,
                variants,
                request_cancellation,
                ..
            } => {
                let cancellation = next_helper(
                    &mut helpers,
                    &MirHelperReference::Cleanup {
                        phase: bray_ir::MirCleanupPhase::TaskCancellation,
                        ty: result.completion_type(),
                    },
                )?;

                let lifecycle = next_helper(
                    &mut helpers,
                    &MirHelperReference::Cleanup {
                        phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                        ty: result.completion_type(),
                    },
                )?;

                let creation = next_helper(&mut helpers, &MirHelperReference::CreateFrame(frame))?;

                let task = self.operand(task)?;

                let result = self.create_task_observation_frame(
                    creation,
                    task,
                    *request_cancellation,
                    result.completion_type(),
                    *variants,
                    cancellation,
                    lifecycle,
                )?;

                if helpers.next().is_some() {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                }

                Ok(Some(result))
            }
        }
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
