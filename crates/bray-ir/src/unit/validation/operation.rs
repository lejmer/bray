use std::collections::BTreeSet;

use bray_bound_tree::BoundUnitKind;
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::{AnySymbolId, TypeId};

use crate::{
    MirAggregateKind, MirAsyncOperation, MirBlockKind, MirCallArgument, MirCallTarget,
    MirConstructionInput, MirGeneratorOperation, MirHostOperation, MirOperand, MirOperation,
    MirOperationId, MirOperationKind, MirPlace, MirProjectionKind, MirStorage, MirStorageId,
    MirStorageKind, MirTaskTerminalState, MirUnit, MirUnitBuildError, MirValueId,
};

use super::core::{validate_frame_state, validate_runtime_role};

pub(super) fn validate_operation(
    unit: &MirUnit,
    block: crate::MirBlockId,
    block_kind: MirBlockKind,
    id: MirOperationId,
    operation: &MirOperation,
) -> Result<(), MirUnitBuildError> {
    validate_operation_result(unit, id, operation)?;
    validate_operation_block(block_kind, id, operation.kind())?;

    match operation.kind() {
        MirOperationKind::AnonymousCallable(key) => {
            if key.kind() != BoundUnitKind::AnonymousCallable {
                return Err(MirUnitBuildError::InvalidAnonymousCallable(id));
            }
        }
        MirOperationKind::Store {
            destination, value, ..
        } => {
            validate_place(unit, destination, block, Some(id))?;
            validate_operand(unit, value, block, Some(id))?;

            if operand_type(unit, value)? != destination.ty() {
                return Err(MirUnitBuildError::StorageTypeMismatch(
                    destination.storage(),
                ));
            }
        }
        MirOperationKind::Borrow { place, .. }
        | MirOperationKind::Finalize(place)
        | MirOperationKind::Destroy(place)
        | MirOperationKind::Cleanup { place, .. } => {
            validate_place(unit, place, block, Some(id))?;
        }
        MirOperationKind::Unary { operand, .. } | MirOperationKind::Convert { operand, .. } => {
            validate_operand(unit, operand, block, Some(id))?;
        }
        MirOperationKind::Binary { left, right, .. } => {
            validate_operand(unit, left, block, Some(id))?;
            validate_operand(unit, right, block, Some(id))?;
        }
        MirOperationKind::PatternProjection { subject, .. } => {
            validate_operand(unit, subject, block, Some(id))?;
        }
        MirOperationKind::PanicReport(cause) => match cause {
            crate::MirPanicCause::Message(message) => {
                validate_operand(unit, message, block, Some(id))?;
            }
            crate::MirPanicCause::Assertion(message) => {
                if let Some(message) = message {
                    validate_operand(unit, message, block, Some(id))?;
                }
            }
        },
        MirOperationKind::Generator(operation) => {
            validate_generator_operation(unit, block, id, operation)?;
        }
        MirOperationKind::Aggregate(aggregate) => {
            if aggregate.kind() == MirAggregateKind::RepeatedArray
                && aggregate.operands().len() != 2
            {
                return Err(MirUnitBuildError::InvalidAggregateOperation(id));
            }

            for operand in aggregate.operands() {
                validate_operand(unit, operand, block, Some(id))?;
            }
        }
        MirOperationKind::Construct(construction) => {
            validate_construction(unit, block, id, construction)?;
        }
        MirOperationKind::Call(call) => {
            validate_call(unit, block, id, call)?;
        }
        MirOperationKind::Async(operation) => {
            validate_async_operation(unit, block, id, operation)?;
        }
        MirOperationKind::Host(operation) => {
            validate_host_operation(unit, id, operation)?;
        }
    }

    Ok(())
}

fn validate_operation_block(
    block_kind: MirBlockKind,
    operation: MirOperationId,
    kind: &MirOperationKind,
) -> Result<(), MirUnitBuildError> {
    let valid = match kind {
        MirOperationKind::Async(MirAsyncOperation::ExecuteCleanupBroadcast { .. }) => {
            block_kind == MirBlockKind::CleanupBroadcast
        }
        MirOperationKind::Async(MirAsyncOperation::ExecuteLifecycleResolution { .. }) => {
            block_kind == MirBlockKind::LifecycleResolution
        }
        MirOperationKind::Async(MirAsyncOperation::RequestTaskCancellation { .. }) => {
            block_kind != MirBlockKind::LifecycleResolution
        }
        MirOperationKind::Async(
            MirAsyncOperation::ResolveTask { .. }
            | MirAsyncOperation::TransferCleanupIncident { .. }
            | MirAsyncOperation::DestroyTerminalTask { .. },
        ) => block_kind != MirBlockKind::CleanupBroadcast,
        MirOperationKind::Async(_) => block_kind == MirBlockKind::Ordinary,
        MirOperationKind::Host(_) => block_kind == MirBlockKind::Ordinary,
        MirOperationKind::Finalize(_) | MirOperationKind::Destroy(_) => {
            block_kind != MirBlockKind::CleanupBroadcast
        }
        MirOperationKind::Cleanup { phase, .. } => match phase {
            crate::MirCleanupPhase::TaskCancellation => {
                block_kind == MirBlockKind::CleanupBroadcast
            }
            crate::MirCleanupPhase::LifecycleResolution => {
                block_kind == MirBlockKind::LifecycleResolution
            }
        },
        MirOperationKind::AnonymousCallable(_)
        | MirOperationKind::Store { .. }
        | MirOperationKind::Borrow { .. }
        | MirOperationKind::Unary { .. }
        | MirOperationKind::Binary { .. }
        | MirOperationKind::PatternProjection { .. }
        | MirOperationKind::Generator(_)
        | MirOperationKind::Aggregate(_)
        | MirOperationKind::Construct(_)
        | MirOperationKind::Convert { .. }
        | MirOperationKind::Call(_)
        | MirOperationKind::PanicReport(_) => true,
    };

    if !valid {
        return Err(MirUnitBuildError::InvalidOperationBlock(operation));
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
        MirOperationKind::AnonymousCallable(_)
            | MirOperationKind::Borrow { .. }
            | MirOperationKind::Unary { .. }
            | MirOperationKind::Binary { .. }
            | MirOperationKind::PatternProjection { .. }
            | MirOperationKind::Generator(MirGeneratorOperation::Finish { .. })
            | MirOperationKind::Aggregate(_)
            | MirOperationKind::Construct(_)
            | MirOperationKind::Convert { .. }
            | MirOperationKind::Call(_)
            | MirOperationKind::PanicReport(_)
            | MirOperationKind::Async(
                MirAsyncOperation::CreateFrame { .. }
                    | MirAsyncOperation::CommitAwaitedCompletion { .. }
                    | MirAsyncOperation::StartTask { .. }
                    | MirAsyncOperation::ObserveCurrentRunCancellation { .. }
                    | MirAsyncOperation::ResolveTask { .. }
            )
    );

    let rejects_result = matches!(
        operation.kind(),
        MirOperationKind::Store { .. }
            | MirOperationKind::Generator(
                MirGeneratorOperation::Begin { .. } | MirGeneratorOperation::Push { .. }
            )
            | MirOperationKind::Finalize(_)
            | MirOperationKind::Destroy(_)
            | MirOperationKind::Cleanup { .. }
            | MirOperationKind::Host(_)
            | MirOperationKind::Async(
                MirAsyncOperation::MoveInactiveFrame { .. }
                    | MirAsyncOperation::ResumeFrame { .. }
                    | MirAsyncOperation::ComposeAwaitedFrame { .. }
                    | MirAsyncOperation::RequestTaskCancellation { .. }
                    | MirAsyncOperation::PublishTerminalState { .. }
                    | MirAsyncOperation::ExecuteCleanupBroadcast { .. }
                    | MirAsyncOperation::ExecuteLifecycleResolution { .. }
                    | MirAsyncOperation::TransferCleanupIncident { .. }
                    | MirAsyncOperation::DestroyTerminalTask { .. }
            )
    );

    if requires_result && operation.result().is_none() {
        return Err(MirUnitBuildError::MissingOperationResult(id));
    }

    if rejects_result && operation.result().is_some() {
        return Err(MirUnitBuildError::UnexpectedOperationResult(id));
    }

    if let (
        MirOperationKind::Convert {
            operand,
            conversion,
        },
        Some(result),
    ) = (operation.kind(), operation.result())
    {
        let Some(result) = unit.value(result) else {
            return Err(MirUnitBuildError::MissingValue(result));
        };

        if operand_type(unit, operand)? != conversion.source_type()
            || result.ty() != conversion.target_type()
        {
            return Err(MirUnitBuildError::OperationResultTypeMismatch(id));
        }
    }

    if let (MirOperationKind::Call(call), Some(result)) = (operation.kind(), operation.result()) {
        let Some(result) = unit.value(result) else {
            return Err(MirUnitBuildError::MissingValue(result));
        };

        if result.ty() != call.result().ty() {
            return Err(MirUnitBuildError::OperationResultTypeMismatch(id));
        }
    }

    if let (
        MirOperationKind::Async(MirAsyncOperation::CreateFrame { initializer, .. }),
        Some(result),
    ) = (operation.kind(), operation.result())
    {
        let Some(result) = unit.value(result) else {
            return Err(MirUnitBuildError::MissingValue(result));
        };

        if initializer.future_type() != Some(result.ty()) {
            return Err(MirUnitBuildError::OperationResultTypeMismatch(id));
        }
    }

    Ok(())
}

fn validate_call(
    unit: &MirUnit,
    block: crate::MirBlockId,
    operation: MirOperationId,
    call: &crate::MirCall,
) -> Result<(), MirUnitBuildError> {
    match call.target() {
        MirCallTarget::Direct(reference) => {
            if let Some(contract) = call
                .contract()
                .and_then(bray_symbols::CallableContractTemplate::source_template)
                && contract.owner() != reference.instance().definition().callable_symbol()
            {
                return Err(MirUnitBuildError::InvalidCall(operation));
            }
        }
        MirCallTarget::Indirect { callee, .. } => {
            validate_operand(unit, callee, block, Some(operation))?;
        }
    }

    match (call.result(), call.phase_behaviors()) {
        (bray_bound_tree::BoundCallResult::Immediate(_), Some(behaviors))
            if behaviors.deferred_execution().is_some() =>
        {
            return Err(MirUnitBuildError::InvalidCall(operation));
        }
        (bray_bound_tree::BoundCallResult::LazyFuture(_), Some(behaviors))
            if behaviors.deferred_execution().is_none() =>
        {
            return Err(MirUnitBuildError::InvalidCall(operation));
        }
        (
            bray_bound_tree::BoundCallResult::Immediate(_)
            | bray_bound_tree::BoundCallResult::LazyFuture(_),
            _,
        ) => {}
    }

    let mut parameters = BTreeSet::new();
    let mut ordinals = BTreeSet::new();
    let mut saw_receiver = false;
    let mut saw_default = false;
    let mut last_default_ordinal = None;

    for argument in call.arguments() {
        match argument {
            MirCallArgument::Receiver { parameter, value } => {
                if saw_receiver || !parameters.is_empty() || saw_default {
                    return Err(MirUnitBuildError::InvalidCall(operation));
                }

                saw_receiver = true;
                validate_operand(unit, value, block, Some(operation))?;

                if !parameters.insert(AnySymbolId::from(*parameter)) {
                    return Err(MirUnitBuildError::InvalidCall(operation));
                }
            }
            MirCallArgument::Explicit {
                parameter,
                ordinal,
                value,
            } => {
                if saw_default || !ordinals.insert(*ordinal) {
                    return Err(MirUnitBuildError::InvalidCall(operation));
                }

                validate_operand(unit, value, block, Some(operation))?;

                if let Some(parameter) = parameter
                    && !parameters.insert(AnySymbolId::from(*parameter))
                {
                    return Err(MirUnitBuildError::InvalidCall(operation));
                }
            }
            MirCallArgument::Default {
                parameter, ordinal, ..
            } => {
                if !parameters.insert(AnySymbolId::from(*parameter))
                    || !ordinals.insert(*ordinal)
                    || last_default_ordinal.is_some_and(|last| last >= *ordinal)
                {
                    return Err(MirUnitBuildError::InvalidCall(operation));
                }

                saw_default = true;
                last_default_ordinal = Some(*ordinal);
            }
        }
    }

    Ok(())
}

fn validate_generator_operation(
    unit: &MirUnit,
    block: crate::MirBlockId,
    operation: MirOperationId,
    generator: &MirGeneratorOperation,
) -> Result<(), MirUnitBuildError> {
    let destination = match generator {
        MirGeneratorOperation::Begin { destination, .. }
        | MirGeneratorOperation::Finish { destination } => destination,
        MirGeneratorOperation::Push { destination, value } => {
            validate_operand(unit, value, block, Some(operation))?;

            destination
        }
    };

    validate_place(unit, destination, block, Some(operation))
}

fn validate_construction(
    unit: &MirUnit,
    block: crate::MirBlockId,
    operation: MirOperationId,
    construction: &crate::MirConstruction,
) -> Result<(), MirUnitBuildError> {
    let mut inputs = BTreeSet::new();
    let mut ordinals = BTreeSet::new();
    let mut last_default_ordinal = None;

    for supplied in construction.inputs() {
        let input = supplied.input();

        if !construction.target().accepts_input(input)
            || !inputs.insert(input)
            || !ordinals.insert(supplied.ordinal())
        {
            return Err(MirUnitBuildError::InvalidConstructionInput(operation));
        }

        match supplied {
            MirConstructionInput::Explicit { value, .. } => {
                if last_default_ordinal.is_some() {
                    return Err(MirUnitBuildError::InvalidConstructionInput(operation));
                }

                validate_operand(unit, value, block, Some(operation))?;
            }
            MirConstructionInput::Default {
                input,
                ordinal,
                provider,
            } => {
                if !input.accepts_default(*provider)
                    || last_default_ordinal.is_some_and(|last| last >= *ordinal)
                {
                    return Err(MirUnitBuildError::InvalidConstructionInput(operation));
                }

                last_default_ordinal = Some(*ordinal);
            }
        }
    }

    Ok(())
}

fn validate_async_operation(
    unit: &MirUnit,
    block: crate::MirBlockId,
    operation_id: MirOperationId,
    operation: &MirAsyncOperation,
) -> Result<(), MirUnitBuildError> {
    match operation {
        MirAsyncOperation::CreateFrame { initializer, .. } => {
            validate_frame_initializer(unit, block, operation_id, initializer)?;
        }
        MirAsyncOperation::MoveInactiveFrame {
            source,
            destination,
            ..
        } => {
            validate_place(unit, source, block, Some(operation_id))?;
            validate_place(unit, destination, block, Some(operation_id))?;

            validate_place_storage_kind(unit, source, MirStorageKind::InactiveFrame)?;
            validate_place_storage_kind(unit, destination, MirStorageKind::InactiveFrame)?;
        }
        MirAsyncOperation::ResumeFrame {
            frame,
            state,
            storage,
            runtime,
        } => {
            validate_current_frame(unit, *frame)?;
            validate_frame_state(unit, *state)?;
            validate_storage_kind(unit, *storage, MirStorageKind::CurrentFrame)?;
            validate_runtime_role(unit, *runtime, RuntimeAbiRole::FrameResume)?;
        }
        MirAsyncOperation::ComposeAwaitedFrame { frame, .. } => {
            validate_operand(unit, frame, block, Some(operation_id))?;
        }
        MirAsyncOperation::CommitAwaitedCompletion { .. } => {}
        MirAsyncOperation::StartTask {
            value,
            allocation,
            start,
            ..
        } => {
            validate_operand(unit, value, block, Some(operation_id))?;

            validate_runtime_role(unit, *allocation, RuntimeAbiRole::TaskAllocation)?;
            validate_runtime_role(unit, *start, RuntimeAbiRole::TaskStart)?;
        }
        MirAsyncOperation::RequestTaskCancellation { task, runtime } => {
            validate_operand(unit, task, block, Some(operation_id))?;
            validate_runtime_role(unit, *runtime, RuntimeAbiRole::TaskCancellationRequest)?;
        }
        MirAsyncOperation::ObserveCurrentRunCancellation { runtime } => {
            validate_runtime_role(
                unit,
                *runtime,
                RuntimeAbiRole::CurrentRunCancellationObservation,
            )?;
        }
        MirAsyncOperation::ResolveTask { task, runtime } => {
            validate_operand(unit, task, block, Some(operation_id))?;
            validate_runtime_role(unit, *runtime, RuntimeAbiRole::JoinRegistration)?;
        }
        MirAsyncOperation::PublishTerminalState { state, runtime } => {
            validate_terminal_state(unit, block, operation_id, state)?;
            validate_runtime_role(unit, *runtime, RuntimeAbiRole::TerminalPublication)?;
        }
        MirAsyncOperation::ExecuteCleanupBroadcast { frame, runtime } => {
            validate_current_frame(unit, *frame)?;
            validate_runtime_role(unit, *runtime, RuntimeAbiRole::FrameTaskBroadcast)?;
        }
        MirAsyncOperation::ExecuteLifecycleResolution { frame, runtime } => {
            validate_current_frame(unit, *frame)?;
            validate_runtime_role(unit, *runtime, RuntimeAbiRole::FrameLifecycleResolution)?;
        }
        MirAsyncOperation::TransferCleanupIncident { incident, runtime } => {
            validate_operand(unit, incident, block, Some(operation_id))?;
            validate_runtime_role(unit, *runtime, RuntimeAbiRole::CleanupIncidentTransfer)?;
        }
        MirAsyncOperation::DestroyTerminalTask { task } => {
            validate_operand(unit, task, block, Some(operation_id))?;
        }
    }

    Ok(())
}

fn validate_frame_initializer(
    unit: &MirUnit,
    block: crate::MirBlockId,
    operation: MirOperationId,
    initializer: &crate::MirFrameInitializer,
) -> Result<(), MirUnitBuildError> {
    match initializer {
        crate::MirFrameInitializer::Callable(call) => validate_call(unit, block, operation, call),
        crate::MirFrameInitializer::TaskObservation { task, .. } => {
            validate_operand(unit, task, block, Some(operation))
        }
    }
}

fn validate_host_operation(
    unit: &MirUnit,
    operation: MirOperationId,
    host_operation: &MirHostOperation,
) -> Result<(), MirUnitBuildError> {
    let crate::MirUnitKind::ExecutableHost(host) = unit.kind() else {
        return Err(MirUnitBuildError::InvalidHostOperation(operation));
    };

    match host_operation {
        MirHostOperation::ExecuteRoot {
            root,
            execution,
            runtime,
        } => {
            if root.kind() != BoundUnitKind::CallableBody || *execution != host.root() {
                return Err(MirUnitBuildError::InvalidHostOperation(operation));
            }

            validate_runtime_role(unit, *runtime, RuntimeAbiRole::RootExecution)
        }
        MirHostOperation::ObserveRootTerminal { runtime } => {
            validate_runtime_role(unit, *runtime, RuntimeAbiRole::RootTerminalObservation)
        }
        MirHostOperation::ReportCleanupIncidents { runtime } => {
            validate_runtime_role(unit, *runtime, RuntimeAbiRole::CleanupIncidentReporting)
        }
        MirHostOperation::StructuredShutdown { runtime } => {
            validate_runtime_role(unit, *runtime, RuntimeAbiRole::StructuredShutdown)
        }
    }
}

fn validate_terminal_state(
    unit: &MirUnit,
    block: crate::MirBlockId,
    operation: MirOperationId,
    state: &MirTaskTerminalState,
) -> Result<(), MirUnitBuildError> {
    match state {
        MirTaskTerminalState::Completed(value) | MirTaskTerminalState::Panicked(value) => {
            validate_operand(unit, value, block, Some(operation))
        }
        MirTaskTerminalState::Cancelled => Ok(()),
    }
}

pub(super) fn validate_operand(
    unit: &MirUnit,
    operand: &MirOperand,
    block: crate::MirBlockId,
    before: Option<MirOperationId>,
) -> Result<(), MirUnitBuildError> {
    match operand {
        MirOperand::Value(value) => validate_value_at(unit, *value, block, before),
        MirOperand::Constant { .. } | MirOperand::Immediate { .. } => Ok(()),
        MirOperand::Copy(place) | MirOperand::Move(place) => {
            validate_place(unit, place, block, before)
        }
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
        MirOperand::Constant { ty, .. } | MirOperand::Immediate { ty, .. } => Ok(*ty),
        MirOperand::Copy(place) | MirOperand::Move(place) => Ok(place.ty()),
    }
}

pub(super) fn validate_place(
    unit: &MirUnit,
    place: &MirPlace,
    block: crate::MirBlockId,
    before: Option<MirOperationId>,
) -> Result<(), MirUnitBuildError> {
    validate_storage(unit, place.storage())?;

    let Some(storage) = unit.storage(place.storage()) else {
        return Err(MirUnitBuildError::MissingStorage(place.storage()));
    };

    let mut expected_type = storage.ty();

    for projection in place.projections() {
        if projection.source_type() != expected_type {
            return Err(MirUnitBuildError::StorageTypeMismatch(place.storage()));
        }

        match projection.kind() {
            MirProjectionKind::Index(value) => validate_operand(unit, value, block, before)?,
            MirProjectionKind::Slice { start, end } => {
                if let Some(value) = start {
                    validate_operand(unit, value, block, before)?;
                }

                if let Some(value) = end {
                    validate_operand(unit, value, block, before)?;
                }
            }
            MirProjectionKind::Dereference
            | MirProjectionKind::Field(_)
            | MirProjectionKind::TupleField(_)
            | MirProjectionKind::ElementFromStart(_)
            | MirProjectionKind::ElementFromEnd(_)
            | MirProjectionKind::Variant(_)
            | MirProjectionKind::ActiveUnionPayloadField { .. }
            | MirProjectionKind::NullableValue => {}
        }

        expected_type = projection.result_type();
    }

    if expected_type != place.ty() {
        return Err(MirUnitBuildError::StorageTypeMismatch(place.storage()));
    }

    Ok(())
}

fn validate_current_frame(
    unit: &MirUnit,
    frame: bray_runtime_interface::ProtectedAsyncFrameId,
) -> Result<(), MirUnitBuildError> {
    if unit.kind().protected_frame() != Some(frame) {
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

fn validate_storage_kind(
    unit: &MirUnit,
    storage: MirStorageId,
    expected: MirStorageKind,
) -> Result<(), MirUnitBuildError> {
    validate_storage(unit, storage)?;

    if unit.storage(storage).map(MirStorage::kind) != Some(expected) {
        return Err(MirUnitBuildError::StorageKindMismatch(storage));
    }

    Ok(())
}

fn validate_place_storage_kind(
    unit: &MirUnit,
    place: &MirPlace,
    expected: MirStorageKind,
) -> Result<(), MirUnitBuildError> {
    validate_storage_kind(unit, place.storage(), expected)
}

pub(super) fn validate_value(unit: &MirUnit, value: MirValueId) -> Result<(), MirUnitBuildError> {
    if unit.value(value).is_none() {
        return Err(missing_or_foreign_value(unit, value));
    }

    Ok(())
}

fn validate_value_at(
    unit: &MirUnit,
    value: MirValueId,
    block: crate::MirBlockId,
    before: Option<MirOperationId>,
) -> Result<(), MirUnitBuildError> {
    validate_value(unit, value)?;

    let Some(value_data) = unit.value(value) else {
        return Err(MirUnitBuildError::MissingValue(value));
    };

    let valid = match value_data.origin() {
        crate::MirValueOrigin::BlockParameter(owner) => owner == block,
        crate::MirValueOrigin::Operation(operation) => {
            let owner = unit
                .block(block)
                .is_some_and(|block| block.operations().contains(&operation));

            let ordered = before.is_none_or(|before| operation.to_index() < before.to_index());

            owner && ordered
        }
    };

    if !valid {
        return Err(MirUnitBuildError::ValueDoesNotDominateUse(value));
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
