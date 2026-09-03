use super::super::core::UnitTranslator;
use super::super::support::{extract_value, int_value, llvm};
use bray_codegen::{CodegenFailure, CodegenHelperMapping, CodegenSymbolKey, CodegenTypeKind};
use bray_ir::MirRunResultVariants;
use inkwell::IntPredicate;
use inkwell::values::{BasicValueEnum, PointerValue};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn allocate_native_task(
        &mut self,
        runtime: bray_ir::MirRuntimeReference,
    ) -> Result<inkwell::values::IntValue<'context>, CodegenFailure> {
        let allocation = self
            .invoke_native_runtime(runtime, &[])?
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let status = extract_value(&self.builder, allocation, 0)?;

        self.require_runtime_success(status, "task.allocation")?;

        int_value(extract_value(&self.builder, allocation, 1)?)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)
    }

    pub(super) fn start_native_task(
        &mut self,
        runtime: bray_ir::MirRuntimeReference,
        task: inkwell::values::IntValue<'context>,
        frame: BasicValueEnum<'context>,
    ) -> Result<(), CodegenFailure> {
        let status = self
            .invoke_native_runtime(runtime, &[task.into(), frame])?
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        self.require_runtime_success(status, "task.start")?;

        Ok(())
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "task observation creation retains its typed result and two cleanup phases"
    )]
    pub(super) fn create_task_observation_frame(
        &mut self,
        creation: &CodegenHelperMapping,
        task: BasicValueEnum<'context>,
        request_cancellation: bool,
        result: bray_symbols::TypeId,
        variants: MirRunResultVariants,
        cancellation: &CodegenHelperMapping,
        lifecycle: &CodegenHelperMapping,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let Some(CodegenSymbolKey::Runtime(runtime)) = creation.symbol() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let layout = self.run_result_layout(result, variants)?;
        let task = int_value(task).ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let boolean = self
            .types
            .context()
            .i8_type()
            .const_int(u64::from(request_cancellation), false);

        let callback_type = self
            .types
            .context()
            .ptr_type(inkwell::AddressSpace::default());

        let cancellation = self
            .helper_address(cancellation)?
            .unwrap_or_else(|| callback_type.const_null().into());

        let lifecycle = self
            .helper_address(lifecycle)?
            .unwrap_or_else(|| callback_type.const_null().into());

        let arguments = [
            task.into(),
            boolean.into(),
            layout.into(),
            cancellation.into(),
            lifecycle.into(),
        ];

        self.invoke_native_runtime(*runtime, &arguments)?
            .ok_or(CodegenFailure::GeneratedModuleInvariant)
    }

    pub(super) fn resolve_native_task(
        &mut self,
        runtime: bray_ir::MirRuntimeReference,
        task: inkwell::values::IntValue<'context>,
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

        let status = self
            .invoke_native_runtime(runtime, &[task.into(), storage.into(), layout.into()])?
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        self.require_runtime_success(status, "task.resolve")?;

        llvm(
            self.builder
                .build_load(result_type, storage, "task.result.value"),
        )
    }

    fn run_result_layout(
        &mut self,
        result: bray_symbols::TypeId,
        selected: MirRunResultVariants,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        let mapping = self
            .type_mapping(result)
            .cloned()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let represented = mapping
            .layout()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let CodegenTypeKind::Union { tag, variants } = mapping.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let variant = |identity| {
            variants
                .iter()
                .find(|variant| variant.variant() == identity)
                .cloned()
                .ok_or(CodegenFailure::GeneratedModuleInvariant)
        };

        let completed = variant(selected.completed())?;
        let panicked = variant(selected.panicked())?;
        let cancelled = variant(selected.cancelled())?;

        let completed_field = completed
            .fields()
            .first()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let panicked_field = panicked
            .fields()
            .first()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let completed_layout = self
            .type_mapping(completed_field.ty())
            .and_then(|mapping| mapping.layout())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let tag = (*tag).ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let tag_size = self
            .type_mapping(tag)
            .and_then(|mapping| mapping.layout())
            .map(|layout| layout.size())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let layout_type =
            crate::native::run_result_layout_type(self.types.context(), self.request.target());

        let usize =
            crate::native::pointer_integer_type(self.types.context(), self.request.target());

        let usize_constant = |value| usize.const_int(value, false).into();

        let tag_constant = |value: &bray_symbols::IntegerConstant| {
            value
                .to_u64()
                .map(|value| {
                    self.types
                        .context()
                        .i64_type()
                        .const_int(value, false)
                        .into()
                })
                .ok_or(CodegenFailure::GeneratedModuleInvariant)
        };

        let value = layout_type.const_named_struct(&[
            usize_constant(represented.size()),
            usize_constant(represented.alignment().get()),
            usize_constant(tag_size),
            tag_constant(
                completed
                    .tag()
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?,
            )?,
            usize_constant(completed_field.offset_bytes()),
            usize_constant(completed_layout.size()),
            usize_constant(completed_layout.alignment().get()),
            tag_constant(
                panicked
                    .tag()
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?,
            )?,
            usize_constant(panicked_field.offset_bytes()),
            tag_constant(
                cancelled
                    .tag()
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?,
            )?,
        ]);

        let storage = self.allocate_temporary(layout_type, "task.result.layout")?;

        llvm(self.builder.build_store(storage, value))?;

        Ok(storage)
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
