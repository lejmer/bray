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
        /// Reads a value from raw storage.
        RawPointerRead => "RawPointerRead",
        /// Writes a value to raw storage.
        RawPointerWrite => "RawPointerWrite",
        /// Copies a non-overlapping raw-memory range.
        MemoryCopy => "MemoryCopy",
        /// Copies a possibly overlapping raw-memory range.
        MemoryCopyOverlapping => "MemoryCopyOverlapping",
        /// Computes the size of a type.
        MemorySizeOf => "MemorySizeOf",
        /// Computes the alignment of a type.
        MemoryAlignOf => "MemoryAlignOf",
        /// Computes the stride of a type.
        MemoryStrideOf => "MemoryStrideOf",
        /// Computes a repeated-value memory layout.
        MemoryLayoutOf => "MemoryLayoutOf",
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
        /// Initializes a byte-buffer range to one repeated byte.
        ByteBufferFill => "ByteBufferFill",
        /// Copies a byte-buffer range without exposing raw initialization state.
        ByteBufferCopy => "ByteBufferCopy",
        /// Reads one initialized byte from byte-buffer storage.
        ByteBufferRead => "ByteBufferRead",
        /// Reads the element count carried by a byte slice.
        ByteSliceLength => "ByteSliceLength",
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
        /// Enters cancellation for the current run.
        CurrentRunCancellationEntry => "CurrentRunCancellationEntry",
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
        Self::RawPointerRead,
        Self::RawPointerWrite,
        Self::MemoryCopy,
        Self::MemoryCopyOverlapping,
        Self::MemorySizeOf,
        Self::MemoryAlignOf,
        Self::MemoryStrideOf,
        Self::MemoryLayoutOf,
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
        Self::ByteBufferFill,
        Self::ByteBufferCopy,
        Self::ByteBufferRead,
        Self::ByteSliceLength,
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
            ImplementationHook::CurrentRunCancellationEntry,
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
        ];

        let mut categorized = ImplementationHook::MEMORY_OPERATIONS.to_vec();

        categorized.extend(non_memory);
        categorized.sort_unstable();
        categorized.dedup();

        assert_eq!(categorized, ImplementationHook::ALL);
    }
}
