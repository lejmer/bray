use super::core::UnitTranslator;
use super::support::{llvm, next_helper};
use bray_codegen::{CodegenFailure, CodegenHelperMapping, CodegenTypeKind};
use bray_ir::{
    BoundUnitKey, MirAsyncOperation, MirFrameInitializer, MirGeneratorKind,
    MirGeneratorOperation, MirHelperReference, MirHostOperation, MirOperation,
    MirOperationKind, MirPlace,
};
use bray_runtime_interface::{
    ProtectedFrameOperation, RootExecution, RuntimeRoleImplementation,
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
            MirOperationKind::PanicReport(cause) => {
                let arguments = match cause {
                    bray_ir::MirPanicCause::Message(message)
                    | bray_ir::MirPanicCause::Assertion(Some(message)) => {
                        vec![self.operand(message)?]
                    }
                    bray_ir::MirPanicCause::Assertion(None) => Vec::new(),
                };

                self.invoke_single_operation_helper(
                    _id,
                    MirHelperReference::PanicReport,
                    &arguments,
                )?
            }
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

        self.helper_address(helper)?
            .ok_or(CodegenFailure::GeneratedModuleInvariant)
    }

    pub(super) fn translate_generator(
        &mut self,
        operation: bray_ir::MirOperationId,
        generator: &MirGeneratorOperation,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let helpers = self.operation_helpers(operation)?;

        match generator {
            MirGeneratorOperation::Begin {
                kind,
                destination,
                element,
                exact_count,
            } => self.translate_generator_begin(
                &helpers,
                *kind,
                destination,
                *element,
                *exact_count,
            ),
            MirGeneratorOperation::Push { destination, value } => {
                self.translate_generator_push(&helpers, destination, value)
            }
            MirGeneratorOperation::Finish { destination } => {
                self.translate_generator_finish(&helpers, destination)
            }
            MirGeneratorOperation::CleanupBroadcast {
                destination,
                element,
                runtime,
            } => self.translate_generator_cleanup(
                &helpers,
                destination,
                *element,
                *runtime,
            ),
            MirGeneratorOperation::Destroy {
                destination,
                element,
                runtime,
            } => self.translate_generator_destroy(
                &helpers,
                destination,
                *element,
                *runtime,
            ),
        }
    }

    fn translate_generator_begin(
        &mut self,
        helpers: &[CodegenHelperMapping],
        kind: MirGeneratorKind,
        destination: &MirPlace,
        element: bray_symbols::TypeId,
        exact_count: Option<bray_symbols::ConstantTermId>,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let [helper] = helpers else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        if helper.reference() != &MirHelperReference::BeginGenerator {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let layout = self
            .request
            .mappings()
            .ty(element)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?
            .layout()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let kind = match kind {
            MirGeneratorKind::Array => 0,
            MirGeneratorKind::General => 1,
        };

        let mut arguments = vec![
            self.place(destination)?.into(),
            self.helper_integer_argument(helper, 1, kind)?,
            self.helper_integer_argument(helper, 2, layout.size())?,
            self.helper_integer_argument(helper, 3, layout.alignment().get())?,
        ];

        if let Some(term) = exact_count {
            let value = self
                .request
                .mappings()
                .constant_term(self.instance.key(), term)
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            arguments.push(self.constant(value)?);
        } else {
            arguments.push(self.helper_integer_argument(helper, 4, 0)?);
        }

        arguments.push(self.helper_boolean_argument(helper, 5, exact_count.is_some())?);

        if self.invoke_helper(helper, &arguments)?.is_some() {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        Ok(None)
    }

    fn translate_generator_push(
        &mut self,
        helpers: &[CodegenHelperMapping],
        destination: &MirPlace,
        value: &bray_ir::MirOperand,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let [helper] = helpers else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        if helper.reference() != &MirHelperReference::PushGenerator {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let element = self.operand_type(value)?;

        let layout = self
            .request
            .mappings()
            .ty(element)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?
            .layout()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let storage =
            self.aligned_alloca(element, layout.alignment().get(), "generator.element")?;

        let value = self.operand(value)?;

        llvm(self.builder.build_store(storage, value))?;

        let destination = self.place(destination)?.into();

        if self
            .invoke_helper(helper, &[destination, storage.into()])?
            .is_some()
        {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        Ok(None)
    }

    fn translate_generator_finish(
        &mut self,
        helpers: &[CodegenHelperMapping],
        destination: &MirPlace,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let [helper] = helpers else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        if helper.reference() != &MirHelperReference::FinishGenerator {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let destination_pointer = self.place(destination)?;

        if self
            .invoke_helper(helper, &[destination_pointer.into()])?
            .is_some()
        {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let value = llvm(self.builder.build_load(
            self.types.map(destination.ty())?,
            destination_pointer,
            "generator.result",
        ))?;

        Ok(Some(value))
    }

    fn translate_generator_cleanup(
        &mut self,
        helpers: &[CodegenHelperMapping],
        destination: &MirPlace,
        element: bray_symbols::TypeId,
        runtime: bray_ir::MirRuntimeReference,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let [helper] = helpers else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let expected = MirHelperReference::Cleanup {
            phase: bray_ir::MirCleanupPhase::TaskCancellation,
            ty: element,
        };

        if helper.reference() != &expected {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let callback = self.generator_callback_argument(helper, runtime, 1)?;
        let destination = self.place(destination)?.into();

        if self
            .invoke_runtime(runtime, &[destination, callback])?
            .is_some()
        {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        Ok(None)
    }

    fn translate_generator_destroy(
        &mut self,
        helpers: &[CodegenHelperMapping],
        destination: &MirPlace,
        element: bray_symbols::TypeId,
        runtime: bray_ir::MirRuntimeReference,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let [finalize, destroy] = helpers else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        if finalize.reference() != &MirHelperReference::Finalize(element)
            || destroy.reference() != &MirHelperReference::Destroy(element)
        {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let finalize = self.generator_callback_argument(finalize, runtime, 1)?;
        let destroy = self.generator_callback_argument(destroy, runtime, 2)?;
        let destination = self.place(destination)?.into();

        if self
            .invoke_runtime(runtime, &[destination, finalize, destroy])?
            .is_some()
        {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        Ok(None)
    }

    fn generator_callback_argument(
        &mut self,
        helper: &CodegenHelperMapping,
        runtime: bray_ir::MirRuntimeReference,
        parameter: usize,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        self.helper_address(helper)?
            .map_or_else(
                || self.runtime_null_pointer_argument(runtime, parameter),
                Ok,
            )
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
            MirHostOperation::ExecuteRoot {
                root,
                execution,
                runtime,
            } => {
                if self.host_role_implementation(*runtime)?
                    == RuntimeRoleImplementation::CompilerLowering
                    && *execution == RootExecution::Synchronous
                {
                    let (function, signature) = self.root_entry(root, *execution)?;

                    let result = self.invoke_function(function, signature, &[], "root")?;

                    return if result.is_none() {
                        Ok(None)
                    } else {
                        Err(CodegenFailure::GeneratedModuleInvariant)
                    };
                }

                let (root, _) = self.root_entry(root, *execution)?;

                self.invoke_runtime(
                    *runtime,
                    &[root.as_global_value().as_pointer_value().into()],
                )
            }
            MirHostOperation::RequestRootCancellation { runtime }
            | MirHostOperation::ObserveRootTerminal { runtime }
            | MirHostOperation::ReportCleanupIncidents { runtime } => {
                if self.host_role_implementation(*runtime)?
                    == RuntimeRoleImplementation::CompilerLowering
                {
                    Ok(None)
                } else {
                    self.invoke_runtime(*runtime, &[])
                }
            }
            MirHostOperation::StructuredShutdown { runtime } => {
                if self.host_role_implementation(*runtime)?
                    == RuntimeRoleImplementation::CompilerLowering
                {
                    self.translate_compiler_shutdown()
                } else {
                    self.invoke_runtime(*runtime, &[])
                }
            }
        }
    }

    fn translate_compiler_shutdown(
        &mut self,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let machine = self.request.target().machine();

        if machine.architecture() != bray_target::TargetArchitecture::X86_64
            || machine.object_format() != bray_target::ObjectFormat::Elf
        {
            return Err(CodegenFailure::UnsupportedTarget);
        }

        let context = self.module.get_context();
        let integer = context.i64_type();

        let function_type =
            context
                .void_type()
                .fn_type(&[integer.into(), integer.into()], false);

        let function = context.create_inline_asm(
            function_type,
            "syscall".to_owned(),
            "{rax},{rdi},~{rcx},~{r11},~{memory}".to_owned(),
            true,
            false,
            None,
            false,
        );

        let arguments = [
            integer.const_int(60, false).into(),
            integer.const_zero().into(),
        ];

        llvm(self.builder.build_indirect_call(
            function_type,
            function,
            &arguments,
            "process.exit",
        ))?;

        Ok(None)
    }

    fn host_role_implementation(
        &self,
        runtime: bray_ir::MirRuntimeReference,
    ) -> Result<RuntimeRoleImplementation, CodegenFailure> {
        let bray_ir::MirUnitKind::ExecutableHost(host) = self.unit.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        host.role_binding(runtime.role())
            .map(bray_runtime_interface::RuntimeRoleBinding::implementation)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)
    }

    fn root_entry(
        &self,
        root: &BoundUnitKey,
        execution: RootExecution,
    ) -> Result<
        (
            inkwell::values::FunctionValue<'context>,
            &'request bray_codegen::CodegenCallableSignature,
        ),
        CodegenFailure,
    > {
        let instance = self
            .request
            .unit()
            .instances()
            .iter()
            .map(|instance| instance.key())
            .chain(self.request.unit().external_instances())
            .find(|instance| {
                matches!(
                    instance.template(),
                    bray_ir::MirUnitKey::Bound(candidate) if candidate == root
                )
            })
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let symbol = match execution {
            RootExecution::Synchronous => self
                .request
                .mappings()
                .instance_symbol(instance),
            RootExecution::Asynchronous { frame } => self
                .request
                .mappings()
                .symbol(&bray_codegen::CodegenSymbolKey::ProtectedFrame {
                    frame,
                    operation: ProtectedFrameOperation::Resume,
                }),
        }
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let function = self
            .module
            .get_function(symbol.name().as_str())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        Ok((function, symbol.signature()))
    }
}
