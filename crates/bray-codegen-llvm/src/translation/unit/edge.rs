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
use super::support::pointer_value;

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
                ..
            } => {
                pending.push(completed.target());

                if !forwards_call_outcomes || !direct_panic_propagation(unit, panicked) {
                    pending.push(panicked.target());
                }

                if !empty_unreachable_target(unit, cancelled.target())
                    && (!forwards_call_outcomes
                        || !direct_cancellation_propagation(unit, cancelled))
                {
                    pending.push(cancelled.target());
                }
            }
            terminator => terminator.for_each_successor(|successor| pending.push(successor)),
        }
    }

    reachable
}

pub(super) fn empty_unreachable_target(unit: &MirUnit, target: MirBlockId) -> bool {
    let block = unit
        .block(target)
        .expect("checked call outcome retains its target block");

    block.operations().is_empty()
        && block.parameters().is_empty()
        && matches!(block.terminator().kind(), MirTerminatorKind::Unreachable)
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
                    | bray_ir::MirTerminatorKind::TakeCallReport { .. }
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
        self.clear_moved_reports()?;

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

        Ok((source, std::mem::take(&mut self.moved_reports)))
    }

    pub(super) fn route_edge(
        &mut self,
        edge: &MirEdge,
        name: &str,
        moved_reports: &[MirPlace],
    ) -> Result<BasicBlock<'context>, CodegenFailure> {
        let route = self.begin_route(name, moved_reports);

        self.add_edge_arguments(edge)?;
        self.finish_route(edge.target())?;

        Ok(route)
    }

    pub(super) fn route_target(
        &mut self,
        target: bray_ir::MirBlockId,
        name: &str,
        moved_reports: &[MirPlace],
    ) -> Result<BasicBlock<'context>, CodegenFailure> {
        let route = self.begin_route(name, moved_reports);

        self.finish_route(target)?;

        Ok(route)
    }

    pub(super) fn route_call_failure(
        &mut self,
        edge: &MirCallFailureEdge,
        context: PointerValue<'context>,
        state: bray_runtime_abi::NativeRunState,
        moved_reports: &[MirPlace],
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

        let route = self.begin_route(name, moved_reports);

        // An empty propagation path transfers this same report back to the caller.
        // Keep its ownership in the incoming context instead of materializing a temporary.
        if self.panic_report_context == Some(context) && direct {
            self.clear_moved_reports()?;
            self.return_propagated_outcome()?;

            return Ok(route);
        }

        self.take_call_report(context, edge.report())?;

        self.finish_route(edge.target())?;

        Ok(route)
    }

    pub(super) fn route_call_completion(
        &mut self,
        edge: &MirEdge,
        report: Option<&MirPlace>,
        context: PointerValue<'context>,
        moved_reports: &[MirPlace],
    ) -> Result<BasicBlock<'context>, CodegenFailure> {
        let route = self.begin_route("call.completed", moved_reports);

        if let Some(report) = report {
            self.take_call_report(context, report)?;
        }

        self.add_edge_arguments(edge)?;
        self.finish_route(edge.target())?;

        Ok(route)
    }

    fn take_call_report(
        &mut self,
        context: PointerValue<'context>,
        destination: &MirPlace,
    ) -> Result<(), CodegenFailure> {
        let layout = self
            .type_mapping(destination.ty())
            .and_then(bray_codegen::CodegenTypeMapping::layout)
            .expect("call panic report types must have represented codegen layouts");

        let destination = self.place(destination)?;

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

        Ok(())
    }

    fn begin_route(&mut self, name: &str, moved_reports: &[MirPlace]) -> BasicBlock<'context> {
        debug_assert!(self.moved_reports.is_empty());

        let route = self.types.context().append_basic_block(self.function, name);

        self.builder.position_at_end(route);
        self.moved_reports.extend_from_slice(moved_reports);

        route
    }

    fn finish_route(&mut self, target: bray_ir::MirBlockId) -> Result<(), CodegenFailure> {
        self.clear_moved_reports()?;

        llvm(self.builder.build_unconditional_branch(self.block(target)))?;

        Ok(())
    }

    pub(super) fn create_block_parameters(&mut self) -> Result<(), CodegenFailure> {
        for (id, block) in self.unit.blocks_with_ids() {
            if !self.reachable_blocks.contains(&id) {
                continue;
            }

            let llvm_block = self.block(id);

            self.builder.position_at_end(llvm_block);

            for parameter in block.parameters() {
                let semantic = self
                    .unit
                    .value(*parameter)
                    .expect("block parameter has its checked type")
                    .ty();

                if self.types.is_address_value(semantic)? {
                    let mapped = self.types.map(semantic)?;

                    let storage =
                        self.allocate_temporary(mapped, &format!("value.{}", parameter.slot()))?;

                    self.values.insert(*parameter, storage.into());

                    continue;
                }

                let ty = self.value_type(*parameter)?;

                let phi = llvm(
                    self.builder
                        .build_phi(ty, &format!("value.{}", parameter.slot())),
                )?;

                self.phis.insert(*parameter, phi);
                self.values.insert(*parameter, phi.as_basic_value());
            }
        }

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

        let mut incoming: Vec<(PhiValue<'context>, BasicValueEnum<'context>)> = Vec::new();
        let mut stored = Vec::new();

        for (parameter, argument) in target.parameters().iter().zip(edge.arguments()) {
            let ty = self
                .unit
                .value(*parameter)
                .expect("block parameter has its checked type")
                .ty();

            let value = self.operand(argument)?;

            if self.types.is_address_value(ty)? {
                let destination = pointer_value(self.values[parameter])
                    .expect("address-valued block parameter owns its storage");

                let source =
                    pointer_value(value).expect("address-valued edge input has its storage");

                if source != destination {
                    stored.push((destination, value, ty));
                }
            } else {
                let phi = self
                    .phis
                    .get(parameter)
                    .copied()
                    .expect("register block parameter has its LLVM phi");

                incoming.push((phi, value));
            }
        }

        let destinations = stored
            .iter()
            .map(|(destination, _, _)| *destination)
            .collect::<std::collections::HashSet<_>>();

        // Snapshot every input that another parameter assignment could overwrite.
        // These private edge scratches are never published as value carriers, so
        // re-entering a loop cannot make a scratch alias an incoming value.
        for (_, value, ty) in &mut stored {
            if destinations
                .contains(&pointer_value(*value).expect("stored edge input has its address"))
            {
                *value = self.snapshot_value(*value, *ty, "edge.snapshot")?;
            }
        }

        for (destination, value, ty) in stored {
            self.store_value(destination, value, ty)?;
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
    fn impossible_cancellation_is_not_a_report_route() {
        let (mut builder, source, [entry, completed, panicked, cancelled], _) =
            propagation_builder(Some(3));

        builder.set_terminator(cancelled, source, MirTerminatorKind::Unreachable);

        let unit = builder.finish(entry);

        assert_eq!(
            reachable_blocks(&unit, true, [entry]),
            [entry, completed].into()
        );

        assert_eq!(
            reachable_blocks(&unit, false, [entry]),
            [entry, completed, panicked].into()
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
                    collection: None,
                    phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                    place: report,
                },
                None,
            )
            .expect("fixture operation must fit its identity space");

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
            .expect("fixture storage must fit its identity space");

        let other = MirPlace::new(other, [], report.ty());

        builder.set_terminator(
            entry,
            source,
            MirTerminatorKind::CheckCallOutcome {
                completed: MirEdge::new(completed, []),
                panicked: MirCallFailureEdge::new(panicked, report.clone()),
                cancelled: MirCallFailureEdge::new(cancelled, other),
                completed_report: None,
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

        let mut builder =
            MirUnitBuilder::for_bound(bound.identity(), MirUnitKind::Synchronous, target);

        let blocks = std::array::from_fn(|_| {
            builder
                .push_block(source.clone(), MirBlockKind::Ordinary)
                .expect("fixture block must fit its identity space")
        });

        let [entry, completed, panicked, cancelled] = blocks;

        let ty = bray_testing::test_mir_type();

        let storage = builder
            .push_storage(source.clone(), MirStorageKind::Temporary, ty)
            .expect("fixture storage must fit its identity space");

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
            .expect("fixture operation must fit its identity space");

        let terminators = [
            MirTerminatorKind::CheckCallOutcome {
                completed: MirEdge::new(completed, []),
                panicked: MirCallFailureEdge::new(panicked, report.clone()),
                cancelled: MirCallFailureEdge::new(cancelled, report.clone()),
                completed_report: None,
            },
            MirTerminatorKind::Return {
                value: None,
                report: None,
            },
            MirTerminatorKind::PropagatePanic {
                report: MirOperand::Move(report.clone()),
                runtime: panic,
            },
            MirTerminatorKind::PropagateCancellation {
                report: MirOperand::Move(report.clone()),
                runtime: panic,
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
