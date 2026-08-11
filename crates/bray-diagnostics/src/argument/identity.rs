use super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates an expected package-identity argument.
    pub fn expected_package_identity(identity: impl Into<String>) -> Self {
        Self::new(
            DiagnosticArgName::ExpectedPackageIdentity,
            DiagnosticArgValue::PackageIdentity(identity.into()),
        )
    }

    /// Creates an expected package-product identity argument.
    pub fn expected_product_identity(identity: impl Into<String>) -> Self {
        Self::new(
            DiagnosticArgName::ExpectedProductIdentity,
            DiagnosticArgValue::ProductIdentity(identity.into()),
        )
    }

    /// Creates a selected code generation backend identity argument.
    pub fn codegen_backend_identity(identity: impl Into<String>) -> Self {
        Self::new(
            DiagnosticArgName::CodegenBackendIdentity,
            DiagnosticArgValue::CodegenBackendIdentity(identity.into()),
        )
    }

    /// Creates a complete selected linker-driver identity argument.
    pub fn linker_driver_identity(identity: crate::DiagnosticLinkerDriverIdentity) -> Self {
        Self::new(
            DiagnosticArgName::LinkerDriverIdentity,
            DiagnosticArgValue::LinkerDriverIdentity(identity),
        )
    }
}
