use std::collections::{BTreeMap, BTreeSet};

use bray_codegen::CodegenFailure;
use bray_ir::{
    MirBlockId, MirCallFailureEdge, MirEdge, MirOperand, MirOperationId, MirPlace,
    MirTerminatorKind, MirUnit,
};
use inkwell::basic_block::BasicBlock;
use inkwell::context::Context;
use inkwell::values::{BasicValueEnum, FunctionValue, PhiValue, PointerValue};

use super::core::UnitTranslator;

pub(super) fn reachable_blocks(
    unit: &MirUnit,
    forwards_call_outcomes: bool,
    entries: impl IntoIterator<Item = MirBlockId>,
) -> BTreeSet<MirBlockId> {
    let mut reachable = BTreeSet::new();
    let mut pending: Vec<_> = entries.into_iter().collect();

    while let Some(block) = pending.pop() {
        if !reachable.insert(block) {
            continue;
        }

        let Some(block) = unit.block(block) else {
            continue;
        };

        match block.terminator().kind() {
            MirTerminatorKind::CheckCallOutcome {
                completed,
                panicked,
                cancelled,
            } if forwards_call_outcomes => {
                pending.push(completed.target());

                if !direct_panic_propagation(unit, panicked) {
                    pending.push(panicked.target());
                }

                if !direct_cancellation_propagation(unit, cancelled) {
                    pending.push(cancelled.target());
                }
            }
            terminator => terminator.for_each_successor(|successor| pending.push(successor)),
        }
    }

    reachable
}

pub(super) fn create_blocks<'context>(
    context: &'context Context,
    function: FunctionValue<'context>,
    unit: &MirUnit,
    reachable: Option<&BTreeSet<MirBlockId>>,
) -> BTreeMap<MirBlockId, BasicBlock<'context>> {
    unit.blocks_with_ids()
        .enumerate()
        .filter(|(_, (id, _))| reachable.is_none_or(|reachable| reachable.contains(id)))
        .map(|(index, (id, _))| {
            let block = context.append_basic_block(function, &format!("block.{index}"));

            (id, block)
        })
        .collect()
}

pub(super) fn direct_panic_propagation(unit: &MirUnit, edge: &MirCallFailureEdge) -> bool {
    let Some(block) = unit.block(edge.target()) else {
        return false;
    };

    if !edge.report().projections().is_empty()
        || !block.operations().is_empty()
        || !block.parameters().is_empty()
    {
        return false;
    }

    let report = MirOperand::Move(edge.report().clone());

    match block.terminator().kind() {
        MirTerminatorKind::PropagatePanic {
            report: propagated, ..
        } => propagated == &report,
        MirTerminatorKind::Panic {
            report: propagated,
            cleanup,
        } if propagated == &report => {
            let Some((destination, report)) =
                empty_cleanup_destination(unit, cleanup, Some(report))
            else {
                return false;
            };

            matches!(destination.terminator().kind(), MirTerminatorKind::PropagatePanic {
                report: propagated,
                ..
            } if Some(propagated) == report.as_ref())
        }
        _ => false,
    }
}

pub(super) fn direct_cancellation_propagation(unit: &MirUnit, edge: &MirCallFailureEdge) -> bool {
    let Some(block) = unit.block(edge.target()) else {
        return false;
    };

    if !edge.report().projections().is_empty()
        || !block.operations().is_empty()
        || !block.parameters().is_empty()
    {
        return false;
    }

    let report = MirOperand::Move(edge.report().clone());

    match block.terminator().kind() {
        MirTerminatorKind::PropagateCancellation { report: propagated, .. } => propagated == &report,
        MirTerminatorKind::CancelCurrentRun { cleanup } => {
            empty_cleanup_destination(unit, cleanup, Some(report)).is_some_and(|(destination, report)| {
                matches!(destination.terminator().kind(), MirTerminatorKind::PropagateCancellation { report: propagated, .. } if Some(propagated) == report.as_ref())
            })
        }
        _ => false,
    }
}

fn empty_cleanup_destination<'unit>(
    unit: &'unit MirUnit,
    cleanup: &bray_ir::MirCleanupEdge,
    mut report: Option<MirOperand>,
) -> Option<(&'unit bray_ir::MirBlock, Option<MirOperand>)> {
    let mut edge = cleanup.edge();

    // MIR cleanup has exactly two phases. Neither may perform work beyond forwarding the outcome.
    for phase in [
        bray_ir::MirCleanupPhase::TaskCancellation,
        bray_ir::MirCleanupPhase::LifecycleResolution,
    ] {
        let block = unit.block(edge.target())?;

        if !block.operations().is_empty() {
            return None;
        }

        match (report.as_ref(), edge.arguments(), block.parameters()) {
            (Some(incoming), [argument], [parameter]) if incoming == argument => {
                report = Some(MirOperand::Value(*parameter));
            }
            (None, [], []) => {}
            _ => return None,
        }

        if phase == bray_ir::MirCleanupPhase::LifecycleResolution {
            return Some((block, report));
        }

        let MirTerminatorKind::ContinueCleanup(next) = block.terminator().kind() else {
            return None;
        };

        edge = next.edge();
    }

    unreachable!("two-phase cleanup traversal must return at lifecycle resolution")
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

    pub(super) fn route_call_failure(
        &mut self,
        edge: &MirCallFailureEdge,
        context: PointerValue<'context>,
        state: bray_runtime_abi::NativeRunState,
        pending_moves: &[MirPlace],
    ) -> Result<BasicBlock<'context>, CodegenFailure> {
        let (name, direct) = if state == bray_runtime_abi::NativeRunState::PANICKED {
            ("call.panicked", direct_panic_propagation(self.unit, edge))
        } else {
            assert_eq!(state, bray_runtime_abi::NativeRunState::CANCELLED);

            (
                "call.cancelled",
                direct_cancellation_propagation(self.unit, edge),
            )
        };

        let route = self.begin_route(name, pending_moves);

        // An empty propagation path transfers this same report back to the caller.
        // Keep its ownership in the incoming context instead of materializing a temporary.
        if self.panic_report_context == Some(context) && direct {
            self.clear_moved_places()?;
            self.return_propagated_outcome()?;

            return Ok(route);
        }

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

        super::support::llvm(self.builder.build_memcpy(
            destination,
            alignment,
            report,
            alignment,
            self.pointer_integer_type().const_int(layout.size(), false),
        ))?;

        super::support::llvm(self.builder.build_store(context, outcome_type.const_zero()))?;

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

#[cfg(test)]
mod tests {
    use bray_ir::{
        MirBlockKind, MirOperationKind, MirProjection, MirProjectionKind, MirRuntimeReference,
        MirSourceAnchor, MirStorageKind, MirUnitBuilder, MirUnitKind,
    };
    use bray_runtime_interface::RuntimeAbiRole;

    use super::{
        MirBlockId, MirCallFailureEdge, MirEdge, MirOperand, MirPlace, MirTerminatorKind,
        direct_panic_propagation, reachable_blocks,
    };

    #[test]
    fn direct_outcomes_are_forwarded_only_with_an_incoming_context() {
        let (builder, _, [entry, completed, panicked, cancelled], _) = propagation_builder(None);

        let unit = builder.finish(entry);

        assert_eq!(
            reachable_blocks(&unit, true, [unit.entry()]),
            [entry, completed].into()
        );

        assert_eq!(
            reachable_blocks(&unit, false, [unit.entry()]),
            [entry, completed, panicked, cancelled].into()
        );
    }

    #[test]
    fn propagation_targets_with_other_predecessors_remain_reachable() {
        let (mut builder, source, [entry, completed, panicked, _], _) =
            propagation_builder(Some(1));

        builder.set_terminator(
            completed,
            source,
            MirTerminatorKind::Goto(MirEdge::new(panicked, [])),
        );

        let unit = builder.finish(entry);

        assert_eq!(
            reachable_blocks(&unit, true, [unit.entry()]),
            [entry, completed, panicked].into()
        );
    }

    #[test]
    fn cleanup_work_and_catch_transfers_preserve_the_report_route() {
        let (mut builder, source, [entry, completed, panicked, cancelled], report) =
            propagation_builder(Some(3));

        builder
            .push_operation(
                panicked,
                source.clone(),
                MirOperationKind::Cleanup {
                    phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                    place: report,
                },
                None,
            )
            .unwrap();

        builder.set_terminator(
            cancelled,
            source,
            MirTerminatorKind::Goto(MirEdge::new(completed, [])),
        );

        let unit = builder.finish(entry);

        assert_eq!(
            reachable_blocks(&unit, true, [unit.entry()]),
            [entry, completed, panicked, cancelled].into()
        );
    }

    #[test]
    fn forwarding_requires_a_move_of_the_exact_report() {
        let (mut builder, source, [entry, completed, panicked, _], report) =
            propagation_builder(Some(2));

        let runtime = MirRuntimeReference::new(
            RuntimeAbiRole::PanicPropagation,
            builder.target().runtime_abi(),
        );

        builder.set_terminator(
            panicked,
            source,
            MirTerminatorKind::PropagatePanic {
                report: MirOperand::Copy(report),
                runtime,
            },
        );

        let unit = builder.finish(entry);

        assert_eq!(
            reachable_blocks(&unit, true, [unit.entry()]),
            [entry, completed, panicked].into()
        );
    }

    #[test]
    fn projected_report_destinations_preserve_selector_evaluation() {
        let (mut builder, source, [entry, _, panicked, _], report) = propagation_builder(Some(2));

        let projected = MirPlace::new(
            report.storage(),
            [MirProjection::new(
                MirProjectionKind::Index(MirOperand::Move(report.clone())),
                report.ty(),
                report.ty(),
            )],
            report.ty(),
        );

        let runtime = MirRuntimeReference::new(
            RuntimeAbiRole::PanicPropagation,
            builder.target().runtime_abi(),
        );

        builder.set_terminator(
            panicked,
            source,
            MirTerminatorKind::PropagatePanic {
                report: MirOperand::Move(projected.clone()),
                runtime,
            },
        );

        let unit = builder.finish(entry);

        assert!(!direct_panic_propagation(
            &unit,
            &MirCallFailureEdge::new(panicked, projected)
        ));
    }

    #[test]
    fn cancellation_with_a_different_report_is_not_forwarded() {
        let (mut builder, source, [entry, completed, panicked, cancelled], report) =
            propagation_builder(Some(0));

        let other = builder
            .push_storage(source.clone(), MirStorageKind::Temporary, report.ty())
            .unwrap();

        let other = MirPlace::new(other, [], report.ty());

        builder.set_terminator(
            entry,
            source,
            MirTerminatorKind::CheckCallOutcome {
                completed: MirEdge::new(completed, []),
                panicked: MirCallFailureEdge::new(panicked, report.clone()),
                cancelled: MirCallFailureEdge::new(cancelled, other),
            },
        );

        let unit = builder.finish(entry);

        assert_eq!(
            reachable_blocks(&unit, true, [unit.entry()]),
            [entry, completed, cancelled].into()
        );
    }

    fn propagation_builder(
        unterminated: Option<usize>,
    ) -> (MirUnitBuilder, MirSourceAnchor, [MirBlockId; 4], MirPlace) {
        let bound = bray_testing::test_bound_unit(611);
        let source = MirSourceAnchor::from(bound.key().source());
        let target = bray_testing::test_mir_target();

        let panic =
            MirRuntimeReference::new(RuntimeAbiRole::PanicPropagation, target.runtime_abi());

        let cancellation = MirRuntimeReference::new(
            RuntimeAbiRole::CurrentRunCancellationPropagation,
            target.runtime_abi(),
        );

        let mut builder =
            MirUnitBuilder::for_bound(bound.identity(), MirUnitKind::Synchronous, target);

        let blocks = std::array::from_fn(|_| {
            builder
                .push_block(source.clone(), MirBlockKind::Ordinary)
                .unwrap()
        });

        let [entry, completed, panicked, cancelled] = blocks;

        let ty = bray_testing::test_mir_type();

        let storage = builder
            .push_storage(source.clone(), MirStorageKind::Temporary, ty)
            .unwrap();

        let report = MirPlace::new(storage, [], ty);

        builder
            .push_operation(
                entry,
                source.clone(),
                MirOperationKind::AdmitOutgoing {
                    owner: bray_ir::MirOutgoingOwner::Value(ty),
                    runtime: MirRuntimeReference::new(
                        RuntimeAbiRole::OutgoingAdmission,
                        builder.target().runtime_abi(),
                    ),
                },
                None,
            )
            .unwrap();

        let terminators = [
            MirTerminatorKind::CheckCallOutcome {
                completed: MirEdge::new(completed, []),
                panicked: MirCallFailureEdge::new(panicked, report.clone()),
                cancelled: MirCallFailureEdge::new(cancelled, report.clone()),
            },
            MirTerminatorKind::Return(None),
            MirTerminatorKind::PropagatePanic {
                report: MirOperand::Move(report.clone()),
                runtime: panic,
            },
            MirTerminatorKind::PropagateCancellation {
                report: MirOperand::Move(report.clone()),
                runtime: cancellation,
            },
        ];

        for (index, (block, terminator)) in blocks.into_iter().zip(terminators).enumerate() {
            if unterminated != Some(index) {
                builder.set_terminator(block, source.clone(), terminator);
            }
        }

        (builder, source, blocks, report)
    }
}
