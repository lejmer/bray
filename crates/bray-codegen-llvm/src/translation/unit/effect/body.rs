use super::super::core::UnitTranslator;
use super::super::support::{
    extract_value, insert_value, int_value, llvm, next_helper, nonzero_integer, pointer_value,
};
use bray_codegen::{CodegenFailure, CodegenSymbolKey, CodegenTypeKind};
use bray_ir::{
    MirAsyncOperation, MirFrameInitializer, MirHelperReference, MirOperation, MirOperationKind,
    MirPlace,
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

                // A transferred source can alias its destination, including self-replacement.
                // Retire the source before installing the already-evaluated value.
                self.clear_moved_places()?;

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

                    let nullable = self
                        .type_mapping(query.nullable_type())
                        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                    if !matches!(
                        mapping.kind(),
                        CodegenTypeKind::Pointer { target, .. }
                            if *target == nullable.ty()
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

                let subject = if *pattern_operation == bray_bound_tree::PatternOperation::Observe {
                    self.observed_operand(subject)?
                } else {
                    self.operand(subject)?
                };

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
            MirOperationKind::Abandon { action, place } => {
                self.translate_lifecycle_helper(
                    id,
                    MirHelperReference::Abandon {
                        action: *action,
                        ty: place.ty(),
                    },
                    place,
                )?;

                None
            }
            MirOperationKind::DestructorRemainder { .. } => {
                return Err(CodegenFailure::GeneratedModuleInvariant);
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
            bray_ir::MirPanicCause::TaskAdmission => (
                bray_runtime_abi::NativePanicCause::TASK_ADMISSION,
                crate::native::string_view_type(self.types.context(), self.request.target())
                    .const_zero()
                    .into(),
            ),
            bray_ir::MirPanicCause::Message(message) => (
                bray_runtime_abi::NativePanicCause::MESSAGE,
                self.native_string_view(message)?,
            ),
            bray_ir::MirPanicCause::FrameAllocation => (
                bray_runtime_abi::NativePanicCause::FRAME_ALLOCATION,
                crate::native::string_view_type(self.types.context(), self.request.target())
                    .const_zero()
                    .into(),
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

        Ok(crate::native::source_anchor_from_mir(self.types.context(), Some(source)).into())
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

        let context = self.operation_panic_report_context(operation)?;

        if helper.symbol().is_none() {
            return Ok(());
        }

        let place = if matches!(
            expected,
            MirHelperReference::Abandon {
                action: bray_ir::MirAbandonmentAction::Destructor,
                ..
            }
        ) {
            self.operand(&bray_ir::MirOperand::Move(place.clone()))?
        } else {
            self.place(place)?.into()
        };

        let result = match context {
            Some(context) => {
                self.invoke_helper_with_panic_report_context(helper, &[place], context)?
            }
            None => self.invoke_helper(helper, &[place])?,
        };

        if result.is_some() {
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
            MirAsyncOperation::CreateFrame {
                frame,
                initializer,
                destination,
                ..
            } => {
                let value = self
                    .translate_frame_creation(operation_id, *frame, initializer)?
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                self.publish_created_frame(value, destination).map(Some)
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
            MirAsyncOperation::ComposeAwaitedFrame {
                child,
                frame,
                entry,
                ..
            } => {
                let frame = self.native_inactive_frame(frame)?;

                let runtime = self.operation_runtime_helper(
                    operation_id,
                    MirHelperReference::ComposeAwaitedFrame(*child),
                )?;

                let entry = self
                    .types
                    .context()
                    .i8_type()
                    .const_int(u64::from(entry.code()), false);

                self.invoke_native_runtime(runtime, &[frame, entry.into()])
            }
            MirAsyncOperation::DestroyInactiveCaptures { frame, runtime } => {
                let frame = self.native_inactive_frame(frame)?;

                let outcome = self
                    .invoke_native_runtime(*runtime, &[frame])?
                    .and_then(int_value)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let context = self.checked_call_panic_report_context()?;

                llvm(self.builder.build_store(context, outcome))?;
                self.retain_checked_call_context(context)?;

                Ok(None)
            }
            MirAsyncOperation::StartTask {
                value,
                destination,
                allocation,
                start,
                ..
            } => {
                let frame = self.native_inactive_frame(value)?;

                self.try_start_native_task(*allocation, *start, frame, destination)
                    .map(Some)
            }
            MirAsyncOperation::RequestTaskCancellation { task, runtime }
            | MirAsyncOperation::ReleaseTaskCompletionBorrow { task, runtime } => {
                let task = self.operand(task)?;

                let status = self
                    .invoke_native_runtime(*runtime, &[task])?
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let name = if matches!(
                    asynchronous,
                    MirAsyncOperation::ReleaseTaskCompletionBorrow { .. }
                ) {
                    "task.completion.release"
                } else {
                    "task.cancellation"
                };

                self.require_runtime_success(status, name)?;

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

                self.resolve_native_run(*runtime, vec![task.into()], result, *variants)
                    .map(Some)
            }
            MirAsyncOperation::BorrowTaskCompletion { task, runtime } => {
                let task = self.operand(task)?;
                let result = self.operation_result_type(operation)?;

                self.borrow_native_task_completion(*runtime, task, result)
                    .map(Some)
            }
            MirAsyncOperation::ResolveAwaitedFrame { variants, runtime } => {
                let result = self.operation_result_type(operation)?;

                self.resolve_native_run(*runtime, Vec::new(), result, *variants)
                    .map(Some)
            }
            MirAsyncOperation::ObserveCurrentRunCancellation { runtime } => {
                let value = self
                    .invoke_native_runtime(*runtime, &[])?
                    .and_then(int_value)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                nonzero_integer(&self.builder, value, "run.cancelled")
                    .map(|value| Some(value.into()))
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
                self.transfer_cleanup_incident(operation_id, incident, *runtime)?;

                Ok(None)
            }
            MirAsyncOperation::DestroyTerminalTask { task, completion } => {
                let helpers = self.operation_helpers(operation_id)?;
                let mut helpers = helpers.iter();

                let cleanup = if let Some(ty) = completion {
                    let helper = next_helper(
                        &mut helpers,
                        &MirHelperReference::Abandon {
                            action: bray_ir::MirAbandonmentAction::Destroy,
                            ty: *ty,
                        },
                    )?;

                    self.task_terminal_cleanup_descriptor(helper)?
                } else {
                    self.types
                        .context()
                        .ptr_type(inkwell::AddressSpace::default())
                        .const_null()
                        .into()
                };

                let destruction =
                    next_helper(&mut helpers, &MirHelperReference::DestroyTerminalTask)?;

                if helpers.next().is_some() {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                }

                let task = self.operand(task)?;

                let status = self
                    .invoke_helper(destruction, &[task, cleanup])?
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
            let element = crate::conversion::target_value(
                self.aggregate_value_element(&fields, index)?,
                "effect_state_field_index",
            )?;

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
            MirFrameInitializer::Lifecycle {
                role, ty, receiver, ..
            } => {
                let _lifecycle = next_helper(&mut helpers, &role.reference(*ty))?;
                let creation = next_helper(&mut helpers, &MirHelperReference::CreateFrame(frame))?;
                let receiver = self.operand(receiver)?;
                let result = self.invoke_helper(creation, &[receiver])?;

                if helpers.next().is_some() {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                }

                Ok(result)
            }
            MirFrameInitializer::Callable(call) => {
                let arguments = self.evaluate_call_arguments(call)?;

                let result = if let bray_ir::MirCallTarget::Indirect { callee, .. } = call.target()
                {
                    self.invoke_indirect_callable(callee, &arguments, false)?
                } else {
                    let helper =
                        next_helper(&mut helpers, &MirHelperReference::CreateFrame(frame))?;

                    self.invoke_helper(helper, &arguments)?
                };

                if helpers.next().is_some() {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                }

                Ok(result)
            }
        }
    }

    fn publish_created_frame(
        &mut self,
        value: BasicValueEnum<'context>,
        destination: &bray_ir::MirPlace,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let storage = extract_value(&self.builder, value, 0)?;
        let storage = pointer_value(storage).ok_or(CodegenFailure::GeneratedModuleInvariant)?;
        let allocated = llvm(self.builder.build_is_not_null(storage, "frame.created"))?;

        let function = self
            .builder
            .get_insert_block()
            .and_then(|block| block.get_parent())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let context = self.types.context();
        let published = context.append_basic_block(function, "frame.publish");
        let finished = context.append_basic_block(function, "frame.creation.finished");

        llvm(
            self.builder
                .build_conditional_branch(allocated, published, finished),
        )?;

        self.builder.position_at_end(published);

        let destination = self.place(destination)?;

        llvm(self.builder.build_store(destination, value))?;
        llvm(self.builder.build_unconditional_branch(finished))?;
        self.builder.position_at_end(finished);

        Ok(allocated.into())
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
