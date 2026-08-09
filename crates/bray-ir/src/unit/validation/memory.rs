use bray_bound_tree::CheckedMemoryOperationKind;

use crate::{MirMemoryOperation, MirOperationId, MirUnit, MirUnitBuildError};

use super::operation::{operand_type, validate_operand};

pub(super) fn validate_memory_operation(
    unit: &MirUnit,
    block: crate::MirBlockId,
    operation: MirOperationId,
    memory: &MirMemoryOperation,
) -> Result<(), MirUnitBuildError> {
    let expected = memory.kind().operand_count();

    if memory.operands().len() != expected || memory.operand_types().len() != expected {
        return Err(MirUnitBuildError::InvalidMemoryOperation(operation));
    }

    for (operand, expected_type) in memory.operands().iter().zip(memory.operand_types()) {
        validate_operand(unit, operand, block, Some(operation))?;

        if operand_type(unit, operand)? != *expected_type {
            return Err(MirUnitBuildError::InvalidMemoryOperation(operation));
        }
    }

    if memory.kind().produces_value() != memory.result_type().is_some() {
        return Err(MirUnitBuildError::InvalidMemoryOperation(operation));
    }

    let types = memory.operand_types();

    let valid = match memory.kind() {
        CheckedMemoryOperationKind::Address { .. }
        | CheckedMemoryOperationKind::Null { .. }
        | CheckedMemoryOperationKind::IsNull { .. }
        | CheckedMemoryOperationKind::Reinterpret { .. } => true,
        CheckedMemoryOperationKind::Offset { .. } => memory.result_type() == Some(types[0]),
        CheckedMemoryOperationKind::Read { pointee, .. } => memory.result_type() == Some(pointee),
        CheckedMemoryOperationKind::Write { pointee } => types[1] == pointee,
        CheckedMemoryOperationKind::Copy { .. } => types[0] == types[1],
        CheckedMemoryOperationKind::LayoutQuery { .. } => true,
        CheckedMemoryOperationKind::RawAllocate => types[0] == types[1],
        CheckedMemoryOperationKind::RawDeallocate => types[1] == types[2],
        CheckedMemoryOperationKind::Allocate
        | CheckedMemoryOperationKind::Deallocate
        | CheckedMemoryOperationKind::RawBufferCapacity
        | CheckedMemoryOperationKind::RawBufferInitializedCount
        | CheckedMemoryOperationKind::RawBufferPointer
        | CheckedMemoryOperationKind::RawBufferInitializedSlice
        | CheckedMemoryOperationKind::RawBufferInitializedSliceMut
        | CheckedMemoryOperationKind::RawBufferSparePointer { .. }
        | CheckedMemoryOperationKind::RawBufferSetInitializedCount
        | CheckedMemoryOperationKind::RawBufferRelease { .. }
        | CheckedMemoryOperationKind::RawBufferReplace { .. } => true,
        CheckedMemoryOperationKind::ByteBufferFill
        | CheckedMemoryOperationKind::ByteBufferCopy
        | CheckedMemoryOperationKind::ByteBufferRead
        | CheckedMemoryOperationKind::SliceLength
        | CheckedMemoryOperationKind::CallbackState { .. } => true,
    };

    if !valid {
        return Err(MirUnitBuildError::InvalidMemoryOperation(operation));
    }

    Ok(())
}
