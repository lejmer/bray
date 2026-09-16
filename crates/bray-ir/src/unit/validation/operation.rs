// rust-style: allow(module-too-large, reason = "operation validation keeps exhaustive cross-operation invariants auditable together")

use std::collections::BTreeSet;

use bray_bound_tree::BoundUnitKind;
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::AnySymbolId;

use crate::{
    MirAggregateKind, MirAsyncOperation, MirBlockKind, MirCallArgument, MirCallTarget,
    MirGeneratorOperation, MirOperand, MirOperation, MirOperationId, MirOperationKind, MirPlace,
    MirProjectionKind, MirStorage, MirStorageId, MirStorageKind, MirTaskTerminalState, MirUnit,
    MirValueId,
};

use super::core::{validate_frame_state, validate_runtime_role};
use super::host::validate_host_operation;

pub(super) fn validate_operation(
    unit: &MirUnit,
    block: crate::MirBlockId,
    block_kind: MirBlockKind,
    id: MirOperationId,
    operation: &MirOperation,
) -> Option<()> {
    validate_operation_result(unit, operation)?;
    validate_operation_block(block_kind, operation.kind())?;

    match operation.kind() {
        MirOperationKind::AdmitOutgoing { runtime, .. } => {
            validate_runtime_role(unit, *runtime, RuntimeAbiRole::OutgoingAdmission)?
        }
        MirOperationKind::DischargeOutgoing { runtime, .. } => {
            validate_runtime_role(unit, *runtime, RuntimeAbiRole::OutgoingDischarge)?
        }
        MirOperationKind::AnonymousCallable(key) => {
            if matches!(
                key,
                crate::MirAnonymousCallableReference::Bound(key)
                    if key.kind() != BoundUnitKind::AnonymousCallable
            ) {
                return None;
            }
        }
        MirOperationKind::DeclaredCallable(_) => {}
        MirOperationKind::Store {
            destination, value, ..
        } => {
            validate_place(unit, destination, block, Some(id))?;
            validate_operand(unit, value, block, Some(id))?;

            if unit.operand_type(value)? != destination.ty() {
                return None;
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
        MirOperationKind::NullableQuery(query) => {
            validate_operand(unit, query.operand(), block, Some(id))?;

            if unit.operand_type(query.operand())? != query.operand_type() {
                return None;
            }
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
                MirAggregateKind::Range => aggregate.operands().len() == 2,
                MirAggregateKind::RepeatedArray | MirAggregateKind::NullablePresent => {
                    aggregate.operands().len() == 1
                }
            };

            if !valid_arity {
                return None;
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
                return None;
            }

            for (operand, ty) in operation.operands().iter().zip(operation.operand_types()) {
                validate_operand(unit, operand, block, Some(id))?;

                if unit.operand_type(operand)? != *ty {
                    return None;
                }
            }
        }
        MirOperationKind::Async(operation) => {
            validate_async_operation(unit, block, id, operation)?;
        }
        MirOperationKind::Host(operation) => {
            validate_host_operation(unit, operation)?;
        }
    }

    Some(())
}
fn validate_operation_block(block_kind: MirBlockKind, kind: &MirOperationKind) -> Option<()> {
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
        | MirOperationKind::NullableQuery(_)
        | MirOperationKind::Call(_)
        | MirOperationKind::Memory(_)
        | MirOperationKind::Text(_)
        | MirOperationKind::AdmitOutgoing { .. }
        | MirOperationKind::DischargeOutgoing { .. }
        | MirOperationKind::PanicReport(_) => true,
    };

    if !valid {
        return None;
    }

    Some(())
}
fn validate_operation_result(unit: &MirUnit, operation: &MirOperation) -> Option<()> {
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
            | MirOperationKind::NullableQuery(_)
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
        MirOperationKind::AdmitOutgoing { .. }
            | MirOperationKind::DischargeOutgoing { .. }
            | MirOperationKind::Store { .. }
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
        return None;
    }

    if rejects_result && operation.result().is_some() {
        return None;
    }

    if let (MirOperationKind::Text(text), Some(result)) = (operation.kind(), operation.result()) {
        let Some(result) = unit.value(result) else {
            return None;
        };

        if Some(result.ty()) != text.result_type() {
            return None;
        }
    }

    if let (MirOperationKind::NullableQuery(query), Some(result)) =
        (operation.kind(), operation.result())
    {
        let Some(result) = unit.value(result) else {
            return None;
        };

        if result.ty() != query.result_type() {
            return None;
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
            return None;
        };

        if unit.operand_type(operand)? != conversion.source_type()
            || result.ty() != conversion.target_type()
        {
            return None;
        }
    }

    if let (MirOperationKind::Call(call), Some(result)) = (operation.kind(), operation.result()) {
        let Some(result) = unit.value(result) else {
            return None;
        };

        if result.ty() != call.result().ty() {
            return None;
        }
    }

    if let MirOperationKind::Memory(memory) = operation.kind() {
        let produces_value = memory.kind().produces_value();

        if produces_value != operation.result().is_some() {
            return None;
        }

        if let Some(result) = operation.result() {
            let Some(result) = unit.value(result) else {
                return None;
            };

            if Some(result.ty()) != memory.result_type() {
                return None;
            }
        }
    }

    if let (
        MirOperationKind::Async(MirAsyncOperation::CreateFrame { initializer, .. }),
        Some(result),
    ) = (operation.kind(), operation.result())
    {
        let Some(result) = unit.value(result) else {
            return None;
        };

        if initializer.future_type() != Some(result.ty()) {
            return None;
        }
    }

    Some(())
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
            (CheckedMemoryOperationKind::ByteBufferCopy, 3, false),
            (CheckedMemoryOperationKind::ByteBufferRead, 2, true),
            (CheckedMemoryOperationKind::SequenceLength, 1, true),
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
            (CheckedMemoryOperationKind::UnreachableTermination, 0, false),
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
) -> Option<()> {
    if call.is_cleanup()
        && (!matches!(call.target(), MirCallTarget::Direct(_))
            || !matches!(
                call.result(),
                bray_bound_tree::BoundCallResult::Immediate(_)
            ))
    {
        return None;
    }

    match call.target() {
        MirCallTarget::Direct(reference) => {
            if let Some(contract) = call
                .contract()
                .and_then(bray_symbols::CallableContractTemplate::source_template)
                && contract.owner() != reference.instance().definition().callable_symbol()
            {
                return None;
            }
        }
        MirCallTarget::DefaultValue { owner, provider } => {
            let accepts_default = match owner {
                crate::MirDefaultOwner::Callable(_) => matches!(
                    provider,
                    bray_bound_tree::DefaultValueProvider::CallableParameter(_)
                ),
                crate::MirDefaultOwner::Type { target, .. } => target.accepts_default(*provider),
            };

            if !accepts_default
                || !matches!(
                    call.result(),
                    bray_bound_tree::BoundCallResult::Immediate(_)
                )
            {
                return None;
            }
        }
        MirCallTarget::Runtime(reference) => {
            validate_runtime_role(unit, *reference, reference.role())?;
        }
        MirCallTarget::Indirect { callee, .. } => {
            validate_operand(unit, callee, block, Some(operation))?;
        }
    }

    match (call.result(), call.phase_behaviors()) {
        (bray_bound_tree::BoundCallResult::Immediate(_), Some(behaviors))
            if behaviors.deferred_execution().is_some() =>
        {
            return None;
        }
        (bray_bound_tree::BoundCallResult::LazyFuture(_), Some(behaviors))
            if behaviors.deferred_execution().is_none() =>
        {
            return None;
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

    for argument in call.arguments() {
        match argument {
            MirCallArgument::Receiver { parameter, value } => {
                if saw_receiver || !parameters.is_empty() {
                    return None;
                }

                saw_receiver = true;
                validate_operand(unit, value, block, Some(operation))?;

                if !parameters.insert(AnySymbolId::from(*parameter)) {
                    return None;
                }
            }
            MirCallArgument::Explicit {
                parameter,
                ordinal,
                value,
            } => {
                if !ordinals.insert(*ordinal) {
                    return None;
                }

                validate_operand(unit, value, block, Some(operation))?;

                if let Some(parameter) = parameter
                    && !parameters.insert(AnySymbolId::from(*parameter))
                {
                    return None;
                }
            }
        }
    }

    Some(())
}

fn validate_generator_operation(
    unit: &MirUnit,
    block: crate::MirBlockId,
    operation: MirOperationId,
    generator: &MirGeneratorOperation,
) -> Option<()> {
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
            validate_runtime_role(unit, *runtime, RuntimeAbiRole::GeneratorCleanupBroadcast)?;

            destination
        }
        MirGeneratorOperation::Destroy {
            destination,
            runtime,
            ..
        } => {
            validate_runtime_role(unit, *runtime, RuntimeAbiRole::GeneratorDestruction)?;

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
) -> Option<()> {
    let mut inputs = BTreeSet::new();
    let mut ordinals = BTreeSet::new();

    for supplied in construction.inputs() {
        let input = supplied.input();

        if !construction.target().accepts_input(input)
            || !inputs.insert(input)
            || !ordinals.insert(supplied.ordinal())
        {
            return None;
        }

        validate_operand(unit, supplied.value(), block, Some(operation))?;
    }

    if ordinals
        .iter()
        .enumerate()
        .any(|(expected, actual)| u32::try_from(expected) != Ok(*actual))
    {
        return None;
    }

    Some(())
}

fn validate_async_operation(
    unit: &MirUnit,
    block: crate::MirBlockId,
    operation_id: MirOperationId,
    operation: &MirAsyncOperation,
) -> Option<()> {
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
        MirAsyncOperation::ResolveTask { task, runtime, .. } => {
            validate_operand(unit, task, block, Some(operation_id))?;
            validate_runtime_role(unit, *runtime, RuntimeAbiRole::TaskResolution)?;
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

    Some(())
}

fn validate_frame_initializer(
    unit: &MirUnit,
    block: crate::MirBlockId,
    operation: MirOperationId,
    initializer: &crate::MirFrameInitializer,
) -> Option<()> {
    match initializer {
        crate::MirFrameInitializer::Callable(call) => validate_call(unit, block, operation, call),
        crate::MirFrameInitializer::TaskObservation { task, runtime, .. } => {
            validate_operand(unit, task, block, Some(operation))?;

            validate_runtime_role(unit, *runtime, RuntimeAbiRole::TaskObservationCreation)
        }
    }
}

fn validate_terminal_state(
    unit: &MirUnit,
    block: crate::MirBlockId,
    operation: MirOperationId,
    state: &MirTaskTerminalState,
) -> Option<()> {
    match state {
        MirTaskTerminalState::Completed(value) | MirTaskTerminalState::Panicked(value) => {
            validate_operand(unit, value, block, Some(operation))
        }
        MirTaskTerminalState::Cancelled => Some(()),
    }
}

pub(super) fn validate_operand(
    unit: &MirUnit,
    operand: &MirOperand,
    block: crate::MirBlockId,
    before: Option<MirOperationId>,
) -> Option<()> {
    match operand {
        MirOperand::Value(value) => validate_value_at(unit, *value, block, before),
        MirOperand::Constant { .. } | MirOperand::Immediate { .. } => Some(()),
        MirOperand::Copy(place) | MirOperand::Move(place) => {
            validate_place(unit, place, block, before)
        }
    }
}

pub(super) fn validate_place(
    unit: &MirUnit,
    place: &MirPlace,
    block: crate::MirBlockId,
    before: Option<MirOperationId>,
) -> Option<()> {
    validate_storage(unit, place.storage())?;

    let Some(storage) = unit.storage(place.storage()) else {
        return None;
    };

    let mut expected_type = storage.ty();

    for projection in place.projections() {
        if projection.source_type() != expected_type {
            return None;
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
            | MirProjectionKind::ActiveUnionPayloadElement { .. }
            | MirProjectionKind::NullableValue
            | MirProjectionKind::OwnedStorage => {}
        }

        expected_type = projection.result_type();
    }

    if expected_type != place.ty() {
        return None;
    }

    Some(())
}

fn validate_current_frame(
    unit: &MirUnit,
    frame: bray_runtime_interface::ProtectedAsyncFrameId,
) -> Option<()> {
    if unit.kind().protected_frame() != Some(frame) {
        return None;
    }

    Some(())
}

fn validate_storage(unit: &MirUnit, storage: MirStorageId) -> Option<()> {
    if unit.storage(storage).is_none() {
        return None;
    }

    Some(())
}

fn validate_storage_kind(
    unit: &MirUnit,
    storage: MirStorageId,
    expected: MirStorageKind,
) -> Option<()> {
    validate_storage(unit, storage)?;

    if unit.storage(storage).map(MirStorage::kind) != Some(&expected) {
        return None;
    }

    Some(())
}

fn validate_place_storage_kind(
    unit: &MirUnit,
    place: &MirPlace,
    expected: MirStorageKind,
) -> Option<()> {
    validate_storage_kind(unit, place.storage(), expected)
}

pub(super) fn validate_value(unit: &MirUnit, value: MirValueId) -> Option<()> {
    if unit.value(value).is_none() {
        return None;
    }

    Some(())
}

fn validate_value_at(
    unit: &MirUnit,
    value: MirValueId,
    block: crate::MirBlockId,
    before: Option<MirOperationId>,
) -> Option<()> {
    validate_value(unit, value)?;

    let Some(value_data) = unit.value(value) else {
        return None;
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
        return None;
    }

    Some(())
}
