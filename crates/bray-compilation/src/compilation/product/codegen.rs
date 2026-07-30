use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use bray_base::{StableDigestHasher, shared_slice};
use bray_codegen::{
    AssemblySyntaxKind, CodegenGenericArgument, CodegenInstance, CodegenInstanceBuildError,
    CodegenInstanceDependency, CodegenInstanceKey, CodegenMappings, CodegenOptions,
    CodegenReachabilityBuildError, CodegenReachabilityBuilder, CodegenSpecialization,
    CodegenTarget, CodegenTargetBuildError, CodegenUnit, CodegenUnitBuildError,
    CodegenValueKey, DebugInformationMode, DebugInformationOutputMode,
    LinkableArtifactKind, demanded_callable_references_for_mir,
    demanded_runtime_references, partition_codegen_units,
};
use bray_emitter::{
    BackendEmissionPolicy, EmissionBackend, EmissionBackendBuildError, ProductLinkFacts,
};
use bray_ir::{MirTargetFacts, MirUnit, MirUnitId, MirUnitKey};
use bray_linker::{
    DeadStripPolicy, DebugLinkPolicy, LinkInputBuildError, LinkInputKind,
    LinkInputMode, LinkInputProvenance, LinkInputSource, LinkInputSpec, LinkModel,
    LinkPolicy, LinkTarget, LinkTargetBuildError, LinkedProductKind, Linker,
    SectionGarbageCollectionPolicy,
};
use bray_runtime_interface::{
    BinarySymbolName, ExecutableHostContract, ExecutableHostContractBuildError,
    ExecutableHostContractBuilder, RootExecution, RuntimeAbiRole, RuntimeArtifact,
    RuntimeCapability, RuntimeRequirements, RuntimeRoleBinding, RuntimeRoleImplementation,
};
use bray_symbols::{
    AnySymbolId, CallableDefinitionId, CallableInstanceData, GenericArgument,
    GenericSubstitutionId, NativeLinkKind, ProductIdentity, ProductKind,
};

use super::super::Compilation;
use super::super::substitution::empty_substitution;
use crate::fact::{
    CancellationToken, CompilationFactKey, FactQueryError, NativeProductFactKey,
};

const CODEGEN_PARTITION_REVISION: u32 = 1;
const GENERATED_HOST_UNIT: MirUnitId = MirUnitId::new(u32::MAX);

/// Compilation-owned native product facts consumed by emission.
#[derive(Clone, Debug)]
pub struct NativeProductFacts {
    backend: EmissionBackend,
    target: CodegenTarget,
    options: CodegenOptions,
    host: Option<ExecutableHostContract>,
    link: ProductLinkFacts,
    units: Arc<[CodegenUnit]>,
    mappings: Arc<[CodegenMappings]>,
}

impl NativeProductFacts {
    /// Returns the selected backend and demanded unit keys.
    pub const fn backend(&self) -> &EmissionBackend {
        &self.backend
    }

    /// Returns the selected code generation target.
    pub const fn target(&self) -> &CodegenTarget {
        &self.target
    }

    /// Returns the selected code generation options.
    pub const fn options(&self) -> &CodegenOptions {
        &self.options
    }

    /// Returns the executable host when the product has a process root.
    pub const fn executable_host(&self) -> Option<&ExecutableHostContract> {
        self.host.as_ref()
    }

    /// Returns the native link facts.
    pub const fn link(&self) -> &ProductLinkFacts {
        &self.link
    }

    pub(in crate::compilation) fn units(&self) -> &[CodegenUnit] {
        &self.units
    }

    pub(in crate::compilation) fn mappings(&self) -> &[CodegenMappings] {
        &self.mappings
    }
}

impl Compilation {
    /// Returns the native facts required to emit one selected product.
    pub fn native_product_facts(
        &self,
        product: ProductIdentity,
        runtime: Option<RuntimeArtifact>,
        required_capabilities: impl IntoIterator<Item = RuntimeCapability>,
        linker: &Linker,
    ) -> Result<Arc<NativeProductFacts>, Arc<NativeProductFactError>> {
        self.native_product_facts_with_cancellation(
            product,
            runtime,
            required_capabilities,
            linker,
            &self.state.cancellation,
        )
    }

    fn native_product_facts_with_cancellation(
        &self,
        product: ProductIdentity,
        runtime: Option<RuntimeArtifact>,
        required_capabilities: impl IntoIterator<Item = RuntimeCapability>,
        linker: &Linker,
        cancellation: &CancellationToken,
    ) -> Result<Arc<NativeProductFacts>, Arc<NativeProductFactError>> {
        let mut required_capabilities: Vec<_> = required_capabilities.into_iter().collect();

        required_capabilities.sort_unstable();
        required_capabilities.dedup();

        let key = NativeProductFactKey::new(
            product.clone(),
            runtime.as_ref().map(|runtime| {
                (
                    runtime.metadata().archive_digest(),
                    runtime.archive().to_path_buf(),
                )
            }),
            Arc::from(required_capabilities.clone()),
            Arc::from(
                linker
                    .driver_identities()
                    .cloned()
                    .collect::<Vec<_>>(),
            ),
        );

        let cell = self
            .state
            .native_products
            .cell(key.clone())
            .map_err(|error| Arc::new(NativeProductFactError::Query(error)))?;

        let result = cell
            .get_or_compute(
                &self.state.fact_runtime,
                CompilationFactKey::NativeProduct(key),
                cancellation,
                || {
                    Ok(self
                        .compute_native_product_facts(
                            product,
                            runtime,
                            required_capabilities,
                            linker,
                            cancellation,
                        )
                        .map(Arc::new)
                        .map_err(Arc::new))
                },
            )
            .map_err(|error| Arc::new(NativeProductFactError::Query(error)))?;

        result.clone()
    }

    fn compute_native_product_facts(
        &self,
        product: ProductIdentity,
        runtime: Option<RuntimeArtifact>,
        required_capabilities: impl IntoIterator<Item = RuntimeCapability>,
        linker: &Linker,
        cancellation: &CancellationToken,
    ) -> Result<NativeProductFacts, NativeProductFactError> {
        let target = self
            .selected_target()
            .target()
            .codegen_target()
            .map_err(NativeProductFactError::InvalidCodegenTarget)?;

        let semantic = self.product_semantic_facts_with_cancellation(cancellation)?;

        if semantic.value().kind() == ProductKind::Test {
            return Err(NativeProductFactError::UnsupportedProductKind(
                ProductKind::Test,
            ));
        }

        if semantic.value().requires_async_runtime() {
            return Err(NativeProductFactError::UnsupportedAsyncProduct);
        }

        let source_roots = self.product_root_instances(semantic.value(), &target, cancellation)?;

        let (host, units, mappings) = if source_roots.is_empty() {
            (None, Arc::from([]), Vec::new())
        } else {
            let source_reachability =
                self.codegen_reachability(source_roots.clone(), None, &target, cancellation)?;

            let host = self.executable_host(
                &product,
                semantic.value().kind(),
                semantic.value().requires_async_runtime(),
                &source_roots,
                &source_reachability,
                runtime.as_ref(),
                required_capabilities,
                &target,
                cancellation,
            )?;

            let reachability = match host.as_ref() {
                Some(host) => {
                    let root = source_roots
                        .first()
                        .ok_or(NativeProductFactError::MissingProductRoot)?;

                    let host_mir = bray_lowering::lower_executable_host(
                        bray_lowering::ExecutableHostLoweringInput::new(
                            GENERATED_HOST_UNIT,
                            bound_template(root)?,
                            host.clone(),
                            root.target().clone(),
                        ),
                    )
                    .map_err(NativeProductFactError::InvalidHostMir)?;

                    let host_key = CodegenInstanceKey::non_generic(&host_mir);

                    self.codegen_reachability(
                        [host_key],
                        Some((host_mir, root.clone())),
                        &target,
                        cancellation,
                    )?
                }
                None => source_reachability,
            };

            let units = partition_codegen_units(CODEGEN_PARTITION_REVISION, &reachability)
                .map_err(NativeProductFactError::InvalidCodegenUnit)?;

            let roots: BTreeSet<_> = reachability.roots().iter().cloned().collect();

            let mappings = units
                .iter()
                .map(|unit| {
                    self.codegen_mappings_for_product(
                        unit,
                        host.as_ref(),
                        &target,
                        &roots,
                        cancellation,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;

            (host, units, mappings)
        };

        let codegen = self
            .state
            .codegen
            .as_ref()
            .ok_or(NativeProductFactError::CodegenUnavailable)?;

        let policy = BackendEmissionPolicy::new(
            DebugInformationMode::None,
            DebugInformationOutputMode::Omit,
            Some(LinkableArtifactKind::RelocatableObject),
            bray_codegen::BackendSerializationOptions::new(
                AssemblySyntaxKind::TargetDefault,
            ),
        );

        let backend = EmissionBackend::try_new(
            codegen.selected().clone(),
            codegen.selected_capabilities().clone(),
            units.iter().map(|unit| unit.key().clone()),
            policy,
        )
        .map_err(NativeProductFactError::InvalidEmissionBackend)?;

        let link = self.product_link_facts(
            semantic.value().kind(),
            host.as_ref(),
            runtime,
            linker,
            &target,
        )?;

        Ok(NativeProductFacts {
            backend,
            target,
            options: CodegenOptions::default(),
            host,
            link,
            units,
            mappings: shared_slice(mappings),
        })
    }

    fn product_root_instances(
        &self,
        semantic: &bray_symbols::ProductSemanticFacts,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<Vec<CodegenInstanceKey>, NativeProductFactError> {
        let symbols: Vec<_> = match semantic.kind() {
            ProductKind::Executable => semantic.entrypoint().map(AnySymbolId::from).into_iter().collect(),
            ProductKind::Test => semantic
                .test_entries()
                .iter()
                .copied()
                .map(AnySymbolId::from)
                .collect(),
            ProductKind::Library => semantic.public_symbols().to_vec(),
        };

        let mut roots = Vec::new();

        for symbol in symbols {
            let Some(definition) = CallableDefinitionId::try_new(symbol) else {
                continue;
            };

            let substitution = empty_substitution(self.semantic_value_store()?, symbol)?;

            roots.push(self.codegen_instance_key(
                CallableInstanceData::new(definition, substitution),
                target,
                cancellation,
            )?);
        }

        roots.sort_unstable();
        roots.dedup();

        if roots.is_empty() && semantic.kind() != ProductKind::Library {
            return Err(NativeProductFactError::MissingProductRoot);
        }

        Ok(roots)
    }

    fn codegen_reachability(
        &self,
        roots: impl IntoIterator<Item = CodegenInstanceKey>,
        generated_host: Option<(MirUnit, CodegenInstanceKey)>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<bray_codegen::CodegenReachability, NativeProductFactError> {
        let mut builder = CodegenReachabilityBuilder::try_new(roots)
            .map_err(NativeProductFactError::InvalidReachability)?;

        let generated_host = generated_host.map(|(mir, root)| {
            (
                CodegenInstanceKey::non_generic(&mir),
                mir,
                CodegenInstanceDependency::definition(root),
            )
        });

        loop {
            let frontier = builder.take_frontier();

            if frontier.is_empty() {
                break;
            }

            for key in frontier.iter() {
                cancellation.check()?;

                if matches!(key.template(), MirUnitKey::ExternalCallable(_)) {
                    builder
                        .push_external(key.clone())
                        .map_err(NativeProductFactError::InvalidReachability)?;

                    continue;
                }

                if let Some((host_key, host_mir, dependency)) = &generated_host
                    && key == host_key
                {
                    let instance = CodegenInstance::try_new(
                        key.clone(),
                        host_mir.clone(),
                        [dependency.clone()],
                    )
                    .map_err(NativeProductFactError::InvalidCodegenInstance)?;

                    builder
                        .push_instance(instance)
                        .map_err(NativeProductFactError::InvalidReachability)?;

                    continue;
                }

                let mir =
                    self.codegen_mir_for_plan(key, MirUnitId::new(0), None, cancellation)?;

                let dependencies = demanded_callable_references_for_mir(&mir)
                    .into_iter()
                    .map(|reference| {
                        self.codegen_instance_key(reference.instance(), target, cancellation)
                            .map(CodegenInstanceDependency::definition)
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                let instance = CodegenInstance::try_new(key.clone(), mir, dependencies)
                    .map_err(NativeProductFactError::InvalidCodegenInstance)?;

                builder
                    .push_instance(instance)
                    .map_err(NativeProductFactError::InvalidReachability)?;
            }
        }

        builder
            .finish()
            .map_err(NativeProductFactError::InvalidReachability)
    }

    pub(super) fn codegen_instance_key(
        &self,
        callable: CallableInstanceData,
        target: &CodegenTarget,
        _cancellation: &CancellationToken,
    ) -> Result<CodegenInstanceKey, super::super::CodegenFactError> {
        let template = self
            .callable_body_key(callable.definition())?
            .map(MirUnitKey::Bound)
            .unwrap_or_else(|| MirUnitKey::ExternalCallable(callable.definition()));

        Ok(CodegenInstanceKey::new(
            template,
            self.codegen_specialization(callable.substitution())?,
            [],
            MirTargetFacts::new(
                target.profile().clone(),
                self.selected_target().target().runtime_abi(),
            ),
        ))
    }

    fn codegen_specialization(
        &self,
        substitution: GenericSubstitutionId,
    ) -> Result<CodegenSpecialization, super::super::CodegenFactError> {
        let values = self.semantic_value_store()?;

        let substitution = values
            .generic_substitution_data(substitution)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        if substitution.bindings().is_empty() {
            return Ok(CodegenSpecialization::NonGeneric);
        }

        let arguments = substitution
            .bindings()
            .iter()
            .map(|binding| {
                let mut digest = StableDigestHasher::new();

                digest.write(b"bray.codegen-generic-argument");
                binding.argument().hash(&mut digest);

                match binding.argument() {
                    GenericArgument::Type(_) => CodegenGenericArgument::Type(CodegenValueKey::new(
                        digest.finalize(),
                    )),
                    GenericArgument::Constant(_) => CodegenGenericArgument::Constant(
                        CodegenValueKey::new(digest.finalize()),
                    ),
                }
            })
            .collect::<Vec<_>>();

        Ok(CodegenSpecialization::generic(arguments))
    }

    fn executable_host(
        &self,
        product: &ProductIdentity,
        kind: ProductKind,
        is_async: bool,
        roots: &[CodegenInstanceKey],
        reachability: &bray_codegen::CodegenReachability,
        runtime: Option<&RuntimeArtifact>,
        required_capabilities: impl IntoIterator<Item = RuntimeCapability>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<Option<ExecutableHostContract>, NativeProductFactError> {
        if kind == ProductKind::Library {
            return Ok(None);
        }

        let root = roots
            .first()
            .and_then(|root| reachability.instance(root))
            .ok_or(NativeProductFactError::MissingProductRoot)?;

        if !matches!(
            self.codegen_instance_signature(root.key(), cancellation)?
                .result(),
            bray_codegen::CodegenResultMapping::Void
        ) {
            return Err(NativeProductFactError::UnsupportedEntryResult);
        }

        let root_execution = match (is_async, root.protected_frame_identity()) {
            (false, _) => RootExecution::Synchronous,
            (true, Some(frame)) => RootExecution::Asynchronous { frame },
            (true, None) => return Err(NativeProductFactError::MissingProtectedRootFrame),
        };

        let runtime_contract = runtime.map(RuntimeArtifact::contract);

        if is_async && runtime_contract.is_none() {
            return Err(NativeProductFactError::MissingRuntime);
        }

        let runtime_roles: BTreeSet<_> = reachability
            .instances()
            .iter()
            .flat_map(|instance| {
                CodegenUnit::try_new(CODEGEN_PARTITION_REVISION, [instance.mir().clone()])
                    .ok()
                    .into_iter()
                    .flat_map(|unit| demanded_runtime_references(&unit))
                    .map(|reference| reference.role())
            })
            .filter(|role| {
                runtime_contract
                    .is_some_and(|runtime| runtime.role_binding(*role).is_some())
            })
            .collect();

        let mut capabilities: BTreeSet<_> = required_capabilities.into_iter().collect();

        if is_async {
            capabilities.insert(RuntimeCapability::CooperativeExecution);
            capabilities.insert(RuntimeCapability::MainThreadLane);
        }

        let requirements = RuntimeRequirements::new(
            runtime_contract.map(|runtime| runtime.identity().clone()),
            self.selected_target().target().runtime_abi(),
            runtime_contract.map(|runtime| runtime.frame_abi()),
            target.identity().clone(),
            target.panic_abi().clone(),
            runtime_roles,
            capabilities,
            [],
        );

        let native_entry =
            BinarySymbolName::try_new("_start").ok_or(NativeProductFactError::InvalidSymbolName)?;

        let mut builder = ExecutableHostContractBuilder::new(
            product.clone(),
            native_entry,
            root_execution,
            requirements,
        );

        for role in [
            RuntimeAbiRole::RootExecution,
            RuntimeAbiRole::RootCancellationRequest,
            RuntimeAbiRole::CleanupIncidentReporting,
            RuntimeAbiRole::RootTerminalObservation,
            RuntimeAbiRole::StructuredShutdown,
        ] {
            let symbol = BinarySymbolName::try_new(format!("bray_host_{}", role.as_str()))
                .ok_or(NativeProductFactError::InvalidSymbolName)?;

            builder.push_role_binding(RuntimeRoleBinding::new(
                role,
                symbol,
                RuntimeRoleImplementation::CompilerLowering,
            ));
        }

        if let Some(runtime) = runtime_contract {
            builder.select_runtime(runtime.clone());
        }

        builder
            .finish()
            .map(Some)
            .map_err(NativeProductFactError::InvalidExecutableHost)
    }

    fn product_link_facts(
        &self,
        kind: ProductKind,
        host: Option<&ExecutableHostContract>,
        runtime: Option<RuntimeArtifact>,
        linker: &Linker,
        target: &CodegenTarget,
    ) -> Result<ProductLinkFacts, NativeProductFactError> {
        let product = match kind {
            ProductKind::Library => LinkedProductKind::StaticLibrary,
            ProductKind::Executable | ProductKind::Test => LinkedProductKind::Executable,
        };

        let link_target = LinkTarget::try_new(
            target.identity().clone(),
            target.triple(),
            target.machine().architecture(),
            target.machine().object_format(),
            target.relocation_model(),
            target.code_model(),
            LinkModel::Static,
        )
        .map_err(NativeProductFactError::InvalidLinkTarget)?;

        let driver = linker
            .select_identity(&link_target, product)
            .map_err(NativeProductFactError::Linker)?;

        let policy = match product {
            LinkedProductKind::StaticLibrary => LinkPolicy::new(
                DeadStripPolicy::Preserve,
                SectionGarbageCollectionPolicy::Preserve,
                DebugLinkPolicy::None,
                None,
            ),
            LinkedProductKind::Executable | LinkedProductKind::SharedLibrary => LinkPolicy::new(
                DeadStripPolicy::RemoveUnreachable,
                SectionGarbageCollectionPolicy::RemoveUnreferenced,
                DebugLinkPolicy::None,
                None,
            ),
        };

        let native_inputs = self
            .options()
            .native_link_inputs()
            .iter()
            .map(|requirement| {
                let (kind, source) = match requirement.kind() {
                    NativeLinkKind::Dynamic | NativeLinkKind::Static | NativeLinkKind::System => (
                        LinkInputKind::NativeLibrary,
                        LinkInputSource::try_native_library(requirement.name())
                            .ok_or(NativeProductFactError::InvalidNativeLinkInput)?,
                    ),
                    NativeLinkKind::Framework => (
                        LinkInputKind::Framework,
                        LinkInputSource::try_framework(requirement.name())
                            .ok_or(NativeProductFactError::InvalidNativeLinkInput)?,
                    ),
                };

                LinkInputSpec::try_new(
                    kind,
                    source,
                    LinkInputProvenance::HostConfiguration,
                    LinkInputMode::Ordinary,
                )
                .map_err(NativeProductFactError::InvalidLinkInput)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut facts = ProductLinkFacts::new(link_target, driver.clone(), policy)
            .with_native_inputs(native_inputs);

        if let Some(runtime) = runtime {
            facts = facts.with_runtime(runtime);
        }

        if let Some(host) = host {
            facts = facts.with_retained_symbols([host.native_entry().clone()]);
        }

        Ok(facts)
    }
}

fn bound_template(
    key: &CodegenInstanceKey,
) -> Result<bray_bound_tree::BoundUnitKey, NativeProductFactError> {
    match key.template() {
        MirUnitKey::Bound(key) => Ok(key.clone()),
        MirUnitKey::ExecutableHost(_) | MirUnitKey::ExternalCallable(_) => {
            Err(NativeProductFactError::MissingProductRoot)
        }
    }
}

/// A failure to derive complete native product facts.
#[derive(Debug)]
pub enum NativeProductFactError {
    /// The compilation has no selected code generation backend.
    CodegenUnavailable,
    /// The selected product has no executable code root.
    MissingProductRoot,
    /// Native test-product hosting is not available.
    UnsupportedProductKind(ProductKind),
    /// Native asynchronous hosting is not available.
    UnsupportedAsyncProduct,
    /// Native process hosting does not support the selected entry result.
    UnsupportedEntryResult,
    /// An asynchronous root has no protected-frame identity.
    MissingProtectedRootFrame,
    /// An asynchronous product has no selected runtime artifact.
    MissingRuntime,
    /// A generated binary symbol name is invalid.
    InvalidSymbolName,
    /// A configured native link input is invalid.
    InvalidNativeLinkInput,
    /// A lazy compilation fact could not be evaluated.
    Query(FactQueryError),
    /// The selected code generation target is invalid.
    InvalidCodegenTarget(CodegenTargetBuildError),
    /// Code generation reachability is inconsistent.
    InvalidReachability(CodegenReachabilityBuildError),
    /// One concrete code generation instance is invalid.
    InvalidCodegenInstance(CodegenInstanceBuildError),
    /// One code generation unit is invalid.
    InvalidCodegenUnit(CodegenUnitBuildError),
    /// The compiler-generated executable host MIR is invalid.
    InvalidHostMir(bray_ir::MirUnitBuildError),
    /// The compiler-generated executable host contract is invalid.
    InvalidExecutableHost(ExecutableHostContractBuildError),
    /// The selected emitter backend description is invalid.
    InvalidEmissionBackend(EmissionBackendBuildError),
    /// The selected linker target is invalid.
    InvalidLinkTarget(LinkTargetBuildError),
    /// A native link input is invalid.
    InvalidLinkInput(LinkInputBuildError),
    /// No configured linker driver supports the selected product.
    Linker(bray_linker::LinkFailure),
    /// One code generation fact is unavailable.
    Codegen(super::super::CodegenFactError),
}

impl NativeProductFactError {
    /// Returns whether the selected product uses a native feature not yet supported.
    pub const fn is_unsupported(&self) -> bool {
        matches!(
            self,
            Self::CodegenUnavailable
                | Self::UnsupportedProductKind(_)
                | Self::UnsupportedAsyncProduct
                | Self::UnsupportedEntryResult
                | Self::MissingProtectedRootFrame
                | Self::MissingRuntime
                | Self::Codegen(
                    super::super::CodegenFactError::UnsupportedType(_)
                        | super::super::CodegenFactError::UnsupportedCallableAbi(_)
                        | super::super::CodegenFactError::UnsupportedHelper(_)
                        | super::super::CodegenFactError::UnsupportedSpecialization(_)
                )
        )
    }
}

impl From<FactQueryError> for NativeProductFactError {
    fn from(error: FactQueryError) -> Self {
        Self::Query(error)
    }
}

impl From<super::super::CodegenFactError> for NativeProductFactError {
    fn from(error: super::super::CodegenFactError) -> Self {
        Self::Codegen(error)
    }
}
