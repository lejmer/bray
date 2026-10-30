use crate::translation::unit::core::UnitTranslator;
use crate::translation::unit::support::{int_value, llvm};
use bray_codegen::CodegenFailure;
use bray_ir::MirRuntimeReference;
use bray_runtime_interface::RuntimeRoleImplementation;
use inkwell::IntPredicate;
use inkwell::values::{BasicValueEnum, IntValue};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn structured_shutdown(
        &mut self,
        execution: Option<MirRuntimeReference>,
        runtime: MirRuntimeReference,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let status = self
            .host_status
            .take()
            .unwrap_or_else(|| self.types.context().i64_type().const_zero());

        // Execution retirement may still need the host's report and cleanup storage.
        if let Some(execution) = execution {
            self.shutdown_service(execution, status)?;
        }

        if self.host_role_implementation(runtime) != RuntimeRoleImplementation::CompilerLowering {
            self.shutdown_service(runtime, status)?;
        }

        self.translate_compiler_shutdown(status)
    }

    fn shutdown_service(
        &mut self,
        runtime: MirRuntimeReference,
        root_status: IntValue<'context>,
    ) -> Result<(), CodegenFailure> {
        let status = self
            .invoke_runtime(runtime, &[])?
            .and_then(int_value)
            .expect("shutdown service must return its native status");

        let success = llvm(self.builder.build_int_compare(
            IntPredicate::EQ,
            status,
            status.get_type().const_int(
                u64::from(bray_runtime_abi::NativeRuntimeStatus::SUCCESS.code()),
                false,
            ),
            "shutdown.succeeded",
        ))?;

        let inactive = llvm(self.builder.build_int_compare(
            IntPredicate::EQ,
            status,
            status.get_type().const_int(
                u64::from(bray_runtime_abi::NativeRuntimeStatus::NOT_INITIALIZED.code()),
                false,
            ),
            "shutdown.inactive",
        ))?;

        let retired = llvm(self.builder.build_or(success, inactive, "shutdown.retired"))?;
        let context = self.types.context();
        let continuation = context.append_basic_block(self.function, "shutdown.retired");
        let failure = context.append_basic_block(self.function, "shutdown.failed");

        llvm(
            self.builder
                .build_conditional_branch(retired, continuation, failure),
        )?;

        self.builder.position_at_end(failure);

        let root_succeeded = llvm(self.builder.build_int_compare(
            IntPredicate::EQ,
            root_status,
            root_status.get_type().const_zero(),
            "shutdown.root.succeeded",
        ))?;

        let failed_status = llvm(self.builder.build_select(
            root_succeeded,
            root_status.get_type().const_int(1, false),
            root_status,
            "shutdown.failure.status",
        ))?
        .into_int_value();

        self.translate_compiler_shutdown(failed_status)?;
        llvm(self.builder.build_unreachable())?;
        self.builder.position_at_end(continuation);

        Ok(())
    }

    pub(super) fn translate_compiler_shutdown(
        &mut self,
        status: IntValue<'context>,
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
}
