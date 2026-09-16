use super::core::UnitTranslator;
use bray_codegen::CodegenFailure;
use bray_ir::{MirOperationId, MirRuntimeReference};
use bray_runtime_interface::RuntimeAbiRole;
use inkwell::values::BasicValueEnum;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn outgoing_capacity(&self, operation: MirOperationId) -> u32 {
        self.request
            .mappings()
            .operation(self.instance.key(), operation)
            .and_then(bray_codegen::CodegenOperationMapping::outgoing_capacity)
            .expect("checked MIR translation requires an outgoing-capacity mapping")
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
            .const_int(u64::from(capacity), false)
            .into();

        if runtime.role() == RuntimeAbiRole::OutgoingAdmission {
            let context = self.checked_call_panic_report_context()?;

            self.set_pending_call_context(context)?;

            self.invoke_runtime(runtime, &[count, context.into()])?;
        } else {
            self.invoke_runtime(runtime, &[count])?;
        }

        Ok(())
    }

    pub(super) fn activate_outgoing_call(
        &mut self,
        operation: MirOperationId,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        if self.outgoing_capacity(operation) == 0 {
            return Ok(None);
        }

        Ok(self
            .invoke_runtime(
            MirRuntimeReference::new(
                RuntimeAbiRole::OutgoingActivation,
                self.unit.target().runtime_abi(),
            ),
            &[],
        )?
        .map(Some)
        .expect("outgoing activation runtime must return its record"))
    }

    pub(super) fn retire_outgoing_call(
        &mut self,
        record: BasicValueEnum<'context>,
    ) -> Result<(), CodegenFailure> {
        let context = self
            .pending_call_panic_report_context
            .expect("checked MIR translation requires an established mapping or value");

        self.invoke_runtime(
            MirRuntimeReference::new(
                RuntimeAbiRole::OutgoingRetirement,
                self.unit.target().runtime_abi(),
            ),
            &[record, context.into()],
        )?;

        Ok(())
    }
}
