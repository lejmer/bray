use std::sync::Arc;

use bray_codegen::{
    BackendArtifactRequest, CodegenInstance, CodegenInstanceBuildError, CodegenMappings,
    CodegenMappingsBuildError, CodegenOptions, CodegenOutcome, CodegenRequest,
    CodegenRequestBuildError, CodegenTarget, CodegenUnit, CodegenUnitBuildError, CodegenUnitKey,
};
#[cfg(test)]
use bray_codegen::{
    CodegenCallableSignature, CodegenLinkage, CodegenResultMapping, CodegenSymbolKey,
    CodegenSymbolMapping, demanded_runtime_references,
};
use bray_diagnostics::DiagnosticBag;
#[cfg(test)]
use bray_ir::MirUnitKind;
use bray_ir::{MirHelperReference, MirUnit, MirUnitBuildError, MirUnitId, MirUnitKey};
use bray_runtime_interface::ExecutableHostContract;
#[cfg(test)]
use bray_runtime_interface::{BinarySymbolName, ProtectedFrameOperation};
#[cfg(test)]
use bray_symbols::CallableAbi;
use bray_symbols::CallableDefinitionId;

use super::Compilation;
use crate::fact::{CancellationToken, CodegenArtifactFactKey, CompilationFactKey, FactQueryError};

impl Compilation {
    pub(super) fn codegen_units_for_plan(
        &self,
        requests: &[BackendArtifactRequest],
        executable_host: Option<&ExecutableHostContract>,
        cancellation: &CancellationToken,
    ) -> Result<Vec<CodegenUnit>, (CodegenUnitKey, CodegenFactError)> {
        let Some(first) = requests.first() else {
            return Ok(Vec::new());
        };

        self.state
            .fact_runtime
            .map_indexed(requests.len(), |index| {
                let request = &requests[index];

                // Scheduled errors own their Arc-backed unit identity past the plan borrow.
                self.codegen_unit_for_plan(request.unit(), executable_host, cancellation)
                    .map_err(|error| (request.unit().clone(), error))
            })
            .map_err(|error| (first.unit().clone(), CodegenFactError::Query(error)))?
            .into_iter()
            .collect()
    }

    fn codegen_unit_for_plan(
        &self,
        key: &CodegenUnitKey,
        executable_host: Option<&ExecutableHostContract>,
        cancellation: &CancellationToken,
    ) -> Result<CodegenUnit, CodegenFactError> {
        let instance_keys = key.instances();

        let instances = self
            .state
            .fact_runtime
            .map_indexed(instance_keys.len(), |index| {
                let instance = &instance_keys[index];

                // Reconstruction errors own the complete shared unit identity past this request.
                let Some(mir_unit) = key.mir_unit(instance) else {
                    return Err(CodegenFactError::UnitMismatch(key.clone()));
                };

                let mir =
                    self.codegen_mir_for_plan(instance, mir_unit, executable_host, cancellation)?;

                let Some(dependencies) = key.dependencies(instance) else {
                    return Err(CodegenFactError::UnitMismatch(key.clone()));
                };

                let instance =
                    CodegenInstance::try_new(instance.clone(), mir, dependencies.iter().cloned())
                        .map_err(CodegenFactError::InvalidInstance)?;

                Ok(instance)
            })?
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;

        CodegenUnit::try_from_key(key, instances).map_err(CodegenFactError::InvalidUnit)
    }

    pub(in crate::compilation) fn codegen_mir_for_plan(
        &self,
        instance: &bray_codegen::CodegenInstanceKey,
        mir_unit: MirUnitId,
        executable_host: Option<&ExecutableHostContract>,
        cancellation: &CancellationToken,
    ) -> Result<MirUnit, CodegenFactError> {
        match instance.template() {
            MirUnitKey::Bound(key) => {
                let lowered = self.lowered_unit_with_priority(
                    key.clone(),
                    cancellation,
                    crate::QueryPriority::Normal,
                )?;

                let Some(mir) = lowered
                    .value()
                    .as_ref()
                    .and_then(bray_lowering::LoweredUnit::mir)
                else {
                    return Err(CodegenFactError::MirUnavailable(MirUnitKey::Bound(
                        key.clone(),
                    )));
                };

                // The reconstructed unit owns the immutable MIR independently of the fact borrow.
                Ok(mir.clone())
            }
            MirUnitKey::ExecutableHost(product) => {
                let Some(host) = executable_host.filter(|host| host.product() == product) else {
                    return Err(CodegenFactError::MirUnavailable(
                        instance.template().clone(),
                    ));
                };

                let semantics = self.product_semantic_facts_with_cancellation(cancellation)?;

                let roots = match semantics.value().kind() {
                    bray_symbols::ProductKind::Test => {
                        let discovery =
                            self.test_discovery_with_cancellation(product.clone(), cancellation)?;

                        discovery
                            .value()
                            .catalog()
                            .entries()
                            .iter()
                            .map(|entry| {
                                let function = discovery
                                    .value()
                                    .function(entry.identity())
                                    .ok_or(CodegenFactError::MissingEntrypoint)?;

                                let definition = CallableDefinitionId::try_new(function.into())
                                    .ok_or(CodegenFactError::MissingEntrypoint)?;

                                self.callable_body_key(definition)?
                                    .ok_or(CodegenFactError::MissingEntrypoint)
                            })
                            .collect::<Result<Vec<_>, _>>()?
                    }
                    bray_symbols::ProductKind::Executable | bray_symbols::ProductKind::Library => {
                        let entrypoint = semantics
                            .value()
                            .entrypoint()
                            .ok_or(CodegenFactError::MissingEntrypoint)?;

                        let definition = CallableDefinitionId::try_new(entrypoint.into())
                            .ok_or(CodegenFactError::MissingEntrypoint)?;

                        vec![
                            self.callable_body_key(definition)?
                                .ok_or(CodegenFactError::MissingEntrypoint)?,
                        ]
                    }
                };

                bray_lowering::lower_executable_host(
                    bray_lowering::ExecutableHostLoweringInput::new(
                        mir_unit,
                        roots,
                        // Generated MIR owns host and target facts after this request.
                        host.clone(),
                        instance.target().clone(),
                    ),
                )
                .map_err(CodegenFactError::InvalidHostMir)
            }
            MirUnitKey::GeneratedLifecycle(_) => Err(CodegenFactError::MirUnavailable(
                instance.template().clone(),
            )),
            MirUnitKey::ImportedExecutable(owner) => self
                .imported_executable_mir(*owner, mir_unit, instance.target().clone(), cancellation)?
                .ok_or_else(|| CodegenFactError::MirUnavailable(instance.template().clone())),
            MirUnitKey::ExternalCallable(_) | MirUnitKey::ExternalRuntimeDefault(_) => Err(
                CodegenFactError::MirUnavailable(instance.template().clone()),
            ),
        }
    }

    /// Returns the lazily generated outcome for one exact code generation contribution.
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
        let capability_revision = codegen.selected_capabilities().revision();
        let product = self.product_kind();

        CodegenRequest::try_new(
            unit,
            backend,
            capability_revision,
            product,
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
            capability_revision,
            product,
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
                self.record_codegen_configuration();

                let request = CodegenRequest::try_new(
                    unit,
                    backend,
                    capability_revision,
                    product,
                    target,
                    mappings,
                    options,
                    artifacts,
                    shared_cancellation,
                )
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

                let span = self.state.fact_runtime.profile().map(|profile| {
                    profile.start(crate::profile::ProfileOperation::CodeGeneration, None)
                });

                let result = codegen.generate(request);

                if let Some(span) = span {
                    span.finish(crate::profile::result_outcome(&result));
                }

                if result.is_ok()
                    && let Some(profile) = self.state.fact_runtime.profile()
                {
                    profile.add_metric(crate::profile::ProfileMetricKind::CodegenUnits, 1);

                    profile.add_metric(
                        crate::profile::ProfileMetricKind::ConcreteInstances,
                        u64::try_from(unit.instances().len()).unwrap_or(u64::MAX),
                    );
                }

                result
                    .map(Arc::new)
                    .map_err(|_| FactQueryError::InfrastructureFailure)
            },
        )?;

        Ok(Arc::clone(outcome))
    }
}

#[cfg(test)]
fn codegen_mappings(
    unit: &CodegenUnit,
    executable_host: Option<&ExecutableHostContract>,
    target: &CodegenTarget,
) -> Result<CodegenMappings, CodegenFactError> {
    let mut symbols = Vec::new();

    for instance in unit.instances() {
        let (name, linkage) = match instance.mir().kind() {
            MirUnitKind::ExecutableHost(host) => {
                // The mapping owns the selected process-entry spelling past the MIR borrow.
                (host.native_entry().clone(), CodegenLinkage::Export)
            }
            MirUnitKind::Synchronous
            | MirUnitKind::ProtectedAsyncFrame(_)
            | MirUnitKind::GeneratedLifecycle(_) => (
                super::product::generated_symbol_name(
                    target,
                    CodegenLinkage::Internal,
                    "instance",
                    instance.key(),
                )?,
                CodegenLinkage::Internal,
            ),
        };

        // The mapping owns the concrete instance identity independently of the unit.
        symbols.push(symbol_mapping(
            CodegenSymbolKey::Instance(instance.key().clone()),
            name,
            linkage,
        ));
    }

    for instance in unit.external_instances() {
        // Imported mappings own identities independently of the unit's dependency recipe.
        symbols.push(symbol_mapping(
            CodegenSymbolKey::Instance(instance.clone()),
            super::product::generated_symbol_name(
                target,
                CodegenLinkage::Import,
                "instance",
                instance,
            )?,
            CodegenLinkage::Import,
        ));
    }

    for reference in demanded_runtime_references(unit) {
        let symbol_name = executable_host
            .and_then(|host| host.role_binding(reference.role()))
            .map(|binding| binding.symbol_name().clone())
            .or_else(|| {
                bray_runtime_interface::native_runtime_role_symbol(reference.role())
                    .and_then(BinarySymbolName::try_new)
            })
            .ok_or(CodegenFactError::MissingRuntimeRole(reference.role()))?;

        // The mapping owns the selected runtime spelling past the host-contract borrow.
        symbols.push(symbol_mapping(
            CodegenSymbolKey::Runtime(reference),
            symbol_name,
            CodegenLinkage::Import,
        ));
    }

    for instance in unit.instances() {
        let Some(frame) = instance.protected_frame_identity() else {
            continue;
        };

        for operation in ProtectedFrameOperation::ALL {
            symbols.push(symbol_mapping(
                CodegenSymbolKey::ProtectedFrame { frame, operation },
                super::product::generated_frame_symbol_name(target, frame, operation)?,
                CodegenLinkage::Internal,
            ));
        }
    }

    CodegenMappings::try_new(unit, target, [], [], symbols, [], [], [], [], [], [])
        .map_err(CodegenFactError::InvalidMappings)
}

#[cfg(test)]
fn symbol_mapping(
    key: CodegenSymbolKey,
    name: BinarySymbolName,
    linkage: CodegenLinkage,
) -> CodegenSymbolMapping {
    CodegenSymbolMapping::new(
        key,
        name,
        linkage,
        CodegenCallableSignature::new([], CodegenResultMapping::Void, CallableAbi::Bray, false),
    )
}

/// A failure to request one lazy code generation contribution fact.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum CodegenFactError {
    /// This compilation was composed without a code generation backend.
    CodegenUnavailable,
    /// The supplied code generation facts do not form a coherent request.
    InvalidRequest(CodegenRequestBuildError),
    /// A plan-named MIR fact is unavailable from this compilation.
    MirUnavailable(MirUnitKey),
    /// Executable host construction requires one selected source entrypoint.
    MissingEntrypoint,
    /// A concrete instance could not be reconstructed from its exact lazy MIR fact.
    InvalidInstance(CodegenInstanceBuildError),
    /// Plan-named instances could not form a valid code generation unit.
    InvalidUnit(CodegenUnitBuildError),
    /// Reconstructed MIR or dependencies differ from the immutable plan identity.
    UnitMismatch(CodegenUnitKey),
    /// A generated executable host did not satisfy the MIR contract.
    InvalidHostMir(MirUnitBuildError),
    /// A generated lifecycle definition did not satisfy the MIR contract.
    InvalidGeneratedLifecycleMir(MirUnitBuildError),
    /// Compilation could not construct complete realization mappings for the planned unit.
    InvalidMappings(CodegenMappingsBuildError),
    /// A MIR runtime role has no selected executable-host binding.
    MissingRuntimeRole(bray_runtime_interface::RuntimeAbiRole),
    /// A constant term needed by code generation still contains unresolved parameters.
    OpenConstantTerm(bray_symbols::ConstantTermId),
    /// A closed array length term did not contain its checked integer value.
    InvalidArrayLength(bray_symbols::ConstantTermId),
    /// A value type contains itself without an indirection boundary.
    RecursiveValueType(bray_symbols::TypeId),
    /// A demanded semantic type still contains an unresolved checking placeholder.
    UnresolvedType(bray_symbols::TypeId),
    /// An unsized semantic type was demanded in a by-value position.
    UnsizedTypeByValue(bray_symbols::TypeId),
    /// A callable signature reached target classification in a non-semantic mapping state.
    InvalidAbiMapping,
    /// The selected backend cannot represent a demanded semantic type.
    UnsupportedType(bray_symbols::TypeId),
    /// A declaration-backed or type-specific MIR helper has no matching concrete dependency.
    MissingHelperInstance(MirHelperReference),
    /// Structured diagnostics prevent a demanded code generation fact from being produced.
    Diagnostics(DiagnosticBag),
    /// A demanded type layout exceeds the selected target's representable size.
    LayoutOverflow(bray_symbols::TypeId),
    /// A deterministic generated binary symbol could not be represented.
    InvalidSymbolName,
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
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Condvar, Mutex};

    use bray_codegen::test_support::{
        codegen_backend_capabilities, codegen_partition_compatibility, codegen_request,
    };
    use bray_codegen::{
        BackendCapabilities, CodeGenerator, CodeGeneratorRegistry, CodegenConfiguration,
        CodegenFailure, CodegenOutcome, CodegenPartitionPolicy, CodegenRequest,
    };
    use bray_diagnostics::DiagnosticBag;

    use super::{Compilation, codegen_mappings};
    use crate::CompilationRequest;
    use crate::test_support::{
        compilation as source_compilation, package_identity, source_callable_body_key, source_input,
    };

    #[test]
    fn plan_named_units_are_reconstructed_from_lazy_mir_facts() {
        let compilation = source_compilation("module app; func main() {}");
        let key = source_callable_body_key(&compilation);

        let lowered = compilation
            .lowered_unit(key)
            .unwrap_or_else(|error| panic!("test source unit must lower: {error:?}"));

        let mir = lowered
            .value()
            .as_ref()
            .and_then(bray_lowering::LoweredUnit::mir)
            .cloned()
            .unwrap_or_else(|| panic!("test source unit must publish MIR"));

        let expected = bray_codegen::CodegenUnit::try_new(
            CodegenPartitionPolicy::NATIVE_BALANCED,
            codegen_partition_compatibility(),
            [mir],
        )
        .unwrap_or_else(|error| panic!("test codegen unit must validate: {error:?}"));

        let actual = compilation
            .codegen_unit_for_plan(expected.key(), None, &crate::CancellationToken::new())
            .unwrap_or_else(|error| panic!("plan-named unit must resolve: {error:?}"));

        assert_eq!(actual, expected);
    }

    #[test]
    fn mapping_free_units_construct_exact_realization_facts() {
        let fixture = codegen_request();
        let request = fixture.request();

        let mappings = codegen_mappings(request.unit(), None, request.target())
            .unwrap_or_else(|error| panic!("mapping-free unit must realize: {error:?}"));

        assert_eq!(mappings.unit(), request.unit().key());
        assert_eq!(mappings.target(), request.target());
    }

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

        let first =
            std::thread::spawn(move || generate(&first_compilation, first_fixture.request()));

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
        CompilationRequest::new(package_identity(), vec![source_input("module app;", 0)])
    }

    fn generate(compilation: &Compilation, request: CodegenRequest<'_>) -> Arc<CodegenOutcome> {
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

    fn capabilities(_request: CodegenRequest<'_>) -> BackendCapabilities {
        codegen_backend_capabilities()
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

        fn generate(&self, request: CodegenRequest<'_>) -> CodegenOutcome {
            self.invocations.fetch_add(1, Ordering::SeqCst);

            if let Some(gate) = &self.gate {
                gate.enter_and_wait();
            }

            CodegenOutcome::failed(
                request,
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
