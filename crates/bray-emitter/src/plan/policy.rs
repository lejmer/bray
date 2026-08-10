use std::sync::Arc;

use bray_codegen::{
    AssemblySyntaxKind, BackendCapabilities, BackendIdentity, BackendSerializationOptions,
    CodegenUnitKey, DebugInformationMode, DebugInformationOutputMode, LinkableArtifactKind,
};

/// Output-affecting backend policy used to derive per-unit artifact requests.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BackendEmissionPolicy {
    debug_information: DebugInformationMode,
    debug_output: DebugInformationOutputMode,
    linkable_artifact: Option<LinkableArtifactKind>,
    serialization: BackendSerializationOptions,
}

impl BackendEmissionPolicy {
    /// Creates typed backend output policy for one emission plan.
    pub const fn new(
        debug_information: DebugInformationMode,
        debug_output: DebugInformationOutputMode,
        linkable_artifact: Option<LinkableArtifactKind>,
        serialization: BackendSerializationOptions,
    ) -> Self {
        Self {
            debug_information,
            debug_output,
            linkable_artifact,
            serialization,
        }
    }

    /// Returns the requested amount of source-correlated debug information.
    pub const fn debug_information(self) -> DebugInformationMode {
        self.debug_information
    }

    /// Returns where requested debug information is serialized.
    pub const fn debug_output(self) -> DebugInformationOutputMode {
        self.debug_output
    }

    /// Returns the target-selected contribution needed by native linking.
    pub const fn linkable_artifact(self) -> Option<LinkableArtifactKind> {
        self.linkable_artifact
    }

    /// Returns output-affecting backend serialization policy.
    pub const fn serialization(self) -> BackendSerializationOptions {
        self.serialization
    }
}

impl Default for BackendEmissionPolicy {
    fn default() -> Self {
        Self::new(
            DebugInformationMode::None,
            DebugInformationOutputMode::Omit,
            None,
            BackendSerializationOptions::new(AssemblySyntaxKind::TargetDefault),
        )
    }
}

/// Selected backend facts and codegen-unit membership available to emission planning.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct EmissionBackend {
    identity: BackendIdentity,
    capabilities: BackendCapabilities,
    units: Arc<[CodegenUnitKey]>,
    policy: BackendEmissionPolicy,
}

impl EmissionBackend {
    /// Creates a backend selection with unique codegen units in canonical order.
    pub fn try_new(
        identity: BackendIdentity,
        capabilities: BackendCapabilities,
        units: impl IntoIterator<Item = CodegenUnitKey>,
        policy: BackendEmissionPolicy,
    ) -> Result<Self, EmissionBackendBuildError> {
        let mut units: Vec<_> = units.into_iter().collect();

        units.sort_unstable();

        if units.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(EmissionBackendBuildError::DuplicateCodegenUnit);
        }

        Ok(Self {
            identity,
            capabilities,
            units: units.into(),
            policy,
        })
    }

    /// Returns the selected backend and toolchain identity.
    pub const fn identity(&self) -> &BackendIdentity {
        &self.identity
    }

    /// Returns the selected backend's declared capabilities.
    pub const fn capabilities(&self) -> &BackendCapabilities {
        &self.capabilities
    }

    /// Returns planned codegen units in canonical structural-key order.
    pub fn units(&self) -> &[CodegenUnitKey] {
        &self.units
    }

    /// Returns output-affecting policy shared by every planned unit.
    pub const fn policy(&self) -> BackendEmissionPolicy {
        self.policy
    }
}

/// A contract violation in a selected emission backend.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EmissionBackendBuildError {
    /// One structural codegen unit was supplied more than once.
    DuplicateCodegenUnit,
}

#[cfg(test)]
mod tests {
    use bray_codegen::{
        AssemblySyntaxKind, BackendCapabilities, BackendSerializationOptions, DebugInformationMode,
        DebugInformationOutputMode, LinkableArtifactKind,
    };

    use super::{BackendEmissionPolicy, EmissionBackend, EmissionBackendBuildError};
    use crate::test_support::{backend_identity, codegen_unit_key};

    #[test]
    fn emission_backends_canonicalize_and_validate_unit_membership() {
        let first = codegen_unit_key(1);
        let second = codegen_unit_key(2);

        let Ok(backend) = EmissionBackend::try_new(
            backend_identity(),
            BackendCapabilities::default(),
            [second.clone(), first.clone()],
            BackendEmissionPolicy::default(),
        ) else {
            panic!("test emission backend must be valid");
        };

        assert_eq!(backend.units(), &[first.clone(), second]);

        assert_eq!(
            EmissionBackend::try_new(
                backend_identity(),
                BackendCapabilities::default(),
                [first.clone(), first],
                BackendEmissionPolicy::default(),
            ),
            Err(EmissionBackendBuildError::DuplicateCodegenUnit)
        );
    }

    #[test]
    fn backend_emission_policy_preserves_typed_output_choices() {
        let serialization = BackendSerializationOptions::new(AssemblySyntaxKind::Intel);

        let policy = BackendEmissionPolicy::new(
            DebugInformationMode::Full,
            DebugInformationOutputMode::Separate,
            Some(LinkableArtifactKind::BackendBitcode),
            serialization,
        );

        assert_eq!(policy.debug_information(), DebugInformationMode::Full);
        assert_eq!(policy.debug_output(), DebugInformationOutputMode::Separate);

        assert_eq!(
            policy.linkable_artifact(),
            Some(LinkableArtifactKind::BackendBitcode)
        );

        assert_eq!(policy.serialization(), serialization);
    }
}
