use bray_bound_tree::CheckedMemoryOperationKind;

use crate::{MirMemoryOperation, MirOperationId, MirUnit, MirUnitBuildError};

use super::operation::validate_operand;

pub(super) fn validate_memory_operation(
    unit: &MirUnit,
    block: crate::MirBlockId,
    operation: MirOperationId,
    memory: &MirMemoryOperation,
) -> Result<(), MirUnitBuildError> {
    if !memory.kind().has_valid_atomic_ordering() {
        return Err(MirUnitBuildError::InvalidMemoryOperation(operation));
    }

    let expected = memory.kind().operand_count();

    if memory.operands().len() != expected || memory.operand_types().len() != expected {
        return Err(MirUnitBuildError::InvalidMemoryOperation(operation));
    }

    for (operand, expected_type) in memory.operands().iter().zip(memory.operand_types()) {
        validate_operand(unit, operand, block, Some(operation))?;

        if unit.operand_type(operand)? != *expected_type {
            return Err(MirUnitBuildError::InvalidMemoryOperation(operation));
        }
    }

    if memory.kind().produces_value() != memory.result_type().is_some() {
        return Err(MirUnitBuildError::InvalidMemoryOperation(operation));
    }

    if !matches!(
        memory.kind(),
        CheckedMemoryOperationKind::InlineAssembly { .. }
    ) && !memory.inline_assembly_symbols().is_empty()
    {
        return Err(MirUnitBuildError::InvalidMemoryOperation(operation));
    }

    let types = memory.operand_types();

    let valid = match memory.kind() {
        CheckedMemoryOperationKind::UninitNew { .. }
        | CheckedMemoryOperationKind::UninitPointer { .. }
        | CheckedMemoryOperationKind::BorrowFrom { .. }
        | CheckedMemoryOperationKind::Address { .. }
        | CheckedMemoryOperationKind::Null { .. }
        | CheckedMemoryOperationKind::IsNull { .. }
        | CheckedMemoryOperationKind::Reinterpret { .. }
        | CheckedMemoryOperationKind::CallableFromPointer { .. }
        | CheckedMemoryOperationKind::PointerFromCallable { .. } => true,
        CheckedMemoryOperationKind::UninitWrite { element } => types[1] == element,
        CheckedMemoryOperationKind::UninitAssumeInitialized { element }
        | CheckedMemoryOperationKind::UninitMove { element } => {
            memory.result_type() == Some(element)
        }
        CheckedMemoryOperationKind::Offset { .. } => memory.result_type() == Some(types[0]),
        CheckedMemoryOperationKind::Read { pointee, .. } => memory.result_type() == Some(pointee),
        CheckedMemoryOperationKind::Write { pointee } => types[1] == pointee,
        CheckedMemoryOperationKind::VolatileRead { pointee, .. } => {
            memory.result_type() == Some(pointee)
        }
        CheckedMemoryOperationKind::VolatileWrite { pointee, .. } => types[1] == pointee,
        CheckedMemoryOperationKind::ExposeAddress { .. }
        | CheckedMemoryOperationKind::FromExposedAddress { .. }
        | CheckedMemoryOperationKind::CompareAddress { .. } => true,
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
        | CheckedMemoryOperationKind::RawBufferSetInitializedCount => true,
        CheckedMemoryOperationKind::RawBufferRelease { .. }
        | CheckedMemoryOperationKind::RawBufferReplace { .. } => false,
        CheckedMemoryOperationKind::RawBufferRelocate { .. } => types[0] == types[1],
        CheckedMemoryOperationKind::ByteBufferFill
        | CheckedMemoryOperationKind::ByteBufferCopy
        | CheckedMemoryOperationKind::ByteBufferRead
        | CheckedMemoryOperationKind::SequenceLength
        | CheckedMemoryOperationKind::CallbackState { .. }
        | CheckedMemoryOperationKind::Fence { .. }
        | CheckedMemoryOperationKind::CatastrophicAbort
        | CheckedMemoryOperationKind::DebuggerTrap
        | CheckedMemoryOperationKind::UnreachableTermination
        | CheckedMemoryOperationKind::SpinLoopHint
        | CheckedMemoryOperationKind::TargetFeatureEnabled { .. } => true,
        CheckedMemoryOperationKind::InlineAssembly {
            inputs,
            output,
            labels,
            contract,
        } => {
            types[0] == inputs
                && labels.is_none()
                && memory.result_type() == output
                && contract
                    .operands()
                    .filter(|operand| {
                        operand.kind() == bray_bound_tree::InlineAssemblyOperandKind::Symbol
                    })
                    .count()
                    == memory.inline_assembly_symbols().len()
        }
        CheckedMemoryOperationKind::AtomicInitialize { .. }
        | CheckedMemoryOperationKind::AtomicNotify { .. } => true,
        CheckedMemoryOperationKind::AtomicLoad { value, .. } => memory.result_type() == Some(value),
        CheckedMemoryOperationKind::AtomicStore { value, .. }
        | CheckedMemoryOperationKind::AtomicWait { value, .. } => types[1] == value,
        CheckedMemoryOperationKind::AtomicExchange { value, .. }
        | CheckedMemoryOperationKind::AtomicFetch { value, .. } => {
            types[1] == value && memory.result_type() == Some(value)
        }
        CheckedMemoryOperationKind::AtomicCompareExchange { value, .. } => {
            types[1] == value && types[2] == value
        }
    };

    if !valid {
        return Err(MirUnitBuildError::InvalidMemoryOperation(operation));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{
        MirBlockKind, MirImmediateValue, MirOperand, MirOperationKind, MirSourceAnchor,
        MirTerminatorKind, MirUnitBuilder, MirUnitKind,
    };
    use crate::{MirMemoryOperation, MirUnitBuildError};
    use bray_bound_tree::CheckedMemoryOperationKind;

    #[test]
    fn raw_buffer_lifecycle_intrinsics_require_expansion_before_mir_publication() {
        let values = bray_symbols::SemanticValueStore::try_new().unwrap();

        let ty = values
            .intern_type(bray_symbols::TypeData::tuple([]))
            .unwrap();

        for kind in [
            CheckedMemoryOperationKind::RawBufferRelease { element: ty },
            CheckedMemoryOperationKind::RawBufferReplace { element: ty },
        ] {
            let bound = bray_testing::test_bound_unit(1802);
            let source = MirSourceAnchor::from(bound.key().source());

            let mut builder = MirUnitBuilder::for_bound(
                bound.identity(),
                MirUnitKind::Synchronous,
                crate::test_support::test_target(),
            );

            let entry = builder
                .push_block(source.clone(), MirBlockKind::Ordinary)
                .unwrap();

            let operands = vec![
                MirOperand::Immediate {
                    value: MirImmediateValue::Unit,
                    ty
                };
                kind.operand_count()
            ];

            let operation = builder
                .push_operation(
                    entry,
                    source.clone(),
                    MirOperationKind::Memory(MirMemoryOperation::new(
                        kind,
                        operands,
                        vec![ty; kind.operand_count()],
                        None,
                    )),
                    None,
                )
                .unwrap()
                .operation();

            builder
                .set_terminator(entry, source, MirTerminatorKind::Return(None))
                .unwrap();

            assert_eq!(
                builder.finish(entry),
                Err(MirUnitBuildError::InvalidMemoryOperation(operation))
            );
        }
    }
}
