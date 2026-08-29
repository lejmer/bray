use super::super::core::UnitTranslator;
use super::super::support::{int_value, llvm, pointer_value};
use bray_codegen::CodegenFailure;
use bray_ir::{BoundUnitKey, MirHostOperation};
use bray_runtime_interface::{ProtectedFrameOperation, RootExecution, RuntimeRoleImplementation};
use inkwell::IntPredicate;
use inkwell::values::BasicValueEnum;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn translate_host_operation(
        &mut self,
        operation_id: bray_ir::MirOperationId,
        operation: &MirHostOperation,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        // Keep this exhaustive so every host operation requires an explicit translation.
        match operation {
            MirHostOperation::MaterializeStatic { place } => {
                let _ = self.place(place)?;

                Ok(None)
            }
            MirHostOperation::SelectTestEntry { entry, runtime } => {
                self.begin_test_entry_selection(*entry, *runtime)?;

                Ok(None)
            }
            MirHostOperation::ExecuteRoot {
                entry,
                root,
                execution,
                runtime,
            } => {
                self.begin_memory_observation()?;
                self.begin_performance_interval()?;

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
                        Some(self.translate_synchronous_root_boundary(*entry, root, *runtime)?);

                    return Ok(None);
                }

                if let RootExecution::Asynchronous { frame } = execution {
                    let (constructor, signature) =
                        self.root_entry(root, RootExecution::Synchronous)?;

                    let inactive = self
                        .invoke_function(constructor, signature, &[], "root.frame")?
                        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                    let context = super::super::support::extract_value(&self.builder, inactive, 0)
                        .and_then(|value| {
                            pointer_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant)
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
                        .entry(*entry)
                        .and_then(bray_runtime_interface::ExecutableHostEntry::root_frame_adapter)
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

                    let adapter_key = bray_codegen::CodegenSymbolKey::ProtectedFrame {
                        frame: *frame,
                        operation: ProtectedFrameOperation::MoveBeforeStart,
                    };

                    let frame = crate::native::invoke_function(
                        self.types.context(),
                        &self.builder,
                        self.request.target(),
                        &adapter_key,
                        adapter,
                        &[context.into()],
                        "root.frame.adapter",
                    )?
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                    let frame_storage =
                        self.allocate_temporary(frame.get_type(), "root.frame.transfer.storage")?;

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
                        IntPredicate::EQ,
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
            MirHostOperation::ObserveRootTerminal { entry, runtime } => {
                self.translate_root_terminal_observation(*entry, *runtime)
            }
            MirHostOperation::ResolveRootTerminal {
                entry,
                error: _,
                completion,
                panic,
                entry_failure,
            } => {
                let status = self.resolve_host_result(
                    operation_id,
                    *entry,
                    self.module.get_context().i64_type(),
                    *completion,
                    *panic,
                    *entry_failure,
                )?;

                let status = self.finish_performance_interval_iteration(status)?;

                self.host_status = Some(match self.host_status.take() {
                    Some(current) => llvm(self.builder.build_or(current, status, "host.status"))?,
                    None => status,
                });

                Ok(None)
            }
            MirHostOperation::BeginStaticCleanup => {
                self.finish_test_entry_selection()?;

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
                    .unwrap_or_else(|| self.module.get_context().i64_type().const_zero());

                if self.host_role_implementation(*runtime)?
                    != RuntimeRoleImplementation::CompilerLowering
                {
                    self.invoke_runtime(*runtime, &[])?;
                }

                self.translate_compiler_shutdown(status)
            }
        }
    }

    fn begin_memory_observation(&self) -> Result<(), CodegenFailure> {
        if self.request.options().runtime_observations()
            != bray_codegen::RuntimeObservationMode::Memory
        {
            return Ok(());
        }

        let function = self
            .module
            .get_function(bray_runtime_abi::MEMORY_OBSERVATION_BEGIN_SYMBOL)
            .unwrap_or_else(|| {
                self.module.add_function(
                    bray_runtime_abi::MEMORY_OBSERVATION_BEGIN_SYMBOL,
                    self.types.context().void_type().fn_type(&[], false),
                    None,
                )
            });

        llvm(
            self.builder
                .build_call(function, &[], "memory.observation.begin"),
        )?;

        Ok(())
    }
    fn begin_test_entry_selection(
        &mut self,
        entry: bray_runtime_interface::ExecutableHostEntryId,
        runtime: bray_ir::MirRuntimeReference,
    ) -> Result<(), CodegenFailure> {
        if let Some(continuation) = self.host_selection_continuation.take() {
            self.finish_selected_entry_path()?;
            self.builder.position_at_end(continuation);
        }

        let selected = self
            .invoke_native_runtime(
                runtime,
                &[self
                    .types
                    .context()
                    .i32_type()
                    .const_int(u64::from(entry.slot()), false)
                    .into()],
            )?
            .and_then(int_value)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let selected = llvm(self.builder.build_int_compare(
            IntPredicate::NE,
            selected,
            selected.get_type().const_zero(),
            "test.entry.selected",
        ))?;

        let execute = self
            .types
            .context()
            .append_basic_block(self.function, "test.entry.execute");

        let continuation = self
            .types
            .context()
            .append_basic_block(self.function, "test.entry.next");

        llvm(
            self.builder
                .build_conditional_branch(selected, execute, continuation),
        )?;

        self.builder.position_at_end(execute);
        self.host_selection_continuation = Some(continuation);

        Ok(())
    }

    fn finish_test_entry_selection(&mut self) -> Result<(), CodegenFailure> {
        let Some(continuation) = self.host_selection_continuation.take() else {
            return Ok(());
        };

        self.finish_selected_entry_path()?;
        self.builder.position_at_end(continuation);

        let shutdown = self.test_host_shutdown_block();
        let status = self.types.context().i64_type().const_int(1, false);

        let block = self
            .builder
            .get_insert_block()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        self.host_selection_statuses.push((status, block));
        llvm(self.builder.build_unconditional_branch(shutdown))?;
        self.builder.position_at_end(shutdown);

        let status = self
            .builder
            .build_phi(self.types.context().i64_type(), "test.host.status")
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

        let incoming = self
            .host_selection_statuses
            .iter()
            .map(|(value, block)| (value as &dyn inkwell::values::BasicValue, *block))
            .collect::<Vec<_>>();

        status.add_incoming(&incoming);
        self.host_status = Some(status.as_basic_value().into_int_value());

        Ok(())
    }

    fn finish_selected_entry_path(&mut self) -> Result<(), CodegenFailure> {
        let status = self
            .host_status
            .take()
            .unwrap_or_else(|| self.types.context().i64_type().const_int(1, false));

        let block = self
            .builder
            .get_insert_block()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let shutdown = self.test_host_shutdown_block();

        self.host_selection_statuses.push((status, block));
        llvm(self.builder.build_unconditional_branch(shutdown))?;

        Ok(())
    }

    fn test_host_shutdown_block(&mut self) -> inkwell::basic_block::BasicBlock<'context> {
        *self.host_selection_shutdown.get_or_insert_with(|| {
            self.types
                .context()
                .append_basic_block(self.function, "test.host.shutdown")
        })
    }

    fn translate_root_terminal_observation(
        &mut self,
        entry: bray_runtime_interface::ExecutableHostEntryId,
        runtime: bray_ir::MirRuntimeReference,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let bray_ir::MirUnitKind::ExecutableHost(host) = self.unit.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let synchronous = host
            .entry(entry)
            .is_some_and(|entry| entry.root() == RootExecution::Synchronous);

        if synchronous
            || self.host_role_implementation(runtime)?
                == RuntimeRoleImplementation::CompilerLowering
        {
            return Ok(None);
        }

        let root = self
            .host_root
            .as_ref()
            .copied()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        self.host_result = self.invoke_native_runtime(runtime, &[root])?;

        Ok(None)
    }

    pub(super) fn invoke_native_runtime(
        &self,
        runtime: bray_ir::MirRuntimeReference,
        arguments: &[BasicValueEnum<'context>],
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let key = bray_codegen::CodegenSymbolKey::Runtime(runtime);

        let function = self
            .request
            .mappings()
            .symbol(&key)
            .and_then(|symbol| self.module.get_function(symbol.name().as_str()))
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let mut native_arguments = Vec::with_capacity(arguments.len());

        for (index, argument) in arguments.iter().copied().enumerate() {
            if crate::native::uses_indirect_argument(self.request.target(), runtime.role(), index) {
                let storage = self.allocate_temporary(
                    argument.get_type(),
                    &format!("{}.argument", runtime.role().as_str()),
                )?;

                llvm(self.builder.build_store(storage, argument))?;
                native_arguments.push(storage.into());
            } else {
                native_arguments.push(argument.into());
            }
        }

        crate::native::invoke_function(
            self.types.context(),
            &self.builder,
            self.request.target(),
            &key,
            function,
            &native_arguments,
            runtime.role().as_str(),
        )
    }

    pub(super) fn translate_frame_terminal_state(
        &mut self,
        state: &bray_ir::MirTaskTerminalState,
    ) -> Result<(), CodegenFailure> {
        let frame_context = self
            .frame_context
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let context = self.frame_context_argument()?;

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

        let progress = self.build_frame_progress(
            u32::try_from(kind).map_err(|_| CodegenFailure::ResourceExhausted)?,
            self.types.context().i32_type().const_zero(),
            payload,
        )?;

        self.frame_progress = Some(progress.into());

        Ok(())
    }

    fn translate_compiler_shutdown(
        &mut self,
        status: inkwell::values::IntValue<'context>,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let context = self.module.get_context();
        let exit_status = context.i32_type();
        let function_type = context.void_type().fn_type(&[exit_status.into()], false);

        let function = self
            .module
            .get_function("exit")
            .unwrap_or_else(|| self.module.add_function("exit", function_type, None));

        let status = llvm(self.builder.build_int_truncate(
            status,
            exit_status,
            "process.exit.status",
        ))?;

        llvm(
            self.builder
                .build_call(function, &[status.into()], "process.exit"),
        )?;

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

    pub(super) fn root_entry(
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
