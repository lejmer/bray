use super::super::core::UnitTranslator;
use super::super::support::{extract_value, insert_value, int_value, llvm, next_helper};
use bray_codegen::{CodegenFailure, CodegenHelperMapping, CodegenSymbolKey, CodegenTypeKind};
use bray_ir::{MirFrameInitializer, MirHelperReference, MirRunResultVariants};
use inkwell::values::{BasicValueEnum, PointerValue};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn compose_awaited_frame(
        &mut self,
        operation: bray_ir::MirOperationId,
        child: bray_ir::MirFrameReference,
        resume_state: bray_ir::MirFrameStateId,
        frame: &bray_ir::MirOperand,
        composition: bray_ir::MirFrameComposition,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let frame = self.native_inactive_frame(frame)?;

        let runtime = self
            .operation_runtime_helper(operation, MirHelperReference::ComposeAwaitedFrame(child))?;

        let state = self
            .types
            .context()
            .i32_type()
            .const_int(u64::from(resume_state.raw()), false);

        let address = match composition.completion() {
            Some(completion) => {
                let destination = self.storage(completion)?;

                llvm(self.builder.build_ptr_to_int(
                    destination,
                    self.pointer_integer_type(),
                    "awaited.destination",
                ))?
            }
            None => self.pointer_integer_type().const_zero(),
        };

        let disposition = match composition {
            bray_ir::MirFrameComposition::Completion(_) => {
                bray_runtime_abi::NativeFrameComposition::Completion
            }
            bray_ir::MirFrameComposition::Abandon => {
                bray_runtime_abi::NativeFrameComposition::Abandon
            }
            bray_ir::MirFrameComposition::Quiesce => {
                bray_runtime_abi::NativeFrameComposition::Quiesce
            }
        };

        let disposition = self
            .types
            .context()
            .i8_type()
            .const_int(u64::from(disposition.code()), false);

        self.invoke_native_runtime(
            runtime,
            &[frame, state.into(), address.into(), disposition.into()],
        )
    }

    pub(super) fn cleanup_inactive_frame(
        &mut self,
        frame: &bray_ir::MirOperand,
        runtime: bray_ir::MirRuntimeReference,
    ) -> Result<(), CodegenFailure> {
        let token = self.native_inactive_frame(frame)?;
        let storage = self.allocate_temporary(token.get_type(), "inactive.token")?;

        llvm(self.builder.build_store(storage, token))?;

        let callback = crate::native::declare_runtime_function(
            self.module,
            self.types.context(),
            self.request.target(),
            bray_runtime_interface::RuntimeAbiRole::ActivationCallback,
        )?
        .as_global_value()
        .as_pointer_value();

        let ready = crate::native::declare_runtime_function(
            self.module,
            self.types.context(),
            self.request.target(),
            bray_runtime_interface::RuntimeAbiRole::ActivationActionReadiness,
        )?
        .as_global_value()
        .as_pointer_value();

        // Native disposal owns a fresh empty destination, independent of an incoming outcome.
        let outcome = self.allocate_panic_report_context()?;

        self.invoke_native_runtime(
            runtime,
            &[
                storage.into(),
                callback.into(),
                ready.into(),
                outcome.into(),
            ],
        )?;

        self.set_pending_call_context(outcome)
    }

    pub(super) fn allocate_native_task(
        &mut self,
        runtime: bray_ir::MirRuntimeReference,
    ) -> Result<inkwell::values::IntValue<'context>, CodegenFailure> {
        let allocation = self
            .invoke_native_runtime(runtime, &[])?
            .expect("checked MIR effect translation requires an established mapping or value");

        let status = extract_value(&self.builder, allocation, 0)?;

        crate::translation::require_runtime_success(
            self.module,
            self.types.context(),
            &self.builder,
            status,
            "task.allocation",
        )?;

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

        crate::translation::require_runtime_success(
            self.module,
            self.types.context(),
            &self.builder,
            status,
            "task.start",
        )?;

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

        let requirements = bray_runtime_interface::ExecutionLaneRequirement::set(
            [cancellation, lifecycle].into_iter().flat_map(|helper| {
                helper
                    .execution_requirements()
                    .expect("observation cleanup has concrete checked execution requirements")
                    .iter()
                    .copied()
            }),
        );

        let requirements = self.types.context().i32_type().const_int(
            u64::from(crate::native::activation::lane_bits(requirements)),
            false,
        );

        let cancellation = crate::mapping::frame::cleanup_callback(
            self.module,
            self.request.mappings(),
            cancellation,
            self.types,
        )?
        .unwrap_or_else(|| callback_type.const_null());

        let lifecycle = crate::mapping::frame::cleanup_callback(
            self.module,
            self.request.mappings(),
            lifecycle,
            self.types,
        )?
        .unwrap_or_else(|| callback_type.const_null());

        let arguments = [
            task.into(),
            boolean.into(),
            layout.into(),
            cancellation.into(),
            lifecycle.into(),
            requirements.into(),
        ];

        let token = self
            .invoke_native_runtime(*runtime, &arguments)?
            .expect("task-observation creation runtime must return its owned inactive token");

        let lease = extract_value(&self.builder, token, 0)?;
        let address = extract_value(&self.builder, lease, 4)?.into_int_value();

        let context = llvm(self.builder.build_int_to_ptr(
            address,
            callback_type,
            "observation.context",
        ))?;

        let descriptor = extract_value(&self.builder, token, 1)?;

        let future = self
            .types
            .context()
            .struct_type(&[callback_type.into(), callback_type.into()], false)
            .const_zero();

        let future =
            llvm(
                self.builder
                    .build_insert_value(future, context, 0, "observation.frame"),
            )?;

        llvm(
            self.builder
                .build_insert_value(future, descriptor, 1, "observation.descriptor"),
        )
        .map(|value| value.into_struct_value().into())
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

        crate::translation::require_runtime_success(
            self.module,
            self.types.context(),
            &self.builder,
            status,
            "task.resolve",
        )?;

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

    pub(super) fn native_inactive_frame(
        &mut self,
        frame: &bray_ir::MirOperand,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let frame_type = self.operand_type(frame);
        let frame = self.operand(frame)?;

        let fields = self
            .type_mapping(frame_type)
            .and_then(|mapping| match mapping.kind() {
                CodegenTypeKind::Aggregate(fields) if fields.len() == 2 => Some(fields.clone()),
                _ => None,
            })
            .expect("checked MIR effect translation requires an established mapping or value");

        let context = extract_value(
            &self.builder,
            frame,
            crate::conversion::resource_limit(
                self.aggregate_value_element(&fields, 0)?,
                "frame_context_field_index",
            )?,
        )?
        .into_pointer_value();

        let descriptor = extract_value(
            &self.builder,
            frame,
            crate::conversion::resource_limit(
                self.aggregate_value_element(&fields, 1)?,
                "frame_descriptor_field_index",
            )?,
        )?
        .into_pointer_value();

        self.inactive_token(context, descriptor)
    }

    pub(in crate::translation::unit) fn inactive_token(
        &self,
        context: inkwell::values::PointerValue<'context>,
        descriptor: inkwell::values::PointerValue<'context>,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let descriptor_type =
            crate::native::activation::descriptor_type(self.types.context(), self.request.target());

        let header_offset = llvm(self.builder.build_struct_gep(
            descriptor_type,
            descriptor,
            1,
            "frame.header.offset.slot",
        ))?;

        let offset = llvm(self.builder.build_load(
            self.pointer_integer_type(),
            header_offset,
            "frame.header.offset",
        ))?
        .into_int_value();

        let header = self.dynamic_offset_pointer(context, offset, 1)?;

        let lease_type =
            crate::mapping::cleanup::lease_type(self.types.context(), self.request.target());

        let lease = llvm(self.builder.build_load(lease_type, header, "frame.lease"))?;

        let token = crate::native::inactive_frame_type(self.types.context(), self.request.target())
            .const_zero()
            .into();

        let token = insert_value(&self.builder, token, lease, 0)?;

        insert_value(&self.builder, token, descriptor.into(), 1)
    }

    pub(super) fn translate_frame_creation(
        &mut self,
        operation_id: bray_ir::MirOperationId,
        frame: bray_ir::MirFrameReference,
        initializer: &MirFrameInitializer,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let helpers = self.operation_helpers(operation_id)?;
        let mut helpers = helpers.iter();

        match initializer {
            MirFrameInitializer::Callable(call) => self.translate_call(operation_id, call),
            MirFrameInitializer::TaskObservation {
                task,
                result,
                variants,
                request_cancellation,
                ..
            } => {
                let cancellation = next_helper(
                    &mut helpers,
                    &MirHelperReference::Cleanup {
                        phase: bray_ir::MirCleanupPhase::TaskCancellation,
                        ty: result.completion_type(),
                    },
                );

                let lifecycle = next_helper(
                    &mut helpers,
                    &MirHelperReference::Cleanup {
                        phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                        ty: result.completion_type(),
                    },
                );

                let creation = next_helper(&mut helpers, &MirHelperReference::CreateFrame(frame));

                let task = self.operand(task)?;

                let result = self.create_task_observation_frame(
                    creation,
                    task,
                    *request_cancellation,
                    result.completion_type(),
                    *variants,
                    cancellation,
                    lifecycle,
                )?;

                if helpers.next().is_some() {
                    panic!(
                        "checked MIR effect translation violated an established compiler contract"
                    );
                }

                Ok(Some(result))
            }
        }
    }
}
