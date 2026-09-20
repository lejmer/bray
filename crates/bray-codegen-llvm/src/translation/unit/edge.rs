use std::collections::BTreeSet;

use bray_codegen::CodegenFailure;
use bray_ir::{MirBlockId, MirCallPanicEdge, MirEdge, MirOperationId, MirPlace, MirUnit};
use inkwell::basic_block::BasicBlock;
use inkwell::values::{BasicValueEnum, PhiValue, PointerValue};

use super::core::UnitTranslator;

pub(super) fn reachable_blocks(unit: &MirUnit) -> BTreeSet<MirBlockId> {
    let mut reachable = BTreeSet::new();
    let mut pending = vec![unit.entry()];

    while let Some(block) = pending.pop() {
        if !reachable.insert(block) {
            continue;
        }

        let Some(block) = unit.block(block) else {
            continue;
        };

        block
            .terminator()
            .kind()
            .for_each_successor(|successor| pending.push(successor));
    }

    reachable
}

pub(super) fn checked_call_operations(unit: &MirUnit) -> BTreeSet<MirOperationId> {
    unit.blocks()
        .iter()
        .filter_map(|block| {
            matches!(
                block.terminator().kind(),
                bray_ir::MirTerminatorKind::CheckCallOutcome { .. }
            )
            .then(|| block.operations().last().copied())
            .flatten()
        })
        .collect()
}
use super::support::llvm;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn translate_goto(&mut self, edge: &MirEdge) -> Result<(), CodegenFailure> {
        self.add_edge_arguments(edge)?;
        self.clear_moved_places()?;

        llvm(
            self.builder
                .build_unconditional_branch(self.block(edge.target())),
        )?;

        Ok(())
    }

    pub(super) fn take_control_source(
        &mut self,
    ) -> Result<(BasicBlock<'context>, Vec<MirPlace>), CodegenFailure> {
        let source = self
            .builder
            .get_insert_block()
            .expect("checked MIR translation requires an established mapping or value");

        Ok((source, std::mem::take(&mut self.pending_moves)))
    }

    pub(super) fn route_edge(
        &mut self,
        edge: &MirEdge,
        name: &str,
        pending_moves: &[MirPlace],
    ) -> Result<BasicBlock<'context>, CodegenFailure> {
        let route = self.begin_route(name, pending_moves);

        self.add_edge_arguments(edge)?;
        self.finish_route(edge.target())?;

        Ok(route)
    }

    pub(super) fn route_target(
        &mut self,
        target: bray_ir::MirBlockId,
        name: &str,
        pending_moves: &[MirPlace],
    ) -> Result<BasicBlock<'context>, CodegenFailure> {
        let route = self.begin_route(name, pending_moves);

        self.finish_route(target)?;

        Ok(route)
    }

    pub(super) fn route_call_panic(
        &mut self,
        edge: &MirCallPanicEdge,
        context: PointerValue<'context>,
        source: Option<BasicValueEnum<'context>>,
        name: &str,
        pending_moves: &[MirPlace],
    ) -> Result<BasicBlock<'context>, CodegenFailure> {
        let route = self.begin_route(name, pending_moves);
        let destination = self.place(edge.report())?;

        let layout = self
            .type_mapping(edge.report().ty())
            .and_then(bray_codegen::CodegenTypeMapping::layout)
            .expect("call panic report types must have represented codegen layouts");

        let alignment = crate::conversion::target_value(
            layout.alignment().get(),
            "call_panic_report_alignment",
        )?;

        let outcome_type =
            crate::native::run_outcome_type(self.types.context(), self.request.target());

        let report = super::support::llvm(self.builder.build_struct_gep(
            outcome_type,
            context,
            2,
            "call.outcome.report",
        ))?;

        if let Some(source) = source {
            let report_type = crate::native::panic_report_type(self.types.context());

            let source_field_count =
                crate::native::source_anchor_type(self.types.context()).count_fields();

            for index in 0..source_field_count {
                let value = super::support::extract_value(&self.builder, source, index)?;

                let field = super::support::llvm(self.builder.build_struct_gep(
                    report_type,
                    report,
                    index,
                    "call.panic.report.source",
                ))?;

                super::support::llvm(self.builder.build_store(field, value))?;
            }
        }

        super::support::llvm(self.builder.build_memcpy(
            destination,
            alignment,
            report,
            alignment,
            self.pointer_integer_type().const_int(layout.size(), false),
        ))?;

        super::support::llvm(
            self.builder
                .build_store(context, outcome_type.const_zero()),
        )?;

        self.finish_route(edge.target())?;

        Ok(route)
    }

    fn begin_route(&mut self, name: &str, pending_moves: &[MirPlace]) -> BasicBlock<'context> {
        debug_assert!(self.pending_moves.is_empty());

        let route = self.types.context().append_basic_block(self.function, name);

        self.builder.position_at_end(route);
        self.pending_moves.extend_from_slice(pending_moves);

        route
    }

    fn finish_route(&mut self, target: bray_ir::MirBlockId) -> Result<(), CodegenFailure> {
        self.clear_moved_places()?;

        llvm(self.builder.build_unconditional_branch(self.block(target)))?;

        Ok(())
    }

    pub(super) fn add_edge_arguments(&mut self, edge: &MirEdge) -> Result<(), CodegenFailure> {
        let target = self
            .unit
            .block(edge.target())
            .expect("checked MIR translation requires an established mapping or value");

        if target.parameters().len() != edge.arguments().len() {
            panic!("checked MIR translation violated an established compiler contract");
        }

        let mut incoming: Vec<(PhiValue<'context>, BasicValueEnum<'context>)> =
            Vec::with_capacity(target.parameters().len());

        for (parameter, argument) in target.parameters().iter().zip(edge.arguments()) {
            let phi = self
                .phis
                .get(parameter)
                .copied()
                .expect("checked MIR translation requires an established mapping or value");

            incoming.push((phi, self.operand(argument)?));
        }

        let source = self
            .builder
            .get_insert_block()
            .expect("checked MIR translation requires an established mapping or value");

        for (phi, value) in incoming {
            phi.add_incoming(&[(&value, source)]);
        }

        Ok(())
    }
}
