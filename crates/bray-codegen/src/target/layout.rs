use std::num::{NonZeroU16, NonZeroU32};
use std::sync::Arc;

/// Scalar category whose exact target representation is fixed before code generation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TargetScalarKind {
    /// Bray boolean storage.
    Boolean,
    /// An integer with the given value width in bits.
    Integer(NonZeroU16),
    /// A floating-point value with the given value width in bits.
    Float(NonZeroU16),
}

/// Size and alignment of one target scalar representation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetScalarLayout {
    kind: TargetScalarKind,
    size_bytes: NonZeroU16,
    alignment_bytes: NonZeroU16,
}

impl TargetScalarLayout {
    /// Creates one scalar representation with sufficient storage and power-of-two alignment.
    pub fn try_new(
        kind: TargetScalarKind,
        size_bytes: NonZeroU16,
        alignment_bytes: NonZeroU16,
    ) -> Result<Self, TargetScalarLayoutBuildError> {
        if !alignment_bytes.get().is_power_of_two() {
            return Err(TargetScalarLayoutBuildError::InvalidAlignment);
        }

        let value_width_bits = match kind {
            TargetScalarKind::Boolean => 1,
            TargetScalarKind::Integer(width) | TargetScalarKind::Float(width) => {
                u32::from(width.get())
            }
        };

        let storage_width_bits = u32::from(size_bytes.get()) * 8;

        if storage_width_bits < value_width_bits {
            return Err(TargetScalarLayoutBuildError::ValueWidthExceedsStorage);
        }

        Ok(Self {
            kind,
            size_bytes,
            alignment_bytes,
        })
    }

    /// Returns the represented scalar category.
    pub const fn kind(self) -> TargetScalarKind {
        self.kind
    }

    /// Returns the scalar storage size in bytes.
    pub const fn size_bytes(self) -> NonZeroU16 {
        self.size_bytes
    }

    /// Returns the scalar ABI alignment in bytes.
    pub const fn alignment_bytes(self) -> NonZeroU16 {
        self.alignment_bytes
    }
}

/// A contract violation that prevents creation of a scalar layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetScalarLayoutBuildError {
    /// The selected storage size cannot contain the scalar value width.
    ValueWidthExceedsStorage,
    /// Scalar ABI alignment is not a power of two.
    InvalidAlignment,
}

/// Language-level address-space role used by lowered MIR.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TargetAddressSpaceKind {
    /// Ordinary pointers without a more specific role.
    Default,
    /// Executable function addresses.
    Function,
    /// Mutable global storage.
    Global,
    /// Read-only constant storage.
    Constant,
    /// Stack storage.
    Stack,
    /// Heap storage.
    Heap,
    /// Device-visible storage selected by the target profile.
    Device,
}

/// Target address-space number assigned to one language-level role.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetAddressSpace {
    kind: TargetAddressSpaceKind,
    number: u32,
}

impl TargetAddressSpace {
    /// Creates one target address-space mapping.
    pub const fn new(kind: TargetAddressSpaceKind, number: u32) -> Self {
        Self { kind, number }
    }

    /// Returns the language-level address-space role.
    pub const fn kind(self) -> TargetAddressSpaceKind {
        self.kind
    }

    /// Returns the backend-neutral target address-space number.
    pub const fn number(self) -> u32 {
        self.number
    }
}

/// Complete scalar, aggregate, and address-space layout rules for one target.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetDataLayout {
    scalars: Arc<[TargetScalarLayout]>,
    aggregate_alignment_bytes: NonZeroU32,
    address_spaces: Arc<[TargetAddressSpace]>,
}

impl TargetDataLayout {
    /// Creates a layout with unique scalar and address-space roles and a required default space.
    pub fn try_new(
        scalars: impl IntoIterator<Item = TargetScalarLayout>,
        aggregate_alignment_bytes: NonZeroU32,
        address_spaces: impl IntoIterator<Item = TargetAddressSpace>,
    ) -> Result<Self, TargetDataLayoutBuildError> {
        let mut scalars: Vec<_> = scalars.into_iter().collect();
        let mut address_spaces: Vec<_> = address_spaces.into_iter().collect();

        scalars.sort_unstable_by_key(|layout| layout.kind());
        address_spaces.sort_unstable_by_key(|space| space.kind());

        if scalars.is_empty() {
            return Err(TargetDataLayoutBuildError::MissingScalarLayouts);
        }

        if has_duplicate_scalar_kinds(&scalars) {
            return Err(TargetDataLayoutBuildError::DuplicateScalarKind);
        }

        if has_duplicate_address_space_kinds(&address_spaces) {
            return Err(TargetDataLayoutBuildError::DuplicateAddressSpaceKind);
        }

        if !address_spaces
            .iter()
            .any(|space| space.kind() == TargetAddressSpaceKind::Default)
        {
            return Err(TargetDataLayoutBuildError::MissingDefaultAddressSpace);
        }

        Ok(Self {
            scalars: scalars.into(),
            aggregate_alignment_bytes,
            address_spaces: address_spaces.into(),
        })
    }

    /// Returns scalar layouts in canonical category order.
    pub fn scalars(&self) -> &[TargetScalarLayout] {
        &self.scalars
    }

    /// Returns the minimum aggregate alignment in bytes.
    pub const fn aggregate_alignment_bytes(&self) -> NonZeroU32 {
        self.aggregate_alignment_bytes
    }

    /// Returns address-space mappings in canonical role order.
    pub fn address_spaces(&self) -> &[TargetAddressSpace] {
        &self.address_spaces
    }

    /// Returns the target number assigned to one address-space role.
    pub fn address_space(&self, kind: TargetAddressSpaceKind) -> Option<u32> {
        self.address_spaces
            .binary_search_by_key(&kind, |space| space.kind())
            .ok()
            .map(|index| self.address_spaces[index].number())
    }
}

/// A contract violation that prevents creation of a target data layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetDataLayoutBuildError {
    /// No scalar representation was supplied.
    MissingScalarLayouts,
    /// Two scalar layouts describe the same scalar category.
    DuplicateScalarKind,
    /// Two address-space mappings describe the same language-level role.
    DuplicateAddressSpaceKind,
    /// The ordinary pointer address space was not mapped.
    MissingDefaultAddressSpace,
}

fn has_duplicate_scalar_kinds(scalars: &[TargetScalarLayout]) -> bool {
    scalars
        .windows(2)
        .any(|pair| pair[0].kind() == pair[1].kind())
}

fn has_duplicate_address_space_kinds(address_spaces: &[TargetAddressSpace]) -> bool {
    address_spaces
        .windows(2)
        .any(|pair| pair[0].kind() == pair[1].kind())
}

#[cfg(test)]
mod tests {
    use std::num::{NonZeroU16, NonZeroU32};

    use super::{
        TargetAddressSpace, TargetAddressSpaceKind, TargetDataLayout, TargetDataLayoutBuildError,
        TargetScalarKind, TargetScalarLayout,
    };

    #[test]
    fn data_layouts_require_unique_roles_and_a_default_address_space() {
        let byte = NonZeroU16::new(1).unwrap_or(NonZeroU16::MIN);
        let aggregate = NonZeroU32::new(8).unwrap_or(NonZeroU32::MIN);

        let Ok(scalar) = TargetScalarLayout::try_new(TargetScalarKind::Boolean, byte, byte) else {
            panic!("test scalar layout must be valid");
        };

        assert_eq!(
            TargetDataLayout::try_new([scalar], aggregate, []),
            Err(TargetDataLayoutBuildError::MissingDefaultAddressSpace)
        );

        assert_eq!(
            TargetDataLayout::try_new(
                [scalar, scalar],
                aggregate,
                [TargetAddressSpace::new(TargetAddressSpaceKind::Default, 0)]
            ),
            Err(TargetDataLayoutBuildError::DuplicateScalarKind)
        );
    }
}
