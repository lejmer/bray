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

                if self.host_role_implementation(*runtime)
                    == RuntimeRoleImplementation::CompilerLowering
                    && *execution == RootExecution::Synchronous
                {
                    self.translate_compiler_lowered_synchronous_root(*entry, root)?;

                    return Ok(None);
                }

                if *execution == RootExecution::Synchronous {
                    self.host_result =
                        Some(self.translate_synchronous_root_boundary(*entry, root, *runtime)?);

                    return Ok(None);
                }

                if let RootExecution::Asynchronous { frame } = execution {
                    if self.begin_performance_interval(self.function)?.is_some() {
                        panic!(
                            "calibrated performance intervals require a synchronous executable entry"
                        );
                    }

                    let (constructor, signature) =
                        self.root_entry(root, RootExecution::Synchronous)?;

                    let inactive = self
                        .invoke_function(constructor, signature, &[], "root.frame")?
                        .expect("checked MIR effect translation requires an established mapping or value");

                    let context = super::super::support::extract_value(&self.builder, inactive, 0)?;

                    let context = pointer_value(context)
                        .expect("inactive root frames must contain a pointer context");

                    let context = llvm(self.builder.build_ptr_to_int(
                        context,
                        crate::native::pointer_integer_type(
                            self.types.context(),
                            self.request.target(),
                        ),
                        "root.frame.context",
                    ))?;

                    let bray_ir::MirUnitKind::ExecutableHost(host) = self.unit.kind() else {
                        panic!(
                            "checked MIR effect translation violated an established compiler contract"
                        );
                    };

                    let adapter_name = host
                        .entry(*entry)
                        .and_then(bray_runtime_interface::ExecutableHostEntry::root_frame_adapter)
                        .expect("checked MIR effect translation requires an established mapping or value");

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
                    .expect(
                        "checked MIR effect translation requires an established mapping or value",
                    );

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
                        .expect("checked MIR effect translation requires an established mapping or value");

                    let status =
                        super::super::support::extract_value(&self.builder, start.into(), 0)?;

                    let status = int_value(status)
                        .expect("root-start results must contain an integer status");

                    let root =
                        super::super::support::extract_value(&self.builder, start.into(), 1)?;

                    let root = int_value(root)
                        .expect("root-start results must contain an integer root handle");

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

                panic!("checked MIR effect translation violated an established compiler contract")
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

                self.finish_asynchronous_performance_interval(*entry, status)?;

                self.host_status = Some(match self.host_status.take() {
                    Some(current) => llvm(self.builder.build_or(current, status, "host.status"))?,
                    None => status,
                });

                Ok(None)
            }
            MirHostOperation::BeginStaticCleanup => {
                self.finish_test_entry_selection()?;
                self.finish_product_statics()?;

                Ok(None)
            }
            MirHostOperation::ReportCleanupIncidents { runtime } => {
                if self.host_role_implementation(*runtime)
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

                if self.host_role_implementation(*runtime)
                    != RuntimeRoleImplementation::CompilerLowering
                {
                    self.invoke_runtime(*runtime, &[])?;
                }

                self.translate_compiler_shutdown(status)
            }
        }
    }

    fn translate_compiler_lowered_synchronous_root(
        &mut self,
        entry: bray_runtime_interface::ExecutableHostEntryId,
        root: &BoundUnitKey,
    ) -> Result<(), CodegenFailure> {
        let (function, signature) = self.root_entry(root, RootExecution::Synchronous)?;

        let performance_loop = self.begin_performance_interval(self.function)?;
        let result = self.invoke_function(function, signature, &[], "root")?;

        if matches!(
            self.request.options().runtime_observations(),
            bray_codegen::RuntimeObservationMode::PerformanceInterval { .. }
        ) {
            let bray_ir::MirUnitKind::ExecutableHost(host) = self.unit.kind() else {
                panic!("checked MIR effect translation violated an established compiler contract");
            };

            let entry_result = host
                .entry(entry)
                .map(bray_runtime_interface::ExecutableHostEntry::result)
                .expect("checked MIR effect translation requires an established mapping or value");

            let succeeded = self.entry_result_succeeded(entry_result, result)?;

            self.finish_performance_interval_iteration(performance_loop, succeeded)?;
        }

        self.host_result = result;

        Ok(())
    }

    fn finish_asynchronous_performance_interval(
        &self,
        entry: bray_runtime_interface::ExecutableHostEntryId,
        status: inkwell::values::IntValue<'context>,
    ) -> Result<(), CodegenFailure> {
        let bray_ir::MirUnitKind::ExecutableHost(host) = self.unit.kind() else {
            panic!("checked MIR effect translation violated an established compiler contract");
        };

        if !host
            .entry(entry)
            .is_some_and(|entry| matches!(entry.root(), RootExecution::Asynchronous { .. }))
            || !matches!(
                self.request.options().runtime_observations(),
                bray_codegen::RuntimeObservationMode::PerformanceInterval { .. }
            )
        {
            return Ok(());
        }

        let successful = llvm(self.builder.build_int_compare(
            IntPredicate::EQ,
            status,
            status.get_type().const_zero(),
            "root.performance.succeeded",
        ))?;

        self.finish_performance_interval_iteration(None, successful)
    }

    fn finish_product_statics(&mut self) -> Result<(), CodegenFailure> {
        let Some(host) = self.request.mappings().product_host() else {
            return Ok(());
        };

        let descriptor = self
            .module
            .get_global(host.descriptor_symbol().as_str())
            .expect("checked MIR effect translation requires an established mapping or value");

        let context = self.types.context();
        let role = bray_runtime_interface::RuntimeAbiRole::ProductHostControl;

        let function = crate::native::declare_runtime_function(
            self.module,
            context,
            self.request.target(),
            role,
        )?;

        let key = bray_codegen::CodegenSymbolKey::Runtime(bray_ir::MirRuntimeReference::new(
            role,
            self.unit.target().runtime_abi(),
        ));

        let observation = crate::native::invoke_function(
            context,
            &self.builder,
            self.request.target(),
            &key,
            function,
            &[
                descriptor.as_pointer_value().into(),
                context
                    .i32_type()
                    .const_int(
                        u64::from(bray_runtime_abi::NativeProductHostOperation::FINISH_ROOT.code()),
                        false,
                    )
                    .into(),
            ],
            "product.cleanup",
        )?
        .expect("checked MIR effect translation requires an established mapping or value");

        let status = int_value(super::super::support::extract_value(
            &self.builder,
            observation,
            0,
        )?)
        .expect("checked MIR effect translation requires an established mapping or value");

        let state = int_value(super::super::support::extract_value(
            &self.builder,
            observation,
            1,
        )?)
        .expect("checked MIR effect translation requires an established mapping or value");

        let not_success = llvm(self.builder.build_int_compare(
            IntPredicate::NE,
            status,
            status.get_type().const_zero(),
            "product.cleanup.not_success",
        ))?;

        let not_closed = llvm(self.builder.build_int_compare(
            IntPredicate::NE,
            status,
            status.get_type().const_int(
                u64::from(bray_runtime_abi::NativeProductHostStatus::CLOSED.code()),
                false,
            ),
            "product.cleanup.not_closed",
        ))?;

        let failed = llvm(self.builder.build_and(
            not_success,
            not_closed,
            "product.cleanup.failed",
        ))?;

        let pending = llvm(self.builder.build_int_compare(
            IntPredicate::NE,
            state,
            state.get_type().const_int(
                u64::from(bray_runtime_abi::NativeProductHostState::CLOSED.code()),
                false,
            ),
            "product.cleanup.pending",
        ))?;

        let failed = llvm(
            self.builder
                .build_or(failed, pending, "product.cleanup.incomplete"),
        )?;

        let failed = llvm(self.builder.build_int_z_extend(
            failed,
            context.i64_type(),
            "product.cleanup.status",
        ))?;

        self.host_status = Some(match self.host_status.take() {
            Some(status) => {
                let succeeded = llvm(self.builder.build_int_compare(
                    IntPredicate::EQ,
                    status,
                    status.get_type().const_zero(),
                    "host.succeeded",
                ))?;

                llvm(
                    self.builder
                        .build_select(succeeded, failed, status, "host.status"),
                )?
                .into_int_value()
            }
            None => failed,
        });

        Ok(())
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
            .expect("checked MIR effect translation requires an established mapping or value");

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
            .expect("checked MIR effect translation requires an established mapping or value");

        self.host_selection_statuses.push((status, block));
        llvm(self.builder.build_unconditional_branch(shutdown))?;
        self.builder.position_at_end(shutdown);

        let status = self
            .builder
            .build_phi(self.types.context().i64_type(), "test.host.status")
            .map_err(CodegenFailure::backend_library)?;

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
            .expect("checked MIR effect translation requires an established mapping or value");

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
            panic!("checked MIR effect translation violated an established compiler contract");
        };

        let synchronous = host
            .entry(entry)
            .is_some_and(|entry| entry.root() == RootExecution::Synchronous);

        if synchronous
            || self.host_role_implementation(runtime) == RuntimeRoleImplementation::CompilerLowering
        {
            return Ok(None);
        }

        let root = self
            .host_root
            .as_ref()
            .copied()
            .expect("checked MIR effect translation requires an established mapping or value");

        self.host_result = self.invoke_native_runtime(runtime, &[root])?;

        Ok(None)
    }

    pub(super) fn translate_frame_terminal_state(
        &mut self,
        state: &bray_ir::MirTaskTerminalState,
    ) -> Result<(), CodegenFailure> {
        let frame_context = self
            .frame_context
            .expect("checked MIR effect translation requires an established mapping or value");

        let context = self.frame_context_argument();

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

                (1, payload.into())
            }
            bray_ir::MirTaskTerminalState::Cancelled => {
                (2, self.types.context().i64_type().const_zero().into())
            }
            bray_ir::MirTaskTerminalState::Panicked(value) => (3, self.operand(value)?),
        };

        let progress = self.build_frame_progress(
            crate::conversion::resource_limit(kind, "host_effect_progress_kind")?,
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
    ) -> RuntimeRoleImplementation {
        let bray_ir::MirUnitKind::ExecutableHost(host) = self.unit.kind() else {
            panic!("checked MIR effect translation violated an established compiler contract");
        };

        host.role_binding(runtime.role())
            .map(bray_runtime_interface::RuntimeRoleBinding::implementation)
            .expect("executable hosts must bind every referenced runtime role")
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
            .expect("checked MIR effect translation requires an established mapping or value");

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
            .expect("checked MIR effect translation requires an established mapping or value");

        let function = self
            .module
            .get_function(symbol.name().as_str())
            .expect("checked MIR effect translation requires an established mapping or value");

        Ok((function, symbol.signature()))
    }
}
