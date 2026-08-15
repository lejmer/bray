use std::num::NonZeroU64;

const ATOMIC_REPRESENTATION_COUNT: usize = 6;

/// A target atomic representation understood independently of source signedness.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TargetAtomicRepresentation {
    /// An 8-bit integer or Boolean representation.
    U8,
    /// A 16-bit integer representation.
    U16,
    /// A 32-bit integer representation.
    U32,
    /// A 64-bit integer representation.
    U64,
    /// A 128-bit integer representation.
    U128,
    /// A raw-pointer representation.
    Pointer,
}

impl TargetAtomicRepresentation {
    /// Every atomic representation in stable language order.
    pub const ALL: [Self; ATOMIC_REPRESENTATION_COUNT] = [
        Self::U8,
        Self::U16,
        Self::U32,
        Self::U64,
        Self::U128,
        Self::Pointer,
    ];

    /// Returns the integer atomic representation for one exact plain-storage size.
    pub const fn for_storage_size(size_bytes: u64) -> Option<Self> {
        match size_bytes {
            1 => Some(Self::U8),
            2 => Some(Self::U16),
            4 => Some(Self::U32),
            8 => Some(Self::U64),
            16 => Some(Self::U128),
            _ => None,
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::U8 => 0,
            Self::U16 => 1,
            Self::U32 => 2,
            Self::U64 => 3,
            Self::U128 => 4,
            Self::Pointer => 5,
        }
    }
}

/// Operations natively available for one atomic representation.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetAtomicOperationFacts {
    load_store: bool,
    exchange: bool,
    compare_exchange: bool,
    fetch_arithmetic: bool,
    fetch_bitwise: bool,
}

impl TargetAtomicOperationFacts {
    /// Creates the exact operation matrix for one representation.
    pub const fn new(
        load_store: bool,
        exchange: bool,
        compare_exchange: bool,
        fetch_arithmetic: bool,
        fetch_bitwise: bool,
    ) -> Self {
        Self {
            load_store,
            exchange,
            compare_exchange,
            fetch_arithmetic,
            fetch_bitwise,
        }
    }

    /// Returns the complete integer operation matrix.
    pub const fn integer() -> Self {
        Self::new(true, true, true, true, true)
    }

    /// Returns the raw-pointer operation matrix.
    pub const fn pointer() -> Self {
        Self::new(true, true, true, false, false)
    }

    /// Returns whether any operation is available.
    pub const fn any(self) -> bool {
        self.load_store
            || self.exchange
            || self.compare_exchange
            || self.fetch_arithmetic
            || self.fetch_bitwise
    }

    /// Returns whether atomic loads and stores are available.
    pub const fn load_store(self) -> bool {
        self.load_store
    }

    /// Returns whether atomic exchange is available.
    pub const fn exchange(self) -> bool {
        self.exchange
    }

    /// Returns whether strong and weak compare-exchange are available.
    pub const fn compare_exchange(self) -> bool {
        self.compare_exchange
    }

    /// Returns whether arithmetic fetch operations are available.
    pub const fn fetch_arithmetic(self) -> bool {
        self.fetch_arithmetic
    }

    /// Returns whether bitwise fetch operations are available.
    pub const fn fetch_bitwise(self) -> bool {
        self.fetch_bitwise
    }
}

/// Availability and guarantees for one atomic representation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetAtomicRepresentationFacts {
    operations: TargetAtomicOperationFacts,
    required_alignment: NonZeroU64,
    always_lock_free: bool,
    wait_notify: bool,
    cross_process: bool,
}

impl TargetAtomicRepresentationFacts {
    /// Creates internally consistent representation facts.
    pub const fn try_new(
        operations: TargetAtomicOperationFacts,
        required_alignment: NonZeroU64,
        always_lock_free: bool,
        wait_notify: bool,
        cross_process: bool,
    ) -> Option<Self> {
        if !required_alignment.get().is_power_of_two()
            || ((!operations.any())
                && (always_lock_free || wait_notify || cross_process))
            || (wait_notify && !operations.load_store())
        {
            return None;
        }

        Some(Self {
            operations,
            required_alignment,
            always_lock_free,
            wait_notify,
            cross_process,
        })
    }

    /// Creates an unavailable representation with its natural alignment.
    pub const fn unavailable(required_alignment: NonZeroU64) -> Self {
        Self {
            operations: TargetAtomicOperationFacts::new(false, false, false, false, false),
            required_alignment,
            always_lock_free: false,
            wait_notify: false,
            cross_process: false,
        }
    }

    /// Returns the exact operation matrix.
    pub const fn operations(self) -> TargetAtomicOperationFacts {
        self.operations
    }

    /// Returns the alignment required for protected atomic storage.
    pub const fn required_alignment(self) -> NonZeroU64 {
        self.required_alignment
    }

    /// Returns whether every available operation is natively lock-free.
    pub const fn always_lock_free(self) -> bool {
        self.always_lock_free
    }

    /// Returns whether wait and notification are available.
    pub const fn wait_notify(self) -> bool {
        self.wait_notify
    }

    /// Returns whether operations may address shared cross-process storage.
    pub const fn cross_process(self) -> bool {
        self.cross_process
    }
}

/// Atomic representation and operation facts for one target.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetAtomicFacts {
    representations: [TargetAtomicRepresentationFacts; ATOMIC_REPRESENTATION_COUNT],
}

impl TargetAtomicFacts {
    /// Creates complete per-representation target facts.
    pub const fn new(
        u8: TargetAtomicRepresentationFacts,
        u16: TargetAtomicRepresentationFacts,
        u32: TargetAtomicRepresentationFacts,
        u64: TargetAtomicRepresentationFacts,
        u128: TargetAtomicRepresentationFacts,
        pointer: TargetAtomicRepresentationFacts,
    ) -> Self {
        Self {
            representations: [u8, u16, u32, u64, u128, pointer],
        }
    }

    /// Returns whether any atomic representation is available.
    pub const fn any(self) -> bool {
        self.u8() || self.u16() || self.u32() || self.u64() || self.u128() || self.pointer()
    }

    /// Returns facts for one representation.
    pub const fn representation(
        self,
        representation: TargetAtomicRepresentation,
    ) -> TargetAtomicRepresentationFacts {
        self.representations[representation.index()]
    }

    /// Returns whether atomic `u8` operations are available.
    pub const fn u8(self) -> bool {
        self.representation(TargetAtomicRepresentation::U8)
            .operations()
            .any()
    }

    /// Returns whether atomic `u16` operations are available.
    pub const fn u16(self) -> bool {
        self.representation(TargetAtomicRepresentation::U16)
            .operations()
            .any()
    }

    /// Returns whether atomic `u32` operations are available.
    pub const fn u32(self) -> bool {
        self.representation(TargetAtomicRepresentation::U32)
            .operations()
            .any()
    }

    /// Returns whether atomic `u64` operations are available.
    pub const fn u64(self) -> bool {
        self.representation(TargetAtomicRepresentation::U64)
            .operations()
            .any()
    }

    /// Returns whether atomic `u128` operations are available.
    pub const fn u128(self) -> bool {
        self.representation(TargetAtomicRepresentation::U128)
            .operations()
            .any()
    }

    /// Returns whether atomic pointer operations are available.
    pub const fn pointer(self) -> bool {
        self.representation(TargetAtomicRepresentation::Pointer)
            .operations()
            .any()
    }
}

impl Default for TargetAtomicFacts {
    fn default() -> Self {
        Self::new(
            unavailable(1),
            unavailable(2),
            unavailable(4),
            unavailable(8),
            unavailable(16),
            unavailable(8),
        )
    }
}

const fn unavailable(alignment: u64) -> TargetAtomicRepresentationFacts {
    let Some(alignment) = NonZeroU64::new(alignment) else {
        panic!("atomic alignment must be nonzero");
    };

    TargetAtomicRepresentationFacts::unavailable(alignment)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use super::{
        TargetAtomicOperationFacts, TargetAtomicRepresentation, TargetAtomicRepresentationFacts,
    };

    #[test]
    fn plain_storage_sizes_map_only_to_supported_integer_representations() {
        assert_eq!(
            TargetAtomicRepresentation::for_storage_size(4),
            Some(TargetAtomicRepresentation::U32)
        );

        assert_eq!(TargetAtomicRepresentation::for_storage_size(3), None);
        assert_eq!(TargetAtomicRepresentation::for_storage_size(0), None);
    }

    #[test]
    fn wait_requires_atomic_load_support() {
        let alignment = NonZeroU64::new(4).unwrap_or(NonZeroU64::MIN);
        let exchange_only = TargetAtomicOperationFacts::new(false, true, false, false, false);

        assert_eq!(
            TargetAtomicRepresentationFacts::try_new(
                exchange_only,
                alignment,
                false,
                true,
                false,
            ),
            None
        );
    }
}
