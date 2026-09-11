use bray_bound_tree::BoundNodeOrigin;
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirEdge, MirImmediateValue, MirOperand, MirOperationKind, MirPlace,
    MirSourceAnchor, MirStoreKind, MirTerminatorKind, MirUnit, MirUnitBuildError, MirUnitBuilder,
};

use crate::lowering::LoweringError;
use crate::lowering::lowerer::{Lowerer, ReceiverCleanupAllowance};

impl Lowerer<'_> {
    pub(in crate::lowering) fn initialize_receiver_allowance(
        &mut self,
        entry: MirBlockId,
        source: &MirSourceAnchor,
    ) -> Result<(), LoweringError> {
        let identity = self
            .input
            .storage_plan()
            .identity_entries()
            .find_map(|(id, _)| {
                bray_bound_tree::storage_identity_is_destructor_receiver(
                    self.input.unit(),
                    self.input.storage_plan(),
                    id,
                )
                .then_some(id)
            });

        let Some(identity) = identity else {
            return Ok(());
        };

        let ty = self
            .input
            .storage_plan()
            .storage_type(identity)
            .ok_or(LoweringError::MissingStorageIdentityRecord(identity))?;

        let receiver = self.place_for_identity(
            identity,
            ty,
            BoundNodeOrigin::source(self.input.unit().key().source()),
        )?;

        let boolean = self.representation_type(RepresentationRole::ScalarBool)?;
        let guard = self.new_initialization_guard(entry, source, boolean, &[], true)?;
        self.cleanup_retained_storages.push(guard.storage());
        self.receiver_allowance = Some(ReceiverCleanupAllowance { receiver, guard });

        Ok(())
    }

    pub(in crate::lowering) fn set_receiver_allowance(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        place: &MirPlace,
        owned: bool,
    ) -> Result<(), MirUnitBuildError> {
        let place = self.ownership_place(place);

        let Some(allowance) = &self.receiver_allowance else {
            return Ok(());
        };

        if place.storage() != allowance.receiver.storage() || !place.projections().is_empty() {
            return Ok(());
        }

        self.builder.push_operation(
            block,
            Self::retained_source(source),
            MirOperationKind::Store {
                kind: MirStoreKind::Assign,
                destination: Self::retained_place(&allowance.guard),
                value: MirOperand::Immediate {
                    value: MirImmediateValue::Boolean(owned),
                    ty: allowance.guard.ty(),
                },
            },
            None,
        )?;

        Ok(())
    }
}

pub(in crate::lowering) fn discharge_receiver_allowance(
    unit: MirUnit,
    allowance: Option<ReceiverCleanupAllowance>,
) -> Result<MirUnit, MirUnitBuildError> {
    let Some(allowance) = allowance else {
        return Ok(unit);
    };

    // Terminal continuations retain their source anchors and operands while the unit is rebuilt.
    let exits = unit
        .blocks_with_ids()
        .filter_map(|(id, block)| {
            matches!(
                block.terminator().kind(),
                MirTerminatorKind::Return(_)
                    | MirTerminatorKind::PropagatePanic { .. }
                    | MirTerminatorKind::PropagateCancellation { .. }
            )
            .then(|| (id, block.kind(), block.terminator().clone()))
        })
        .collect::<Vec<_>>();

    let entry = unit.entry();
    let mut builder = MirUnitBuilder::from_unit(unit);

    for (block, kind, terminal) in exits {
        discharge_receiver_exit(&mut builder, block, kind, terminal, &allowance)?;
    }

    builder.finish(entry)
}

fn discharge_receiver_exit(
    builder: &mut MirUnitBuilder,
    block: MirBlockId,
    kind: bray_ir::MirBlockKind,
    terminal: bray_ir::MirTerminator,
    allowance: &ReceiverCleanupAllowance,
) -> Result<(), MirUnitBuildError> {
    let source = terminal.source();
    let discharge = builder.push_block(source.clone(), kind)?;
    let finished = builder.push_block(source.clone(), kind)?;

    let payload = match terminal.kind() {
        MirTerminatorKind::Return(value) => value.as_ref(),
        MirTerminatorKind::PropagatePanic { report, .. } => Some(report),
        _ => None,
    };

    let mut discharged = Vec::new();
    let mut output = None;

    if let Some(payload) = payload {
        let ty = builder.operand_type(payload)?;
        let input = builder.push_block_parameter(discharge, source.clone(), ty)?;
        let result = builder.push_block_parameter(finished, source.clone(), ty)?;
        discharged.push(MirOperand::Value(input));
        output = Some(MirOperand::Value(result));
    }

    // Each mutually exclusive edge transfers its terminal payload into block-local SSA.
    let arguments = payload.cloned().into_iter().collect::<Vec<_>>();
    builder.take_terminator(block)?;

    builder.set_terminator(
        block,
        source.clone(),
        MirTerminatorKind::Branch {
            condition: MirOperand::Copy(allowance.guard.clone()),
            then_edge: MirEdge::new(discharge, arguments.clone()),
            else_edge: MirEdge::new(finished, arguments),
        },
    )?;

    builder.push_operation(
        discharge,
        source.clone(),
        MirOperationKind::DischargeCleanup(allowance.receiver.ty()),
        None,
    )?;

    builder.set_terminator(
        discharge,
        source.clone(),
        MirTerminatorKind::Goto(MirEdge::new(finished, discharged)),
    )?;

    let terminal = match (terminal.kind(), output) {
        (MirTerminatorKind::Return(_), output) => MirTerminatorKind::Return(output),
        (MirTerminatorKind::PropagatePanic { runtime, .. }, Some(report)) => {
            MirTerminatorKind::PropagatePanic {
                report,
                runtime: *runtime,
            }
        }
        // Cancellation has no terminal value to forward.
        (terminal, _) => terminal.clone(),
    };

    builder.set_terminator(finished, source.clone(), terminal)
}
