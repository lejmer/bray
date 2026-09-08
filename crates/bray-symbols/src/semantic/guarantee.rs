use crate::SymbolOrdinal;

/// A callable execution property that must be verified on its declared input domain.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ExecutionProperty {
    /// Execution observes inputs and computes values without runtime effects.
    Pure,
    /// Execution, including its cleanup, terminates normally.
    Total,
}

impl ExecutionProperty {
    /// Resolves one language-defined property name.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "pure" => Some(Self::Pure),
            "total" => Some(Self::Total),
            _ => None,
        }
    }

    /// Returns the property's language spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pure => "pure",
            Self::Total => "total",
        }
    }
}

/// A declared execution promise, not evidence that its implementation satisfies it.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableExecutionGuarantee {
    property: ExecutionProperty,
    guard: Option<SymbolOrdinal>,
}

impl CallableExecutionGuarantee {
    /// Declares a property under the supplied entry guard, or unconditionally.
    pub const fn new(property: ExecutionProperty, guard: Option<SymbolOrdinal>) -> Self {
        Self { property, guard }
    }

    /// Returns the promised execution property.
    pub const fn property(self) -> ExecutionProperty {
        self.property
    }

    /// Returns the entry guard's ordinal in the owning contract, if conditional.
    pub const fn guard(self) -> Option<SymbolOrdinal> {
        self.guard
    }
}

#[cfg(test)]
mod tests {
    use super::ExecutionProperty;

    #[test]
    fn execution_property_vocabulary_is_closed_and_independent() {
        for property in [ExecutionProperty::Pure, ExecutionProperty::Total] {
            assert_eq!(
                ExecutionProperty::from_name(property.as_str()),
                Some(property)
            );
        }

        for invalid in ["", "Pure", "const", "pure,total", "io", "pure()"] {
            assert_eq!(ExecutionProperty::from_name(invalid), None);
        }
    }
}
