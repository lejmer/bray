use serde::Serialize;

#[derive(Serialize)]
pub(in crate::output::diagnostic::json) struct DiagnosticRuntimeArtifactProblemJson {
    category: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    purpose: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    capability: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    component: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    native_cause: Option<&'static str>,
}

impl DiagnosticRuntimeArtifactProblemJson {
    pub(in crate::output::diagnostic::json) fn from_problem(
        problem: &bray_diagnostics::DiagnosticRuntimeArtifactProblem,
    ) -> Self {
        use bray_diagnostics::DiagnosticRuntimeArtifactProblem as Problem;

        let mut output = Self {
            category: problem.as_str(),
            purpose: None,
            role: None,
            capability: None,
            component: None,
            native_cause: None,
        };

        match problem {
            Problem::DuplicateContractRole(role)
            | Problem::CompilerOwnedRole(role)
            | Problem::UnknownComponentRole(role) => {
                output.role = Some(role.to_owned());
            }
            Problem::UnknownComponentCapability(capability) => {
                output.capability = Some(capability.to_owned());
            }
            Problem::DuplicateComponent(component)
            | Problem::TestRoleInProductComponent(component) => {
                output.component = Some(component.to_owned());
            }
            Problem::MissingRoleOwner { purpose, role }
            | Problem::DuplicateRoleOwner { purpose, role } => {
                output.purpose = Some(purpose.as_str());
                output.role = Some(role.to_owned());
            }
            Problem::MissingCapabilityOwner {
                purpose,
                capability,
            }
            | Problem::DuplicateCapabilityOwner {
                purpose,
                capability,
            } => {
                output.purpose = Some(purpose.as_str());
                output.capability = Some(capability.to_owned());
            }
            Problem::DuplicatePlatformServiceOwner { purpose } => {
                output.purpose = Some(purpose.as_str());
            }
            Problem::InvalidNativeArtifact(cause) => {
                output.native_cause = Some(cause.as_str());
            }
            Problem::MetadataSizeLimitExceeded
            | Problem::MalformedMetadata
            | Problem::UnsupportedFormat
            | Problem::InvalidRuntimeIdentity
            | Problem::InvalidArtifactIdentity
            | Problem::InvalidTarget
            | Problem::InvalidPanicAbi
            | Problem::UnknownCapability
            | Problem::UnknownRole
            | Problem::UnknownPlatformService
            | Problem::InvalidRoleSymbol
            | Problem::UnknownRoleImplementation
            | Problem::UnknownComponentPurpose
            | Problem::InvalidComponentIdentity
            | Problem::InvalidNativeIndexDigest
            | Problem::MissingCooperativeExecution
            | Problem::InvalidNativeIndexFileName
            | Problem::InvalidNativeIndexes
            | Problem::InvalidNativeTarget
            | Problem::IncompatibleIndexTarget => {}
        }

        output
    }
}
