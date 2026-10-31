use super::super::core::UnitTranslator;
use super::super::support::{extract_value, int_value, llvm, next_helper};
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

        // Inactive disposal obeys the same checked-call outcome contract as its
        // enclosing source or generated helper, including an incoming destination.
        let outcome = self.checked_call_panic_report_context()?;

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

    pub(super) fn start_native_task(
        &mut self,
        allocation: bray_ir::MirRuntimeReference,
        start: bray_ir::MirRuntimeReference,
        frame: BasicValueEnum<'context>,
    ) -> Result<inkwell::values::IntValue<'context>, CodegenFailure> {
        let allocated = self.invoke_native_runtime(allocation, &[])?
            .expect("task allocation returns its admitted identity or rejection");

        let status = extract_value(&self.builder, allocated, 0)?.into_int_value();
        let task = extract_value(&self.builder, allocated, 1)?.into_int_value();
        let context = self.checked_call_panic_report_context()?;
        let llvm_context = self.types.context();
        let starting = llvm_context.append_basic_block(self.function, "task.starting");
        let rejected = llvm_context.append_basic_block(self.function, "task.rejected");
        let failed = llvm_context.append_basic_block(self.function, "task.failed");
        let completed = llvm_context.append_basic_block(self.function, "task.admission.completed");

        let accepted = llvm(self.builder.build_int_compare(
            inkwell::IntPredicate::EQ, status, status.get_type().const_zero(), "task.allocated",
        ))?;

        llvm(self.builder.build_conditional_branch(accepted, starting, failed))?;
        self.builder.position_at_end(starting);

        let status = self.invoke_native_runtime(start, &[task.into(), frame])?
            .expect("task start returns its ownership transition status").into_int_value();

        let accepted = llvm(self.builder.build_int_compare(
            inkwell::IntPredicate::EQ, status, status.get_type().const_zero(), "task.started",
        ))?;

        llvm(self.builder.build_conditional_branch(accepted, completed, rejected))?;
        self.builder.position_at_end(rejected);

        let destruction = bray_ir::MirRuntimeReference::new(
            bray_runtime_interface::RuntimeAbiRole::TaskDestruction, allocation.abi_version(),
        );

        let status = self.invoke_native_runtime(destruction, &[task.into()])?
            .expect("rejected start releases its unstarted task control");

        crate::translation::require_runtime_success(
            self.module, llvm_context, &self.builder, status, "task.rejection.retirement",
        )?;

        llvm(self.builder.build_unconditional_branch(failed))?;
        self.builder.position_at_end(failed);

        self.invoke_native_runtime(bray_ir::MirRuntimeReference::new(
            bray_runtime_interface::RuntimeAbiRole::TaskAdmissionFailure, allocation.abi_version(),
        ), &[context.into()])?;

        llvm(self.builder.build_unconditional_branch(completed))?;
        self.builder.position_at_end(completed);
        self.set_pending_call_context(context)?;

        Ok(task)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "task observation creation retains its typed result and two cleanup phases"
    )]
    pub(super) fn create_task_observation_frame(
        &mut self,
        creation: &CodegenHelperMapping,
        task: BasicValueEnum<'context>,
        future_type: bray_symbols::TypeId,
        request_cancellation: bool,
        result: bray_symbols::TypeId,
        variants: MirRunResultVariants,
        cancellation: &CodegenHelperMapping,
        quiesce: &CodegenHelperMapping,
        lifecycle: &CodegenHelperMapping,
        destruction: &CodegenHelperMapping,
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
            [cancellation, destruction].into_iter().flat_map(|helper| {
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

        let destruction = crate::mapping::frame::cleanup_callback(
            self.module, self.request.mappings(), destruction, self.types,
        )?.unwrap_or_else(|| callback_type.const_null());

        let (quiesce, quiesce_storage) = crate::mapping::frame::lifecycle_factory_callback(
            self.module, self.request.mappings(), quiesce, self.types,
        )?.unwrap_or_else(|| (callback_type.const_null(), callback_type.const_null()));

        let (lifecycle, lifecycle_storage) = crate::mapping::frame::lifecycle_factory_callback(
            self.module, self.request.mappings(), lifecycle, self.types,
        )?.unwrap_or_else(|| (callback_type.const_null(), callback_type.const_null()));

        let context = self.types.context();
        let state_type = context.struct_type(&[context.i32_type().into(); 2], false);

        let glue_type = context.struct_type(&[
            callback_type.into(), callback_type.into(), callback_type.into(), callback_type.into(),
            callback_type.into(), callback_type.into(), state_type.into(), self.pointer_integer_type().into(),
        ], false);

        let glue = self.allocate_temporary(glue_type, "observation.glue")?;

        let affinity = llvm(self.builder.build_select(
            llvm(self.builder.build_int_compare(
                inkwell::IntPredicate::NE,
                llvm(self.builder.build_and(requirements, context.i32_type().const_int(4, false), "observation.main"))?,
                context.i32_type().const_zero(), "observation.main.required",
            ))?,
            context.i32_type().const_int(2, false), context.i32_type().const_zero(), "observation.affinity",
        ))?;

        let state = llvm(self.builder.build_insert_value(state_type.const_zero(), affinity, 0, "observation.affinity.state"))?;
        let state = llvm(self.builder.build_insert_value(state, requirements, 1, "observation.requirements"))?;
        let provider = crate::mapping::provider::key(self.module, &self.builder, self.types)?;
        let mut value = glue_type.const_zero();

        let fields: [BasicValueEnum<'context>; 8] = [
            quiesce.into(), lifecycle.into(), cancellation.into(), destruction.into(),
            quiesce_storage.into(), lifecycle_storage.into(), state.into_struct_value().into(), provider.into(),
        ];

        for (index, field) in fields.into_iter().enumerate() {
            value = llvm(self.builder.build_insert_value(value, field, u32::try_from(index).expect("glue fields fit u32"), "observation.glue.field"))?.into_struct_value();
        }

        llvm(self.builder.build_store(glue, value))?;

        let outcome = self.checked_call_panic_report_context()?;
        let arguments = [task.into(), boolean.into(), layout.into(), glue.into(), outcome.into()];

        let token = self
            .invoke_native_runtime(*runtime, &arguments)?
            .expect("task-observation creation runtime must return its owned inactive token");

        self.set_pending_call_context(outcome)?;

        let lease = extract_value(&self.builder, token, 0)?;
        let address = extract_value(&self.builder, lease, 4)?.into_int_value();

        let context = llvm(self.builder.build_int_to_ptr(
            address,
            callback_type,
            "observation.context",
        ))?;

        let descriptor = extract_value(&self.builder, token, 1)?;

        let future = self.types.map(future_type)?.into_struct_type().const_zero();

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

        self.load_value(storage, result, "task.result.value")
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

        crate::native::activation::inactive_token(
            self.types.context(),
            self.request.target(),
            &self.builder,
            context,
            descriptor,
        )
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
                let future_type = self.operation_result_type(
                    self.unit.operation(operation_id)
                        .expect("task-observation creation retains its MIR operation"),
                );

                let cancellation = next_helper(
                    &mut helpers,
                    &MirHelperReference::Cleanup {
                        phase: bray_ir::MirCleanupPhase::TaskCancellation,
                        ty: result.completion_type(),
                    },
                );

                let quiesce = next_helper(
                    &mut helpers,
                    &MirHelperReference::LifecycleFrame {
                        role: bray_ir::MirGeneratedLifecycleRole::Quiesce,
                        ty: result.completion_type(),
                    },
                );

                let lifecycle = next_helper(
                    &mut helpers,
                    &MirHelperReference::LifecycleFrame {
                        role: bray_ir::MirGeneratedLifecycleRole::Cleanup(bray_ir::MirCleanupPhase::LifecycleResolution),
                        ty: result.completion_type(),
                    },
                );

                let destruction = next_helper(
                    &mut helpers,
                    &MirHelperReference::Destroy(result.completion_type(), bray_ir::MirDestructionDisposition::Abandoned),
                );

                let creation = next_helper(&mut helpers, &MirHelperReference::CreateFrame(frame));

                let task = self.operand(task)?;

                let result = self.create_task_observation_frame(
                    creation,
                    task,
                    future_type,
                    *request_cancellation,
                    result.completion_type(),
                    *variants,
                    cancellation,
                    quiesce,
                    lifecycle,
                    destruction,
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
