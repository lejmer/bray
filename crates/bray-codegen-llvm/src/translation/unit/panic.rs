use bray_codegen::{CodegenCallableSignature, CodegenFailure, CodegenResultMapping};
use bray_ir::{MirEdge, MirOperand};
use inkwell::values::{FunctionValue, PointerValue};

use super::core::UnitTranslator;
use super::support::{
    llvm, native_run_outcome, native_run_outcome_value, native_run_state_is, pointer_value,
};

use bray_runtime_abi::NativeRunState;

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
        let report = self.operand(report)?;

        self.clear_moved_places()?;

        if self.signature.has_panic_report_context() {
            let context = self
                .panic_report_context
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            let outcome = native_run_outcome_value(
                self.types.context(),
                &self.builder,
                self.request.target(),
                NativeRunState::PANICKED,
                report,
            )?;

            llvm(self.builder.build_store(context, outcome))?;

            self.return_propagated_outcome()?;
        } else {
            self.invoke_runtime(runtime, &[report.into()])?;

            llvm(self.builder.build_unreachable())?;
        }

        Ok(())
    }

    pub(super) fn translate_cancellation_propagation(
        &mut self,
        runtime: bray_ir::MirRuntimeReference,
    ) -> Result<(), CodegenFailure> {
        self.clear_moved_places()?;

        if self.signature.has_panic_report_context() {
            let context = self
                .panic_report_context
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            let ty =
                crate::native::pointer_integer_type(self.types.context(), self.request.target());

            let outcome = native_run_outcome_value(
                self.types.context(),
                &self.builder,
                self.request.target(),
                NativeRunState::CANCELLED,
                ty.const_zero().into(),
            )?;

            llvm(self.builder.build_store(context, outcome))?;

            self.return_propagated_outcome()?;
        } else {
            self.invoke_runtime(runtime, &[])?;

            llvm(self.builder.build_unreachable())?;
        }

        Ok(())
    }

    pub(super) fn translate_call_panic(
        &mut self,
        block: bray_ir::MirBlockId,
        completed: &MirEdge,
        panicked: bray_ir::MirCallPanicEdge,
        cancelled_edge: &MirEdge,
    ) -> Result<(), CodegenFailure> {
        let admission = self
            .unit
            .block(block)
            .and_then(|block| block.operations().last())
            .copied()
            .filter(|id| {
                self.unit.operation(*id).is_some_and(|operation| {
                    matches!(
                        operation.kind(),
                        bray_ir::MirOperationKind::AdmitOutgoing { .. }
                    )
                })
            });

        if let Some(operation) = admission
            && self.outgoing_capacity(operation)? == 0
        {
            return self.translate_goto(completed);
        }

        let context = self
            .pending_call_panic_report_context
            .take()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let ty = crate::native::run_outcome_type(self.types.context(), self.request.target());
        let outcome = llvm(self.builder.build_load(ty, context, "call.outcome"))?;

        let (state, _) = native_run_outcome(&self.builder, outcome)?;

        let report = super::support::extract_value(&self.builder, outcome, 2)?;

        let report = if let Some(operation) = admission {
            // Admission supplies the exact cause; this source operation supplies its occurrence.
            let source = self.native_source_anchor(operation)?;
            let mut report = report;

            for (index, source_index) in (0..5).enumerate() {
                let field = super::support::extract_value(&self.builder, source, source_index)?;
                report = super::support::insert_value(&self.builder, report, field, index)?;
            }

            report
        } else {
            report
        };

        let cancelled = native_run_state_is(
            &self.builder,
            state,
            NativeRunState::CANCELLED,
            "call.cancelled",
        )?;

        let function = self
            .builder
            .get_insert_block()
            .and_then(inkwell::basic_block::BasicBlock::get_parent)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let propagate_cancellation = self
            .types
            .context()
            .append_basic_block(function, "call.propagate_cancellation");

        let inspect_panic = self
            .types
            .context()
            .append_basic_block(function, "call.inspect_panic");

        llvm(self.builder.build_conditional_branch(
            cancelled,
            propagate_cancellation,
            inspect_panic,
        ))?;

        self.builder.position_at_end(propagate_cancellation);
        llvm(self.builder.build_store(context, ty.const_zero()))?;

        let (_, pending_moves) = self.take_control_source()?;

        let cancellation_route =
            self.route_edge(cancelled_edge, "call.cancelled", &pending_moves)?;

        self.builder.position_at_end(propagate_cancellation);
        llvm(self.builder.build_unconditional_branch(cancellation_route))?;

        self.builder.position_at_end(inspect_panic);

        let pending = native_run_state_is(
            &self.builder,
            state,
            NativeRunState::PANICKED,
            "call.panicked",
        )?;

        let (source, _) = self.take_control_source()?;

        let completed_route = self.route_edge(completed, "call.completed", &pending_moves)?;

        let panicked_route =
            self.route_call_panic(panicked, report.into(), "call.panicked", &pending_moves)?;

        // Successful calls leave the empty destination intact. Clear transferred ownership
        // only on the failure routes, before another cleanup call can reuse this destination.
        let first = panicked_route
            .get_first_instruction()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        self.builder.position_before(&first);
        llvm(self.builder.build_store(context, ty.const_zero()))?;

        self.builder.position_at_end(source);

        llvm(
            self.builder
                .build_conditional_branch(pending, panicked_route, completed_route),
        )?;

        Ok(())
    }

    fn return_propagated_outcome(&mut self) -> Result<(), CodegenFailure> {
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
