define_catalog_enum! {
    /// Identifies language-defined representation behavior for a catalog entry.
    ///
    /// Roles do not contain target layout values. The selected target profile and
    /// the owning semantic phase derive layout and operational contracts separately.
    #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
    pub enum RepresentationRole {
        /// The Boolean scalar type.
        ScalarBool => "ScalarBool",
        /// The Unicode scalar-value type.
        ScalarChar => "ScalarChar",
        /// A signed 8-bit integer scalar.
        ScalarI8 => "ScalarI8",
        /// A signed 16-bit integer scalar.
        ScalarI16 => "ScalarI16",
        /// A signed 32-bit integer scalar.
        ScalarI32 => "ScalarI32",
        /// A signed 64-bit integer scalar.
        ScalarI64 => "ScalarI64",
        /// A signed 128-bit integer scalar.
        ScalarI128 => "ScalarI128",
        /// An unsigned 8-bit integer scalar.
        ScalarU8 => "ScalarU8",
        /// An unsigned 16-bit integer scalar.
        ScalarU16 => "ScalarU16",
        /// An unsigned 32-bit integer scalar.
        ScalarU32 => "ScalarU32",
        /// An unsigned 64-bit integer scalar.
        ScalarU64 => "ScalarU64",
        /// An unsigned 128-bit integer scalar.
        ScalarU128 => "ScalarU128",
        /// A machine-sized signed integer scalar.
        ScalarIsize => "ScalarIsize",
        /// A machine-sized unsigned integer scalar.
        ScalarUsize => "ScalarUsize",
        /// A 16-bit real scalar.
        ScalarR16 => "ScalarR16",
        /// A 32-bit real scalar.
        ScalarR32 => "ScalarR32",
        /// A 64-bit real scalar.
        ScalarR64 => "ScalarR64",
        /// A 128-bit real scalar.
        ScalarR128 => "ScalarR128",
        /// A 32-bit complex scalar.
        ScalarC32 => "ScalarC32",
        /// A 64-bit complex scalar.
        ScalarC64 => "ScalarC64",
        /// A 128-bit complex scalar.
        ScalarC128 => "ScalarC128",
        /// A 256-bit complex scalar.
        ScalarC256 => "ScalarC256",
        /// The unit type.
        Unit => "Unit",
        /// The uninhabited type.
        Never => "Never",
        /// The compiler-known string type.
        String => "String",
        /// A compiler-controlled raw pointer.
        RawPointer => "RawPointer",
        /// A compiler-controlled pointer in the selected target's device address space.
        DevicePointer => "DevicePointer",
        /// Protected compiler-controlled atomic storage.
        Atomic => "Atomic",
        /// Protected storage with the size and alignment of its represented value.
        Uninit => "Uninit",
        /// A compiler-known result value.
        Result => "Result",
        /// A compiler-known run-boundary result value.
        RunResult => "RunResult",
        /// A protected panic report.
        PanicReport => "PanicReport",
        /// A compiler-known conversion error.
        ConversionError => "ConversionError",
        /// An owned inactive asynchronous computation.
        Future => "Future",
        /// A protected task handle.
        Task => "Task",
        /// The language-known `true` value.
        BooleanTrue => "BooleanTrue",
        /// The language-known `false` value.
        BooleanFalse => "BooleanFalse",
        /// The language-known unit value.
        UnitValue => "UnitValue",
        /// The language-known absent nullable value.
        NoneValue => "NoneValue",
    }
}

/// Classifies the numeric domain represented by a compiler-known scalar role.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NumericRepresentationKind {
    /// A signed, unsigned, or machine-sized integer representation.
    Integer,
    /// A real floating-point representation.
    Real,
    /// A complex floating-point representation.
    Complex,
}

/// The signedness and fixed or target-selected width of an integer scalar role.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum IntegerRepresentation {
    /// A signed integer with the given bit width.
    Signed(u16),
    /// An unsigned integer with the given bit width.
    Unsigned(u16),
    /// A signed integer whose width comes from the selected target.
    TargetSigned,
    /// An unsigned integer whose width comes from the selected target.
    TargetUnsigned,
}

impl RepresentationRole {
    /// Returns the numeric representation category for this role.
    pub const fn numeric_kind(self) -> Option<NumericRepresentationKind> {
        match self {
            Self::ScalarI8
            | Self::ScalarI16
            | Self::ScalarI32
            | Self::ScalarI64
            | Self::ScalarI128
            | Self::ScalarU8
            | Self::ScalarU16
            | Self::ScalarU32
            | Self::ScalarU64
            | Self::ScalarU128
            | Self::ScalarIsize
            | Self::ScalarUsize => Some(NumericRepresentationKind::Integer),
            Self::ScalarR16 | Self::ScalarR32 | Self::ScalarR64 | Self::ScalarR128 => {
                Some(NumericRepresentationKind::Real)
            }
            Self::ScalarC32 | Self::ScalarC64 | Self::ScalarC128 | Self::ScalarC256 => {
                Some(NumericRepresentationKind::Complex)
            }
            _ => None,
        }
    }

    /// Returns the real component representation of a complex scalar role.
    pub const fn complex_component(self) -> Option<Self> {
        match self {
            Self::ScalarC32 => Some(Self::ScalarR16),
            Self::ScalarC64 => Some(Self::ScalarR32),
            Self::ScalarC128 => Some(Self::ScalarR64),
            Self::ScalarC256 => Some(Self::ScalarR128),
            _ => None,
        }
    }

    /// Returns the integer representation described by this scalar role.
    pub const fn integer_representation(self) -> Option<IntegerRepresentation> {
        match self {
            Self::ScalarI8 => Some(IntegerRepresentation::Signed(8)),
            Self::ScalarI16 => Some(IntegerRepresentation::Signed(16)),
            Self::ScalarI32 => Some(IntegerRepresentation::Signed(32)),
            Self::ScalarI64 => Some(IntegerRepresentation::Signed(64)),
            Self::ScalarI128 => Some(IntegerRepresentation::Signed(128)),
            Self::ScalarU8 => Some(IntegerRepresentation::Unsigned(8)),
            Self::ScalarU16 => Some(IntegerRepresentation::Unsigned(16)),
            Self::ScalarU32 => Some(IntegerRepresentation::Unsigned(32)),
            Self::ScalarU64 => Some(IntegerRepresentation::Unsigned(64)),
            Self::ScalarU128 => Some(IntegerRepresentation::Unsigned(128)),
            Self::ScalarIsize => Some(IntegerRepresentation::TargetSigned),
            Self::ScalarUsize => Some(IntegerRepresentation::TargetUnsigned),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{IntegerRepresentation, NumericRepresentationKind, RepresentationRole};

    #[test]
    fn type_and_special_value_roles_remain_distinct() {
        assert_ne!(
            RepresentationRole::ScalarBool,
            RepresentationRole::BooleanTrue
        );

        assert_ne!(RepresentationRole::Unit, RepresentationRole::UnitValue);
    }

    #[test]
    fn numeric_roles_report_their_numeric_domain() {
        let cases = [
            (
                RepresentationRole::ScalarI32,
                Some(NumericRepresentationKind::Integer),
            ),
            (
                RepresentationRole::ScalarUsize,
                Some(NumericRepresentationKind::Integer),
            ),
            (
                RepresentationRole::ScalarR64,
                Some(NumericRepresentationKind::Real),
            ),
            (
                RepresentationRole::ScalarC128,
                Some(NumericRepresentationKind::Complex),
            ),
            (RepresentationRole::ScalarBool, None),
            (RepresentationRole::Future, None),
        ];

        for (role, expected) in cases {
            assert_eq!(role.numeric_kind(), expected);
        }
    }

    #[test]
    fn complex_roles_report_their_real_component_representation() {
        let cases = [
            (RepresentationRole::ScalarC32, RepresentationRole::ScalarR16),
            (RepresentationRole::ScalarC64, RepresentationRole::ScalarR32),
            (
                RepresentationRole::ScalarC128,
                RepresentationRole::ScalarR64,
            ),
            (
                RepresentationRole::ScalarC256,
                RepresentationRole::ScalarR128,
            ),
        ];

        for (complex, real) in cases {
            assert_eq!(complex.complex_component(), Some(real));
        }

        assert_eq!(RepresentationRole::ScalarR64.complex_component(), None);
    }

    #[test]
    fn integer_roles_report_width_and_signedness() {
        let cases = [
            (
                RepresentationRole::ScalarI8,
                IntegerRepresentation::Signed(8),
            ),
            (
                RepresentationRole::ScalarU128,
                IntegerRepresentation::Unsigned(128),
            ),
            (
                RepresentationRole::ScalarIsize,
                IntegerRepresentation::TargetSigned,
            ),
            (
                RepresentationRole::ScalarUsize,
                IntegerRepresentation::TargetUnsigned,
            ),
        ];

        for (role, representation) in cases {
            assert_eq!(role.integer_representation(), Some(representation));
        }

        assert_eq!(RepresentationRole::ScalarR64.integer_representation(), None);
    }
}
