use bray_codegen::CodegenFailure;
use bray_ir::{MirEdge, MirPlace};
use inkwell::basic_block::BasicBlock;
use inkwell::values::{BasicValueEnum, PhiValue};

use super::core::UnitTranslator;
use super::support::llvm;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn take_control_source(
        &mut self,
    ) -> Result<(BasicBlock<'context>, Vec<MirPlace>), CodegenFailure> {
        let source = self
            .builder
            .get_insert_block()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

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

    fn begin_route(&mut self, name: &str, pending_moves: &[MirPlace]) -> BasicBlock<'context> {
        debug_assert!(self.pending_moves.is_empty());

        let route = self.types.context().append_basic_block(self.function, name);

        self.builder.position_at_end(route);
        self.pending_moves.extend_from_slice(pending_moves);

        route
    }

    fn finish_route(&mut self, target: bray_ir::MirBlockId) -> Result<(), CodegenFailure> {
        self.clear_moved_places()?;

        llvm(self.builder.build_unconditional_branch(self.block(target)?))?;

        Ok(())
    }

    pub(super) fn add_edge_arguments(&mut self, edge: &MirEdge) -> Result<(), CodegenFailure> {
        let target = self
            .unit
            .block(edge.target())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        if target.parameters().len() != edge.arguments().len() {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let mut incoming: Vec<(PhiValue<'context>, BasicValueEnum<'context>)> =
            Vec::with_capacity(target.parameters().len());

        for (parameter, argument) in target.parameters().iter().zip(edge.arguments()) {
            let phi = self
                .phis
                .get(parameter)
                .copied()
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            incoming.push((phi, self.operand(argument)?));
        }

        let source = self
            .builder
            .get_insert_block()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        for (phi, value) in incoming {
            phi.add_incoming(&[(&value, source)]);
        }

        Ok(())
    }
}
