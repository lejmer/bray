use std::sync::Arc;

use bray_codegen::{
    BackendArtifactRequest, CodegenMappings, CodegenOptions, CodegenOutcome, CodegenRequest,
    CodegenRequestBuildError, CodegenTarget, CodegenUnit,
};

use super::Compilation;
use crate::fact::{
    CancellationToken, CodegenArtifactFactKey, CompilationFactKey, FactQueryError,
};

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
        options: &CodegenOptions,
        artifacts: &BackendArtifactRequest,
    ) -> Result<Arc<CodegenOutcome>, CodegenFactError> {
        self.codegen_artifact_with_cancellation(
            unit,
            mappings,
            target,
            options,
            artifacts,
            &self.state.cancellation,
        )
    }

    /// Returns one code generation contribution while observing caller cancellation.
    #[expect(
        clippy::too_many_arguments,
        reason = "each argument is an independent output-affecting code generation fact"
    )]
    pub fn codegen_artifact_with_cancellation(
        &self,
        unit: &CodegenUnit,
        mappings: &CodegenMappings,
        target: &CodegenTarget,
        options: &CodegenOptions,
        artifacts: &BackendArtifactRequest,
        cancellation: &CancellationToken,
    ) -> Result<Arc<CodegenOutcome>, CodegenFactError> {
        let Some(codegen) = &self.state.codegen else {
            return Err(CodegenFactError::CodegenUnavailable);
        };

        let backend = codegen.selected();

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

        let key = CodegenArtifactFactKey::new(
            unit.key().clone(),
            mappings.clone(),
            target.clone(),
            backend.clone(),
            *options,
            artifacts.clone(),
        );

        let cell = self.state.codegen_artifacts.cell(key.clone())?;

        let priority = self
            .state
            .fact_runtime
            .current_priority()?
            .unwrap_or(crate::QueryPriority::Normal);

        let outcome = cell.get_or_compute_requested(
            &self.state.fact_runtime,
            CompilationFactKey::CodegenArtifact(key),
            cancellation,
            priority,
            |shared_cancellation| {
                let request = CodegenRequest::try_new(
                    unit,
                    backend,
                    target,
                    mappings,
                    options,
                    artifacts,
                    shared_cancellation,
                )
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

                codegen
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
    /// This compilation was composed without a code generation backend.
    CodegenUnavailable,
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
    use std::sync::{Arc, Condvar, Mutex};
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_codegen::test_support::codegen_request;
    use bray_codegen::{
        AssemblySyntaxKind, BackendCapabilities, BackendTargetPlatform, CodeGenerator,
        CodeGeneratorRegistry, CodegenConfiguration, CodegenFailure, CodegenOutcome,
        CodegenRequest,
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
        let compilation = compilation_with_counter(request, Arc::clone(&invocations), None);

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

    #[test]
    fn cancelled_codegen_requests_do_not_invoke_or_publish_the_backend() {
        let fixture = codegen_request();
        let request = fixture.request();
        let invocations = Arc::new(AtomicUsize::new(0));
        let compilation = compilation_with_counter(request, Arc::clone(&invocations), None);
        let cancellation = crate::CancellationToken::new();

        cancellation.cancel();

        let result = compilation.codegen_artifact_with_cancellation(
            request.unit(),
            request.mappings(),
            request.target(),
            request.options(),
            request.artifacts(),
            &cancellation,
        );

        assert!(matches!(
            result,
            Err(super::CodegenFactError::Query(
                crate::FactQueryError::Cancelled
            ))
        ));

        assert_eq!(invocations.load(Ordering::SeqCst), 0);

        let _ = generate(&compilation, request);

        assert_eq!(invocations.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn concurrent_same_key_requests_share_one_backend_invocation() {
        let fixture = Arc::new(codegen_request());
        let invocations = Arc::new(AtomicUsize::new(0));
        let gate = Arc::new(GenerationGate::default());

        let compilation = compilation_with_counter(
            fixture.request(),
            Arc::clone(&invocations),
            Some(Arc::clone(&gate)),
        );

        let first_compilation = compilation.clone();
        let first_fixture = Arc::clone(&fixture);

        let first = std::thread::spawn(move || {
            generate(&first_compilation, first_fixture.request())
        });

        gate.wait_until_entered();

        let second_compilation = compilation.clone();
        let second_fixture = Arc::clone(&fixture);

        let (started, receiver) = std::sync::mpsc::channel();

        let second = std::thread::spawn(move || {
            started
                .send(())
                .unwrap_or_else(|_| panic!("test request start must be observed"));

            generate(&second_compilation, second_fixture.request())
        });

        receiver
            .recv()
            .unwrap_or_else(|_| panic!("concurrent test request must start"));

        gate.release();

        let first = first
            .join()
            .unwrap_or_else(|_| panic!("first code generation request must finish"));

        let second = second
            .join()
            .unwrap_or_else(|_| panic!("second code generation request must finish"));

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(invocations.load(Ordering::SeqCst), 1);
    }

    fn compilation_with_counter(
        request: CodegenRequest<'_>,
        invocations: Arc<AtomicUsize>,
        gate: Option<Arc<GenerationGate>>,
    ) -> Compilation {
        let generator = Arc::new(CountingCodeGenerator {
            identity: request.backend().clone(),
            capabilities: capabilities(request),
            invocations,
            gate,
        });

        let selected = generator.identity().clone();

        let registry = CodeGeneratorRegistry::try_new([generator as Arc<dyn CodeGenerator>])
            .unwrap_or_else(|error| panic!("test backend must register: {error:?}"));

        let codegen = CodegenConfiguration::try_new(registry, selected)
            .unwrap_or_else(|error| panic!("test backend must select: {error:?}"));

        Compilation::load_with_codegen(compilation_request(), codegen)
            .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"))
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
        gate: Option<Arc<GenerationGate>>,
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

            if let Some(gate) = &self.gate {
                gate.enter_and_wait();
            }

            CodegenOutcome::failed(
                CodegenFailure::BackendLibrary,
                DiagnosticBag::new(),
            )
        }
    }

    #[derive(Default)]
    struct GenerationGate {
        state: Mutex<GenerationGateState>,
        changed: Condvar,
    }

    #[derive(Default)]
    struct GenerationGateState {
        entered: bool,
        released: bool,
    }

    impl GenerationGate {
        fn enter_and_wait(&self) {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|_| panic!("generation gate must remain available"));

            state.entered = true;
            self.changed.notify_all();

            while !state.released {
                state = self
                    .changed
                    .wait(state)
                    .unwrap_or_else(|_| panic!("generation gate must remain available"));
            }
        }

        fn wait_until_entered(&self) {
            let state = self
                .state
                .lock()
                .unwrap_or_else(|_| panic!("generation gate must remain available"));

            let waited = self
                .changed
                .wait_while(state, |state| !state.entered)
                .unwrap_or_else(|_| panic!("generation gate must remain available"));

            drop(waited);
        }

        fn release(&self) {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|_| panic!("generation gate must remain available"));

            state.released = true;
            self.changed.notify_all();
        }
    }
}
