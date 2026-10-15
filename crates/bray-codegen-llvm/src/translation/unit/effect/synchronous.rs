use bray_codegen::CodegenFailure;
use bray_ir::BoundUnitKey;
use bray_runtime_abi::NativeRunState;
use bray_runtime_interface::{ExecutableEntryResult, RootExecution};

use super::super::core::UnitTranslator;
use super::super::support::{
    int_value, llvm, native_run_outcome, native_run_outcome_value, native_run_state_is,
    pointer_value,
};

pub(in crate::translation::unit) struct SynchronousContinuation<'context> {
    host: inkwell::basic_block::BasicBlock<'context>,
    status: inkwell::values::PointerValue<'context>,
    runtime: bray_ir::MirRuntimeReference,
}

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn begin_synchronous_root_boundary(
        &mut self,
        entry: bray_runtime_interface::ExecutableHostEntryId,
        root: &BoundUnitKey,
        runtime: bray_ir::MirRuntimeReference,
    ) -> Result<(), CodegenFailure> {
        let bray_ir::MirUnitKind::ExecutableHost(host) = self.unit.kind() else {
            panic!("checked MIR effect translation violated an established compiler contract");
        };

        let entry_result = host
            .entry(entry)
            .map(bray_runtime_interface::ExecutableHostEntry::result)
            .expect("checked MIR effect translation requires an established mapping or value");

        let result_type = match entry_result {
            ExecutableEntryResult::Unit => None,
            ExecutableEntryResult::I32 => Some(self.types.context().i32_type().into()),
            ExecutableEntryResult::Fallible { ty, .. } => Some(self.types.map(ty)?),
        };

        let exit_status =
            self.allocate_temporary(self.types.context().i64_type(), "root.exit.storage")?;

        llvm(self.builder.build_store(
            exit_status,
            self.types.context().i64_type().const_int(1, false),
        ))?;

        let usize =
            crate::native::pointer_integer_type(self.types.context(), self.request.target());

        let callback_type = self.types.context().void_type().fn_type(
            &[
                usize.into(),
                self.types
                    .context()
                    .ptr_type(inkwell::AddressSpace::default())
                    .into(),
                self.types
                    .context()
                    .ptr_type(inkwell::AddressSpace::default())
                    .into(),
            ],
            false,
        );

        let callback = self.module.add_function(
            &format!("bray_host_synchronous_root_callback_{}", entry.slot()),
            callback_type,
            Some(inkwell::module::Linkage::Private),
        );

        let host_block = self
            .builder
            .get_insert_block()
            .expect("checked MIR effect translation requires an established mapping or value");

        let callback_block = self
            .types
            .context()
            .append_basic_block(callback, "root.callback");

        self.builder.position_at_end(callback_block);

        let result_destination = match result_type {
            Some(result_type) => self.allocate_temporary(result_type, "root.result.storage")?,
            None => self
                .types
                .context()
                .ptr_type(inkwell::AddressSpace::default())
                .const_null(),
        };

        let result_handle = llvm(self.builder.build_ptr_to_int(
            result_destination,
            usize,
            "root.result.handle",
        ))?;

        let (function, signature) = self.root_entry(root, RootExecution::Synchronous)?;

        let outcome_type =
            crate::native::run_outcome_type(self.types.context(), self.request.target());

        let panic_report = callback
            .get_nth_param(1)
            .and_then(pointer_value)
            .expect("checked MIR effect translation requires an established mapping or value");

        llvm(
            self.builder
                .build_store(panic_report, outcome_type.const_zero()),
        )?;

        let performance_loop = self.begin_performance_interval(callback)?;

        let result = self.invoke_function_with_panic_report_context(
            function,
            signature,
            &[],
            "root",
            Some(panic_report),
        )?;

        match (result_type, result) {
            (Some(result_type), Some(result)) => {
                if result.get_type() != result_type {
                    panic!(
                        "checked MIR effect translation violated an established compiler contract"
                    );
                }

                llvm(self.builder.build_store(result_destination, result))?;
            }
            (None, None) => {}
            unexpected => panic!(
                "checked MIR effect translation violated an established compiler contract: {unexpected:?}"
            ),
        }

        let observed_outcome = if matches!(
            self.request.options().runtime_observations(),
            bray_codegen::RuntimeObservationMode::PerformanceInterval { .. }
        ) {
            let value = llvm(self.builder.build_load(
                outcome_type,
                panic_report,
                "root.performance.outcome",
            ))?;

            let (state, _) = native_run_outcome(&self.builder, value)?;

            let completed = native_run_state_is(
                &self.builder,
                state,
                NativeRunState::COMPLETED,
                "root.performance.completed",
            )?;

            let entry_succeeded = self.entry_result_succeeded(entry_result, result)?;

            let successful = llvm(self.builder.build_and(
                completed,
                entry_succeeded,
                "root.performance.succeeded",
            ))?;

            self.finish_performance_interval_iteration(performance_loop, successful)?;

            Some(value)
        } else {
            None
        };

        let value = match observed_outcome {
            Some(value) => value,
            None => llvm(
                self.builder
                    .build_load(outcome_type, panic_report, "root.outcome"),
            )?,
        };

        let (state, report) = native_run_outcome(&self.builder, value)?;

        let completed = native_run_state_is(
            &self.builder,
            state,
            NativeRunState::COMPLETED,
            "root.completed",
        )?;

        let payload = llvm(self.builder.build_select(
            completed,
            result_handle,
            report,
            "root.outcome.payload",
        ))?;

        let destination = llvm(self.builder.build_struct_gep(
            outcome_type,
            panic_report,
            1,
            "root.outcome.payload.destination",
        ))?;

        llvm(self.builder.build_store(destination, payload))?;

        self.host_result = Some(llvm(self.builder.build_load(
            outcome_type,
            panic_report,
            "root.terminal",
        ))?);

        self.synchronous_continuation = Some(SynchronousContinuation {
            host: host_block,
            status: exit_status,
            runtime,
        });

        Ok(())
    }

    pub(super) fn finish_synchronous_root_boundary(
        &mut self,
        exit: inkwell::values::IntValue<'context>,
    ) -> Result<inkwell::values::IntValue<'context>, CodegenFailure> {
        let continuation = self
            .synchronous_continuation
            .take()
            .expect("synchronous root callback is pending");

        let callback = self
            .builder
            .get_insert_block()
            .and_then(inkwell::basic_block::BasicBlock::get_parent)
            .expect("root resolution remains inside its entry callback");

        let context = self.types.context();
        let pointer = context.ptr_type(inkwell::AddressSpace::default());

        let handle = callback
            .get_first_param()
            .and_then(int_value)
            .expect("terminal callback receives its exit-status slot");

        let destination = llvm(self.builder.build_int_to_ptr(
            handle,
            pointer,
            "root.exit.destination",
        ))?;

        let outcome = callback
            .get_nth_param(1)
            .and_then(pointer_value)
            .expect("terminal callback receives its outcome slot");

        let outcome_type = crate::native::run_outcome_type(context, self.request.target());

        let original = llvm(
            self.builder
                .build_load(outcome_type, outcome, "root.observed"),
        )?;

        let (state, _) = native_run_outcome(&self.builder, original)?;

        let cancelled = native_run_state_is(
            &self.builder,
            state,
            NativeRunState::CANCELLED,
            "root.cancelled",
        )?;

        let state = llvm(self.builder.build_select(
            cancelled,
            state,
            state.get_type().const_zero(),
            "root.continuation.state",
        ))?;

        let finished = super::super::support::insert_value(
            &self.builder,
            outcome_type.const_zero().into(),
            state,
            0,
        )?;

        llvm(self.builder.build_store(destination, exit))?;
        llvm(self.builder.build_store(outcome, finished))?;
        llvm(self.builder.build_return(None))?;
        self.builder.position_at_end(continuation.host);

        let usize = crate::native::pointer_integer_type(context, self.request.target());

        let destination_handle = llvm(self.builder.build_ptr_to_int(
            continuation.status,
            usize,
            "root.exit.handle",
        ))?;

        let outcome = self.allocate_temporary(outcome_type, "root.entry.outcome")?;

        let initial = native_run_outcome_value(
            context,
            &self.builder,
            self.request.target(),
            NativeRunState::RUNTIME_FAILURE,
            usize.const_int(4, false).into(),
        )?;

        llvm(self.builder.build_store(outcome, initial))?;

        let terminal = self
            .invoke_native_runtime(
                continuation.runtime,
                &[
                    callback.as_global_value().as_pointer_value().into(),
                    destination_handle.into(),
                    outcome.into(),
                ],
            )?
            .expect("source entry returns its terminal status");

        let outcome = llvm(
            self.builder
                .build_load(outcome_type, outcome, "root.entry.result"),
        )?;

        // Only pre-invocation failures and later entry cleanup remain unobserved here.
        super::super::callback::resolve_callback_outcome(
            context,
            self.module,
            self.request,
            &self.builder,
            self.function,
            outcome,
        )?;

        super::super::callback::resolve_terminal_status(
            context,
            self.module,
            self.request,
            &self.builder,
            self.function,
            terminal,
        )?;

        let exit = llvm(self.builder.build_load(
            context.i64_type(),
            continuation.status,
            "root.exit",
        ))?
        .into_int_value();

        let terminal = llvm(self.builder.build_int_z_extend(
            terminal.into_int_value(),
            context.i64_type(),
            "root.entry.status",
        ))?;

        llvm(
            self.builder
                .build_or(exit, terminal, "root.terminal.status"),
        )
    }
}
