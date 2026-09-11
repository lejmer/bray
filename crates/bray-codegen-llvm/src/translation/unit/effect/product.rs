use super::super::core::UnitTranslator;
use super::super::support::{extract_value, int_value, llvm, pointer_value};
use bray_codegen::CodegenFailure;
use bray_ir::MirRuntimeReference;
use bray_runtime_abi::NativeProductHostOperation;
use bray_runtime_interface::RuntimeAbiRole;
use inkwell::IntPredicate;
use inkwell::values::{BasicValueEnum, IntValue, StructValue};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn host_runtime_configuration(
        &self,
    ) -> Result<StructValue<'context>, CodegenFailure> {
        let bray_ir::MirUnitKind::ExecutableHost(host) = self.unit.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let capacity = host
            .capacity_limits()
            .tasks()
            .map_or(u64::MAX, |capacity| u64::from(capacity.get()));

        let context = self.types.context();
        let usize = crate::native::pointer_integer_type(context, self.request.target());

        Ok(
            crate::native::runtime_configuration_type(context, self.request.target())
                .const_named_struct(&[
                    usize.const_int(capacity, false).into(),
                    usize.const_all_ones().into(),
                ]),
        )
    }

    pub(super) fn begin_product_execution(
        &mut self,
        startup: Option<MirRuntimeReference>,
        control: MirRuntimeReference,
    ) -> Result<(), CodegenFailure> {
        if let Some(startup) = startup {
            let configuration = self.host_runtime_configuration()?;

            let status = self
                .invoke_native_runtime(startup, &[configuration.into()])?
                .and_then(int_value)
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            self.require_product_admission(status)?;
        }

        let context = self.types.context();
        let pointer = context.ptr_type(inkwell::AddressSpace::default());
        let usize = crate::native::pointer_integer_type(context, self.request.target());

        let execution = if startup.is_some() {
            self.invoke_native_runtime(
                MirRuntimeReference::new(RuntimeAbiRole::ExecutionServices, control.abi_version()),
                &[],
            )?
            .and_then(pointer_value)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?
        } else {
            pointer.const_null()
        };

        let binding_type = context.struct_type(
            &[usize.into(), pointer.into(), usize.into(), pointer.into()],
            false,
        );

        let binding = llvm(self.builder.build_alloca(binding_type, "product.capacity"))?;
        llvm(self.builder.build_store(binding, binding_type.const_zero()))?;

        let status = self
            .invoke_native_runtime(
                MirRuntimeReference::new(
                    RuntimeAbiRole::ProductServicesFormation,
                    control.abi_version(),
                ),
                &[binding.into(), execution.into()],
            )?
            .and_then(int_value)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        self.require_product_admission(status)?;

        let observation = self.invoke_product_control(
            control,
            NativeProductHostOperation::ACQUIRE_ENTRY,
            Some(binding),
        )?;

        self.invoke_native_runtime(
            MirRuntimeReference::new(
                RuntimeAbiRole::ProductServicesRelease,
                control.abi_version(),
            ),
            &[binding.into()],
        )?;

        let status = int_value(extract_value(&self.builder, observation, 0)?)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        self.require_product_admission(status)
    }

    fn require_product_admission(
        &mut self,
        status: IntValue<'context>,
    ) -> Result<(), CodegenFailure> {
        let context = self.types.context();

        let function = self
            .builder
            .get_insert_block()
            .and_then(|block| block.get_parent())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let admitted = context.append_basic_block(function, "product.admitted");
        let rejected = context.append_basic_block(function, "product.rejected");

        let success = llvm(self.builder.build_int_compare(
            IntPredicate::EQ,
            status,
            status.get_type().const_zero(),
            "product.admission.success",
        ))?;

        llvm(
            self.builder
                .build_conditional_branch(success, admitted, rejected),
        )?;

        self.builder.position_at_end(rejected);

        self.invoke_native_runtime(
            MirRuntimeReference::new(
                RuntimeAbiRole::StructuredShutdown,
                self.unit.target().runtime_abi(),
            ),
            &[],
        )?;

        let failure = llvm(self.builder.build_int_z_extend(
            status,
            context.i64_type(),
            "product.admission.failure",
        ))?;

        self.translate_compiler_shutdown(failure)?;
        llvm(self.builder.build_unreachable())?;
        self.builder.position_at_end(admitted);

        Ok(())
    }

    pub(super) fn product_descriptor(
        &self,
    ) -> Result<inkwell::values::PointerValue<'context>, CodegenFailure> {
        let host = self
            .request
            .mappings()
            .product_host()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        self.module
            .get_global(host.descriptor_symbol().as_str())
            .map(|descriptor| descriptor.as_pointer_value())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)
    }

    fn invoke_product_control(
        &mut self,
        runtime: MirRuntimeReference,
        operation: NativeProductHostOperation,
        capacity: Option<inkwell::values::PointerValue<'context>>,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let descriptor = self.product_descriptor()?;

        let operation = self
            .types
            .context()
            .i32_type()
            .const_int(u64::from(operation.code()), false);

        let context = self.types.context();

        let function = crate::native::declare_runtime_function(
            self.module,
            context,
            self.request.target(),
            runtime.role(),
        )?;

        let key = bray_codegen::CodegenSymbolKey::Runtime(runtime);

        crate::native::invoke_function(
            context,
            &self.builder,
            self.request.target(),
            &key,
            function,
            &[
                descriptor.into(),
                operation.into(),
                capacity
                    .unwrap_or_else(|| {
                        context
                            .ptr_type(inkwell::AddressSpace::default())
                            .const_null()
                    })
                    .into(),
            ],
            "product.control",
        )?
        .ok_or(CodegenFailure::GeneratedModuleInvariant)
    }

    pub(super) fn finish_product_statics(&mut self) -> Result<(), CodegenFailure> {
        if self.request.mappings().product_host().is_none() {
            return Ok(());
        }

        let context = self.types.context();

        let runtime = MirRuntimeReference::new(
            RuntimeAbiRole::ProductHostControl,
            self.unit.target().runtime_abi(),
        );

        let release_failed = if self.unit.operations().iter().any(|operation| {
            matches!(
                operation.kind(),
                bray_ir::MirOperationKind::Host(bray_ir::MirHostOperation::BeginExecution { .. })
            )
        }) {
            let release = self.invoke_product_control(
                runtime,
                NativeProductHostOperation::RELEASE_ENTRY,
                None,
            )?;

            let status = int_value(extract_value(&self.builder, release, 0)?)
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            // Another owner may have requested closure while the executable entry was held.
            llvm(self.builder.build_int_compare(
                IntPredicate::UGT,
                status,
                status.get_type().const_int(
                    u64::from(bray_runtime_abi::NativeProductHostStatus::CLOSED.code()),
                    false,
                ),
                "product.release.invalid",
            ))?
        } else {
            context.bool_type().const_zero()
        };

        let observation =
            self.invoke_product_control(runtime, NativeProductHostOperation::FINISH_ROOT, None)?;

        let status = int_value(super::super::support::extract_value(
            &self.builder,
            observation,
            0,
        )?)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let state = int_value(super::super::support::extract_value(
            &self.builder,
            observation,
            1,
        )?)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

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

        let failed = llvm(
            self.builder
                .build_or(failed, release_failed, "product.release.failed"),
        )?;

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
}
