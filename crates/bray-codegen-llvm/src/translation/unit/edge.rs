use std::collections::BTreeSet;

use bray_codegen::CodegenFailure;
use bray_ir::{MirBlockId, MirCallPanicEdge, MirEdge, MirOperationId, MirPlace, MirUnit};
use inkwell::basic_block::BasicBlock;
use inkwell::values::{BasicValueEnum, PhiValue};

use super::core::UnitTranslator;

pub(super) fn reachable_blocks(unit: &MirUnit) -> BTreeSet<MirBlockId> {
    let mut pending = vec![unit.entry()];

    if let Some(frame) = unit.frame_descriptor() {
        pending.extend(frame.states().iter().map(bray_ir::MirFrameState::entry));
        pending.extend(frame.inactive_cleanup());

        if let Some((quiescence, destruction)) = frame.capture_abandonment() {
            pending.extend([quiescence, destruction]);
        }
    }

    unit.reachable_blocks(pending)
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

#[cfg(test)]
mod tests {
    use bray_ir::{
        MirBlockKind, MirCleanupEdge, MirCleanupPhase, MirEdge, MirFrameDescriptor, MirFrameState,
        MirFrameStateId, MirSourceAnchor, MirTerminatorKind, MirUnitBuilder, MirUnitKind,
    };
    use bray_runtime_interface::{ProtectedAsyncFrameId, ProtectedFrameAbiVersions};

    #[test]
    fn frame_dispatch_entries_and_capture_cleanup_are_reachable() {
        let bound = bray_testing::test_bound_unit(918);
        let source = MirSourceAnchor::from(bound.key().source());
        let target = bray_testing::test_mir_target();
        let abi = target.runtime_abi();
        let frame = ProtectedAsyncFrameId::new([9; 32]);
        let values = bray_symbols::SemanticValueStore::try_new().unwrap();

        let ty = values
            .intern_type(bray_symbols::TypeData::tuple([]))
            .unwrap();

        let mut builder = MirUnitBuilder::for_bound(
            bound.identity(),
            MirUnitKind::ProtectedAsyncFrame(frame),
            target,
        );

        let body = builder
            .push_block(source.clone(), MirBlockKind::Ordinary)
            .unwrap();

        let resumed = builder
            .push_block(source.clone(), MirBlockKind::Ordinary)
            .unwrap();

        let unrelated = builder
            .push_block(source.clone(), MirBlockKind::Ordinary)
            .unwrap();

        let captures = builder
            .push_block(source.clone(), MirBlockKind::CleanupBroadcast)
            .unwrap();

        let finished = builder
            .push_block(source.clone(), MirBlockKind::LifecycleResolution)
            .unwrap();

        for block in [body, resumed, unrelated, finished] {
            builder
                .set_terminator(block, source.clone(), MirTerminatorKind::Return(None))
                .unwrap();
        }

        builder
            .set_terminator(
                captures,
                source,
                MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                    MirCleanupPhase::LifecycleResolution,
                    MirEdge::new(finished, []),
                )),
            )
            .unwrap();

        builder
            .set_frame_descriptor(
                MirFrameDescriptor::try_new(
                    frame,
                    abi,
                    ProtectedFrameAbiVersions::uniform(abi),
                    ty,
                    [
                        MirFrameState::new(MirFrameStateId::new(0), body, [], []),
                        MirFrameState::new(MirFrameStateId::new(1), resumed, [], []),
                    ],
                )
                .unwrap()
                .with_inactive_cleanup(captures),
            )
            .unwrap();

        let unit = builder.finish(body).unwrap();

        assert_eq!(
            super::reachable_blocks(&unit),
            [body, resumed, captures, finished].into_iter().collect()
        );
    }
}

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
        target: MirBlockId,
        name: &str,
        pending_moves: &[MirPlace],
    ) -> Result<BasicBlock<'context>, CodegenFailure> {
        let route = self.begin_route(name, pending_moves);

        self.finish_route(target)?;

        Ok(route)
    }

    pub(super) fn route_call_panic(
        &mut self,
        edge: MirCallPanicEdge,
        report: BasicValueEnum<'context>,
        name: &str,
        pending_moves: &[MirPlace],
    ) -> Result<BasicBlock<'context>, CodegenFailure> {
        let route = self.begin_route(name, pending_moves);

        let target = self
            .unit
            .block(edge.target())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let [parameter] = target.parameters() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let phi = self
            .phis
            .get(parameter)
            .copied()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        phi.add_incoming(&[(&report, route)]);
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

    fn finish_route(&mut self, target: MirBlockId) -> Result<(), CodegenFailure> {
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
