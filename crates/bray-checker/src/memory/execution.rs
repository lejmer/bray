use bray_bound_tree::{CheckedMemoryOperationKind as Operation, MemoryReadKind};
use bray_symbols::ExecutionProperty;

/// Closed operations whose checked storage domain requires trusted caller evidence.
pub(crate) const fn requires_trust(kind: Operation) -> bool {
    matches!(
        kind,
        Operation::UninitAssumeInitialized { .. }
            | Operation::UninitMove { .. }
            | Operation::BorrowFrom { .. }
            | Operation::Reinterpret { .. }
            | Operation::Read { .. }
            | Operation::Write { .. }
            | Operation::Copy { .. }
            | Operation::RawAllocate
            | Operation::RawDeallocate
            | Operation::RawBufferAllocate
            | Operation::Allocate
            | Operation::Deallocate
            | Operation::RawBufferSparePointer { .. }
            | Operation::RawBufferPush { .. }
            | Operation::RawBufferPop { .. }
            | Operation::RawBufferSetInitializedCount
            | Operation::RawBufferRelease { .. }
            | Operation::RawBufferReplace { .. }
            | Operation::RawBufferRelocate { .. }
            | Operation::ByteBufferFill
            | Operation::ByteBufferRead
            | Operation::CallbackState { .. }
            | Operation::VolatileRead { .. }
            | Operation::VolatileWrite { .. }
    )
}

/// Intrinsic execution properties on the operation's checked storage domain.
/// Argument evaluation, requirements, conversions and cleanup remain separate obligations.
pub(crate) fn guarantees(operation: Operation, property: ExecutionProperty) -> bool {
    match operation {
        Operation::UninitNew { .. }
        | Operation::UninitFrom { .. }
        | Operation::UninitAssumeInitialized { .. }
        | Operation::UninitPointer { .. }
        | Operation::BorrowFrom { .. }
        | Operation::Address { .. }
        | Operation::Null { .. }
        | Operation::IsNull { .. }
        | Operation::Offset { .. }
        | Operation::ByteAddress { .. }
        | Operation::Reinterpret { .. }
        | Operation::CallableFromPointer { .. }
        | Operation::PointerFromCallable { .. }
        | Operation::Read {
            kind: MemoryReadKind::Copy,
            ..
        }
        | Operation::LayoutQuery { .. }
        | Operation::RawBufferCapacity
        | Operation::RawBufferInitializedCount
        | Operation::RawBufferPointer
        | Operation::RawBufferInitializedSlice
        | Operation::RawBufferInitializedSliceMut
        | Operation::RawBufferSparePointer { .. }
        | Operation::ByteBufferRead
        | Operation::SequenceLength
        | Operation::AtomicInitialize { .. } => true,
        // These bounded transfers do not call payload lifecycle or allocate backing.
        // In particular, committing or extracting Uninit contents is not disposal.
        Operation::UninitWrite { .. }
        | Operation::UninitMove { .. }
        | Operation::Read {
            kind: MemoryReadKind::Move,
            ..
        }
        | Operation::Write { .. }
        | Operation::Copy { .. }
        | Operation::RawBufferPush { .. }
        | Operation::RawBufferSetInitializedCount
        | Operation::RawBufferRelocate { .. }
        | Operation::ExposeAddress { .. }
        | Operation::FromExposedAddress { .. }
        | Operation::SpinLoopHint
        | Operation::AtomicLoad { .. }
        | Operation::AtomicStore { .. }
        | Operation::AtomicExchange { .. }
        | Operation::AtomicCompareExchange { .. }
        | Operation::AtomicFetch { .. }
        | Operation::AtomicNotify { .. } => property == ExecutionProperty::Total,
        // Allocation, selected disposal, waiting and target-controlled execution need
        // their own checked completion contract; this table supplies no blanket proof.
        Operation::RawAllocate
        | Operation::RawDeallocate
        | Operation::Allocate
        | Operation::Deallocate
        | Operation::RawBufferAllocate
        | Operation::RawBufferPop { .. }
        | Operation::RawBufferRelease { .. }
        | Operation::RawBufferReplace { .. }
        | Operation::ByteBufferFill
        | Operation::CallbackState { .. }
        | Operation::VolatileRead { .. }
        | Operation::VolatileWrite { .. }
        | Operation::CompareAddress { .. }
        | Operation::Fence { .. }
        | Operation::CatastrophicAbort
        | Operation::DebuggerTrap
        | Operation::UnreachableTermination
        | Operation::TargetFeatureEnabled { .. }
        | Operation::InlineAssembly { .. }
        | Operation::AtomicWait { .. } => false,
    }
}

/// Effects on storage that existed before argument evaluation completed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StorageEffect {
    /// Existing contents, ownership, and storage validity are unchanged.
    None,
    /// The operation changes contents without retiring their backing storage.
    Contents,
    /// Lifecycle, external execution, or another effect can retire backing storage.
    Unknown,
}

/// Classifies the checked intrinsic. Argument transfers and cleanup apply separately.
pub(crate) fn storage_effect(operation: Operation) -> StorageEffect {
    match operation {
        Operation::UninitNew { .. }
        | Operation::UninitFrom { .. }
        | Operation::UninitAssumeInitialized { .. }
        | Operation::UninitPointer { .. }
        | Operation::BorrowFrom { .. }
        | Operation::Address { .. }
        | Operation::Null { .. }
        | Operation::IsNull { .. }
        | Operation::Offset { .. }
        | Operation::ByteAddress { .. }
        | Operation::Reinterpret { .. }
        | Operation::CallableFromPointer { .. }
        | Operation::PointerFromCallable { .. }
        | Operation::Read {
            kind: MemoryReadKind::Copy,
            ..
        }
        | Operation::VolatileRead {
            kind: MemoryReadKind::Copy,
            ..
        }
        | Operation::LayoutQuery { .. }
        | Operation::RawAllocate
        | Operation::Allocate
        | Operation::RawBufferAllocate
        | Operation::RawBufferCapacity
        | Operation::RawBufferInitializedCount
        | Operation::RawBufferPointer
        | Operation::RawBufferInitializedSlice
        | Operation::RawBufferInitializedSliceMut
        | Operation::RawBufferSparePointer { .. }
        | Operation::ByteBufferRead
        | Operation::SequenceLength
        | Operation::CallbackState { .. }
        | Operation::ExposeAddress { .. }
        | Operation::FromExposedAddress { .. }
        | Operation::CompareAddress { .. }
        | Operation::SpinLoopHint
        | Operation::TargetFeatureEnabled { .. }
        | Operation::AtomicInitialize { .. }
        | Operation::AtomicLoad { .. }
        | Operation::AtomicWait { .. }
        | Operation::AtomicNotify { .. }
        | Operation::Fence { .. }
        // The analysis graph terminates this operation without source cleanup.
        // There is no returning storage effect, even in a conditional branch.
        | Operation::CatastrophicAbort => StorageEffect::None,
        Operation::UninitWrite { .. }
        | Operation::UninitMove { .. }
        | Operation::Read {
            kind: MemoryReadKind::Move,
            ..
        }
        | Operation::Write { .. }
        | Operation::VolatileRead {
            kind: MemoryReadKind::Move,
            ..
        }
        | Operation::VolatileWrite { .. }
        | Operation::Copy { .. }
        | Operation::AtomicStore { .. }
        | Operation::AtomicExchange { .. }
        | Operation::AtomicCompareExchange { .. }
        | Operation::AtomicFetch { .. } => StorageEffect::Contents,
        Operation::RawDeallocate
        | Operation::Deallocate
        | Operation::RawBufferPush { .. }
        | Operation::RawBufferPop { .. }
        | Operation::RawBufferSetInitializedCount
        | Operation::RawBufferRelease { .. }
        | Operation::RawBufferReplace { .. }
        | Operation::RawBufferRelocate { .. }
        | Operation::ByteBufferFill
        | Operation::DebuggerTrap
        | Operation::UnreachableTermination
        | Operation::InlineAssembly { .. } => StorageEffect::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::{Operation, StorageEffect, guarantees, storage_effect};
    use bray_symbols::ExecutionProperty;

    #[test]
    fn protected_storage_transfers_complete_without_certifying_mutation_as_pure() {
        let element = crate::test_support::error_type();

        for operation in [
            Operation::UninitNew { element },
            Operation::UninitFrom { element },
            Operation::UninitAssumeInitialized { element },
        ] {
            assert!(guarantees(operation, ExecutionProperty::Pure));
            assert!(guarantees(operation, ExecutionProperty::Total));
        }

        for operation in [
            Operation::UninitWrite { element },
            Operation::UninitMove { element },
        ] {
            assert!(!guarantees(operation, ExecutionProperty::Pure));
            assert!(guarantees(operation, ExecutionProperty::Total));
        }
    }

    #[test]
    fn observation_order_and_ownership_transfer_have_distinct_storage_effects() {
        let element = crate::test_support::error_type();

        let load = Operation::AtomicLoad {
            value: element,
            order: bray_bound_tree::MemoryOrder::Acquire,
        };

        assert_eq!(storage_effect(load), StorageEffect::None);
        assert!(!guarantees(load, ExecutionProperty::Pure));

        let read = |kind| Operation::Read {
            pointee: element,
            kind,
        };

        assert_eq!(
            storage_effect(read(bray_bound_tree::MemoryReadKind::Copy)),
            StorageEffect::None
        );

        assert_eq!(
            storage_effect(read(bray_bound_tree::MemoryReadKind::Move)),
            StorageEffect::Contents
        );

        assert!(guarantees(
            read(bray_bound_tree::MemoryReadKind::Move),
            ExecutionProperty::Total
        ));

        assert_eq!(
            storage_effect(Operation::UninitMove { element }),
            StorageEffect::Contents
        );

        for (kind, effect) in [
            (bray_bound_tree::MemoryReadKind::Copy, StorageEffect::None),
            (
                bray_bound_tree::MemoryReadKind::Move,
                StorageEffect::Contents,
            ),
        ] {
            let operation = Operation::VolatileRead {
                pointee: element,
                kind,
                address_space: bray_bound_tree::VolatileAddressSpace::Device,
            };

            assert_eq!(storage_effect(operation), effect);
            assert!(!guarantees(operation, ExecutionProperty::Pure));
        }

        assert_eq!(storage_effect(Operation::RawAllocate), StorageEffect::None);

        assert_eq!(
            storage_effect(Operation::RawDeallocate),
            StorageEffect::Unknown
        );
    }

    #[test]
    fn payload_disposal_and_waiting_do_not_inherit_transfer_guarantees() {
        let element = crate::test_support::error_type();

        for operation in [
            Operation::RawBufferRelease { element },
            Operation::RawBufferReplace { element },
            Operation::RawAllocate,
            Operation::AtomicWait {
                value: element,
                order: bray_bound_tree::MemoryOrder::Acquire,
            },
        ] {
            assert!(!guarantees(operation, ExecutionProperty::Pure));
            assert!(!guarantees(operation, ExecutionProperty::Total));
        }
    }
}
