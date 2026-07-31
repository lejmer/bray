use super::super::core::UnitTranslator;
use super::super::support::{int_value, llvm};
use bray_codegen::CodegenFailure;
use bray_ir::{BoundUnitKey, MirHostOperation};
use bray_runtime_interface::{
    ExecutableEntryResult, ProtectedFrameOperation, RootExecution, RuntimeRoleImplementation,
};
use inkwell::values::BasicValueEnum;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn translate_host_operation(
        &mut self,
        operation_id: bray_ir::MirOperationId,
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

                    self.host_result = result;

                    return Ok(None);
                }

                if *execution == RootExecution::Synchronous {
                    self.host_result =
                        Some(self.translate_synchronous_root_boundary(root, *runtime)?);

                    return Ok(None);
                }

                if let RootExecution::Asynchronous { .. } = execution {
                    let (constructor, signature) =
                        self.root_entry(root, RootExecution::Synchronous)?;

                    let inactive = self
                        .invoke_function(constructor, signature, &[], "root.frame")?
                        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                    let context = super::super::support::extract_value(&self.builder, inactive, 0)
                        .and_then(|value| {
                            super::super::support::pointer_value(value)
                                .ok_or(CodegenFailure::GeneratedModuleInvariant)
                        })?;

                    let context = llvm(self.builder.build_ptr_to_int(
                        context,
                        crate::native::pointer_integer_type(
                            self.types.context(),
                            self.request.target(),
                        ),
                        "root.frame.context",
                    ))?;

                    let bray_ir::MirUnitKind::ExecutableHost(host) = self.unit.kind() else {
                        return Err(CodegenFailure::GeneratedModuleInvariant);
                    };

                    let adapter_name = host
                        .root_frame_adapter()
                        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                    let adapter = self
                        .module
                        .get_function(adapter_name.as_str())
                        .unwrap_or_else(|| {
                            self.module.add_function(
                                adapter_name.as_str(),
                                crate::native::frame_operation_type(
                                    self.types.context(),
                                    self.request.target(),
                                    ProtectedFrameOperation::MoveBeforeStart,
                                ),
                                None,
                            )
                        });

                    let frame = llvm(self.builder.build_call(
                        adapter,
                        &[context.into()],
                        "root.frame.adapter",
                    ))?
                    .try_as_basic_value()
                    .basic()
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                    let frame_storage = llvm(self.builder.build_alloca(
                        frame.get_type(),
                        "root.frame.transfer.storage",
                    ))?;

                    llvm(self.builder.build_store(frame_storage, frame))?;

                    let capacity = host
                        .capacity_limits()
                        .tasks()
                        .map_or(u64::MAX, |capacity| u64::from(capacity.get()));

                    let usize = crate::native::pointer_integer_type(
                        self.types.context(),
                        self.request.target(),
                    );

                    let frame_transfer = llvm(self.builder.build_ptr_to_int(
                        frame_storage,
                        usize,
                        "root.frame.transfer",
                    ))?;

                    let configuration = crate::native::runtime_configuration_type(
                        self.types.context(),
                        self.request.target(),
                    )
                    .const_named_struct(&[
                        usize.const_int(capacity, false).into(),
                        usize.const_all_ones().into(),
                    ]);

                    let start = self
                        .invoke_native_runtime(
                            *runtime,
                            &[frame_transfer.into(), configuration.into()],
                        )?
                        .and_then(|value| match value {
                            BasicValueEnum::StructValue(value) => Some(value),
                            _ => None,
                        })
                        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                    let status =
                        super::super::support::extract_value(&self.builder, start.into(), 0)
                            .and_then(|value| {
                                int_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant)
                            })?;

                    let root = super::super::support::extract_value(&self.builder, start.into(), 1)
                        .and_then(|value| {
                            int_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant)
                        })?;

                    let started = llvm(self.builder.build_int_compare(
                        inkwell::IntPredicate::EQ,
                        status,
                        status.get_type().const_zero(),
                        "root.started",
                    ))?;

                    let root = llvm(self.builder.build_select(
                        started,
                        root,
                        root.get_type().const_zero(),
                        "root.handle",
                    ))?;

                    self.host_root = Some(root);

                    return Ok(None);
                }

                Err(CodegenFailure::GeneratedModuleInvariant)
            }
            MirHostOperation::ObserveRootTerminal { runtime } => {
                if self.host_role_implementation(*runtime)?
                    == RuntimeRoleImplementation::CompilerLowering
                {
                    Ok(None)
                } else {
                    let root = self
                        .host_root
                        .as_ref()
                        .copied()
                        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                    self.host_result = self.invoke_native_runtime(*runtime, &[root])?;

                    Ok(None)
                }
            }
            MirHostOperation::ResolveRootTerminal {
                error: _,
                completion,
                panic,
                entry_failure,
            } => {
                self.host_status = Some(self.resolve_host_result(
                    operation_id,
                    self.module.get_context().i64_type(),
                    *completion,
                    *panic,
                    *entry_failure,
                )?);

                Ok(None)
            }
            MirHostOperation::ReportCleanupIncidents { runtime } => {
                if self.host_role_implementation(*runtime)?
                    == RuntimeRoleImplementation::CompilerLowering
                {
                    Ok(None)
                } else {
                    self.invoke_runtime(*runtime, &[])
                }
            }
            MirHostOperation::StructuredShutdown { runtime } => {
                let status = self
                    .host_status
                    .take()
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                if self.host_role_implementation(*runtime)?
                    != RuntimeRoleImplementation::CompilerLowering
                {
                    self.invoke_runtime(*runtime, &[])?;
                }

                self.translate_compiler_shutdown(status)
            }
        }
    }

    pub(super) fn invoke_native_runtime(
        &self,
        runtime: bray_ir::MirRuntimeReference,
        arguments: &[BasicValueEnum<'context>],
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let function = self
            .request
            .mappings()
            .symbol(&bray_codegen::CodegenSymbolKey::Runtime(runtime))
            .and_then(|symbol| self.module.get_function(symbol.name().as_str()))
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let arguments = arguments
            .iter()
            .copied()
            .map(Into::into)
            .collect::<Vec<_>>();

        Ok(llvm(
            self.builder
                .build_call(function, &arguments, runtime.role().as_str()),
        )?
        .try_as_basic_value()
        .basic())
    }

    pub(super) fn translate_frame_terminal_state(
        &mut self,
        state: &bray_ir::MirTaskTerminalState,
    ) -> Result<(), CodegenFailure> {
        let frame_context = self
            .frame_context
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let context = self
            .function
            .get_first_param()
            .and_then(int_value)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let pointer = llvm(
            self.builder.build_int_to_ptr(
                context,
                self.types
                    .context()
                    .ptr_type(inkwell::AddressSpace::default()),
                "frame.context",
            ),
        )?;

        let (kind, payload) = match state {
            bray_ir::MirTaskTerminalState::Completed(value) => {
                let completion = llvm(self.builder.build_struct_gep(
                    frame_context,
                    pointer,
                    1,
                    "frame.completion",
                ))?;

                let value = self.operand(value)?;

                llvm(self.builder.build_store(completion, value))?;

                let payload = llvm(self.builder.build_ptr_to_int(
                    completion,
                    self.types.context().i64_type(),
                    "frame.completion.handle",
                ))?;

                (1, payload)
            }
            bray_ir::MirTaskTerminalState::Cancelled => {
                (2, self.types.context().i64_type().const_zero())
            }
            bray_ir::MirTaskTerminalState::Panicked(value) => {
                let payload = match self.operand(value)? {
                    BasicValueEnum::PointerValue(value) => llvm(self.builder.build_ptr_to_int(
                        value,
                        self.types.context().i64_type(),
                        "frame.panic.handle",
                    ))?,
                    BasicValueEnum::IntValue(value) => {
                        llvm(self.builder.build_int_z_extend_or_bit_cast(
                            value,
                            self.types.context().i64_type(),
                            "frame.panic.handle",
                        ))?
                    }
                    _ => return Err(CodegenFailure::GeneratedModuleInvariant),
                };

                (3, payload)
            }
        };

        let mut progress = crate::native::frame_progress_type(self.types.context()).get_undef();

        let fields: [BasicValueEnum<'context>; 3] = [
            self.types
                .context()
                .i32_type()
                .const_int(kind, false)
                .into(),
            self.types.context().i32_type().const_zero().into(),
            payload.into(),
        ];

        for (index, field) in fields.into_iter().enumerate() {
            progress = llvm(self.builder.build_insert_value(
                progress,
                field,
                u32::try_from(index).map_err(|_| CodegenFailure::ResourceExhausted)?,
                "frame.progress.field",
            ))?
            .into_struct_value();
        }

        self.frame_progress = Some(progress.into());

        Ok(())
    }

    fn translate_compiler_shutdown(
        &mut self,
        status: inkwell::values::IntValue<'context>,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let machine = self.request.target().machine();

        if machine.architecture() != bray_target::TargetArchitecture::X86_64
            || machine.object_format() != bray_target::ObjectFormat::Elf
        {
            return Err(CodegenFailure::UnsupportedTarget);
        }

        let context = self.module.get_context();
        let integer = context.i64_type();

        let function_type = context
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

        let arguments = [integer.const_int(60, false).into(), status.into()];

        llvm(self.builder.build_indirect_call(
            function_type,
            function,
            &arguments,
            "process.exit",
        ))?;

        Ok(None)
    }

    pub(super) fn host_role_implementation(
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

    fn translate_synchronous_root_boundary(
        &mut self,
        root: &BoundUnitKey,
        runtime: bray_ir::MirRuntimeReference,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let bray_ir::MirUnitKind::ExecutableHost(host) = self.unit.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let result_type = match host.entry_result() {
            ExecutableEntryResult::Unit => None,
            ExecutableEntryResult::I32 => Some(self.types.context().i32_type().into()),
            ExecutableEntryResult::Fallible { ty, .. } => Some(self.types.map(ty)?),
        };

        let destination = match result_type {
            Some(result_type) => llvm(
                self.builder
                    .build_alloca(result_type, "root.result.storage"),
            )?,
            None => self
                .types
                .context()
                .ptr_type(inkwell::AddressSpace::default())
                .const_null(),
        };

        let usize =
            crate::native::pointer_integer_type(self.types.context(), self.request.target());

        let destination_handle = llvm(self.builder.build_ptr_to_int(
            destination,
            usize,
            "root.result.handle",
        ))?;

        let callback_type = self
            .types
            .context()
            .void_type()
            .fn_type(&[usize.into()], false);

        let callback = self.module.add_function(
            "bray_host_synchronous_root_callback",
            callback_type,
            Some(inkwell::module::Linkage::Private),
        );

        let host_block = self
            .builder
            .get_insert_block()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let callback_block = self
            .types
            .context()
            .append_basic_block(callback, "root.callback");

        self.builder.position_at_end(callback_block);

        let (function, signature) = self.root_entry(root, RootExecution::Synchronous)?;

        let result = self.invoke_function(function, signature, &[], "root")?;

        match (result_type, result) {
            (Some(result_type), Some(result)) => {
                let destination = callback
                    .get_first_param()
                    .and_then(int_value)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let destination = llvm(
                    self.builder.build_int_to_ptr(
                        destination,
                        self.types
                            .context()
                            .ptr_type(inkwell::AddressSpace::default()),
                        "root.result.destination",
                    ),
                )?;

                if result.get_type() != result_type {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                }

                llvm(self.builder.build_store(destination, result))?;
            }
            (None, None) => {}
            _ => return Err(CodegenFailure::GeneratedModuleInvariant),
        }

        llvm(self.builder.build_return(None))?;
        self.builder.position_at_end(host_block);

        let callback = callback.as_global_value().as_pointer_value();

        self.invoke_native_runtime(runtime, &[callback.into(), destination_handle.into()])?
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

        let symbol =
            match execution {
                RootExecution::Synchronous => self.request.mappings().instance_symbol(instance),
                RootExecution::Asynchronous { frame } => self.request.mappings().symbol(
                    &bray_codegen::CodegenSymbolKey::ProtectedFrame {
                        frame,
                        operation: ProtectedFrameOperation::Resume,
                    },
                ),
            }
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let function = self
            .module
            .get_function(symbol.name().as_str())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        Ok((function, symbol.signature()))
    }
}
