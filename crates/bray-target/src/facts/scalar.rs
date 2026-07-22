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
    pub(crate) const fn bit(self) -> u32 {
        match self {
            Self::Bool => 1 << 0,
            Self::Char => 1 << 1,
            Self::I8 => 1 << 2,
            Self::I16 => 1 << 3,
            Self::I32 => 1 << 4,
            Self::I64 => 1 << 5,
            Self::I128 => 1 << 6,
            Self::U8 => 1 << 7,
            Self::U16 => 1 << 8,
            Self::U32 => 1 << 9,
            Self::U64 => 1 << 10,
            Self::U128 => 1 << 11,
            Self::Isize => 1 << 12,
            Self::Usize => 1 << 13,
            Self::R16 => 1 << 14,
            Self::R32 => 1 << 15,
            Self::R64 => 1 << 16,
            Self::R128 => 1 << 17,
            Self::C32 => 1 << 18,
            Self::C64 => 1 << 19,
            Self::C128 => 1 << 20,
            Self::C256 => 1 << 21,
        }
    }
}

/// Availability of target-conditional scalar representations.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetScalarFacts {
    real16: bool,
    real128: bool,
    complex32: bool,
    complex256: bool,
}

impl TargetScalarFacts {
    /// Creates the target-conditional scalar availability facts.
    pub const fn new(real16: bool, real128: bool, complex32: bool, complex256: bool) -> Self {
        Self {
            real16,
            real128,
            complex32,
            complex256,
        }
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
