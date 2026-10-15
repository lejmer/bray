use super::core::UnitTranslator;
use bray_codegen::{CodegenCleanupRequirement, CodegenFailure};
use bray_ir::{MirOperationId, MirRuntimeReference};
use bray_runtime_interface::RuntimeAbiRole;
use inkwell::values::{BasicValueEnum, PointerValue};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    fn outgoing_requirements(
        &self,
        operation: MirOperationId,
    ) -> &'request [CodegenCleanupRequirement] {
        self.request
            .mappings()
            .operation(self.instance.key(), operation)
            .and_then(bray_codegen::CodegenOperationMapping::outgoing_requirements)
            .expect("checked MIR translation requires outgoing requirements")
    }

    pub(super) fn admit_root_frame(
        &mut self,
        root: &bray_ir::BoundUnitKey,
        outcome: PointerValue<'context>,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        let descriptor = crate::mapping::cleanup::storage_descriptor(
            self.module,
            self.request.mappings(),
            self.request.unit().executable_root_instance(root),
        );

        let count = self.pointer_integer_type().const_int(1, false);

        let bundle = crate::mapping::cleanup::bundle(
            &self.builder,
            descriptor.as_pointer_value(),
            count,
            self.types,
        )?;

        self.invoke_native_runtime(
            MirRuntimeReference::new(
                RuntimeAbiRole::OutgoingAdmission,
                self.unit.target().runtime_abi(),
            ),
            &[bundle.into(), outcome.into()],
        )?;

        Ok(bundle)
    }

    pub(super) fn outgoing_capacity(&self, operation: MirOperationId) -> u32 {
        u32::try_from(self.outgoing_requirements(operation).len())
            .expect("outgoing requirement count must fit native ABI")
    }

    fn outgoing_descriptors(
        &mut self,
        operation: MirOperationId,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        crate::mapping::cleanup::descriptors(
            self.module,
            &self.builder,
            self.outgoing_requirements(operation),
            self.types,
        )
    }

    pub(super) fn translate_outgoing(
        &mut self,
        operation: MirOperationId,
        runtime: MirRuntimeReference,
    ) -> Result<(), CodegenFailure> {
        let capacity = self.outgoing_capacity(operation);

        if capacity == 0 {
            return Ok(());
        }

        let count = self
            .pointer_integer_type()
            .const_int(u64::from(capacity), false);

        let descriptors = self.outgoing_descriptors(operation)?;

        let bundle =
            crate::mapping::cleanup::bundle(&self.builder, descriptors, count, self.types)?;

        if runtime.role() == RuntimeAbiRole::OutgoingAdmission {
            let context = self.checked_call_panic_report_context()?;

            self.set_pending_call_context(context)?;

            self.invoke_runtime(runtime, &[bundle.into(), context.into()])?;
        } else {
            self.invoke_runtime(runtime, &[bundle.into()])?;
        }

        Ok(())
    }

    pub(super) fn activate_outgoing_call(
        &mut self,
        operation: MirOperationId,
    ) -> Result<Option<(BasicValueEnum<'context>, PointerValue<'context>)>, CodegenFailure> {
        if self.outgoing_capacity(operation) == 0 {
            return Ok(None);
        }

        let descriptors = self.outgoing_descriptors(operation)?;
        let context = self.types.context();
        let lease_type = crate::mapping::cleanup::lease_type(context, self.types.target());
        let lease = self.allocate_temporary(lease_type, "cleanup.lease")?;

        self.builder
            .build_store(lease, lease_type.const_zero())
            .map_err(CodegenFailure::backend_library)?;

        Ok(self
            .invoke_runtime(
                MirRuntimeReference::new(
                    RuntimeAbiRole::OutgoingActivation,
                    self.unit.target().runtime_abi(),
                ),
                &[descriptors.into(), lease.into()],
            )?
            .map(|record| Some((record, lease)))
            .expect("outgoing activation runtime must return its record"))
    }

    pub(super) fn retire_outgoing_call(
        &mut self,
        (record, lease): (BasicValueEnum<'context>, PointerValue<'context>),
    ) -> Result<(), CodegenFailure> {
        let context = self
            .pending_call_panic_report_context
            .expect("checked MIR translation requires an established mapping or value");

        self.invoke_runtime(
            MirRuntimeReference::new(
                RuntimeAbiRole::OutgoingRetirement,
                self.unit.target().runtime_abi(),
            ),
            &[record, lease.into(), context.into()],
        )?;

        Ok(())
    }
}
