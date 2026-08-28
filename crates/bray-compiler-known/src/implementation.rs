define_catalog_enum! {
    /// Identifies compiler-provided behavior without selecting a lowering strategy.
    ///
    /// Checker, constant-evaluation, lowering, and code-generation code interpret
    /// these hooks in their owning crates through exhaustive matches or validated
    /// registries.
    #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
    pub enum ImplementationHook {
        /// Produces a raw pointer to borrowed storage.
        AddressOf => "AddressOf",
        /// Produces a raw pointer to mutably borrowed storage.
        AddressOfMut => "AddressOfMut",
        /// Produces the null raw pointer for an element type.
        RawPointerNull => "RawPointerNull",
        /// Tests whether a raw pointer is null.
        RawPointerIsNull => "RawPointerIsNull",
        /// Offsets a raw pointer by an element count.
        RawPointerOffset => "RawPointerOffset",
        /// Offsets a raw pointer by a byte count.
        RawPointerByteOffset => "RawPointerByteOffset",
        /// Reinterprets a raw pointer's element type.
        RawPointerReinterpret => "RawPointerReinterpret",
        /// Creates an ABI-qualified callable from a checked code pointer.
        CallableFromPointer => "CallableFromPointer",
        /// Exposes an ABI-qualified callable's code pointer.
        PointerFromCallable => "PointerFromCallable",
        /// Reads a value from raw storage.
        RawPointerRead => "RawPointerRead",
        /// Writes a value to raw storage.
        RawPointerWrite => "RawPointerWrite",
        /// Copies a non-overlapping raw-memory range.
        MemoryCopy => "MemoryCopy",
        /// Copies a possibly overlapping raw-memory range.
        MemoryCopyOverlapping => "MemoryCopyOverlapping",
        /// Creates protected storage without creating a value of its represented type.
        UninitNew => "UninitNew",
        /// Produces a shared raw pointer to protected storage.
        UninitPointer => "UninitPointer",
        /// Produces a mutable raw pointer to protected storage.
        UninitPointerMut => "UninitPointerMut",
        /// Safely initializes protected storage from an owned value.
        UninitWrite => "UninitWrite",
        /// Trusts that protected storage contains an initialized represented value.
        UninitAssumeInitialized => "UninitAssumeInitialized",
        /// Moves an initialized represented value out of protected storage.
        UninitMove => "UninitMove",
        /// Constructs a shared borrow anchored to an explicit owner.
        BorrowFrom => "BorrowFrom",
        /// Constructs a mutable borrow anchored to an explicit scoped capability.
        BorrowMutFrom => "BorrowMutFrom",
        /// Computes the size of a type.
        MemorySizeOf => "MemorySizeOf",
        /// Computes the alignment of a type.
        MemoryAlignOf => "MemoryAlignOf",
        /// Computes the stride of a type.
        MemoryStrideOf => "MemoryStrideOf",
        /// Computes a repeated-value memory layout.
        MemoryLayoutOf => "MemoryLayoutOf",
        /// Computes a flexible product's complete trailing allocation layout.
        MemoryTrailingLayoutOf => "MemoryTrailingLayoutOf",
        /// Allocates raw storage from separate byte count and alignment values.
        RawAllocate => "RawAllocate",
        /// Releases raw storage described by separate pointer and layout values.
        RawDeallocate => "RawDeallocate",
        /// Allocates an owned raw allocation.
        Allocate => "Allocate",
        /// Releases an owned raw allocation.
        Deallocate => "Deallocate",
        /// Reads a raw buffer's capacity.
        RawBufferCapacity => "RawBufferCapacity",
        /// Reads a raw buffer's initialized element count.
        RawBufferInitializedCount => "RawBufferInitializedCount",
        /// Reads a raw buffer's storage pointer.
        RawBufferPointer => "RawBufferPointer",
        /// Borrows a raw buffer's initialized elements.
        RawBufferInitializedSlice => "RawBufferInitializedSlice",
        /// Mutably borrows a raw buffer's initialized elements.
        RawBufferInitializedSliceMut => "RawBufferInitializedSliceMut",
        /// Produces a pointer to a raw buffer's spare storage.
        RawBufferSparePointer => "RawBufferSparePointer",
        /// Updates a raw buffer's initialized element count.
        RawBufferSetInitializedCount => "RawBufferSetInitializedCount",
        /// Destroys initialized elements and releases a raw buffer in place.
        RawBufferRelease => "RawBufferRelease",
        /// Replaces one raw-buffer owner and clears the transferred source.
        RawBufferReplace => "RawBufferReplace",
        /// Relocates initialized values between distinct raw-buffer owners.
        RawBufferRelocate => "RawBufferRelocate",
        /// Initializes a byte-buffer range to one repeated byte.
        ByteBufferFill => "ByteBufferFill",
        /// Copies an initialized byte slice into distinct writable storage.
        ByteBufferCopy => "ByteBufferCopy",
        /// Reads one initialized byte from byte-buffer storage.
        ByteBufferRead => "ByteBufferRead",
        /// Reads the element count carried by a byte slice.
        SliceLength => "SliceLength",
        /// Reconstructs a state borrow at a checked foreign-callback entry.
        CallbackState => "CallbackState",
        /// Reconstructs a mutable borrow to an exclusively transferred value.
        TransferredValueBorrow => "TransferredValueBorrow",
        /// Reads initialized storage with volatile access semantics.
        VolatileLoad => "VolatileLoad",
        /// Writes storage with volatile access semantics.
        VolatileStore => "VolatileStore",
        /// Reads initialized device storage with volatile access semantics.
        DeviceVolatileLoad => "DeviceVolatileLoad",
        /// Writes device storage with volatile access semantics.
        DeviceVolatileStore => "DeviceVolatileStore",
        /// Exposes a raw pointer address as a provenance-free integer.
        PointerExposeAddress => "PointerExposeAddress",
        /// Reconstructs a raw pointer from a provenance-free integer address.
        PointerFromExposedAddress => "PointerFromExposedAddress",
        /// Compares two provenance-free pointer addresses for equality.
        PointerAddressEqual => "PointerAddressEqual",
        /// Orders two provenance-free pointer addresses.
        PointerAddressLess => "PointerAddressLess",
        /// Prevents compiler reordering across the operation.
        CompilerFence => "CompilerFence",
        /// Emits a target hardware synchronization fence.
        HardwareFence => "HardwareFence",
        /// Terminates the product catastrophically without source cleanup.
        CatastrophicAbort => "CatastrophicAbort",
        /// Requests a debugger trap on the selected target.
        DebuggerTrap => "DebuggerTrap",
        /// Marks a reached path as impossible and terminates it.
        UnreachableTermination => "UnreachableTermination",
        /// Emits the selected target's spin-loop hint.
        SpinLoopHint => "SpinLoopHint",
        /// Tests a compiler-known target instruction feature.
        TargetFeatureEnabled => "TargetFeatureEnabled",
        /// Executes checked trusted target-gated inline assembly.
        InlineAssembly => "InlineAssembly",
        /// Executes checked trusted inline assembly that cannot continue.
        DivergingInlineAssembly => "DivergingInlineAssembly",
        /// Executes checked trusted inline assembly with alternate label continuations.
        BranchingInlineAssembly => "BranchingInlineAssembly",
        /// Initializes protected atomic storage.
        AtomicInitialize => "AtomicInitialize",
        /// Atomically loads protected storage.
        AtomicLoad => "AtomicLoad",
        /// Atomically stores protected storage.
        AtomicStore => "AtomicStore",
        /// Atomically exchanges protected storage.
        AtomicExchange => "AtomicExchange",
        /// Performs strong atomic compare-exchange.
        AtomicCompareExchange => "AtomicCompareExchange",
        /// Performs weak atomic compare-exchange.
        AtomicCompareExchangeWeak => "AtomicCompareExchangeWeak",
        /// Atomically adds and returns the previous value.
        AtomicFetchAdd => "AtomicFetchAdd",
        /// Atomically subtracts and returns the previous value.
        AtomicFetchSub => "AtomicFetchSub",
        /// Atomically applies bitwise conjunction and returns the previous value.
        AtomicFetchAnd => "AtomicFetchAnd",
        /// Atomically applies bitwise disjunction and returns the previous value.
        AtomicFetchOr => "AtomicFetchOr",
        /// Atomically applies bitwise exclusive disjunction and returns the previous value.
        AtomicFetchXor => "AtomicFetchXor",
        /// Emits a target hardware memory fence.
        AtomicFence => "AtomicFence",
        /// Emits a compiler-only memory fence.
        AtomicCompilerFence => "AtomicCompilerFence",
        /// Waits while protected atomic storage equals an expected value.
        AtomicWait => "AtomicWait",
        /// Notifies one waiter observing protected atomic storage.
        AtomicNotifyOne => "AtomicNotifyOne",
        /// Notifies every waiter observing protected atomic storage.
        AtomicNotifyAll => "AtomicNotifyAll",
        /// Starts an inactive asynchronous computation as a task.
        FutureStart => "FutureStart",
        /// Joins and observes an independently running task.
        TaskJoin => "TaskJoin",
        /// Requests task cancellation and observes its terminal result.
        TaskCancel => "TaskCancel",
        /// Tests whether the current execution lane permits blocking work.
        BlockingExecution => "BlockingExecution",
        /// Tests whether the current execution lane permits sustained compute work.
        ComputeExecution => "ComputeExecution",
        /// Tests whether execution is on the distinguished initial thread.
        MainThreadExecution => "MainThreadExecution",
        /// Observes whether cancellation is requested for the current run.
        CurrentRunCancellationObservation => "CurrentRunCancellationObservation",
        /// Propagates cancellation to the current run boundary.
        CurrentRunCancellationPropagation => "CurrentRunCancellationPropagation",
        /// Executes one Bray-owned native thread through its runtime boundary.
        NativeThreadExecution => "NativeThreadExecution",
        /// Starts an operating-system thread and transfers its explicit state.
        NativeThreadStart => "NativeThreadStart",
        /// Reads the process-wide identity of the current native thread.
        CurrentNativeThreadIdentity => "CurrentNativeThreadIdentity",
        /// Reads the process-wide identity of the distinguished initial native thread.
        MainNativeThreadIdentity => "MainNativeThreadIdentity",
        /// Recovers one owned panic report published by a native-thread boundary.
        NativeThreadPanicReportRecovery => "NativeThreadPanicReportRecovery",
        /// Reports and resolves a native-thread panic suppressed by caller cancellation.
        NativeThreadPanicReporting => "NativeThreadPanicReporting",
        /// Creates one runtime-owned task event.
        TaskEventCreation => "TaskEventCreation",
        /// Signals one runtime-owned task event.
        TaskEventSignal => "TaskEventSignal",
        /// Releases one runtime-owned task event.
        TaskEventDestruction => "TaskEventDestruction",
        /// Suspends the current task until a runtime task event is signaled.
        TaskEventWait => "TaskEventWait",
        /// Cooperatively yields the current task.
        TaskYield => "TaskYield",
        /// Counts Unicode scalar values in valid UTF-8 text.
        StringScalarCount => "StringScalarCount",
        /// Tests whether UTF-8 text is empty.
        StringIsEmpty => "StringIsEmpty",
        /// Compares two UTF-8 text values for equality.
        StringEquals => "StringEquals",
        /// Returns one Unicode scalar by scalar index.
        StringScalarAt => "StringScalarAt",
        /// Copies one half-open Unicode scalar range into owned text.
        StringScalarSlice => "StringScalarSlice",
        /// Borrows the UTF-8 bytes of text.
        StringUtf8 => "StringUtf8",
        /// Validates UTF-8 bytes and copies them into owned text.
        StringFromUtf8 => "StringFromUtf8",
        /// Returns a character's Unicode scalar value.
        CharacterScalarValue => "CharacterScalarValue",
        /// Constructs a character from a valid Unicode scalar value.
        CharacterFromScalarValue => "CharacterFromScalarValue",
        /// Returns a character's UTF-8 encoded length.
        CharacterUtf8Length => "CharacterUtf8Length",
        /// Returns one byte from a character's UTF-8 encoding.
        CharacterUtf8Byte => "CharacterUtf8Byte",
        /// Tests whether a character is alphabetic in the selected Unicode data.
        CharacterIsAlphabetic => "CharacterIsAlphabetic",
        /// Tests whether a character is numeric in the selected Unicode data.
        CharacterIsNumeric => "CharacterIsNumeric",
        /// Tests whether a character is whitespace in the selected Unicode data.
        CharacterIsWhitespace => "CharacterIsWhitespace",
        /// Truncates a numeric value to the selected target representation.
        NumericTruncate => "NumericTruncate",
        /// Copies a shared range into its iteration cursor.
        RangeSharedIterate => "RangeSharedIterate",
        /// Moves a range into its iteration cursor.
        RangeMoveIterate => "RangeMoveIterate",
        /// Advances a range cursor by one element.
        RangeNext => "RangeNext",
        /// Fails the current test root with one owned message.
        TestingFail => "TestingFail",
    }
}

impl ImplementationHook {
    /// Compiler-provided raw-memory and layout operations.
    pub const MEMORY_OPERATIONS: &'static [Self] = &[
        Self::AddressOf,
        Self::AddressOfMut,
        Self::RawPointerNull,
        Self::RawPointerIsNull,
        Self::RawPointerOffset,
        Self::RawPointerByteOffset,
        Self::RawPointerReinterpret,
        Self::CallableFromPointer,
        Self::PointerFromCallable,
        Self::RawPointerRead,
        Self::RawPointerWrite,
        Self::MemoryCopy,
        Self::MemoryCopyOverlapping,
        Self::UninitNew,
        Self::UninitPointer,
        Self::UninitPointerMut,
        Self::UninitWrite,
        Self::UninitAssumeInitialized,
        Self::UninitMove,
        Self::BorrowFrom,
        Self::BorrowMutFrom,
        Self::MemorySizeOf,
        Self::MemoryAlignOf,
        Self::MemoryStrideOf,
        Self::MemoryLayoutOf,
        Self::MemoryTrailingLayoutOf,
        Self::RawAllocate,
        Self::RawDeallocate,
        Self::Allocate,
        Self::Deallocate,
        Self::RawBufferCapacity,
        Self::RawBufferInitializedCount,
        Self::RawBufferPointer,
        Self::RawBufferInitializedSlice,
        Self::RawBufferInitializedSliceMut,
        Self::RawBufferSparePointer,
        Self::RawBufferSetInitializedCount,
        Self::RawBufferRelease,
        Self::RawBufferReplace,
        Self::RawBufferRelocate,
        Self::ByteBufferFill,
        Self::ByteBufferCopy,
        Self::ByteBufferRead,
        Self::SliceLength,
        Self::CallbackState,
        Self::TransferredValueBorrow,
        Self::VolatileLoad,
        Self::VolatileStore,
        Self::DeviceVolatileLoad,
        Self::DeviceVolatileStore,
        Self::PointerExposeAddress,
        Self::PointerFromExposedAddress,
        Self::PointerAddressEqual,
        Self::PointerAddressLess,
        Self::CompilerFence,
        Self::HardwareFence,
        Self::CatastrophicAbort,
        Self::DebuggerTrap,
        Self::UnreachableTermination,
        Self::SpinLoopHint,
        Self::TargetFeatureEnabled,
        Self::InlineAssembly,
        Self::DivergingInlineAssembly,
        Self::BranchingInlineAssembly,
        Self::AtomicInitialize,
        Self::AtomicLoad,
        Self::AtomicStore,
        Self::AtomicExchange,
        Self::AtomicCompareExchange,
        Self::AtomicCompareExchangeWeak,
        Self::AtomicFetchAdd,
        Self::AtomicFetchSub,
        Self::AtomicFetchAnd,
        Self::AtomicFetchOr,
        Self::AtomicFetchXor,
        Self::AtomicFence,
        Self::AtomicCompilerFence,
        Self::AtomicWait,
        Self::AtomicNotifyOne,
        Self::AtomicNotifyAll,
    ];
}

#[cfg(test)]
mod tests {
    use super::ImplementationHook;

    #[test]
    fn hooks_are_data_without_executable_payloads() {
        fn assert_copy<T: Copy>() {}

        assert_copy::<ImplementationHook>();

        assert_ne!(
            ImplementationHook::RawPointerRead,
            ImplementationHook::RawPointerWrite
        );
    }

    #[test]
    fn memory_operation_inventory_partitions_the_closed_hook_set() {
        let non_memory = [
            ImplementationHook::FutureStart,
            ImplementationHook::TaskJoin,
            ImplementationHook::TaskCancel,
            ImplementationHook::BlockingExecution,
            ImplementationHook::ComputeExecution,
            ImplementationHook::MainThreadExecution,
            ImplementationHook::CurrentRunCancellationObservation,
            ImplementationHook::CurrentRunCancellationPropagation,
            ImplementationHook::NativeThreadExecution,
            ImplementationHook::NativeThreadStart,
            ImplementationHook::CurrentNativeThreadIdentity,
            ImplementationHook::MainNativeThreadIdentity,
            ImplementationHook::NativeThreadPanicReportRecovery,
            ImplementationHook::NativeThreadPanicReporting,
            ImplementationHook::TaskEventCreation,
            ImplementationHook::TaskEventSignal,
            ImplementationHook::TaskEventDestruction,
            ImplementationHook::TaskEventWait,
            ImplementationHook::TaskYield,
            ImplementationHook::StringScalarCount,
            ImplementationHook::StringIsEmpty,
            ImplementationHook::StringEquals,
            ImplementationHook::StringScalarAt,
            ImplementationHook::StringScalarSlice,
            ImplementationHook::StringUtf8,
            ImplementationHook::StringFromUtf8,
            ImplementationHook::CharacterScalarValue,
            ImplementationHook::CharacterFromScalarValue,
            ImplementationHook::CharacterUtf8Length,
            ImplementationHook::CharacterUtf8Byte,
            ImplementationHook::CharacterIsAlphabetic,
            ImplementationHook::CharacterIsNumeric,
            ImplementationHook::CharacterIsWhitespace,
            ImplementationHook::NumericTruncate,
            ImplementationHook::RangeSharedIterate,
            ImplementationHook::RangeMoveIterate,
            ImplementationHook::RangeNext,
            ImplementationHook::TestingFail,
        ];

        let mut categorized = ImplementationHook::MEMORY_OPERATIONS.to_vec();

        categorized.extend(non_memory);
        categorized.sort_unstable();
        categorized.dedup();

        assert_eq!(categorized, ImplementationHook::ALL);
    }
}
