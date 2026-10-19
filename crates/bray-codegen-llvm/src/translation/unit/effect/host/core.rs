use crate::translation::unit::core::UnitTranslator;
use crate::translation::unit::support::{int_value, llvm};
use bray_codegen::CodegenFailure;
use bray_ir::MirHostOperation;
use bray_runtime_interface::{RootExecution, RuntimeRoleImplementation};
use inkwell::IntPredicate;
use inkwell::values::BasicValueEnum;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(in crate::translation::unit::effect) fn translate_host_operation(
        &mut self,
        operation_id: bray_ir::MirOperationId,
        operation: &MirHostOperation,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        // Keep this exhaustive so every host operation requires an explicit translation.
        match operation {
            MirHostOperation::InitializeRuntime { runtime } => {
                self.initialize_host_runtime(*runtime)?;

                Ok(None)
            }
            MirHostOperation::BeginProductEntry => {
                self.begin_product_entry()?;

                Ok(None)
            }
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
            } => self.translate_root_execution(*entry, root, *execution, *runtime),
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

                let status = if self.synchronous_continuation.is_some() {
                    self.finish_synchronous_root_boundary(status)?
                } else {
                    self.finish_asynchronous_performance_interval(*entry, status)?;

                    status
                };

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

    fn initialize_host_runtime(
        &mut self,
        runtime: bray_ir::MirRuntimeReference,
    ) -> Result<(), CodegenFailure> {
        let bray_ir::MirUnitKind::ExecutableHost(host) = self.unit.kind() else {
            panic!("runtime initialization requires an executable host");
        };

        let usize =
            crate::native::pointer_integer_type(self.types.context(), self.request.target());

        let capacity = host
            .capacity_limits()
            .tasks()
            .map_or(u64::MAX, |capacity| u64::from(capacity.get()));

        let status = self
            .invoke_native_runtime(
                runtime,
                &[
                    usize.const_int(capacity, false).into(),
                    usize.const_all_ones().into(),
                ],
            )?
            .and_then(int_value)
            .expect("runtime initialization must return its native status");

        let initialized = llvm(self.builder.build_int_compare(
            IntPredicate::EQ,
            status,
            status.get_type().const_int(
                u64::from(bray_runtime_abi::NativeRuntimeStatus::SUCCESS.code()),
                false,
            ),
            "runtime.initialized",
        ))?;

        let already_initialized = llvm(self.builder.build_int_compare(
            IntPredicate::EQ,
            status,
            status.get_type().const_int(
                u64::from(bray_runtime_abi::NativeRuntimeStatus::ALREADY_INITIALIZED.code()),
                false,
            ),
            "runtime.already_initialized",
        ))?;

        let ready = llvm(
            self.builder
                .build_or(initialized, already_initialized, "runtime.ready"),
        )?;

        let context = self.types.context();
        let continuation = context.append_basic_block(self.function, "runtime.ready");
        let failure = context.append_basic_block(self.function, "runtime.initialization.failed");

        llvm(
            self.builder
                .build_conditional_branch(ready, continuation, failure),
        )?;

        self.builder.position_at_end(failure);
        self.translate_compiler_shutdown(context.i64_type().const_int(1, false))?;
        llvm(self.builder.build_unreachable())?;
        self.builder.position_at_end(continuation);

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

    fn begin_product_entry(&mut self) -> Result<(), CodegenFailure> {
        if self.request.mappings().product_host().is_none() {
            return Ok(());
        }

        let context = self.types.context();

        let observation = crate::translation::product::control(
            context,
            self.module,
            self.request,
            &self.builder,
            bray_runtime_abi::NativeProductHostOperation::ACQUIRE_ENTRY,
        )?;

        let status =
            crate::translation::unit::support::extract_value(&self.builder, observation, 0)?
                .into_int_value();

        let accepted = llvm(self.builder.build_int_compare(
            IntPredicate::EQ,
            status,
            status.get_type().const_zero(),
            "product.entry.accepted",
        ))?;

        let continuation = context.append_basic_block(self.function, "product.entry.accepted");
        let rejection = context.append_basic_block(self.function, "product.entry.rejected");

        llvm(
            self.builder
                .build_conditional_branch(accepted, continuation, rejection),
        )?;

        self.builder.position_at_end(rejection);
        self.translate_compiler_shutdown(context.i64_type().const_int(1, false))?;
        llvm(self.builder.build_unreachable())?;
        self.builder.position_at_end(continuation);

        Ok(())
    }

    fn finish_product_statics(&mut self) -> Result<(), CodegenFailure> {
        if self.request.mappings().product_host().is_none() {
            return Ok(());
        }

        let context = self.types.context();

        let released = crate::translation::product::control(
            context,
            self.module,
            self.request,
            &self.builder,
            bray_runtime_abi::NativeProductHostOperation::RELEASE_ENTRY,
        )?;

        let release_status =
            crate::translation::unit::support::extract_value(&self.builder, released, 0)?
                .into_int_value();

        let release_failed = llvm(self.builder.build_int_compare(
            IntPredicate::EQ,
            release_status,
            release_status.get_type().const_int(
                u64::from(bray_runtime_abi::NativeProductHostStatus::RUNTIME_FAILURE.code()),
                false,
            ),
            "product.entry.release_failed",
        ))?;

        let observation = crate::translation::product::control(
            context,
            self.module,
            self.request,
            &self.builder,
            bray_runtime_abi::NativeProductHostOperation::FINISH_ROOT,
        )?;

        let status = int_value(crate::translation::unit::support::extract_value(
            &self.builder,
            observation,
            0,
        )?)
        .expect("checked MIR effect translation requires an established mapping or value");

        let state = int_value(crate::translation::unit::support::extract_value(
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

        let failed = llvm(self.builder.build_or(
            failed,
            release_failed,
            "product.cleanup.or_release_failed",
        ))?;

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

    pub(super) fn begin_memory_observation(&self) -> Result<(), CodegenFailure> {
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

    pub(in crate::translation::unit::effect) fn observe_frame_cancellation(
        &self,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let context_type = self
            .frame_context
            .expect("frame cancellation requires a protected frame");

        let pointer = self.frame_context_pointer()?;

        crate::translation::frame::cancellation_requested(
            self.types.context(),
            &self.builder,
            context_type,
            pointer,
        )
        .map(Into::into)
    }

    pub(in crate::translation::unit::effect) fn translate_frame_terminal_state(
        &mut self,
        state: &bray_ir::MirTaskTerminalState,
    ) -> Result<(), CodegenFailure> {
        let frame_context = self
            .frame_context
            .expect("checked MIR effect translation requires an established mapping or value");

        let pointer = self.frame_context_pointer()?;

        let state_pointer =
            llvm(
                self.builder
                    .build_struct_gep(frame_context, pointer, 0, "frame.state.pointer"),
            )?;

        let completion_state = if matches!(state, bray_ir::MirTaskTerminalState::Completed { .. }) {
            self.unit
                .frame_descriptor()
                .expect("protected frame retains its descriptor")
                .completion_state()
        } else {
            None
        };

        let terminal = match completion_state {
            Some(state) => self
                .types
                .context()
                .i32_type()
                .const_int(u64::from(state.raw()), false),
            None => self.terminal_frame_state()?,
        };

        llvm(self.builder.build_store(state_pointer, terminal))?;

        let (kind, payload) = match state {
            bray_ir::MirTaskTerminalState::Completed { value, .. } => {
                let completion = llvm(self.builder.build_struct_gep(
                    frame_context,
                    pointer,
                    1,
                    "frame.completion",
                ))?;

                let ty = self.operand_type(value);
                let value = self.operand(value)?;

                self.store_value(completion, value, ty)?;

                let payload = llvm(self.builder.build_ptr_to_int(
                    completion,
                    self.types.context().i64_type(),
                    "frame.completion.handle",
                ))?;

                (1, payload.into())
            }
            bray_ir::MirTaskTerminalState::Cancelled(report) => (2, self.operand(report)?),
            bray_ir::MirTaskTerminalState::Panicked(value) => (3, self.operand(value)?),
        };

        let progress = self.build_frame_progress(
            crate::conversion::resource_limit(kind, "host_effect_progress_kind")?,
            self.types.context().i32_type().const_int(
                u64::from(completion_state.map_or(0, |state| state.raw())),
                false,
            ),
            payload,
        )?;

        let progress = if let bray_ir::MirTaskTerminalState::Completed {
            report: Some(report),
            ..
        } = state
        {
            let report = self.operand(report)?;

            super::super::super::support::insert_value(&self.builder, progress.into(), report, 3)?
        } else {
            progress.into()
        };

        self.frame_progress = Some(progress);

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

    pub(in crate::translation::unit::effect) fn host_role_implementation(
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
}
