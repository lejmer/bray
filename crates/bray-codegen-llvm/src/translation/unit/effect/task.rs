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
            .expect("checked MIR effect translation requires an established mapping or value");

        let status = extract_value(&self.builder, allocation, 0)?;

        self.require_runtime_success(status, "task.allocation")?;

        Ok(int_value(extract_value(&self.builder, allocation, 1)?)
            .expect("task allocation runtime must return an integer task handle"))
    }

    pub(super) fn start_native_task(
        &mut self,
        runtime: bray_ir::MirRuntimeReference,
        task: inkwell::values::IntValue<'context>,
        frame: BasicValueEnum<'context>,
    ) -> Result<(), CodegenFailure> {
        let status = self
            .invoke_native_runtime(runtime, &[task.into(), frame])?
            .expect("checked MIR effect translation requires an established mapping or value");

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
            panic!("checked MIR effect translation violated an established compiler contract");
        };

        let layout = self.run_result_layout(result, variants)?;

        let task = int_value(task)
            .expect("checked MIR effect translation requires an established mapping or value");

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

        Ok(self
            .invoke_native_runtime(*runtime, &arguments)?
            .expect("task-observation creation runtime must return a frame"))
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
            .expect("checked MIR effect translation requires an established mapping or value");

        let storage = self.aligned_alloca(result, mapping.alignment().get(), "task.result")?;
        let layout = self.run_result_layout(result, variants)?;

        let status = self
            .invoke_native_runtime(runtime, &[task.into(), storage.into(), layout.into()])?
            .expect("checked MIR effect translation requires an established mapping or value");

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
            .expect("checked MIR effect translation requires an established mapping or value");

        let represented = mapping
            .layout()
            .expect("checked MIR effect translation requires an established mapping or value");

        let CodegenTypeKind::Union { tag, .. } = mapping.kind() else {
            panic!("checked MIR effect translation violated an established compiler contract");
        };

        let variant = |identity| {
            mapping
                .kind()
                .union_variant(identity)
                .cloned()
                .expect("run-result variants must be represented in the mapped union")
        };

        let completed = variant(selected.completed());
        let panicked = variant(selected.panicked());
        let cancelled = variant(selected.cancelled());

        let completed_field = completed
            .fields()
            .first()
            .expect("checked MIR effect translation requires an established mapping or value");

        let panicked_field = panicked
            .fields()
            .first()
            .expect("checked MIR effect translation requires an established mapping or value");

        let completed_layout = self
            .type_mapping(completed_field.ty())
            .and_then(|mapping| mapping.layout())
            .expect("checked MIR effect translation requires an established mapping or value");

        let tag = (*tag)
            .expect("checked MIR effect translation requires an established mapping or value");

        let tag_size = self
            .type_mapping(tag)
            .and_then(|mapping| mapping.layout())
            .map(|layout| layout.size())
            .expect("checked MIR effect translation requires an established mapping or value");

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
                .expect("run-result discriminants must fit the native runtime layout")
        };

        let value =
            layout_type.const_named_struct(&[
                usize_constant(represented.size()),
                usize_constant(represented.alignment().get()),
                usize_constant(tag_size),
                tag_constant(completed.tag().expect(
                    "checked MIR effect translation requires an established mapping or value",
                )),
                usize_constant(completed_field.offset_bytes()),
                usize_constant(completed_layout.size()),
                usize_constant(completed_layout.alignment().get()),
                tag_constant(panicked.tag().expect(
                    "checked MIR effect translation requires an established mapping or value",
                )),
                usize_constant(panicked_field.offset_bytes()),
                tag_constant(cancelled.tag().expect(
                    "checked MIR effect translation requires an established mapping or value",
                )),
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
        let status = int_value(status)
            .expect("checked MIR effect translation requires an established mapping or value");

        let function = self
            .builder
            .get_insert_block()
            .and_then(|block| block.get_parent())
            .expect("checked MIR effect translation requires an established mapping or value");

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
