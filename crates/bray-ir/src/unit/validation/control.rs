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
                    return Err(MirUnitBuildError::DuplicateSwitchCase(block_id));
                }

                validate_local_edge(unit, block_id, case.edge())?;
            }

            validate_local_edge(unit, block_id, otherwise)?;
        }
        MirTerminatorKind::Return(value) => {
            if let Some(value) = value {
                validate_operand(unit, value, block_id, None)?;
            }
        }
        MirTerminatorKind::Unreachable => {}
        MirTerminatorKind::Suspend {
            resume_state,
            resume,
            cancellation,
            registration,
            wake,
        } => {
            validate_frame_state(unit, *resume_state)?;
            validate_ordinary_edge(unit, block_id, resume)?;

            let Some(descriptor) = unit.frame_descriptor() else {
                return Err(MirUnitBuildError::MissingFrameState);
            };

            let expected = descriptor
                .states()
                .iter()
                .find(|state| state.state() == *resume_state)
                .map(crate::MirFrameStateFacts::entry);

            if expected != Some(resume.target()) {
                return Err(MirUnitBuildError::InvalidFrameStateEntry(resume.target()));
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
                | MirTerminatorKind::ContinueCleanup(_)
        )
    {
        return Err(MirUnitBuildError::CleanupPhaseOrderViolation(block_id));
    }

    Ok(())
}

fn validate_iteration_item(
    unit: &MirUnit,
    item: MirBlockId,
    element_type: bray_symbols::TypeId,
) -> Result<(), MirUnitBuildError> {
    let Some(block) = unit.block(item) else {
        return Err(missing_or_foreign_block(unit, item));
    };

    let [parameter] = block.parameters() else {
        return Err(MirUnitBuildError::EdgeArgumentCountMismatch(item));
    };

    let Some(parameter) = unit.value(*parameter) else {
        return Err(MirUnitBuildError::MissingValue(*parameter));
    };

    if block.kind() != MirBlockKind::Ordinary || parameter.ty() != element_type {
        return Err(MirUnitBuildError::EdgeArgumentTypeMismatch(item));
    }

    Ok(())
}

fn validate_local_edge(
    unit: &MirUnit,
    source: MirBlockId,
    edge: &MirEdge,
) -> Result<(), MirUnitBuildError> {
    validate_edge(unit, source, edge)?;

    let source_kind = unit
        .block(source)
        .map(MirBlock::kind)
        .ok_or_else(|| missing_or_foreign_block(unit, source))?;

    if unit.block(edge.target()).map(MirBlock::kind) != Some(source_kind) {
        return Err(MirUnitBuildError::CleanupPhaseOrderViolation(edge.target()));
    }

    Ok(())
}

fn validate_goto_edge(
    unit: &MirUnit,
    source: MirBlockId,
    edge: &MirEdge,
) -> Result<(), MirUnitBuildError> {
    validate_edge(unit, source, edge)?;

    let source_kind = unit
        .block(source)
        .map(MirBlock::kind)
        .ok_or_else(|| missing_or_foreign_block(unit, source))?;

    let target_kind = unit.block(edge.target()).map(MirBlock::kind);

    if target_kind != Some(source_kind)
        && !(source_kind == MirBlockKind::LifecycleResolution
            && target_kind == Some(MirBlockKind::Ordinary))
    {
        return Err(MirUnitBuildError::CleanupPhaseOrderViolation(edge.target()));
    }

    Ok(())
}

fn validate_ordinary_edge(
    unit: &MirUnit,
    source: MirBlockId,
    edge: &MirEdge,
) -> Result<(), MirUnitBuildError> {
    validate_edge(unit, source, edge)?;

    if unit.block(edge.target()).map(MirBlock::kind) != Some(MirBlockKind::Ordinary) {
        return Err(MirUnitBuildError::CleanupPhaseOrderViolation(edge.target()));
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

    validate_cleanup_edge(unit, block, cleanup)
}

fn validate_cleanup_edge(
    unit: &MirUnit,
    source: MirBlockId,
    cleanup: &MirCleanupEdge,
) -> Result<(), MirUnitBuildError> {
    validate_edge(unit, source, cleanup.edge())?;

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

fn validate_edge(
    unit: &MirUnit,
    source: MirBlockId,
    edge: &MirEdge,
) -> Result<(), MirUnitBuildError> {
    let Some(target) = unit.block(edge.target()) else {
        return Err(missing_or_foreign_block(unit, edge.target()));
    };

    if edge.arguments().len() != target.parameters().len() {
        return Err(MirUnitBuildError::EdgeArgumentCountMismatch(edge.target()));
    }

    for (argument, parameter) in edge.arguments().iter().zip(target.parameters()) {
        validate_operand(unit, argument, source, None)?;

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
