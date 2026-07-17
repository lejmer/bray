use std::collections::BTreeSet;

use crate::{
    MirBlock, MirBlockId, MirBlockKind, MirCleanupEdge, MirCleanupPhase, MirEdge,
    MirTerminatorKind, MirUnit, MirUnitBuildError,
};

use super::core::{missing_or_foreign_block, validate_frame_state, validate_runtime_role};
use super::operation::{operand_type, validate_operand};

pub(super) fn validate_terminator(
    unit: &MirUnit,
    block_id: MirBlockId,
    block: &MirBlock,
) -> Result<(), MirUnitBuildError> {
    match block.terminator().kind() {
        MirTerminatorKind::Goto(edge) => validate_edge(unit, edge)?,
        MirTerminatorKind::Branch {
            condition,
            then_edge,
            else_edge,
        } => {
            validate_operand(unit, condition)?;
            validate_edge(unit, then_edge)?;
            validate_edge(unit, else_edge)?;
        }
        MirTerminatorKind::Switch {
            discriminant,
            cases,
            otherwise,
        } => {
            validate_operand(unit, discriminant)?;

            let mut values = BTreeSet::new();

            for case in cases.iter() {
                if !values.insert(case.value()) {
                    return Err(MirUnitBuildError::DuplicateSwitchCase(block_id));
                }

                validate_edge(unit, case.edge())?;
            }

            validate_edge(unit, otherwise)?;
        }
        MirTerminatorKind::Return(value) => {
            if let Some(value) = value {
                validate_operand(unit, value)?;
            }
        }
        MirTerminatorKind::Unreachable => {}
        MirTerminatorKind::Suspend {
            resume_state,
            resume,
            cancellation,
            runtime,
        } => {
            validate_frame_state(unit, *resume_state)?;
            validate_edge(unit, resume)?;
            validate_cleanup_start(unit, block_id, cancellation)?;
            validate_runtime_role(
                *runtime,
                bray_runtime_interface::RuntimeAbiRole::SuspensionRegistration,
            )?;
        }
        MirTerminatorKind::ForwardRunResult { result, edges } => {
            validate_operand(unit, result)?;
            validate_edge(unit, edges.completed())?;
            validate_cleanup_start(unit, block_id, edges.panicked())?;
            validate_cleanup_start(unit, block_id, edges.cancelled())?;
        }
        MirTerminatorKind::BeginCleanup(edge) => {
            if block.kind() == MirBlockKind::LifecycleResolution {
                return Err(MirUnitBuildError::CleanupPhaseOrderViolation(block_id));
            }

            validate_cleanup_start(unit, block_id, edge)?;
        }
        MirTerminatorKind::ContinueCleanup(edge) => {
            if block.kind() != MirBlockKind::CleanupBroadcast
                || edge.phase() != MirCleanupPhase::LifecycleResolution
            {
                return Err(MirUnitBuildError::CleanupPhaseOrderViolation(block_id));
            }

            validate_cleanup_edge(unit, edge)?;
        }
        MirTerminatorKind::Panic { report, cleanup } => {
            validate_operand(unit, report)?;
            validate_cleanup_start(unit, block_id, cleanup)?;
        }
        MirTerminatorKind::CancelCurrentRun { cleanup } => {
            validate_cleanup_start(unit, block_id, cleanup)?;
        }
    }

    if block.kind() == MirBlockKind::CleanupBroadcast
        && !matches!(
            block.terminator().kind(),
            MirTerminatorKind::ContinueCleanup(_)
        )
    {
        return Err(MirUnitBuildError::CleanupPhaseOrderViolation(block_id));
    }

    Ok(())
}

fn validate_cleanup_start(
    unit: &MirUnit,
    block: MirBlockId,
    cleanup: &MirCleanupEdge,
) -> Result<(), MirUnitBuildError> {
    if cleanup.phase() != MirCleanupPhase::TaskCancellation {
        return Err(MirUnitBuildError::CleanupPhaseOrderViolation(block));
    }

    validate_cleanup_edge(unit, cleanup)
}

fn validate_cleanup_edge(
    unit: &MirUnit,
    cleanup: &MirCleanupEdge,
) -> Result<(), MirUnitBuildError> {
    validate_edge(unit, cleanup.edge())?;

    let target = cleanup.edge().target();
    let Some(block) = unit.block(target) else {
        return Err(missing_or_foreign_block(unit, target));
    };

    let expected = match cleanup.phase() {
        MirCleanupPhase::TaskCancellation => MirBlockKind::CleanupBroadcast,
        MirCleanupPhase::LifecycleResolution => MirBlockKind::LifecycleResolution,
    };

    if block.kind() != expected {
        return Err(MirUnitBuildError::CleanupTargetMismatch {
            phase: cleanup.phase(),
            target,
        });
    }

    Ok(())
}

fn validate_edge(unit: &MirUnit, edge: &MirEdge) -> Result<(), MirUnitBuildError> {
    let Some(target) = unit.block(edge.target()) else {
        return Err(missing_or_foreign_block(unit, edge.target()));
    };

    if edge.arguments().len() != target.parameters().len() {
        return Err(MirUnitBuildError::EdgeArgumentCountMismatch(edge.target()));
    }

    for (argument, parameter) in edge.arguments().iter().zip(target.parameters()) {
        validate_operand(unit, argument)?;

        let argument_type = operand_type(unit, argument)?;
        let Some(parameter) = unit.value(*parameter) else {
            return Err(MirUnitBuildError::MissingValue(*parameter));
        };

        if argument_type != parameter.ty() {
            return Err(MirUnitBuildError::EdgeArgumentTypeMismatch(edge.target()));
        }
    }

    Ok(())
}
