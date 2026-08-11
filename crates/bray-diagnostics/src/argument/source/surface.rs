use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates an expected declaration-visibility argument.
    pub const fn expected_visibility(visibility: DiagnosticVisibility) -> Self {
        Self::new(
            DiagnosticArgName::ExpectedVisibility,
            DiagnosticArgValue::Visibility(visibility),
        )
    }

    /// Creates an actual declaration-visibility argument.
    pub const fn actual_visibility(visibility: DiagnosticVisibility) -> Self {
        Self::new(
            DiagnosticArgName::ActualVisibility,
            DiagnosticArgValue::Visibility(visibility),
        )
    }

    /// Creates an expected module-trust argument.
    pub const fn expected_module_trust(trust: DiagnosticModuleTrust) -> Self {
        Self::new(
            DiagnosticArgName::ExpectedModuleTrust,
            DiagnosticArgValue::ModuleTrust(trust),
        )
    }

    /// Creates an actual module-trust argument.
    pub const fn actual_module_trust(trust: DiagnosticModuleTrust) -> Self {
        Self::new(
            DiagnosticArgName::ActualModuleTrust,
            DiagnosticArgValue::ModuleTrust(trust),
        )
    }
}

/// Locale-neutral effective visibility used by declaration diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticVisibility {
    /// Public declaration visibility.
    Public,
    /// Internal declaration visibility.
    Internal,
}

impl DiagnosticVisibility {
    /// Returns the stable machine key for this visibility.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Internal => "internal",
        }
    }
}

/// Locale-neutral module trust state used by declaration diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticModuleTrust {
    /// Module permits trusted declarations.
    Trusted,
    /// Module does not permit trusted declarations.
    Ordinary,
}

impl DiagnosticModuleTrust {
    /// Returns the stable machine key for this trust state.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Trusted => "trusted",
            Self::Ordinary => "ordinary",
        }
    }
}
