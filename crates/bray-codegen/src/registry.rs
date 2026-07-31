use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use bray_diagnostics::DiagnosticBag;

use crate::{
    AssemblySyntaxKind, BackendArtifactKind, BackendArtifactRequirement, BackendCapabilities,
    BackendIdentity, CodeGenerator, CodegenFailure, CodegenOutcome, CodegenRequest,
};

/// Immutable code generators available to one compiler composition.
#[derive(Clone, Default)]
pub struct CodeGeneratorRegistry {
    generators: Arc<BTreeMap<BackendIdentity, Arc<dyn CodeGenerator>>>,
}

/// One validated immutable backend selection for a compiler composition.
#[derive(Clone, Debug)]
pub struct CodegenConfiguration {
    generators: CodeGeneratorRegistry,
    selected: BackendIdentity,
}

impl CodegenConfiguration {
    /// Selects one available backend for all code generation in a compilation.
    pub fn try_new(
        generators: CodeGeneratorRegistry,
        selected: BackendIdentity,
    ) -> Result<Self, BackendSelectionError> {
        if !generators.contains(&selected) {
            return Err(BackendSelectionError::Unavailable(selected));
        }

        Ok(Self {
            generators,
            selected,
        })
    }

    /// Returns the selected backend identity.
    pub const fn selected(&self) -> &BackendIdentity {
        &self.selected
    }

    /// Returns the declared capabilities of the selected backend.
    pub fn selected_capabilities(&self) -> &BackendCapabilities {
        self.generators
            .generator(&self.selected)
            .map(CodeGenerator::capabilities)
            .unwrap_or_else(|| panic!("selected code generator must remain registered"))
    }

    /// Validates and executes one request through the selected backend.
    pub fn generate(
        &self,
        request: CodegenRequest<'_>,
    ) -> Result<CodegenOutcome, BackendSelectionError> {
        if request.backend() != &self.selected {
            return Err(BackendSelectionError::NotSelected(
                request.backend().clone(),
            ));
        }

        self.generators.generate(request)
    }
}

impl CodeGeneratorRegistry {
    /// Creates a registry when every backend identity is unique.
    pub fn try_new(
        generators: impl IntoIterator<Item = Arc<dyn CodeGenerator>>,
    ) -> Result<Self, CodeGeneratorRegistryBuildError> {
        let mut entries = BTreeMap::new();

        for generator in generators {
            let identity = generator.identity().clone();

            if entries.insert(identity.clone(), generator).is_some() {
                return Err(CodeGeneratorRegistryBuildError::DuplicateIdentity(identity));
            }
        }

        Ok(Self {
            generators: Arc::new(entries),
        })
    }

    /// Returns whether an exact backend identity is available.
    pub fn contains(&self, identity: &BackendIdentity) -> bool {
        self.generators.contains_key(identity)
    }

    /// Returns available backend identities in canonical order.
    pub fn identities(&self) -> impl ExactSizeIterator<Item = &BackendIdentity> {
        self.generators.keys()
    }

    fn generator(&self, identity: &BackendIdentity) -> Option<&dyn CodeGenerator> {
        self.generators.get(identity).map(Arc::as_ref)
    }

    /// Validates and executes one request through its selected backend.
    pub fn generate(
        &self,
        request: CodegenRequest<'_>,
    ) -> Result<CodegenOutcome, BackendSelectionError> {
        let Some(generator) = self.generators.get(request.backend()) else {
            return Err(BackendSelectionError::Unavailable(
                request.backend().clone(),
            ));
        };

        if let Err(failure) = validate_capabilities(generator.as_ref(), request) {
            return Ok(CodegenOutcome::failed(failure, DiagnosticBag::new()));
        }

        if let Err(failure) = generator.validate_target(request.target()) {
            return Ok(CodegenOutcome::failed(failure, DiagnosticBag::new()));
        }

        Ok(generator.generate(request))
    }
}

impl fmt::Debug for CodeGeneratorRegistry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CodeGeneratorRegistry")
            .field("identities", &self.generators.keys().collect::<Vec<_>>())
            .finish()
    }
}

/// A contract violation that prevents construction of a backend registry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CodeGeneratorRegistryBuildError {
    /// Multiple generators declare the same exact backend identity.
    DuplicateIdentity(BackendIdentity),
}

/// A backend-selection failure for one exact code generation request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BackendSelectionError {
    /// The requested backend identity is not available to this compiler composition.
    Unavailable(BackendIdentity),
    /// The request names an available backend that was not selected for this composition.
    NotSelected(BackendIdentity),
}

fn validate_capabilities(
    generator: &dyn CodeGenerator,
    request: CodegenRequest<'_>,
) -> Result<(), CodegenFailure> {
    let capabilities = generator.capabilities();

    if !capabilities.supports_platform(request.target()) {
        return Err(CodegenFailure::UnsupportedTarget);
    }

    for entry in request.artifacts().entries() {
        if entry.requirement() == BackendArtifactRequirement::Required
            && !capabilities.supports_artifact(entry.id().kind())
        {
            return Err(CodegenFailure::UnsupportedArtifact(entry.id().kind()));
        }
    }

    if !capabilities.supports_debug_information(request.options().debug_information()) {
        return Err(CodegenFailure::InvalidConfiguration);
    }

    let syntax = request.artifacts().serialization().assembly_syntax_kind();

    let requires_assembly = request.artifacts().entries().iter().any(|entry| {
        entry.id().kind() == BackendArtifactKind::Assembly
            && entry.requirement() == BackendArtifactRequirement::Required
    });

    if requires_assembly
        && syntax != AssemblySyntaxKind::TargetDefault
        && !capabilities.supports_assembly_syntax_kind(syntax)
    {
        return Err(CodegenFailure::InvalidConfiguration);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_diagnostics::DiagnosticBag;

    use super::{CodeGeneratorRegistry, CodeGeneratorRegistryBuildError, CodegenConfiguration};
    use crate::{
        BackendCapabilities, BackendIdentity, CodeGenerator, CodegenFailure, CodegenOutcome,
        CodegenRequest, CodegenStatus,
    };

    #[test]
    fn registries_select_generators_by_exact_identity() {
        let generator = Arc::new(TestCodeGenerator::new("one"));
        let identity = generator.identity().clone();

        let registry =
            CodeGeneratorRegistry::try_new([Arc::clone(&generator) as Arc<dyn CodeGenerator>])
                .unwrap_or_else(|error| panic!("unique backend must register: {error:?}"));

        assert!(registry.contains(&identity));
        assert_eq!(registry.identities().collect::<Vec<_>>(), [&identity]);
    }

    #[test]
    fn registries_reject_duplicate_backend_identities() {
        let first = Arc::new(TestCodeGenerator::new("same"));
        let second = Arc::new(TestCodeGenerator::new("same"));
        let identity = first.identity().clone();

        let result = CodeGeneratorRegistry::try_new([
            first as Arc<dyn CodeGenerator>,
            second as Arc<dyn CodeGenerator>,
        ]);

        assert!(matches!(
            result,
            Err(CodeGeneratorRegistryBuildError::DuplicateIdentity(found))
                if found == identity
        ));
    }

    #[test]
    fn registries_validate_capabilities_before_invoking_backends() {
        let generator = Arc::new(TestCodeGenerator::new("unsupported"));

        let fixture =
            crate::test_support::codegen_request_for_backend(generator.identity().clone());

        let registry =
            CodeGeneratorRegistry::try_new([Arc::clone(&generator) as Arc<dyn CodeGenerator>])
                .unwrap_or_else(|error| panic!("unique backend must register: {error:?}"));

        let outcome = registry
            .generate(fixture.request())
            .unwrap_or_else(|error| panic!("registered backend must be selected: {error:?}"));

        assert!(matches!(
            outcome.status(),
            CodegenStatus::Failed(CodegenFailure::UnsupportedTarget)
        ));
    }

    #[test]
    fn configurations_reject_requests_for_unselected_backends() {
        let selected = Arc::new(TestCodeGenerator::new("selected"));
        let unselected = Arc::new(TestCodeGenerator::new("unselected"));

        let fixture =
            crate::test_support::codegen_request_for_backend(unselected.identity().clone());

        let registry = CodeGeneratorRegistry::try_new([
            Arc::clone(&selected) as Arc<dyn CodeGenerator>,
            Arc::clone(&unselected) as Arc<dyn CodeGenerator>,
        ])
        .unwrap_or_else(|error| panic!("unique backends must register: {error:?}"));

        let configuration = CodegenConfiguration::try_new(registry, selected.identity().clone())
            .unwrap_or_else(|error| panic!("available backend must select: {error:?}"));

        assert!(matches!(
            configuration.generate(fixture.request()),
            Err(super::BackendSelectionError::NotSelected(identity))
                if identity == *unselected.identity()
        ));
    }

    struct TestCodeGenerator {
        identity: BackendIdentity,
        capabilities: BackendCapabilities,
    }

    impl TestCodeGenerator {
        fn new(revision: &str) -> Self {
            let Some(identity) = BackendIdentity::try_new("test", revision, "test-toolchain")
            else {
                panic!("test backend identity must be valid");
            };

            Self {
                identity,
                capabilities: BackendCapabilities::default(),
            }
        }
    }

    impl CodeGenerator for TestCodeGenerator {
        fn identity(&self) -> &BackendIdentity {
            &self.identity
        }

        fn capabilities(&self) -> &BackendCapabilities {
            &self.capabilities
        }

        fn validate_target(&self, _target: &crate::CodegenTarget) -> Result<(), CodegenFailure> {
            Ok(())
        }

        fn generate(&self, _request: CodegenRequest<'_>) -> CodegenOutcome {
            let outcome =
                CodegenOutcome::failed(CodegenFailure::InvalidConfiguration, DiagnosticBag::new());

            assert!(matches!(
                outcome.status(),
                CodegenStatus::Failed(CodegenFailure::InvalidConfiguration)
            ));

            outcome
        }
    }
}
