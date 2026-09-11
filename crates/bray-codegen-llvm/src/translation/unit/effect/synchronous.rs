use bray_codegen::CodegenFailure;
use bray_ir::BoundUnitKey;
use bray_runtime_abi::NativeRunState;
use bray_runtime_interface::{ExecutableEntryResult, RootExecution};
use inkwell::IntPredicate;
use inkwell::values::BasicValueEnum;

use super::super::core::UnitTranslator;
use super::super::support::{int_value, llvm, native_run_outcome_value, pointer_value};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn translate_synchronous_root_boundary(
        &mut self,
        entry: bray_runtime_interface::ExecutableHostEntryId,
        root: &BoundUnitKey,
        runtime: bray_ir::MirRuntimeReference,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let bray_ir::MirUnitKind::ExecutableHost(host) = self.unit.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let entry_result = host
            .entry(entry)
            .map(bray_runtime_interface::ExecutableHostEntry::result)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let result_type = match entry_result {
            ExecutableEntryResult::Unit => None,
            ExecutableEntryResult::I32 => Some(self.types.context().i32_type().into()),
            ExecutableEntryResult::Fallible { ty, .. } => Some(self.types.map(ty)?),
        };

        let destination = match result_type {
            Some(result_type) => self.allocate_temporary(result_type, "root.result.storage")?,
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

        let callback_type = self.types.context().void_type().fn_type(
            &[
                usize.into(),
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
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let callback_block = self
            .types
            .context()
            .append_basic_block(callback, "root.callback");

        self.builder.position_at_end(callback_block);

        let (function, signature) = self.root_entry(root, RootExecution::Synchronous)?;

        let panic_report = self.allocate_panic_report_context()?;

        let result = self.invoke_function_with_panic_report_context(
            function,
            signature,
            &[],
            "root",
            Some(panic_report),
        )?;

        let callback_destination_handle = callback
            .get_first_param()
            .and_then(int_value)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        match (result_type, result) {
            (Some(result_type), Some(result)) => {
                let destination = llvm(
                    self.builder.build_int_to_ptr(
                        callback_destination_handle,
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

        let report = llvm(
            self.builder
                .build_load(usize, panic_report, "root.panic.report"),
        )?;

        let report = int_value(report).ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let cancelled = llvm(self.builder.build_int_compare(
            IntPredicate::EQ,
            report,
            usize.const_int(crate::translation::CANCELLATION_OUTCOME_SENTINEL, false),
            "root.cancelled",
        ))?;

        let panicked = llvm(self.builder.build_int_compare(
            IntPredicate::NE,
            report,
            usize.const_zero(),
            "root.panicked",
        ))?;

        let cancelled_block = self
            .types
            .context()
            .append_basic_block(callback, "root.cancelled");

        let panicked_block = self
            .types
            .context()
            .append_basic_block(callback, "root.panicked");

        let inspect_panic_block = self
            .types
            .context()
            .append_basic_block(callback, "root.inspect_panic");

        let completed_block = self
            .types
            .context()
            .append_basic_block(callback, "root.completed");

        llvm(self.builder.build_conditional_branch(
            cancelled,
            cancelled_block,
            inspect_panic_block,
        ))?;

        let outcome = callback
            .get_nth_param(1)
            .and_then(pointer_value)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        self.builder.position_at_end(cancelled_block);

        let cancelled_outcome = native_run_outcome_value(
            self.types.context(),
            &self.builder,
            self.request.target(),
            NativeRunState::CANCELLED,
            usize.const_zero(),
        )?;

        llvm(self.builder.build_store(outcome, cancelled_outcome))?;
        llvm(self.builder.build_return(None))?;

        self.builder.position_at_end(inspect_panic_block);

        llvm(
            self.builder
                .build_conditional_branch(panicked, panicked_block, completed_block),
        )?;

        self.builder.position_at_end(panicked_block);

        let panicked_outcome = native_run_outcome_value(
            self.types.context(),
            &self.builder,
            self.request.target(),
            NativeRunState::PANICKED,
            report,
        )?;

        llvm(self.builder.build_store(outcome, panicked_outcome))?;
        llvm(self.builder.build_return(None))?;

        self.builder.position_at_end(completed_block);

        let completed_outcome = native_run_outcome_value(
            self.types.context(),
            &self.builder,
            self.request.target(),
            NativeRunState::COMPLETED,
            callback_destination_handle,
        )?;

        llvm(self.builder.build_store(outcome, completed_outcome))?;
        llvm(self.builder.build_return(None))?;

        self.builder.position_at_end(host_block);

        let callback = callback.as_global_value().as_pointer_value();

        self.invoke_native_runtime(runtime, &[callback.into(), destination_handle.into()])?
            .ok_or(CodegenFailure::GeneratedModuleInvariant)
    }
}
