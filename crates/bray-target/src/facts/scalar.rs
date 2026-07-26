use std::num::NonZeroU64;

const SCALAR_KIND_COUNT: usize = 22;

/// One built-in scalar representation described by target and ABI facts.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TargetScalarKind {
    /// The Boolean scalar.
    Bool,
    /// The Unicode scalar-value representation.
    Char,
    /// The signed 8-bit integer scalar.
    I8,
    /// The signed 16-bit integer scalar.
    I16,
    /// The signed 32-bit integer scalar.
    I32,
    /// The signed 64-bit integer scalar.
    I64,
    /// The signed 128-bit integer scalar.
    I128,
    /// The unsigned 8-bit integer scalar.
    U8,
    /// The unsigned 16-bit integer scalar.
    U16,
    /// The unsigned 32-bit integer scalar.
    U32,
    /// The unsigned 64-bit integer scalar.
    U64,
    /// The unsigned 128-bit integer scalar.
    U128,
    /// The target-sized signed integer scalar.
    Isize,
    /// The target-sized unsigned integer scalar.
    Usize,
    /// The 16-bit real scalar.
    R16,
    /// The 32-bit real scalar.
    R32,
    /// The 64-bit real scalar.
    R64,
    /// The 128-bit real scalar.
    R128,
    /// The complex scalar with two 16-bit components.
    C32,
    /// The complex scalar with two 32-bit components.
    C64,
    /// The complex scalar with two 64-bit components.
    C128,
    /// The complex scalar with two 128-bit components.
    C256,
}

impl TargetScalarKind {
    /// Every scalar kind in stable language order.
    pub const ALL: [Self; SCALAR_KIND_COUNT] = [
        Self::Bool,
        Self::Char,
        Self::I8,
        Self::I16,
        Self::I32,
        Self::I64,
        Self::I128,
        Self::U8,
        Self::U16,
        Self::U32,
        Self::U64,
        Self::U128,
        Self::Isize,
        Self::Usize,
        Self::R16,
        Self::R32,
        Self::R64,
        Self::R128,
        Self::C32,
        Self::C64,
        Self::C128,
        Self::C256,
    ];

    const fn index(self) -> usize {
        match self {
            Self::Bool => 0,
            Self::Char => 1,
            Self::I8 => 2,
            Self::I16 => 3,
            Self::I32 => 4,
            Self::I64 => 5,
            Self::I128 => 6,
            Self::U8 => 7,
            Self::U16 => 8,
            Self::U32 => 9,
            Self::U64 => 10,
            Self::U128 => 11,
            Self::Isize => 12,
            Self::Usize => 13,
            Self::R16 => 14,
            Self::R32 => 15,
            Self::R64 => 16,
            Self::R128 => 17,
            Self::C32 => 18,
            Self::C64 => 19,
            Self::C128 => 20,
            Self::C256 => 21,
        }
    }

    pub(crate) const fn bit(self) -> u32 {
        1 << self.index()
    }
}

/// Availability and physical alignment of scalar representations.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetScalarFacts {
    real16: bool,
    real128: bool,
    complex32: bool,
    complex256: bool,
    alignments: [NonZeroU64; SCALAR_KIND_COUNT],
}

impl TargetScalarFacts {
    /// Creates scalar facts with the portable baseline alignments.
    pub const fn new(real16: bool, real128: bool, complex32: bool, complex256: bool) -> Self {
        Self {
            real16,
            real128,
            complex32,
            complex256,
            alignments: portable_alignments(),
        }
    }

    /// Returns a copy with one scalar alignment when it is a power of two.
    pub const fn try_with_alignment(
        mut self,
        kind: TargetScalarKind,
        alignment: NonZeroU64,
    ) -> Option<Self> {
        if !alignment.get().is_power_of_two() {
            return None;
        }

        self.alignments[kind.index()] = alignment;

        Some(self)
    }

    /// Returns whether the target provides this scalar representation.
    pub const fn supports(self, kind: TargetScalarKind) -> bool {
        match kind {
            TargetScalarKind::R16 => self.real16,
            TargetScalarKind::R128 => self.real128,
            TargetScalarKind::C32 => self.complex32,
            TargetScalarKind::C256 => self.complex256,
            _ => true,
        }
    }

    /// Returns the scalar's physical alignment on this target.
    pub const fn alignment(self, kind: TargetScalarKind) -> NonZeroU64 {
        self.alignments[kind.index()]
    }

    /// Returns whether `r16` is available.
    pub const fn real16(self) -> bool {
        self.real16
    }

    /// Returns whether `r128` is available.
    pub const fn real128(self) -> bool {
        self.real128
    }

    /// Returns whether `c32` is available.
    pub const fn complex32(self) -> bool {
        self.complex32
    }

    /// Returns whether `c256` is available.
    pub const fn complex256(self) -> bool {
        self.complex256
    }
}

impl Default for TargetScalarFacts {
    fn default() -> Self {
        Self::new(false, false, false, false)
    }
}

const fn portable_alignments() -> [NonZeroU64; SCALAR_KIND_COUNT] {
    [
        nonzero(1),
        nonzero(4),
        nonzero(1),
        nonzero(2),
        nonzero(4),
        nonzero(8),
        nonzero(16),
        nonzero(1),
        nonzero(2),
        nonzero(4),
        nonzero(8),
        nonzero(16),
        nonzero(8),
        nonzero(8),
        nonzero(2),
        nonzero(4),
        nonzero(8),
        nonzero(16),
        nonzero(2),
        nonzero(4),
        nonzero(8),
        nonzero(16),
    ]
}

const fn nonzero(value: u64) -> NonZeroU64 {
    match NonZeroU64::new(value) {
        Some(value) => value,
        None => panic!("scalar alignment must be nonzero"),
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use super::{TargetScalarFacts, TargetScalarKind};

    #[test]
    fn scalar_alignments_are_selected_target_facts() {
        let alignment = NonZeroU64::new(4).unwrap_or(NonZeroU64::MIN);

        let facts = TargetScalarFacts::default()
            .try_with_alignment(TargetScalarKind::I64, alignment)
            .unwrap_or_else(|| panic!("test scalar alignment must be valid"));

        assert_eq!(facts.alignment(TargetScalarKind::I64), alignment);

        assert_eq!(
            TargetScalarFacts::default()
                .alignment(TargetScalarKind::I64)
                .get(),
            8
        );
    }
}
