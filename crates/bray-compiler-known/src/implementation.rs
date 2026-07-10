/// Identifies compiler-provided behavior without selecting a lowering strategy.
///
/// Checker, constant-evaluation, lowering, and code-generation code interpret
/// these hooks in their owning crates through exhaustive matches or validated
/// registries.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ImplementationHook {
    /// Produces a raw pointer to borrowed storage.
    AddressOf,
    /// Produces a raw pointer to mutably borrowed storage.
    AddressOfMut,
    /// Produces the null raw pointer for an element type.
    RawPointerNull,
    /// Tests whether a raw pointer is null.
    RawPointerIsNull,
    /// Offsets a raw pointer by an element count.
    RawPointerOffset,
    /// Offsets a raw pointer by a byte count.
    RawPointerByteOffset,
    /// Reinterprets a raw pointer's element type.
    RawPointerReinterpret,
    /// Reads a value from raw storage.
    RawPointerRead,
    /// Writes a value to raw storage.
    RawPointerWrite,
    /// Copies a non-overlapping raw-memory range.
    MemoryCopy,
    /// Copies a possibly overlapping raw-memory range.
    MemoryCopyOverlapping,
    /// Computes the size of a type.
    MemorySizeOf,
    /// Computes the alignment of a type.
    MemoryAlignOf,
    /// Computes the stride of a type.
    MemoryStrideOf,
    /// Computes a repeated-value memory layout.
    MemoryLayoutOf,
    /// Allocates raw storage.
    Allocate,
    /// Releases raw storage.
    Deallocate,
}

impl ImplementationHook {
    pub(crate) fn from_catalog_spelling(spelling: &str) -> Option<Self> {
        match spelling {
            "AddressOf" => Some(Self::AddressOf),
            "AddressOfMut" => Some(Self::AddressOfMut),
            "RawPointerNull" => Some(Self::RawPointerNull),
            "RawPointerIsNull" => Some(Self::RawPointerIsNull),
            "RawPointerOffset" => Some(Self::RawPointerOffset),
            "RawPointerByteOffset" => Some(Self::RawPointerByteOffset),
            "RawPointerReinterpret" => Some(Self::RawPointerReinterpret),
            "RawPointerRead" => Some(Self::RawPointerRead),
            "RawPointerWrite" => Some(Self::RawPointerWrite),
            "MemoryCopy" => Some(Self::MemoryCopy),
            "MemoryCopyOverlapping" => Some(Self::MemoryCopyOverlapping),
            "MemorySizeOf" => Some(Self::MemorySizeOf),
            "MemoryAlignOf" => Some(Self::MemoryAlignOf),
            "MemoryStrideOf" => Some(Self::MemoryStrideOf),
            "MemoryLayoutOf" => Some(Self::MemoryLayoutOf),
            "Allocate" => Some(Self::Allocate),
            "Deallocate" => Some(Self::Deallocate),
            _ => None,
        }
    }
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
}
