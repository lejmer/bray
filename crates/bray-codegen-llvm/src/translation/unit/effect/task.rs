use super::super::core::UnitTranslator;
use super::super::support::{extract_value, int_value, llvm};
use bray_codegen::CodegenFailure;
use bray_ir::MirRunResultVariants;
use inkwell::IntPredicate;
use inkwell::values::BasicValueEnum;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn try_start_native_task(
        &mut self,
        allocation: bray_ir::MirRuntimeReference,
        start: bray_ir::MirRuntimeReference,
        frame: BasicValueEnum<'context>,
        destination: &bray_ir::MirPlace,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let context = self.types.context();

        let function = self
            .builder
            .get_insert_block()
            .and_then(|block| block.get_parent())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let allocated = context.append_basic_block(function, "task.allocated");
        let published = context.append_basic_block(function, "task.published");
        let rejected = context.append_basic_block(function, "task.rejected");
        let finished = context.append_basic_block(function, "task.admission.finished");

        let allocation = self
            .invoke_native_runtime(allocation, &[])?
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let status = int_value(extract_value(&self.builder, allocation, 0)?)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let task = extract_value(&self.builder, allocation, 1)?;

        let success = llvm(self.builder.build_int_compare(
            IntPredicate::EQ,
            status,
            status.get_type().const_zero(),
            "task.allocation.success",
        ))?;

        llvm(
            self.builder
                .build_conditional_branch(success, allocated, rejected),
        )?;

        self.builder.position_at_end(allocated);

        let status = self
            .invoke_native_runtime(start, &[task, frame])?
            .and_then(int_value)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let success = llvm(self.builder.build_int_compare(
            IntPredicate::EQ,
            status,
            status.get_type().const_zero(),
            "task.publication.success",
        ))?;

        llvm(
            self.builder
                .build_conditional_branch(success, published, rejected),
        )?;

        self.builder.position_at_end(published);

        let destination = self.place(destination)?;

        llvm(self.builder.build_store(destination, task))?;
        llvm(self.builder.build_unconditional_branch(finished))?;
        self.builder.position_at_end(rejected);
        llvm(self.builder.build_unconditional_branch(finished))?;
        self.builder.position_at_end(finished);

        let result = llvm(self.builder.build_phi(context.bool_type(), "task.admitted"))?;

        result.add_incoming(&[
            (&context.bool_type().const_int(1, false), published),
            (&context.bool_type().const_zero(), rejected),
        ]);

        Ok(result.as_basic_value())
    }

    pub(super) fn resolve_native_run(
        &mut self,
        runtime: bray_ir::MirRuntimeReference,
        mut arguments: Vec<BasicValueEnum<'context>>,
        result: bray_symbols::TypeId,
        variants: MirRunResultVariants,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let result_type = self.types.map(result)?;

        let mapping = self
            .type_mapping(result)
            .and_then(|mapping| mapping.layout())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let storage = self.aligned_alloca(result, mapping.alignment().get(), "task.result")?;
        let layout = self.run_result_layout(result, variants)?;

        arguments.extend([
            BasicValueEnum::PointerValue(storage),
            BasicValueEnum::PointerValue(layout),
        ]);

        let status = self
            .invoke_native_runtime(runtime, &arguments)?
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        self.require_runtime_success(status, "task.resolve")?;

        llvm(
            self.builder
                .build_load(result_type, storage, "task.result.value"),
        )
    }

    pub(super) fn borrow_native_task_completion(
        &mut self,
        runtime: bray_ir::MirRuntimeReference,
        task: BasicValueEnum<'context>,
        result: bray_symbols::TypeId,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let context = self.types.context();
        let usize = crate::native::pointer_integer_type(context, self.request.target());
        let output = self.allocate_temporary(usize, "task.completion.address")?;

        let status = self
            .invoke_native_runtime(runtime, &[task, output.into()])?
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        self.require_runtime_success(status, "task.completion.borrow")?;

        let address = llvm(self.builder.build_load(
            usize,
            output,
            "task.completion.address.value",
        ))?
        .into_int_value();

        let pointer = llvm(self.builder.build_int_to_ptr(
            address,
            context.ptr_type(inkwell::AddressSpace::default()),
            "task.completion.borrowed",
        ))?;

        let present = super::super::support::nonzero_integer(
            &self.builder,
            address,
            "task.completion.present",
        )?;

        let value = self.construct_nullable_present(result, pointer.into())?;
        let absent = self.types.map(result)?.const_zero();

        llvm(
            self.builder
                .build_select(present, value, absent, "task.completion"),
        )
    }

    pub(super) fn require_runtime_success(
        &mut self,
        status: BasicValueEnum<'context>,
        name: &str,
    ) -> Result<(), CodegenFailure> {
        let status = int_value(status).ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let function = self
            .builder
            .get_insert_block()
            .and_then(|block| block.get_parent())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let failed = self
            .types
            .context()
            .append_basic_block(function, &format!("{name}.failed"));

        let succeeded = self
            .types
            .context()
            .append_basic_block(function, &format!("{name}.succeeded"));

        let success = llvm(self.builder.build_int_compare(
            IntPredicate::EQ,
            status,
            status.get_type().const_zero(),
            &format!("{name}.success"),
        ))?;

        llvm(
            self.builder
                .build_conditional_branch(success, succeeded, failed),
        )?;

        self.builder.position_at_end(failed);
        self.call_void_intrinsic("llvm.trap")?;
        llvm(self.builder.build_unreachable())?;

        self.builder.position_at_end(succeeded);

        Ok(())
    }
}
