use std::sync::Arc;

use bray_codegen::{
    BackendArtifactRequest, BackendIdentity, CodegenMappings, CodegenOptions, CodegenOutcome,
    CodegenRequest, CodegenRequestBuildError, CodegenTarget, CodegenUnit,
};

use super::Compilation;
use crate::fact::{CodegenArtifactFactKey, CompilationFactKey, FactQueryError};

impl Compilation {
    /// Returns the lazily generated outcome for one exact code generation contribution.
    #[expect(
        clippy::too_many_arguments,
        reason = "each argument is an independent output-affecting code generation fact"
    )]
    pub fn codegen_artifact(
        &self,
        unit: &CodegenUnit,
        mappings: &CodegenMappings,
        target: &CodegenTarget,
        backend: &BackendIdentity,
        options: &CodegenOptions,
        artifacts: &BackendArtifactRequest,
    ) -> Result<Arc<CodegenOutcome>, CodegenFactError> {
        CodegenRequest::try_new(
            unit,
            backend,
            target,
            mappings,
            options,
            artifacts,
            &self.state.cancellation,
        )
        .map_err(CodegenFactError::InvalidRequest)?;

        if !self.state.code_generators.contains(backend) {
            return Err(CodegenFactError::BackendUnavailable(backend.clone()));
        }

        let key = CodegenArtifactFactKey::new(
            unit.key().clone(),
            mappings.clone(),
            target.clone(),
            backend.clone(),
            *options,
            artifacts.clone(),
        );

        let cell = self.state.codegen_artifacts.cell(key.clone())?;

        let outcome = cell.get_or_compute(
            &self.state.fact_runtime,
            CompilationFactKey::CodegenArtifact(key),
            &self.state.cancellation,
            || {
                let request = CodegenRequest::try_new(
                    unit,
                    backend,
                    target,
                    mappings,
                    options,
                    artifacts,
                    &self.state.cancellation,
                )
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

                self.state
                    .code_generators
                    .generate(request)
                    .map(Arc::new)
                    .map_err(|_| FactQueryError::InfrastructureFailure)
            },
        )?;

        Ok(Arc::clone(outcome))
    }
}

/// A failure to request one lazy code generation contribution fact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CodegenFactError {
    /// The selected backend is not available in this compiler composition.
    BackendUnavailable(BackendIdentity),
    /// The supplied code generation facts do not form a coherent request.
    InvalidRequest(CodegenRequestBuildError),
    /// The compilation fact runtime could not complete the request.
    Query(FactQueryError),
}

impl From<FactQueryError> for CodegenFactError {
    fn from(error: FactQueryError) -> Self {
        Self::Query(error)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_codegen::test_support::codegen_request;
    use bray_codegen::{
        AssemblySyntaxKind, BackendCapabilities, BackendTargetPlatform, CodeGenerator,
        CodeGeneratorRegistry, CodegenFailure, CodegenOutcome, CodegenRequest,
    };
    use bray_diagnostics::DiagnosticBag;

    use super::Compilation;
    use crate::test_support::{package_identity, source_input};
    use crate::CompilationRequest;

    #[test]
    fn exact_codegen_requests_are_generated_once_and_reused_by_updated_snapshots() {
        let fixture = codegen_request();
        let request = fixture.request();
        let invocations = Arc::new(AtomicUsize::new(0));

        let generator = Arc::new(CountingCodeGenerator {
            identity: request.backend().clone(),
            capabilities: capabilities(request),
            invocations: Arc::clone(&invocations),
        });

        let registry = CodeGeneratorRegistry::try_new([generator as Arc<dyn CodeGenerator>])
            .unwrap_or_else(|error| panic!("test backend must register: {error:?}"));

        let compilation = Compilation::load_with_code_generators(
            compilation_request(),
            registry,
        )
        .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"));

        let first = generate(&compilation, request);
        let second = generate(&compilation, request);

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(invocations.load(Ordering::SeqCst), 1);

        let updated = compilation
            .updated(compilation_request())
            .unwrap_or_else(|error| panic!("updated compilation must load: {error:?}"));

        let reused = generate(&updated, request);

        assert!(Arc::ptr_eq(&first, &reused));
        assert_eq!(invocations.load(Ordering::SeqCst), 1);
    }

    fn compilation_request() -> CompilationRequest {
        CompilationRequest::new(
            package_identity(),
            vec![source_input("module app;", 0)],
        )
    }

    fn generate(
        compilation: &Compilation,
        request: CodegenRequest<'_>,
    ) -> Arc<CodegenOutcome> {
        compilation
            .codegen_artifact(
                request.unit(),
                request.mappings(),
                request.target(),
                request.backend(),
                request.options(),
                request.artifacts(),
            )
            .unwrap_or_else(|error| panic!("code generation fact must publish: {error:?}"))
    }

    fn capabilities(request: CodegenRequest<'_>) -> BackendCapabilities {
        let machine = request.target().machine();

        BackendCapabilities::new(
            [BackendTargetPlatform::new(
                machine.architecture(),
                machine.object_format(),
            )],
            request.artifacts().entries().iter().map(|entry| entry.id().kind()),
            [request.options().debug_information()],
            [AssemblySyntaxKind::TargetDefault],
        )
    }

    struct CountingCodeGenerator {
        identity: bray_codegen::BackendIdentity,
        capabilities: BackendCapabilities,
        invocations: Arc<AtomicUsize>,
    }

    impl CodeGenerator for CountingCodeGenerator {
        fn identity(&self) -> &bray_codegen::BackendIdentity {
            &self.identity
        }

        fn capabilities(&self) -> &BackendCapabilities {
            &self.capabilities
        }

        fn validate_target(
            &self,
            _target: &bray_codegen::CodegenTarget,
        ) -> Result<(), CodegenFailure> {
            Ok(())
        }

        fn generate(&self, _request: CodegenRequest<'_>) -> CodegenOutcome {
            self.invocations.fetch_add(1, Ordering::SeqCst);

            CodegenOutcome::failed(
                CodegenFailure::BackendLibrary,
                DiagnosticBag::new(),
            )
        }
    }
}
