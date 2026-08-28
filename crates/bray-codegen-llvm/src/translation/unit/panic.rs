use bray_codegen::{CodegenCallableSignature, CodegenFailure, CodegenResultMapping};
use bray_ir::{MirEdge, MirOperand};
use inkwell::IntPredicate;
use inkwell::values::{BasicValueEnum, FunctionValue, PointerValue};

use super::core::UnitTranslator;
use super::support::{int_value, llvm, pointer_value};

pub(super) fn incoming_panic_report_context<'context>(
    function: FunctionValue<'context>,
    signature: &CodegenCallableSignature,
) -> Result<Option<PointerValue<'context>>, CodegenFailure> {
    if !signature.has_panic_report_context() {
        return Ok(None);
    }

    function
        .get_last_param()
        .and_then(pointer_value)
        .map(Some)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)
}

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn translate_panic_propagation(
        &mut self,
        report: &MirOperand,
        runtime: bray_ir::MirRuntimeReference,
    ) -> Result<(), CodegenFailure> {
        let report = match self.operand(report)? {
            BasicValueEnum::PointerValue(value) => llvm(self.builder.build_ptr_to_int(
                value,
                crate::native::pointer_integer_type(self.types.context(), self.request.target()),
                "panic.report",
            ))?,
            BasicValueEnum::IntValue(value) => value,
            _ => return Err(CodegenFailure::GeneratedModuleInvariant),
        };

        self.clear_moved_places()?;

        if self.signature.has_panic_report_context() {
            let context = self
                .panic_report_context
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            llvm(self.builder.build_store(context, report))?;
            self.return_propagated_panic()?;
        } else {
            self.invoke_runtime(runtime, &[report.into()])?;
            llvm(self.builder.build_unreachable())?;
        }

        Ok(())
    }

    pub(super) fn translate_call_panic(
        &mut self,
        completed: &MirEdge,
        panicked: bray_ir::MirCallPanicEdge,
    ) -> Result<(), CodegenFailure> {
        let context = self
            .pending_call_panic_report_context
            .take()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let ty = crate::native::pointer_integer_type(self.types.context(), self.request.target());

        let report = llvm(
            self.builder
                .build_load(ty, context, "call.panic.report"),
        )?;

        llvm(self.builder.build_store(context, ty.const_zero()))?;

        let report = int_value(report).ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let pending = llvm(self.builder.build_int_compare(
            IntPredicate::NE,
            report,
            ty.const_zero(),
            "call.panic.pending",
        ))?;

        let (source, pending_moves) = self.take_control_source()?;

        let completed_route =
            self.route_edge(completed, "call.completed", &pending_moves)?;

        let panicked_route = self.route_call_panic(
            panicked,
            report.into(),
            "call.panicked",
            &pending_moves,
        )?;

        self.builder.position_at_end(source);

        llvm(
            self.builder
                .build_conditional_branch(pending, panicked_route, completed_route),
        )?;

        Ok(())
    }

    fn return_propagated_panic(&mut self) -> Result<(), CodegenFailure> {
        match self.signature.result() {
            CodegenResultMapping::Void => {
                llvm(self.builder.build_return(None))?;
            }
            CodegenResultMapping::Direct { ty, .. } => {
                let value = self.types.map(*ty)?.const_zero();

                llvm(self.builder.build_return(Some(&value)))?;
            }
            CodegenResultMapping::Indirect { pointee, .. } => {
                let destination = self
                    .function
                    .get_first_param()
                    .and_then(pointer_value)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                llvm(
                    self.builder
                        .build_store(destination, self.types.map(*pointee)?.const_zero()),
                )?;

                llvm(self.builder.build_return(None))?;
            }
        }

        Ok(())
    }
}
