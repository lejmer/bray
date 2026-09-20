use std::collections::BTreeSet;

use crate::{
    MirBlock, MirBlockId, MirBlockKind, MirCleanupEdge, MirCleanupPhase, MirEdge,
    MirTerminatorKind, MirUnit,
};

use super::core::{validate_frame_state, validate_runtime_role};
use super::operation::validate_operand;

pub(super) fn validate_terminator(
    unit: &MirUnit,
    block_id: MirBlockId,
    block: &MirBlock,
) -> Option<()> {
    match block.terminator().kind() {
        MirTerminatorKind::Goto(edge) => validate_goto_edge(unit, block_id, edge)?,
        MirTerminatorKind::Branch {
            condition,
            then_edge,
            else_edge,
        } => {
            validate_operand(unit, condition, block_id, None)?;
            validate_local_edge(unit, block_id, then_edge)?;
            validate_local_edge(unit, block_id, else_edge)?;
        }
        MirTerminatorKind::PatternBranch {
            subject,
            matched,
            unmatched,
            ..
        } => {
            validate_operand(unit, subject, block_id, None)?;
            validate_local_edge(unit, block_id, matched)?;
            validate_local_edge(unit, block_id, unmatched)?;
        }
        MirTerminatorKind::Iterate {
            cursor,
            element_type,
            item,
            exhausted,
            ..
        }
        | MirTerminatorKind::RangeIterate {
            cursor,
            element_type,
            item,
            exhausted,
            ..
        } => {
            super::operation::validate_place(unit, cursor, block_id, None)?;
            validate_iteration_item(unit, *item, *element_type)?;
            validate_ordinary_edge(unit, block_id, exhausted)?;
        }
        MirTerminatorKind::Switch {
            discriminant,
            cases,
            otherwise,
        } => {
            validate_operand(unit, discriminant, block_id, None)?;

            let mut values = BTreeSet::new();

            for case in cases.iter() {
                if !values.insert(case.value()) {
                    return None;
                }

                validate_local_edge(unit, block_id, case.edge())?;
            }

            validate_local_edge(unit, block_id, otherwise)?;
        }
        MirTerminatorKind::InlineAssembly(assembly) => {
            validate_operand(unit, assembly.inputs(), block_id, None)?;

            let label_count = assembly
                .contract()
                .operands()
                .filter(|operand| {
                    operand.kind() == bray_bound_tree::InlineAssemblyOperandKind::Label
                })
                .count();

            if unit.operand_type(assembly.inputs())? != assembly.inputs_type()
                || label_count != assembly.alternates().len()
                || assembly
                    .contract()
                    .operands()
                    .filter(|operand| {
                        operand.kind() == bray_bound_tree::InlineAssemblyOperandKind::Symbol
                    })
                    .count()
                    != assembly.symbols().len()
            {
                return None;
            }

            let mut successors = BTreeSet::from([assembly.normal()]);

            for alternate in assembly.alternates() {
                let Some(block) = unit.block(*alternate) else {
                    return None;
                };

                if !successors.insert(*alternate)
                    || block.kind() != MirBlockKind::Ordinary
                    || !block.parameters().is_empty()
                {
                    return None;
                }
            }

            let Some(normal) = unit.block(assembly.normal()) else {
                return None;
            };

            let [parameter] = normal.parameters() else {
                return None;
            };

            let Some(parameter) = unit.value(*parameter) else {
                return None;
            };

            if normal.kind() != MirBlockKind::Ordinary || parameter.ty() != assembly.output_type() {
                return None;
            }
        }
        MirTerminatorKind::Return(value) => {
            if let Some(value) = value {
                validate_operand(unit, value, block_id, None)?;
            }
        }
        MirTerminatorKind::Unreachable => {}
        MirTerminatorKind::Suspend {
            kind,
            payload,
            resume_state,
            resume,
            cancellation,
            registration,
            wake,
            ..
        } => {
            match (kind, payload) {
                (crate::MirSuspensionKind::TaskEvent, Some(payload)) => {
                    validate_operand(unit, payload, block_id, None)?;
                }
                (crate::MirSuspensionKind::Awaited | crate::MirSuspensionKind::Yield, None) => {}
                _ => return None,
            };

            validate_frame_state(unit, *resume_state)?;
            validate_ordinary_edge(unit, block_id, resume)?;

            let Some(descriptor) = unit.frame_descriptor() else {
                return None;
            };

            let expected = descriptor
                .states()
                .iter()
                .find(|state| state.state() == *resume_state)
                .map(crate::MirFrameState::entry);

            if expected != Some(resume.target()) {
                return None;
            }

            validate_cleanup_start(unit, block_id, cancellation)?;

            validate_runtime_role(
                unit,
                *registration,
                bray_runtime_interface::RuntimeAbiRole::SuspensionRegistration,
            )?;

            validate_runtime_role(unit, *wake, bray_runtime_interface::RuntimeAbiRole::Wake)?;
        }
        MirTerminatorKind::ForwardRunResult { result, edges } => {
            validate_operand(unit, result, block_id, None)?;

            validate_ordinary_edge(unit, block_id, edges.completed())?;
            validate_cleanup_start(unit, block_id, edges.panicked())?;
            validate_cleanup_start(unit, block_id, edges.cancelled())?;
        }
        MirTerminatorKind::CheckCallOutcome {
            completed,
            panicked,
            cancelled,
        } => {
            validate_call_panic_check(unit, block_id, block, completed, panicked)?;
            validate_local_edge(unit, block_id, cancelled)?;
        }
        MirTerminatorKind::BeginCleanup(edge) => {
            if block.kind() == MirBlockKind::LifecycleResolution {
                return None;
            }

            validate_cleanup_start(unit, block_id, edge)?;
        }
        MirTerminatorKind::ContinueCleanup(edge) => {
            if block.kind() != MirBlockKind::CleanupBroadcast
                || edge.phase() != MirCleanupPhase::LifecycleResolution
            {
                return None;
            }

            validate_cleanup_edge(unit, block_id, edge)?;
        }
        MirTerminatorKind::Panic { report, cleanup } => {
            validate_operand(unit, report, block_id, None)?;
            validate_cleanup_start(unit, block_id, cleanup)?;
        }
        MirTerminatorKind::PropagatePanic { report, runtime } => {
            validate_operand(unit, report, block_id, None)?;

            validate_runtime_role(
                unit,
                *runtime,
                bray_runtime_interface::RuntimeAbiRole::PanicPropagation,
            )?;
        }
        MirTerminatorKind::PropagateCancellation { runtime } => {
            validate_runtime_role(
                unit,
                *runtime,
                bray_runtime_interface::RuntimeAbiRole::CurrentRunCancellationPropagation,
            )?;
        }
        MirTerminatorKind::CancelCurrentRun { cleanup } => {
            validate_cleanup_start(unit, block_id, cleanup)?;
        }
    }

    if block.kind() == MirBlockKind::CleanupBroadcast
        && !matches!(
            block.terminator().kind(),
            MirTerminatorKind::Goto(_)
                | MirTerminatorKind::Branch { .. }
                | MirTerminatorKind::PatternBranch { .. }
                | MirTerminatorKind::Switch { .. }
                | MirTerminatorKind::InlineAssembly(_)
                | MirTerminatorKind::ContinueCleanup(_)
                | MirTerminatorKind::CheckCallOutcome { .. }
        )
    {
        return None;
    }

    Some(())
}

fn validate_iteration_item(
    unit: &MirUnit,
    item: MirBlockId,
    element_type: bray_symbols::TypeId,
) -> Option<()> {
    let Some(block) = unit.block(item) else {
        return None;
    };

    let [parameter] = block.parameters() else {
        return None;
    };

    let Some(parameter) = unit.value(*parameter) else {
        return None;
    };

    if block.kind() != MirBlockKind::Ordinary || parameter.ty() != element_type {
        return None;
    }

    Some(())
}

fn validate_call_panic_edge(
    unit: &MirUnit,
    source: MirBlockId,
    edge: &crate::MirCallPanicEdge,
) -> Option<()> {
    let Some(block) = unit.block(edge.target()) else {
        return None;
    };

    if unit.block(source).map(MirBlock::kind) != Some(block.kind())
        || !block.parameters().is_empty()
    {
        return None;
    }

    super::operation::validate_place(unit, edge.report(), source, None)?;

    Some(())
}

fn validate_call_panic_check(
    unit: &MirUnit,
    source: MirBlockId,
    block: &MirBlock,
    completed: &MirEdge,
    panicked: &crate::MirCallPanicEdge,
) -> Option<()> {
    validate_local_edge(unit, source, completed)?;
    validate_call_panic_edge(unit, source, panicked)?;

    let propagates_panic = block
        .operations()
        .last()
        .and_then(|operation| unit.operation(*operation))
        .is_some_and(|operation| match operation.kind() {
            crate::MirOperationKind::Call(call) => call.may_propagate_panic(),
            crate::MirOperationKind::Construct(construction) => matches!(
                construction.target(),
                crate::ConstructionTarget::TypeForm { .. }
            ),
            crate::MirOperationKind::AdmitOutgoing { .. }
            | crate::MirOperationKind::Cleanup { .. }
            | crate::MirOperationKind::Finalize(_)
            | crate::MirOperationKind::Destroy(_) => true,
            _ => false,
        });

    if !propagates_panic {
        return None;
    }

    Some(())
}

fn validate_local_edge(unit: &MirUnit, source: MirBlockId, edge: &MirEdge) -> Option<()> {
    validate_edge(unit, source, edge)?;

    let source_kind = unit.block(source).map(MirBlock::kind)?;

    if unit.block(edge.target()).map(MirBlock::kind) != Some(source_kind) {
        return None;
    }

    Some(())
}

fn validate_goto_edge(unit: &MirUnit, source: MirBlockId, edge: &MirEdge) -> Option<()> {
    validate_edge(unit, source, edge)?;

    let source_kind = unit.block(source).map(MirBlock::kind)?;

    let target_kind = unit.block(edge.target()).map(MirBlock::kind);

    if target_kind != Some(source_kind)
        && !(source_kind == MirBlockKind::LifecycleResolution
            && target_kind == Some(MirBlockKind::Ordinary))
    {
        return None;
    }

    Some(())
}

fn validate_ordinary_edge(unit: &MirUnit, source: MirBlockId, edge: &MirEdge) -> Option<()> {
    validate_edge(unit, source, edge)?;

    if unit.block(edge.target()).map(MirBlock::kind) != Some(MirBlockKind::Ordinary) {
        return None;
    }

    Some(())
}

fn validate_cleanup_start(
    unit: &MirUnit,
    block: MirBlockId,
    cleanup: &MirCleanupEdge,
) -> Option<()> {
    if cleanup.phase() != MirCleanupPhase::TaskCancellation {
        return None;
    }

    validate_cleanup_edge(unit, block, cleanup)
}

fn validate_cleanup_edge(
    unit: &MirUnit,
    source: MirBlockId,
    cleanup: &MirCleanupEdge,
) -> Option<()> {
    validate_edge(unit, source, cleanup.edge())?;

    let target = cleanup.edge().target();

    let Some(block) = unit.block(target) else {
        return None;
    };

    let expected = match cleanup.phase() {
        MirCleanupPhase::TaskCancellation => MirBlockKind::CleanupBroadcast,
        MirCleanupPhase::LifecycleResolution => MirBlockKind::LifecycleResolution,
    };

    if block.kind() != expected {
        return None;
    }

    Some(())
}
fn validate_edge(unit: &MirUnit, source: MirBlockId, edge: &MirEdge) -> Option<()> {
    let Some(target) = unit.block(edge.target()) else {
        return None;
    };

    if edge.arguments().len() != target.parameters().len() {
        return None;
    }

    for (argument, parameter) in edge.arguments().iter().zip(target.parameters()) {
        validate_operand(unit, argument, source, None)?;

        let argument_type = unit.operand_type(argument)?;

        let Some(parameter) = unit.value(*parameter) else {
            return None;
        };

        if argument_type != parameter.ty() {
            return None;
        }
    }

    Some(())
}
