use super::super::core::UnitTranslator;
use super::super::support::{
    extract_value, insert_value, int_value, llvm, nonzero_integer, pointer_value,
};
use bray_codegen::{CodegenFailure, CodegenSymbolKey, CodegenTypeBehavior, CodegenTypeKind};
use bray_ir::{MirAsyncOperation, MirHelperReference, MirOperation, MirOperationKind, MirPlace};
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
                self.translate_store(destination, value)?;

                None
            }
            MirOperationKind::Borrow { place, .. } => {
                let pointer = self.place(place)?;
                let result = self.operation_result_type(operation);

                let source = self.type_mapping(place.ty()).expect(
                    "checked MIR effect translation requires an established mapping or value",
                );

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
            MirOperationKind::TraitView { operand, entries } => {
                Some(self.translate_trait_view(id, operation, operand, entries)?)
            }
            MirOperationKind::TraitViewMethod { operand, slot } => {
                Some(self.translate_trait_view_method(operation, operand, *slot)?)
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
                let source_place = matches!(
                    operand,
                    bray_ir::MirOperand::Copy(_) | bray_ir::MirOperand::Move(_)
                );

                let operand = self.operand(operand)?;

                let operand = if source_place
                    && matches!(conversion.target(), bray_ir::ConversionTarget::Composite(_))
                {
                    self.snapshot_value(operand, conversion.source_type(), "conversion.input")?
                } else {
                    operand
                };

                let helpers = self.operation_helpers(id)?;
                let converted = self.translate_conversion_plan(operand, conversion)?;

                if !helpers.is_empty() {
                    panic!(
                        "checked MIR effect translation violated an established compiler contract"
                    );
                }

                let converted = if source_place
                    && matches!(conversion.target(), bray_ir::ConversionTarget::Identity)
                    && conversion.source_type() == conversion.target_type()
                {
                    self.snapshot_value(converted, conversion.target_type(), "conversion.snapshot")?
                } else {
                    converted
                };

                Some(converted)
            }
            MirOperationKind::NumericConversion { operand, .. } => {
                let source = self.operand_type(operand);
                let target = self.operation_result_type(operation);
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
                    panic!(
                        "checked MIR effect translation violated an established compiler contract"
                    );
                }

                let present = if query.operand_type() == query.nullable_type() {
                    self.nullable_present(operand, query.nullable_type())?
                } else {
                    let mapping = self.type_mapping(query.operand_type()).expect(
                        "checked MIR effect translation requires an established mapping or value",
                    );

                    let nullable = self.type_mapping(query.nullable_type()).expect(
                        "checked MIR effect translation requires an established mapping or value",
                    );

                    if !matches!(
                        mapping.kind(),
                        CodegenTypeKind::Pointer { target, .. }
                            if *target == nullable.ty()
                    ) {
                        panic!(
                            "checked MIR effect translation violated an established compiler contract"
                        );
                    }

                    let pointer = pointer_value(operand).expect(
                        "checked MIR effect translation requires an established mapping or value",
                    );

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
                let result = self.operation_result_type(operation);
                let subject_type = self.operand_type(subject);

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
            MirOperationKind::AdmitOutgoing { owner, runtime }
            | MirOperationKind::DischargeOutgoing { owner, runtime } => {
                self.translate_outgoing(id, owner, *runtime)?;

                None
            }
            MirOperationKind::ReleaseBuffer { place, .. }
            | MirOperationKind::Quiesce { place, .. }
            | MirOperationKind::Cleanup { place, .. } => {
                self.translate_lifecycle_helper(id, place, None)?;

                None
            }
            MirOperationKind::Destroy {
                place, abandoned, ..
            } => {
                self.translate_lifecycle_helper(id, place, abandoned.as_ref())?;

                None
            }
            MirOperationKind::Async(asynchronous) => {
                self.translate_async_operation(id, operation, asynchronous)?
            }
            MirOperationKind::Host(operation) => self.translate_host_operation(id, operation)?,
        };

        self.clear_moved_reports()?;

        if let Some(id) = operation.result() {
            let value = result
                .expect("checked MIR effect translation requires an established mapping or value");

            self.values.insert(id, value);
        } else if result.is_some() {
            panic!("checked MIR effect translation violated an established compiler contract");
        }

        Ok(())
    }

    fn translate_store(
        &mut self,
        destination: &MirPlace,
        value: &bray_ir::MirOperand,
    ) -> Result<(), CodegenFailure> {
        if let bray_ir::MirOperand::Move(source) = value
            && self
                .type_mapping(destination.ty())
                .is_some_and(|mapping| mapping.behavior() == Some(CodegenTypeBehavior::PanicReport))
        {
            if source == destination {
                return Ok(());
            }

            let source_pointer = self.place(source)?;
            let destination_pointer = self.place(destination)?;

            let layout = self
                .type_mapping(destination.ty())
                .and_then(bray_codegen::CodegenTypeMapping::layout)
                .expect("panic report types must have represented codegen layouts");

            let alignment = crate::conversion::target_value(
                layout.alignment().get(),
                "panic_report_move_alignment",
            )?;

            llvm(self.builder.build_memcpy(
                destination_pointer,
                alignment,
                source_pointer,
                alignment,
                self.pointer_integer_type().const_int(layout.size(), false),
            ))?;

            llvm(
                self.builder
                    .build_store(source_pointer, self.types.map(source.ty())?.const_zero()),
            )?;

            return Ok(());
        }

        let ty = destination.ty();
        let destination = self.place(destination)?;
        let value = self.operand(value)?;

        // Report transfer installs the native empty representation before a possibly
        // overlapping destination receives its already-evaluated value.
        self.clear_moved_reports()?;

        self.store_value(destination, value, ty)
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
            bray_ir::MirPanicCause::MemoryLayoutSizeOverflow => (
                bray_runtime_abi::NativePanicCause::MEMORY_LAYOUT_SIZE_OVERFLOW,
                crate::native::string_view_type(self.types.context(), self.request.target())
                    .const_zero()
                    .into(),
            ),
            bray_ir::MirPanicCause::MemoryLayoutUnsupportedAlignment => (
                bray_runtime_abi::NativePanicCause::MEMORY_LAYOUT_UNSUPPORTED_ALIGNMENT,
                crate::native::string_view_type(self.types.context(), self.request.target())
                    .const_zero()
                    .into(),
            ),
        };

        let cause = self
            .types
            .context()
            .i32_type()
            .const_int(u64::from(cause.code()), false)
            .into();

        let source = self.native_source_anchor(operation)?;

        let source_field_count = source.into_struct_value().get_type().count_fields();

        let source_fields = (0..source_field_count)
            .map(|index| extract_value(&self.builder, source, index))
            .collect::<Result<Vec<_>, _>>()?;

        let message_field_count = message.into_struct_value().get_type().count_fields();

        let message_fields = (0..message_field_count)
            .map(|index| extract_value(&self.builder, message, index))
            .collect::<Result<Vec<_>, _>>()?;

        let helpers = self.operation_helpers(operation)?;

        let [helper] = helpers.as_slice() else {
            panic!("checked MIR effect translation violated an established compiler contract");
        };

        if helper.reference() != &MirHelperReference::PanicReport {
            panic!("checked MIR effect translation violated an established compiler contract");
        }

        let Some(CodegenSymbolKey::Runtime(runtime)) = helper.symbol() else {
            panic!("checked MIR effect translation violated an established compiler contract");
        };

        if runtime.role() != bray_runtime_interface::RuntimeAbiRole::PanicReportConstruction {
            panic!("checked MIR effect translation violated an established compiler contract");
        }

        let arguments = std::iter::once(cause)
            .chain(source_fields)
            .chain(message_fields)
            .chain(std::iter::once(
                crate::mapping::provider::key(self.module, &self.builder, self.types)?.into(),
            ))
            .collect::<Vec<_>>();

        self.invoke_native_runtime(*runtime, &arguments)
    }

    fn native_string_view(
        &mut self,
        message: &bray_ir::MirOperand,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let message_type = self.operand_type(message);
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
            panic!("checked MIR effect translation violated an established compiler contract");
        };

        if !matches!(
            helper.reference(),
            MirHelperReference::AnonymousCallable(helper_unit) if helper_unit == unit
        ) {
            panic!("checked MIR effect translation violated an established compiler contract");
        }

        Ok(self
            .helper_address(helper)?
            .expect("anonymous callable helpers must have an address"))
    }

    pub(super) fn translate_declared_callable(
        &self,
        operation: bray_ir::MirOperationId,
        callable: bray_ir::MirCallableReference,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let helpers = self.operation_helpers(operation)?;

        let [helper] = helpers.as_slice() else {
            panic!("checked MIR effect translation violated an established compiler contract");
        };

        if helper.reference() != &MirHelperReference::DeclaredCallable(callable) {
            panic!("checked MIR effect translation violated an established compiler contract");
        }

        Ok(self
            .helper_address(helper)?
            .expect("declared callable helpers must have an address"))
    }

    fn translate_lifecycle_helper(
        &mut self,
        operation: bray_ir::MirOperationId,
        place: &MirPlace,
        abandoned: Option<&bray_ir::MirOperand>,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let helpers = self.operation_helpers(operation)?;

        let [helper] = helpers.as_slice() else {
            panic!("checked MIR effect translation violated an established compiler contract");
        };

        let mir_operation = self
            .unit
            .operation(operation)
            .expect("translated cleanup operation exists");

        assert_eq!(
            mir_operation.kind().helper_references(),
            [helper.reference().clone()],
            "lifecycle mapping retains the selected MIR dependency"
        );

        if helper.symbol().is_none() {
            if self.checked_call_operations.contains(&operation) {
                let context = self.checked_call_panic_report_context()?;

                self.set_pending_call_context(context)?;
            }

            return Ok(None);
        }

        let mut arguments = vec![self.place(place)?.into()];

        if matches!(
            helper.reference(),
            MirHelperReference::Destroy(_, bray_ir::MirDestructionDisposition::Dynamic)
        ) {
            arguments.push(match abandoned {
                Some(value) => self.operand(value)?,
                None => self.types.context().bool_type().const_zero().into(),
            });
        }

        let collection = mir_operation.kind().cleanup_collection();

        let owned = if collection.is_none() {
            Some(self.checked_call_panic_report_context()?)
        } else {
            None
        };

        let slots = if let Some(collection) = collection {
            [
                self.place(collection.state())?,
                self.place(collection.report())?,
            ]
        } else {
            crate::mapping::cleanup::begin_collection(
                self.module,
                &self.builder,
                owned.expect("local collection has its outcome"),
                self.types,
            )?
        };

        arguments.extend(slots.map(BasicValueEnum::from));

        let result = self.invoke_helper(helper, &arguments)?;

        if let Some(outcome) = owned {
            crate::mapping::cleanup::publish_collection(
                self.module,
                &self.builder,
                slots[0],
                self.types,
            )?;

            if self.checked_call_operations.contains(&operation) {
                self.set_pending_call_context(outcome)?;
            } else {
                self.propagate_boundary_call_panic(outcome)?;
            }
        }

        assert!(
            result.is_none(),
            "lifecycle disposal does not return a value"
        );

        Ok(None)
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
                source,
                destination,
                ..
            } => {
                let ty = self.types.map(source.ty())?;
                let source = self.place(source)?;
                let destination = self.place(destination)?;
                let value = llvm(self.builder.build_load(ty, source, "inactive.move"))?;

                llvm(self.builder.build_store(destination, value))?;

                Ok(None)
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
                resume_state,
                frame,
                composition,
                ..
            } => {
                self.compose_awaited_frame(operation_id, *child, *resume_state, frame, *composition)
            }
            MirAsyncOperation::CommitAwaitedCompletion { child, completion } => {
                let runtime = self.operation_runtime_helper(
                    operation_id,
                    MirHelperReference::CommitAwaitedCompletion(*child),
                )?;

                let outcome = self.checked_call_panic_report_context()?;

                let frame_type = self
                    .frame_context
                    .expect("await belongs to a protected frame");

                let frame = self.frame_context_pointer()?;

                let header =
                    crate::translation::frame::header_pointer(&self.builder, frame_type, frame)?;

                self.invoke_native_runtime(runtime, &[header.into(), outcome.into()])?;

                self.set_pending_call_context(outcome)?;

                let Some(completion) = completion else {
                    return Ok(None);
                };

                let outcome_type =
                    crate::native::run_outcome_type(self.types.context(), self.request.target());

                let published = llvm(self.builder.build_load(
                    outcome_type,
                    outcome,
                    "awaited.outcome",
                ))?;

                let (state, _) =
                    super::super::support::native_run_outcome(&self.builder, published)?;

                let completed = super::super::support::native_run_state_is(
                    &self.builder,
                    state,
                    bray_runtime_abi::NativeRunState::COMPLETED,
                    "awaited.completed",
                )?;

                let semantic = self.operation_result_type(operation);
                let result_type = self.types.map(semantic)?;
                let destination = self.storage(*completion)?;

                let address = llvm(self.builder.build_ptr_to_int(
                    destination,
                    self.pointer_integer_type(),
                    "awaited.result.address",
                ))?;

                let address = self.safe_completion_payload(address, completed, result_type)?;

                let selected = llvm(self.builder.build_int_to_ptr(
                    address,
                    self.types.context().ptr_type(Default::default()),
                    "awaited.result.source",
                ))?;

                self.load_value(selected, semantic, "awaited.result")
                    .map(Some)
            }
            MirAsyncOperation::StartTask {
                value,
                allocation,
                start,
                ..
            } => {
                let frame = self.native_inactive_frame(value)?;
                let task = self.start_native_task(*allocation, *start, frame)?;

                Ok(Some(task.into()))
            }
            MirAsyncOperation::RequestTaskCancellation { task, runtime } => {
                let task = self.observed_place(task)?;

                let status = self.invoke_runtime(*runtime, &[task])?.expect(
                    "checked MIR effect translation requires an established mapping or value",
                );

                crate::translation::require_runtime_success(
                    self.module,
                    self.types.context(),
                    &self.builder,
                    status,
                    "task.cancellation",
                )?;

                Ok(None)
            }
            MirAsyncOperation::ResolveTask {
                task,
                variants,
                runtime,
            } => {
                let task = int_value(self.observed_place(task)?).expect(
                    "checked MIR effect translation requires an established mapping or value",
                );

                let result = self.operation_result_type(operation);

                self.resolve_native_task(*runtime, task, result, *variants)
                    .map(Some)
            }
            MirAsyncOperation::ObserveFrameCancellation => {
                self.observe_frame_cancellation().map(Some)
            }
            MirAsyncOperation::ObserveCurrentRunCancellation { runtime } => {
                let value = self
                    .invoke_native_runtime(*runtime, &[])?
                    .and_then(int_value)
                    .expect(
                        "checked MIR effect translation requires an established mapping or value",
                    );

                nonzero_integer(&self.builder, value, "run.cancelled")
                    .map(|value| Some(value.into()))
            }
            MirAsyncOperation::PublishTerminalState { state, .. } => {
                self.translate_frame_terminal_state(state)?;

                Ok(None)
            }
            MirAsyncOperation::BroadcastInactiveFrame { frame, runtime }
            | MirAsyncOperation::ResolveInactiveFrame { frame, runtime }
            | MirAsyncOperation::DestroyInactiveFrame { frame, runtime } => {
                self.cleanup_inactive_frame(frame, *runtime)?;

                Ok(None)
            }
            MirAsyncOperation::TransferCleanupIncident {
                incident,
                report,
                runtime,
                ..
            } => {
                self.transfer_cleanup_incident(operation_id, incident, report, *runtime)?;

                Ok(None)
            }
            MirAsyncOperation::DestroyTerminalTask { task } => {
                let task = self.operand(task)?;

                let status = self
                    .invoke_single_operation_helper(
                        operation_id,
                        MirHelperReference::DestroyTerminalTask,
                        &[task],
                    )?
                    .expect(
                        "checked MIR effect translation requires an established mapping or value",
                    );

                crate::translation::require_runtime_success(
                    self.module,
                    self.types.context(),
                    &self.builder,
                    status,
                    "task.destruction",
                )?;

                Ok(None)
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
            panic!("checked MIR effect translation violated an established compiler contract");
        };

        if helper.reference() != &expected {
            panic!("checked MIR effect translation violated an established compiler contract");
        }

        self.invoke_helper(helper, arguments)
    }

    pub(super) fn operation_runtime_helper(
        &self,
        operation: bray_ir::MirOperationId,
        expected: MirHelperReference,
    ) -> Result<bray_ir::MirRuntimeReference, CodegenFailure> {
        let helpers = self.operation_helpers(operation)?;

        let [helper] = helpers.as_slice() else {
            panic!("checked MIR effect translation violated an established compiler contract");
        };

        if helper.reference() != &expected {
            panic!("checked MIR effect translation violated an established compiler contract");
        }

        let Some(CodegenSymbolKey::Runtime(runtime)) = helper.symbol() else {
            panic!("checked MIR effect translation violated an established compiler contract");
        };

        Ok(*runtime)
    }
}
