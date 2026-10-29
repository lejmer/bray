use crate::translation::unit::core::UnitTranslator;
use crate::translation::unit::support::{int_value, llvm, pointer_value};
use bray_codegen::CodegenFailure;
use bray_ir::BoundUnitKey;
use bray_runtime_interface::{ProtectedFrameOperation, RootExecution, RuntimeRoleImplementation};
use inkwell::IntPredicate;
use inkwell::values::BasicValueEnum;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn translate_root_execution(
        &mut self,
        entry: bray_runtime_interface::ExecutableHostEntryId,
        root: &BoundUnitKey,
        execution: RootExecution,
        runtime: bray_ir::MirRuntimeReference,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        self.begin_memory_observation()?;

        if self.host_role_implementation(runtime) == RuntimeRoleImplementation::CompilerLowering
            && execution == RootExecution::Synchronous
        {
            self.translate_compiler_lowered_synchronous_root(entry, root)?;

            return Ok(None);
        }

        if execution == RootExecution::Synchronous {
            self.begin_synchronous_root_boundary(entry, root, runtime)?;

            return Ok(None);
        }

        if let RootExecution::Asynchronous { .. } = execution {
            if self.begin_performance_interval(self.function)?.is_some() {
                panic!("calibrated performance intervals require a synchronous executable entry");
            }

            let context = self.types.context();
            let outcome_type = crate::native::run_outcome_type(context, self.request.target());
            let outcome = self.allocate_panic_report_context()?;
            let root_type = self.pointer_integer_type();
            let root_storage = self.allocate_temporary(root_type, "root.handle")?;

            llvm(
                self.builder
                    .build_store(root_storage, root_type.const_zero()),
            )?;

            let backing = self.admit_root_frame(root, outcome)?;

            let admission = llvm(
                self.builder
                    .build_load(outcome_type, outcome, "root.admission"),
            )?;

            let (state, _) =
                crate::translation::unit::support::native_run_outcome(&self.builder, admission)?;

            let admitted = crate::translation::unit::support::native_run_state_is(
                &self.builder,
                state,
                bray_runtime_abi::NativeRunState::COMPLETED,
                "root.admitted",
            )?;

            let construct = context.append_basic_block(self.function, "root.construct");
            let joined = context.append_basic_block(self.function, "root.admission.finished");

            llvm(
                self.builder
                    .build_conditional_branch(admitted, construct, joined),
            )?;

            self.builder.position_at_end(construct);

            let start = self.start_admitted_root(root, runtime, backing)?;

            let status =
                crate::translation::unit::support::extract_value(&self.builder, start.into(), 0)?;

            let status =
                int_value(status).expect("root-start results must contain an integer status");

            let root =
                crate::translation::unit::support::extract_value(&self.builder, start.into(), 1)?;

            let root =
                int_value(root).expect("root-start results must contain an integer root handle");

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

            llvm(self.builder.build_store(root_storage, root))?;

            let failure = crate::translation::unit::support::native_run_outcome_value(
                context,
                &self.builder,
                self.request.target(),
                bray_runtime_abi::NativeRunState::RUNTIME_FAILURE,
                Some(llvm(self.builder.build_int_cast(
                    status,
                    root_type,
                    "root.failure.status",
                ))?),
                None,
            )?;

            // Observation replaces this only for a successfully started root. Failed
            // admission retains its original outcome and still reaches host teardown.
            llvm(self.builder.build_store(outcome, failure))?;
            llvm(self.builder.build_unconditional_branch(joined))?;
            self.builder.position_at_end(joined);

            self.host_root = Some(llvm(self.builder.build_load(
                root_type,
                root_storage,
                "root.handle.value",
            ))?);

            self.host_result = Some(llvm(self.builder.build_load(
                outcome_type,
                outcome,
                "root.initial.outcome",
            ))?);

            return Ok(None);
        }

        panic!("checked MIR effect translation violated an established compiler contract")
    }

    fn start_admitted_root(
        &mut self,
        root: &BoundUnitKey,
        runtime: bray_ir::MirRuntimeReference,
        backing: inkwell::values::PointerValue<'context>,
    ) -> Result<inkwell::values::StructValue<'context>, CodegenFailure> {
        let (constructor, signature) = self.root_entry(root, RootExecution::Synchronous)?;

        let inactive = self
            .invoke_function(constructor, signature, &[], "root.frame")?
            .expect("checked MIR effect translation requires an established mapping or value");

        self.invoke_native_runtime(
            bray_ir::MirRuntimeReference::new(
                bray_runtime_interface::RuntimeAbiRole::OutgoingDischarge,
                self.unit.target().runtime_abi(),
            ),
            &[backing.into()],
        )?;

        let context = crate::translation::unit::support::extract_value(&self.builder, inactive, 0)?;

        let context =
            pointer_value(context).expect("inactive root frames must contain a pointer context");

        let descriptor =
            crate::translation::unit::support::extract_value(&self.builder, inactive, 1)?
                .into_pointer_value();

        let frame = crate::native::activation::inactive_token(
            self.types.context(),
            self.request.target(),
            &self.builder,
            context,
            descriptor,
        )?;

        let bray_ir::MirUnitKind::ExecutableHost(host) = self.unit.kind() else {
            panic!("root execution belongs to an executable host");
        };

        let frame_storage =
            self.allocate_temporary(frame.get_type(), "root.frame.transfer.storage")?;

        llvm(self.builder.build_store(frame_storage, frame))?;

        let capacity = host
            .capacity_limits()
            .tasks()
            .map_or(u64::MAX, |capacity| u64::from(capacity.get()));

        let usize =
            crate::native::pointer_integer_type(self.types.context(), self.request.target());

        let frame_transfer = llvm(self.builder.build_ptr_to_int(
            frame_storage,
            usize,
            "root.frame.transfer",
        ))?;

        let configuration =
            crate::native::runtime_configuration_type(self.types.context(), self.request.target())
                .const_named_struct(&[
                    usize.const_int(capacity, false).into(),
                    usize.const_all_ones().into(),
                ]);

        let start = self
            .invoke_native_runtime(runtime, &[frame_transfer.into(), configuration.into()])?
            .and_then(|value| match value {
                BasicValueEnum::StructValue(value) => Some(value),
                _ => None,
            })
            .expect("checked MIR effect translation requires an established mapping or value");

        Ok(start)
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

    pub(super) fn translate_root_terminal_observation(
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

        let failure = self
            .host_result
            .take()
            .expect("root admission retains its outcome");

        let current = self
            .builder
            .get_insert_block()
            .expect("root observation has a current block");

        let context = self.types.context();
        let observe = context.append_basic_block(self.function, "root.observe");
        let joined = context.append_basic_block(self.function, "root.observation.finished");

        let present = crate::translation::unit::support::nonzero_integer(
            &self.builder,
            root.into_int_value(),
            "root.present",
        )?;

        llvm(
            self.builder
                .build_conditional_branch(present, observe, joined),
        )?;

        self.builder.position_at_end(observe);

        let observed = self
            .invoke_native_runtime(runtime, &[root])?
            .expect("root observation returns an outcome");

        let observed_block = self
            .builder
            .get_insert_block()
            .expect("root observation has a completion block");

        llvm(self.builder.build_unconditional_branch(joined))?;
        self.builder.position_at_end(joined);

        let outcome = llvm(self.builder.build_phi(failure.get_type(), "root.outcome"))?;

        outcome.add_incoming(&[(&failure, current), (&observed, observed_block)]);
        self.host_result = Some(outcome.as_basic_value());

        Ok(None)
    }

    pub(in crate::translation::unit::effect) fn root_entry(
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
        let instance = self.request.unit().executable_root_instance(root);

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
