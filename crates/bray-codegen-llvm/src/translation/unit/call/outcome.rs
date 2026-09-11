use super::super::core::UnitTranslator;
use super::super::support::llvm;
use bray_codegen::{CodegenCallableSignature, CodegenFailure};
use inkwell::values::{BasicValue, PointerValue};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn boundary_panic_report_context(
        &mut self,
        signature: &CodegenCallableSignature,
    ) -> Result<Option<PointerValue<'context>>, CodegenFailure> {
        if !signature.has_panic_report_context() {
            return Ok(None);
        }

        self.allocate_panic_report_context().map(Some)
    }

    pub(super) fn checked_panic_report_context(
        &mut self,
        signature: &CodegenCallableSignature,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        if !signature.has_panic_report_context() {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        match self.panic_report_context {
            Some(context) => Ok(context),
            None => self.allocate_panic_report_context(),
        }
    }

    pub(in crate::translation::unit) fn allocate_panic_report_context(
        &mut self,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        let ty = crate::native::pointer_integer_type(self.types.context(), self.request.target());
        let context = self.allocate_temporary(ty, "call.panic.report.context")?;

        llvm(self.builder.build_store(context, ty.const_zero()))?;

        Ok(context)
    }

    pub(super) fn propagate_boundary_call_panic(
        &mut self,
        context: PointerValue<'context>,
    ) -> Result<(), CodegenFailure> {
        let ty = crate::native::pointer_integer_type(self.types.context(), self.request.target());

        let (report, continued, cancelled) = crate::translation::branch_on_pending_outcome(
            self.types.context(),
            &self.builder,
            ty,
            context,
        )?;

        let runtime = bray_ir::MirRuntimeReference::new(
            bray_runtime_interface::RuntimeAbiRole::PanicPropagation,
            self.unit.target().runtime_abi(),
        );

        self.invoke_runtime(runtime, &[report.into()])?;
        llvm(self.builder.build_unreachable())?;

        self.builder.position_at_end(cancelled);

        self.invoke_runtime(
            bray_ir::MirRuntimeReference::new(
                bray_runtime_interface::RuntimeAbiRole::CurrentRunCancellationPropagation,
                self.unit.target().runtime_abi(),
            ),
            &[],
        )?;

        llvm(self.builder.build_unreachable())?;

        self.builder.position_at_end(continued);

        Ok(())
    }

    pub(in crate::translation::unit) fn aligned_alloca(
        &mut self,
        pointee: bray_symbols::TypeId,
        alignment: u64,
        name: &str,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        let pointee = self.types.map(pointee)?;
        let storage = self.allocate_temporary(pointee, name)?;

        let alignment = crate::conversion::target_value(alignment, "allocation_alignment")?;

        storage
            .as_instruction_value()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?
            .set_alignment(alignment)
            .map_err(CodegenFailure::unsupported_target_report)?;

        Ok(storage)
    }
}
