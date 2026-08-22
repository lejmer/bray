use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates a target representation argument.
    pub const fn target_representation(kind: DiagnosticTargetRepresentation) -> Self {
        Self::new(
            DiagnosticArgName::TargetRepresentation,
            DiagnosticArgValue::TargetRepresentation(kind),
        )
    }
}

/// Locale-neutral target representation categories used by target diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticTargetRepresentation {
    Unsupported,
    Bool,
    Char,
    I8,
    I16,
    I32,
    I64,
    I128,
    U8,
    U16,
    U32,
    U64,
    U128,
    Isize,
    Usize,
    R16,
    R32,
    R64,
    R128,
    C32,
    C64,
    C128,
    C256,
    RawPointer,
    AbiQualifiedCallable,
    DefaultLayoutAggregate,
    StableLayoutAggregate,
    CLayoutAggregate,
    TransparentLayoutAggregate,
}

impl DiagnosticTargetRepresentation {
    /// Returns the stable machine key for this representation category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unsupported => "unsupported",
            Self::Bool => "bool",
            Self::Char => "char",
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::I128 => "i128",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::U128 => "u128",
            Self::Isize => "isize",
            Self::Usize => "usize",
            Self::R16 => "r16",
            Self::R32 => "r32",
            Self::R64 => "r64",
            Self::R128 => "r128",
            Self::C32 => "c32",
            Self::C64 => "c64",
            Self::C128 => "c128",
            Self::C256 => "c256",
            Self::RawPointer => "raw_pointer",
            Self::AbiQualifiedCallable => "abi_qualified_callable",
            Self::DefaultLayoutAggregate => "default_layout_aggregate",
            Self::StableLayoutAggregate => "stable_layout_aggregate",
            Self::CLayoutAggregate => "c_layout_aggregate",
            Self::TransparentLayoutAggregate => "transparent_layout_aggregate",
        }
    }
}
