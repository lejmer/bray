use bray_codegen::CodegenFailure;
use bray_ir::{MirOperand, MirOperationId, MirRuntimeReference};

use super::super::core::UnitTranslator;
use super::super::support::llvm;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn transfer_cleanup_incident(
        &mut self,
        operation: MirOperationId,
        error: &MirOperand,
        runtime: MirRuntimeReference,
    ) -> Result<(), CodegenFailure> {
        let mappings = self.request.mappings();

        let memory = mappings
            .operation(self.instance.key(), operation)
            .and_then(bray_codegen::CodegenOperationMapping::incident)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let error_type = self.operand_type(error)?;
        let error_value = self.operand(error)?;

        let alignment = self
            .type_mapping(error_type)
            .and_then(bray_codegen::CodegenTypeMapping::layout)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?
            .alignment()
            .get();

        let error_pointer = self.aligned_alloca(error_type, alignment, "cleanup.error")?;

        llvm(self.builder.build_store(error_pointer, error_value))?;

        let outcome = self.allocate_panic_report_context()?;

        let finished = self
            .types
            .context()
            .append_basic_block(self.function, "cleanup.incident.finished");

        let incident = crate::mapping::create_owned_cleanup_incident(
            self.module,
            mappings,
            self.instance.key(),
            memory,
            &self.builder,
            error_pointer,
            outcome,
            finished,
            self.types,
        )?;

        let destination = self.allocate_temporary(incident.get_type(), "cleanup.incident")?;

        llvm(self.builder.build_store(destination, incident))?;

        let status = self
            .invoke_native_runtime(runtime, &[destination.into()])?
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        self.require_runtime_success(status, "cleanup.incident.transfer")?;
        llvm(self.builder.build_unconditional_branch(finished))?;
        self.builder.position_at_end(finished);

        self.retain_checked_call_context(outcome)
    }
}
