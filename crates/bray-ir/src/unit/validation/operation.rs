use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::TypeId;

use crate::{
    MirCallTarget, MirExecutionOperation, MirOperand, MirOperation, MirOperationId,
    MirOperationKind, MirPlace, MirProjection, MirStorageId, MirTaskTerminalState, MirUnit,
    MirUnitBuildError, MirValueId,
};

use super::core::{validate_frame_state, validate_runtime_role};

pub(super) fn validate_operation(
    unit: &MirUnit,
    id: MirOperationId,
    operation: &MirOperation,
) -> Result<(), MirUnitBuildError> {
    validate_operation_result(unit, id, operation)?;

    match operation.kind() {
        MirOperationKind::Store { destination, value } => {
            validate_place(unit, destination)?;
            validate_operand(unit, value)?;
        }
        MirOperationKind::Borrow { place, .. }
        | MirOperationKind::Finalize(place)
        | MirOperationKind::Destroy(place) => validate_place(unit, place)?,
        MirOperationKind::Unary { operand, .. } | MirOperationKind::Convert { operand, .. } => {
            validate_operand(unit, operand)?;
        }
        MirOperationKind::Binary { left, right, .. } => {
            validate_operand(unit, left)?;
            validate_operand(unit, right)?;
        }
        MirOperationKind::Call(call) => {
            match call.target() {
                MirCallTarget::Direct(_) | MirCallTarget::Runtime(_) => {}
                MirCallTarget::Indirect(value) => validate_value(unit, *value)?,
            }

            for argument in call.arguments() {
                validate_operand(unit, argument)?;
            }
        }
        MirOperationKind::Execution(operation) => validate_execution_operation(unit, operation)?,
    }

    Ok(())
}

fn validate_operation_result(
    unit: &MirUnit,
    id: MirOperationId,
    operation: &MirOperation,
) -> Result<(), MirUnitBuildError> {
    let requires_result = matches!(
        operation.kind(),
        MirOperationKind::Borrow { .. }
            | MirOperationKind::Unary { .. }
            | MirOperationKind::Binary { .. }
            | MirOperationKind::Convert { .. }
            | MirOperationKind::Execution(
                MirExecutionOperation::ObserveCurrentRunCancellation { .. }
                    | MirExecutionOperation::ResolveTask { .. }
            )
    );
    let rejects_result = matches!(
        operation.kind(),
        MirOperationKind::Store { .. }
            | MirOperationKind::Finalize(_)
            | MirOperationKind::Destroy(_)
            | MirOperationKind::Execution(
                MirExecutionOperation::CreateFrame { .. }
                    | MirExecutionOperation::MoveInactiveFrame { .. }
                    | MirExecutionOperation::ResumeFrame { .. }
                    | MirExecutionOperation::ComposeAwaitedFrame { .. }
                    | MirExecutionOperation::CommitAwaitedCompletion { .. }
                    | MirExecutionOperation::StartTask { .. }
                    | MirExecutionOperation::RequestTaskCancellation { .. }
                    | MirExecutionOperation::PublishTerminalState { .. }
                    | MirExecutionOperation::ExecuteCleanupBroadcast { .. }
                    | MirExecutionOperation::ExecuteLifecycleResolution { .. }
                    | MirExecutionOperation::TransferCleanupIncident { .. }
                    | MirExecutionOperation::DestroyTerminalTask { .. }
            )
    );

    if requires_result && operation.result().is_none() {
        return Err(MirUnitBuildError::MissingOperationResult(id));
    }

    if rejects_result && operation.result().is_some() {
        return Err(MirUnitBuildError::UnexpectedOperationResult(id));
    }

    if let (MirOperationKind::Convert { target, .. }, Some(result)) =
        (operation.kind(), operation.result())
    {
        let Some(result) = unit.value(result) else {
            return Err(MirUnitBuildError::MissingValue(result));
        };

        if result.ty() != *target {
            return Err(MirUnitBuildError::OperationResultTypeMismatch(id));
        }
    }

    Ok(())
}

fn validate_execution_operation(
    unit: &MirUnit,
    operation: &MirExecutionOperation,
) -> Result<(), MirUnitBuildError> {
    match operation {
        MirExecutionOperation::CreateFrame { destination, .. } => {
            validate_place(unit, destination)?;
        }
        MirExecutionOperation::MoveInactiveFrame {
            source,
            destination,
            ..
        } => {
            validate_place(unit, source)?;
            validate_place(unit, destination)?;
        }
        MirExecutionOperation::ResumeFrame {
            frame,
            state,
            storage,
            runtime,
        } => {
            validate_current_frame(unit, *frame)?;
            validate_frame_state(unit, *state)?;
            validate_storage(unit, *storage)?;
            validate_runtime_role(*runtime, RuntimeAbiRole::FrameResume)?;
        }
        MirExecutionOperation::ComposeAwaitedFrame { frame, .. } => {
            validate_operand(unit, frame)?;
        }
        MirExecutionOperation::CommitAwaitedCompletion {
            value, destination, ..
        } => {
            validate_operand(unit, value)?;
            validate_place(unit, destination)?;
        }
        MirExecutionOperation::StartTask {
            value,
            task,
            allocation,
            start,
            ..
        } => {
            validate_operand(unit, value)?;
            validate_storage(unit, *task)?;
            validate_runtime_role(*allocation, RuntimeAbiRole::TaskAllocation)?;
            validate_runtime_role(*start, RuntimeAbiRole::TaskStart)?;
        }
        MirExecutionOperation::RequestTaskCancellation { task, runtime } => {
            validate_storage(unit, *task)?;
            validate_runtime_role(*runtime, RuntimeAbiRole::TaskCancellationRequest)?;
        }
        MirExecutionOperation::ObserveCurrentRunCancellation { runtime } => {
            validate_runtime_role(*runtime, RuntimeAbiRole::CurrentRunCancellationObservation)?;
        }
        MirExecutionOperation::ResolveTask { task, runtime } => {
            validate_storage(unit, *task)?;
            validate_runtime_role(*runtime, RuntimeAbiRole::JoinRegistration)?;
        }
        MirExecutionOperation::PublishTerminalState {
            task,
            state,
            runtime,
        } => {
            validate_storage(unit, *task)?;
            validate_terminal_state(unit, state)?;
            validate_runtime_role(*runtime, RuntimeAbiRole::TerminalPublication)?;
        }
        MirExecutionOperation::ExecuteCleanupBroadcast { frame, runtime } => {
            validate_current_frame(unit, *frame)?;
            validate_runtime_role(*runtime, RuntimeAbiRole::FrameTaskBroadcast)?;
        }
        MirExecutionOperation::ExecuteLifecycleResolution { frame, runtime } => {
            validate_current_frame(unit, *frame)?;
            validate_runtime_role(*runtime, RuntimeAbiRole::FrameLifecycleResolution)?;
        }
        MirExecutionOperation::TransferCleanupIncident { incident, runtime } => {
            validate_operand(unit, incident)?;
            validate_runtime_role(*runtime, RuntimeAbiRole::CleanupIncidentTransfer)?;
        }
        MirExecutionOperation::DestroyTerminalTask { task } => {
            validate_storage(unit, *task)?;
        }
    }

    Ok(())
}

fn validate_terminal_state(
    unit: &MirUnit,
    state: &MirTaskTerminalState,
) -> Result<(), MirUnitBuildError> {
    match state {
        MirTaskTerminalState::Completed(value) | MirTaskTerminalState::Panicked(value) => {
            validate_operand(unit, value)
        }
        MirTaskTerminalState::Cancelled => Ok(()),
    }
}

pub(super) fn validate_operand(
    unit: &MirUnit,
    operand: &MirOperand,
) -> Result<(), MirUnitBuildError> {
    match operand {
        MirOperand::Value(value) => validate_value(unit, *value),
        MirOperand::Constant { .. } => Ok(()),
        MirOperand::Copy(place) | MirOperand::Move(place) => validate_place(unit, place),
    }
}

pub(super) fn operand_type(
    unit: &MirUnit,
    operand: &MirOperand,
) -> Result<TypeId, MirUnitBuildError> {
    match operand {
        MirOperand::Value(value) => {
            let Some(value) = unit.value(*value) else {
                return Err(missing_or_foreign_value(unit, *value));
            };

            Ok(value.ty())
        }
        MirOperand::Constant { ty, .. } => Ok(*ty),
        MirOperand::Copy(place) | MirOperand::Move(place) => Ok(place.ty()),
    }
}

fn validate_place(unit: &MirUnit, place: &MirPlace) -> Result<(), MirUnitBuildError> {
    validate_storage(unit, place.storage())?;

    for projection in place.projections() {
        match projection {
            MirProjection::Index(value) => validate_value(unit, *value)?,
            MirProjection::Slice { start, end } => {
                if let Some(value) = start {
                    validate_value(unit, *value)?;
                }

                if let Some(value) = end {
                    validate_value(unit, *value)?;
                }
            }
            MirProjection::Dereference
            | MirProjection::Field(_)
            | MirProjection::TupleField(_)
            | MirProjection::Variant(_) => {}
        }
    }

    Ok(())
}

fn validate_current_frame(
    unit: &MirUnit,
    frame: bray_runtime_interface::ProtectedAsyncFrameId,
) -> Result<(), MirUnitBuildError> {
    if unit.execution().protected_frame() != Some(frame) {
        return Err(MirUnitBuildError::ProtectedFrameMismatch);
    }

    Ok(())
}

fn validate_storage(unit: &MirUnit, storage: MirStorageId) -> Result<(), MirUnitBuildError> {
    if unit.storage(storage).is_none() {
        return Err(if storage.unit() == unit.unit() {
            MirUnitBuildError::MissingStorage(storage)
        } else {
            MirUnitBuildError::ForeignStorage(storage)
        });
    }

    Ok(())
}

pub(super) fn validate_value(unit: &MirUnit, value: MirValueId) -> Result<(), MirUnitBuildError> {
    if unit.value(value).is_none() {
        return Err(missing_or_foreign_value(unit, value));
    }

    Ok(())
}

fn missing_or_foreign_value(unit: &MirUnit, value: MirValueId) -> MirUnitBuildError {
    if value.unit() == unit.unit() {
        MirUnitBuildError::MissingValue(value)
    } else {
        MirUnitBuildError::ForeignValue(value)
    }
}
