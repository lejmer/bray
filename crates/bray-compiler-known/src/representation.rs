/// Identifies language-defined representation behavior for a catalog entry.
///
/// Roles do not contain target layout values. The selected target profile and
/// the owning semantic phase derive layout and operational facts separately.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RepresentationRole {
    /// The Boolean scalar type.
    ScalarBool,
    /// The Unicode scalar-value type.
    ScalarChar,
    /// A signed 8-bit integer scalar.
    ScalarI8,
    /// A signed 16-bit integer scalar.
    ScalarI16,
    /// A signed 32-bit integer scalar.
    ScalarI32,
    /// A signed 64-bit integer scalar.
    ScalarI64,
    /// A signed 128-bit integer scalar.
    ScalarI128,
    /// An unsigned 8-bit integer scalar.
    ScalarU8,
    /// An unsigned 16-bit integer scalar.
    ScalarU16,
    /// An unsigned 32-bit integer scalar.
    ScalarU32,
    /// An unsigned 64-bit integer scalar.
    ScalarU64,
    /// An unsigned 128-bit integer scalar.
    ScalarU128,
    /// A machine-sized signed integer scalar.
    ScalarIsize,
    /// A machine-sized unsigned integer scalar.
    ScalarUsize,
    /// A 16-bit real scalar.
    ScalarR16,
    /// A 32-bit real scalar.
    ScalarR32,
    /// A 64-bit real scalar.
    ScalarR64,
    /// A 128-bit real scalar.
    ScalarR128,
    /// A 32-bit complex scalar.
    ScalarC32,
    /// A 64-bit complex scalar.
    ScalarC64,
    /// A 128-bit complex scalar.
    ScalarC128,
    /// A 256-bit complex scalar.
    ScalarC256,
    /// The unit type.
    Unit,
    /// The uninhabited type.
    Never,
    /// The compiler-known string type.
    String,
    /// A compiler-controlled raw pointer.
    RawPointer,
    /// A compiler-known result value.
    Result,
    /// A compiler-known run-boundary result value.
    RunResult,
    /// A protected panic report.
    PanicReport,
    /// A compiler-known conversion error.
    ConversionError,
    /// A protected task handle.
    Task,
    /// A protected thread handle.
    Thread,
    /// The language-known `true` value.
    BooleanTrue,
    /// The language-known `false` value.
    BooleanFalse,
    /// The language-known unit value.
    UnitValue,
    /// The language-known absent nullable value.
    NoneValue,
}

impl RepresentationRole {
    pub(crate) fn from_catalog_spelling(spelling: &str) -> Option<Self> {
        match spelling {
            "ScalarBool" => Some(Self::ScalarBool),
            "ScalarChar" => Some(Self::ScalarChar),
            "ScalarI8" => Some(Self::ScalarI8),
            "ScalarI16" => Some(Self::ScalarI16),
            "ScalarI32" => Some(Self::ScalarI32),
            "ScalarI64" => Some(Self::ScalarI64),
            "ScalarI128" => Some(Self::ScalarI128),
            "ScalarU8" => Some(Self::ScalarU8),
            "ScalarU16" => Some(Self::ScalarU16),
            "ScalarU32" => Some(Self::ScalarU32),
            "ScalarU64" => Some(Self::ScalarU64),
            "ScalarU128" => Some(Self::ScalarU128),
            "ScalarIsize" => Some(Self::ScalarIsize),
            "ScalarUsize" => Some(Self::ScalarUsize),
            "ScalarR16" => Some(Self::ScalarR16),
            "ScalarR32" => Some(Self::ScalarR32),
            "ScalarR64" => Some(Self::ScalarR64),
            "ScalarR128" => Some(Self::ScalarR128),
            "ScalarC32" => Some(Self::ScalarC32),
            "ScalarC64" => Some(Self::ScalarC64),
            "ScalarC128" => Some(Self::ScalarC128),
            "ScalarC256" => Some(Self::ScalarC256),
            "Unit" => Some(Self::Unit),
            "Never" => Some(Self::Never),
            "String" => Some(Self::String),
            "RawPointer" => Some(Self::RawPointer),
            "Result" => Some(Self::Result),
            "RunResult" => Some(Self::RunResult),
            "PanicReport" => Some(Self::PanicReport),
            "ConversionError" => Some(Self::ConversionError),
            "Task" => Some(Self::Task),
            "Thread" => Some(Self::Thread),
            "BooleanTrue" => Some(Self::BooleanTrue),
            "BooleanFalse" => Some(Self::BooleanFalse),
            "UnitValue" => Some(Self::UnitValue),
            "NoneValue" => Some(Self::NoneValue),
            _ => None,
        }
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
