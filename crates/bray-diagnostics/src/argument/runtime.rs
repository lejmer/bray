use super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates an expected runtime ABI version argument.
    pub const fn expected_runtime_abi(version: DiagnosticRuntimeAbiVersion) -> Self {
        Self::new(
            DiagnosticArgName::ExpectedRuntimeAbi,
            DiagnosticArgValue::RuntimeAbi(version),
        )
    }

    /// Creates an actual runtime ABI version argument.
    pub const fn actual_runtime_abi(version: DiagnosticRuntimeAbiVersion) -> Self {
        Self::new(
            DiagnosticArgName::ActualRuntimeAbi,
            DiagnosticArgValue::RuntimeAbi(version),
        )
    }
}

/// Locale-neutral private runtime ABI version used by diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticRuntimeAbiVersion {
    major: u16,
    minor: u16,
}

impl DiagnosticRuntimeAbiVersion {
    /// Creates a runtime ABI version from its compatibility components.
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }

    /// Returns the compatibility-breaking component.
    pub const fn major(self) -> u16 {
        self.major
    }

    /// Returns the backwards-compatible component.
    pub const fn minor(self) -> u16 {
        self.minor
    }
}
