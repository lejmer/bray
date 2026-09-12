/// An independent property of ordinary callable execution.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ExecutionProperty {
    /// Execution has no runtime effects, including cleanup effects.
    Pure,
    /// Execution and cleanup terminate normally on valid inputs.
    Total,
}

impl ExecutionProperty {
    /// Resolves a language execution property name.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "pure" => Some(Self::Pure),
            "total" => Some(Self::Total),
            _ => None,
        }
    }

    /// Returns the property's source spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pure => "pure",
            Self::Total => "total",
        }
    }
}
