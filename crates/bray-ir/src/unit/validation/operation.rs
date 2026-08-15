// rust-style: allow(module-too-large, reason = "operation validation keeps exhaustive cross-operation invariants auditable together")

use std::collections::BTreeSet;

use bray_bound_tree::BoundUnitKind;
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::{AnySymbolId, TypeId};

use crate::{
    MirAggregateKind, MirAsyncOperation, MirBlockKind, MirCallArgument, MirCallTarget,
    MirConstructionInput, MirGeneratorOperation, MirOperand, MirOperation, MirOperationId,
    MirOperationKind, MirPlace, MirProjectionKind, MirStorage, MirStorageId, MirStorageKind,
    MirTaskTerminalState, MirUnit, MirUnitBuildError, MirValueId,
};

use super::core::{validate_frame_state, validate_runtime_role};
use super::host::validate_host_operation;

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
            if matches!(
                key,
                crate::MirAnonymousCallableReference::Bound(key)
                    if key.kind() != BoundUnitKind::AnonymousCallable
            ) {
                return Err(MirUnitBuildError::InvalidAnonymousCallable(id));
            }
        }
        MirOperationKind::DeclaredCallable(_) => {}
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
        MirOperationKind::Unary { operand, .. }
        | MirOperationKind::Convert { operand, .. }
        | MirOperationKind::NumericConversion { operand, .. } => {
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
            crate::MirPanicCause::Message(message)
            | crate::MirPanicCause::ExplicitTestFailure(message) => {
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
            let valid_arity = match aggregate.kind() {
                MirAggregateKind::Tuple | MirAggregateKind::Array => true,
                MirAggregateKind::RepeatedArray => aggregate.operands().len() == 2,
                MirAggregateKind::NullablePresent => aggregate.operands().len() == 1,
            };

            if !valid_arity {
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
        MirOperationKind::Memory(operation) => {
            super::memory::validate_memory_operation(unit, block, id, operation)?;
        }
        MirOperationKind::Text(operation) => {
            if operation.operands().len() != operation.operand_types().len() {
                return Err(MirUnitBuildError::OperationResultTypeMismatch(id));
            }

            for (operand, ty) in operation.operands().iter().zip(operation.operand_types()) {
                validate_operand(unit, operand, block, Some(id))?;

                if operand_type(unit, operand)? != *ty {
                    return Err(MirUnitBuildError::OperationResultTypeMismatch(id));
                }
            }
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
        MirOperationKind::Generator(MirGeneratorOperation::CleanupBroadcast { .. }) => {
            block_kind == MirBlockKind::CleanupBroadcast
        }
        MirOperationKind::Generator(MirGeneratorOperation::Destroy { .. }) => {
            block_kind != MirBlockKind::CleanupBroadcast
        }
        MirOperationKind::AnonymousCallable(_)
        | MirOperationKind::DeclaredCallable(_)
        | MirOperationKind::Store { .. }
        | MirOperationKind::Borrow { .. }
        | MirOperationKind::Unary { .. }
        | MirOperationKind::Binary { .. }
        | MirOperationKind::PatternProjection { .. }
        | MirOperationKind::Generator(
            MirGeneratorOperation::Begin { .. }
            | MirGeneratorOperation::Push { .. }
            | MirGeneratorOperation::Finish { .. },
        )
        | MirOperationKind::Aggregate(_)
        | MirOperationKind::Construct(_)
        | MirOperationKind::Convert { .. }
        | MirOperationKind::NumericConversion { .. }
        | MirOperationKind::Call(_)
        | MirOperationKind::Memory(_)
        | MirOperationKind::Text(_)
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
            | MirOperationKind::DeclaredCallable(_)
            | MirOperationKind::Borrow { .. }
            | MirOperationKind::Unary { .. }
            | MirOperationKind::Binary { .. }
            | MirOperationKind::PatternProjection { .. }
            | MirOperationKind::Generator(MirGeneratorOperation::Finish { .. })
            | MirOperationKind::Aggregate(_)
            | MirOperationKind::Construct(_)
            | MirOperationKind::Convert { .. }
            | MirOperationKind::NumericConversion { .. }
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

    let requires_result = requires_result
        || matches!(operation.kind(), MirOperationKind::Text(text) if text.result_type().is_some());

    let rejects_result = matches!(
        operation.kind(),
        MirOperationKind::Store { .. }
            | MirOperationKind::Generator(
                MirGeneratorOperation::Begin { .. }
                    | MirGeneratorOperation::Push { .. }
                    | MirGeneratorOperation::CleanupBroadcast { .. }
                    | MirGeneratorOperation::Destroy { .. }
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

    let rejects_result = rejects_result
        || matches!(operation.kind(), MirOperationKind::Text(text) if text.result_type().is_none());

    if requires_result && operation.result().is_none() {
        return Err(MirUnitBuildError::MissingOperationResult(id));
    }

    if rejects_result && operation.result().is_some() {
        return Err(MirUnitBuildError::UnexpectedOperationResult(id));
    }

    if let (MirOperationKind::Text(text), Some(result)) = (operation.kind(), operation.result()) {
        let Some(result) = unit.value(result) else {
            return Err(MirUnitBuildError::MissingValue(result));
        };

        if Some(result.ty()) != text.result_type() {
            return Err(MirUnitBuildError::OperationResultTypeMismatch(id));
        }
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

    if let MirOperationKind::Memory(memory) = operation.kind() {
        let produces_value = memory.kind().produces_value();

        if produces_value != operation.result().is_some() {
            return Err(MirUnitBuildError::InvalidMemoryOperation(id));
        }

        if let Some(result) = operation.result() {
            let Some(result) = unit.value(result) else {
                return Err(MirUnitBuildError::MissingValue(result));
            };

            if Some(result.ty()) != memory.result_type() {
                return Err(MirUnitBuildError::OperationResultTypeMismatch(id));
            }
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

#[cfg(test)]
#[expect(
    clippy::items_after_test_module,
    reason = "operation tests stay adjacent to the validation entry points they exercise"
)]
mod tests {
    use bray_bound_tree::{
        CheckedMemoryOperationKind, InlineAssemblyContract, MemoryAddressKind, MemoryCopyKind,
        MemoryLayoutQueryKind, MemoryOffsetUnit, MemoryReadKind,
    };

    use crate::test_support::{test_constant_value, test_other_type, test_type};

    #[test]
    fn memory_operation_shapes_cover_every_explicit_family() {
        let ty = test_type();
        let other = test_other_type();
        let constant = test_constant_value();

        let assembly = InlineAssemblyContract::try_new(
            constant,
            constant,
            constant,
            constant,
            constant,
            [None; bray_bound_tree::MAX_INLINE_ASSEMBLY_OPERANDS],
            0,
            "",
            "",
        )
        .unwrap_or_else(|| panic!("test assembly contract must validate"));

        let cases = [
            (
                CheckedMemoryOperationKind::UninitNew { element: ty },
                0,
                true,
            ),
            (
                CheckedMemoryOperationKind::UninitPointer {
                    kind: MemoryAddressKind::Shared,
                    element: ty,
                },
                1,
                true,
            ),
            (
                CheckedMemoryOperationKind::UninitWrite { element: ty },
                2,
                true,
            ),
            (
                CheckedMemoryOperationKind::UninitAssumeInitialized { element: ty },
                1,
                true,
            ),
            (
                CheckedMemoryOperationKind::UninitMove { element: ty },
                1,
                true,
            ),
            (
                CheckedMemoryOperationKind::BorrowFrom {
                    kind: MemoryAddressKind::Mutable,
                    pointee: ty,
                },
                2,
                true,
            ),
            (
                CheckedMemoryOperationKind::Address {
                    kind: MemoryAddressKind::Shared,
                    pointee: ty,
                },
                1,
                true,
            ),
            (CheckedMemoryOperationKind::Null { pointee: ty }, 0, true),
            (CheckedMemoryOperationKind::IsNull { pointee: ty }, 1, true),
            (
                CheckedMemoryOperationKind::Offset {
                    unit: MemoryOffsetUnit::Element,
                    pointee: ty,
                },
                2,
                true,
            ),
            (
                CheckedMemoryOperationKind::Reinterpret {
                    source: ty,
                    target: other,
                },
                1,
                true,
            ),
            (
                CheckedMemoryOperationKind::Read {
                    pointee: ty,
                    kind: MemoryReadKind::Move,
                },
                1,
                true,
            ),
            (CheckedMemoryOperationKind::Write { pointee: ty }, 2, false),
            (
                CheckedMemoryOperationKind::Copy {
                    pointee: ty,
                    kind: MemoryCopyKind::NonOverlapping,
                },
                3,
                false,
            ),
            (
                CheckedMemoryOperationKind::LayoutQuery {
                    ty,
                    kind: MemoryLayoutQueryKind::Size,
                },
                0,
                true,
            ),
            (
                CheckedMemoryOperationKind::LayoutQuery {
                    ty,
                    kind: MemoryLayoutQueryKind::Layout,
                },
                1,
                true,
            ),
            (CheckedMemoryOperationKind::RawAllocate, 2, true),
            (CheckedMemoryOperationKind::RawDeallocate, 3, false),
            (CheckedMemoryOperationKind::Allocate, 1, true),
            (CheckedMemoryOperationKind::Deallocate, 1, false),
            (CheckedMemoryOperationKind::RawBufferCapacity, 1, true),
            (
                CheckedMemoryOperationKind::RawBufferInitializedCount,
                1,
                true,
            ),
            (CheckedMemoryOperationKind::RawBufferPointer, 1, true),
            (
                CheckedMemoryOperationKind::RawBufferInitializedSlice,
                1,
                true,
            ),
            (
                CheckedMemoryOperationKind::RawBufferInitializedSliceMut,
                1,
                true,
            ),
            (
                CheckedMemoryOperationKind::RawBufferSparePointer { element: ty },
                1,
                true,
            ),
            (
                CheckedMemoryOperationKind::RawBufferRelocate { element: ty },
                2,
                false,
            ),
            (
                CheckedMemoryOperationKind::RawBufferSetInitializedCount,
                2,
                false,
            ),
            (
                CheckedMemoryOperationKind::RawBufferRelease { element: ty },
                1,
                false,
            ),
            (
                CheckedMemoryOperationKind::RawBufferReplace { element: ty },
                2,
                false,
            ),
            (CheckedMemoryOperationKind::ByteBufferFill, 3, false),
            (CheckedMemoryOperationKind::ByteSliceCopy, 2, false),
            (CheckedMemoryOperationKind::ByteBufferRead, 2, true),
            (CheckedMemoryOperationKind::SliceLength, 1, true),
            (
                CheckedMemoryOperationKind::VolatileRead {
                    pointee: ty,
                    address_space: bray_bound_tree::VolatileAddressSpace::Host,
                    kind: MemoryReadKind::Copy,
                },
                1,
                true,
            ),
            (
                CheckedMemoryOperationKind::VolatileWrite {
                    pointee: ty,
                    address_space: bray_bound_tree::VolatileAddressSpace::Host,
                },
                2,
                false,
            ),
            (
                CheckedMemoryOperationKind::ExposeAddress { pointee: ty },
                1,
                true,
            ),
            (
                CheckedMemoryOperationKind::FromExposedAddress { pointee: ty },
                1,
                true,
            ),
            (
                CheckedMemoryOperationKind::CompareAddress {
                    pointee: ty,
                    comparison: bray_bound_tree::PointerAddressComparison::Equal,
                },
                2,
                true,
            ),
            (
                CheckedMemoryOperationKind::Fence {
                    compiler_only: true,
                    order: bray_bound_tree::MemoryOrder::SequentiallyConsistent,
                },
                0,
                false,
            ),
            (CheckedMemoryOperationKind::CatastrophicAbort, 0, false),
            (CheckedMemoryOperationKind::DebuggerTrap, 0, false),
            (
                CheckedMemoryOperationKind::UnreachableTermination,
                0,
                false,
            ),
            (CheckedMemoryOperationKind::SpinLoopHint, 0, false),
            (
                CheckedMemoryOperationKind::TargetFeatureEnabled { feature: constant },
                0,
                true,
            ),
            (
                CheckedMemoryOperationKind::InlineAssembly {
                    inputs: ty,
                    output: Some(ty),
                    labels: None,
                    contract: assembly,
                },
                1,
                true,
            ),
            (
                CheckedMemoryOperationKind::InlineAssembly {
                    inputs: ty,
                    output: None,
                    labels: None,
                    contract: assembly,
                },
                1,
                false,
            ),
        ];

        for (kind, operands, produces_value) in cases {
            assert_eq!(kind.operand_count(), operands);
            assert_eq!(kind.produces_value(), produces_value);
        }
    }
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
        MirGeneratorOperation::CleanupBroadcast {
            destination,
            runtime,
            ..
        } => {
            validate_runtime_role(
                unit,
                *runtime,
                bray_runtime_interface::RuntimeAbiRole::GeneratorCleanupBroadcast,
            )?;

            destination
        }
        MirGeneratorOperation::Destroy {
            destination,
            runtime,
            ..
        } => {
            validate_runtime_role(
                unit,
                *runtime,
                bray_runtime_interface::RuntimeAbiRole::GeneratorDestruction,
            )?;

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
            | MirProjectionKind::NullableValue
            | MirProjectionKind::OwnedStorage => {}
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
