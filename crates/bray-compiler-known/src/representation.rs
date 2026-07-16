define_catalog_enum! {
    /// Identifies language-defined representation behavior for a catalog entry.
    ///
    /// Roles do not contain target layout values. The selected target profile and
    /// the owning semantic phase derive layout and operational facts separately.
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

#[cfg(test)]
mod tests {
    use super::RepresentationRole;

    #[test]
    fn type_and_special_value_roles_remain_distinct() {
        assert_ne!(
            RepresentationRole::ScalarBool,
            RepresentationRole::BooleanTrue
        );

        assert_ne!(RepresentationRole::Unit, RepresentationRole::UnitValue);
    }
}
