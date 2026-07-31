use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_binder::SymbolFactProvider;
use bray_base::shared_slice;
use bray_codegen::{
    AssemblySyntaxKind, CodegenInstance, CodegenInstanceBuildError,
    CodegenInstanceDependency, CodegenInstanceKey, CodegenMappings, CodegenOptions,
    CodegenReachabilityBuildError, CodegenReachabilityBuilder, CodegenTarget,
    CodegenTargetBuildError, CodegenUnit, CodegenUnitBuildError,
    DebugInformationMode, DebugInformationOutputMode, LinkableArtifactKind,
    demanded_runtime_references, partition_codegen_units,
};
use bray_emitter::{
    BackendEmissionPolicy, EmissionBackend, EmissionBackendBuildError, ProductLinkFacts,
};
use bray_ir::{MirUnit, MirUnitId, MirUnitKey};
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
    AnySymbolId, CallableDefinitionId, CallableInstanceData,
    GenericDeclarationTemplateFact, GenericOwnerId, NativeLinkKind, ProductIdentity,
    ProductKind, SymbolFactRequest,
};

use super::specialization::{
    ConcreteCodegenInstance, ConcreteCodegenReachability,
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
    link: Option<ProductLinkFacts>,
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

    /// Returns native link facts when link planning was requested.
    pub const fn link(&self) -> Option<&ProductLinkFacts> {
        self.link.as_ref()
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
        linker: Option<&Linker>,
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
        linker: Option<&Linker>,
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
                    .into_iter()
                    .flat_map(Linker::driver_identities)
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
        linker: Option<&Linker>,
        cancellation: &CancellationToken,
    ) -> Result<NativeProductFacts, NativeProductFactError> {
        let target = self
            .selected_target()
            .target()
            .codegen_target()
            .map_err(NativeProductFactError::InvalidCodegenTarget)?;

        let semantic = self.product_semantic_facts_with_cancellation(cancellation)?;
        let product_kind = semantic.value().kind();

        if product_kind == ProductKind::Test {
            return Err(NativeProductFactError::UnsupportedProductKind(
                ProductKind::Test,
            ));
        }

        if semantic.value().requires_async_runtime() {
            return Err(NativeProductFactError::UnsupportedAsyncProduct);
        }

        let source_roots =
            self.product_root_instances(semantic.value(), &target, cancellation)?;

        let (host, units, mappings) = if source_roots.is_empty() {
            (None, Arc::from([]), Vec::new())
        } else {
            let source_reachability =
                self.codegen_reachability(source_roots.clone(), None, &target, cancellation)?;

            let host = self.executable_host(
                &product,
                product_kind,
                semantic.value().requires_async_runtime(),
                &source_roots,
                source_reachability.graph(),
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
                            bound_template(root.key())?,
                            host.clone(),
                            root.key().target().clone(),
                        ),
                    )
                    .map_err(NativeProductFactError::InvalidHostMir)?;

                    let host = ConcreteCodegenInstance::generated(
                        CodegenInstanceKey::non_generic(&host_mir),
                    );

                    self.codegen_reachability(
                        [host],
                        Some((host_mir, root.clone())),
                        &target,
                        cancellation,
                    )?
                }
                None => source_reachability,
            };

            let units =
                partition_codegen_units(CODEGEN_PARTITION_REVISION, reachability.graph())
                    .map_err(NativeProductFactError::InvalidCodegenUnit)?;

            let roots: BTreeSet<_> = reachability
                .graph()
                .roots()
                .iter()
                .cloned()
                .collect();

            let mappings = units
                .iter()
                .map(|unit| {
                    self.codegen_mappings_for_product(
                        unit,
                        host.as_ref(),
                        &target,
                        &roots,
                        &reachability,
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

        let link = linker
            .map(|linker| {
                self.product_link_facts(product_kind, host.as_ref(), runtime, linker, &target)
            })
            .transpose()?;

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
    ) -> Result<Vec<ConcreteCodegenInstance>, NativeProductFactError> {
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
        let facts = self.binder_facts(cancellation)?;

        for symbol in symbols {
            let Some(definition) = CallableDefinitionId::try_new(symbol) else {
                continue;
            };

            let owner =
                GenericOwnerId::try_new(symbol).ok_or(FactQueryError::InfrastructureFailure)?;

            let generic = facts
                .symbol_fact(SymbolFactRequest::<GenericDeclarationTemplateFact>::new(
                    owner,
                ))
                .map_err(super::super::binder::binder_fact_error)?;

            if !generic.value().parameters().is_empty() {
                continue;
            }

            let substitution = empty_substitution(self.semantic_value_store()?, symbol)?;

            roots.push(self.concrete_codegen_callable(
                CallableInstanceData::new(definition, substitution),
                [],
                target,
                cancellation,
            )?);
        }

        roots.sort_unstable_by(|left, right| left.key().cmp(right.key()));
        roots.dedup_by(|left, right| left.key() == right.key());

        if roots.is_empty() && semantic.kind() != ProductKind::Library {
            return Err(NativeProductFactError::MissingProductRoot);
        }

        Ok(roots)
    }

    fn codegen_reachability(
        &self,
        roots: impl IntoIterator<Item = ConcreteCodegenInstance>,
        generated_host: Option<(MirUnit, ConcreteCodegenInstance)>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<ConcreteCodegenReachability, NativeProductFactError> {
        let roots: Vec<_> = roots.into_iter().collect();

        let mut realizations: BTreeMap<_, _> = roots
            .iter()
            .cloned()
            .map(|instance| (instance.key().clone(), instance))
            .collect();

        let mut builder = CodegenReachabilityBuilder::try_new(
            roots.iter().map(|instance| instance.key().clone()),
        )
            .map_err(NativeProductFactError::InvalidReachability)?;

        let generated_host = generated_host.map(|(mir, root)| {
            realizations.insert(root.key().clone(), root.clone());

            (
                CodegenInstanceKey::non_generic(&mir),
                mir,
                CodegenInstanceDependency::definition(root.key().clone()),
            )
        });

        loop {
            let frontier = builder.take_frontier();

            if frontier.is_empty() {
                break;
            }

            for key in frontier.iter() {
                cancellation.check()?;

                let realization = realizations
                    .get(key)
                    .cloned()
                    .ok_or(FactQueryError::InfrastructureFailure)?;

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

                let mir = match realization.generated_lifecycle_reference() {
                    Some(reference) => self.codegen_generated_lifecycle_mir(
                        key,
                        reference,
                        MirUnitId::new(0),
                        cancellation,
                    )?,
                    None => {
                        self.codegen_mir_for_plan(
                            key,
                            MirUnitId::new(0),
                            None,
                            cancellation,
                        )?
                    }
                };

                let concrete_dependencies = self
                    .concrete_codegen_dependencies_for_mir(
                        &realization,
                        &mir,
                        target,
                        cancellation,
                    )?;

                let dependencies = concrete_dependencies
                    .iter()
                    .map(|dependency| {
                        CodegenInstanceDependency::definition(
                            dependency.key().clone(),
                        )
                    })
                    .collect::<Vec<_>>();

                for dependency in concrete_dependencies {
                    match realizations.entry(dependency.key().clone()) {
                        std::collections::btree_map::Entry::Vacant(entry) => {
                            entry.insert(dependency);
                        }
                        std::collections::btree_map::Entry::Occupied(entry) => {
                            if entry.get() != &dependency {
                                return Err(
                                    FactQueryError::InfrastructureFailure.into(),
                                );
                            }
                        }
                    }
                }

                let instance = CodegenInstance::try_new(key.clone(), mir, dependencies)
                    .map_err(NativeProductFactError::InvalidCodegenInstance)?;

                builder
                    .push_instance(instance)
                    .map_err(NativeProductFactError::InvalidReachability)?;
            }
        }

        let graph = builder
            .finish()
            .map_err(NativeProductFactError::InvalidReachability)?;

        Ok(ConcreteCodegenReachability::new(graph, realizations))
    }

    fn executable_host(
        &self,
        product: &ProductIdentity,
        kind: ProductKind,
        is_async: bool,
        roots: &[ConcreteCodegenInstance],
        reachability: &bray_codegen::CodegenReachability,
        runtime: Option<&RuntimeArtifact>,
        required_capabilities: impl IntoIterator<Item = RuntimeCapability>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<Option<ExecutableHostContract>, NativeProductFactError> {
        if kind == ProductKind::Library {
            return Ok(None);
        }

        let root_realization = roots
            .first()
            .ok_or(NativeProductFactError::MissingProductRoot)?;

        let root = reachability
            .instance(root_realization.key())
            .ok_or(NativeProductFactError::MissingProductRoot)?;

        if !matches!(
            self.codegen_instance_signature(root_realization, cancellation)?
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
    let MirUnitKey::Bound(template) = key.template() else {
        return Err(NativeProductFactError::MissingProductRoot);
    };

    Ok(template.clone())
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
    /// Returns whether the selected target cannot realize a demanded native representation.
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use bray_codegen::{
        CodegenGenericArgument, CodegenResultMapping, CodegenSpecialization,
        partition_codegen_units,
    };
    use bray_compiler_known::RepresentationRole;
    use bray_symbols::{
        CallableDefinitionId, CallableInstanceData, GenericArgument, GenericOwnerId,
        GenericParameterSymbolId, GenericSubstitutionData, ImplementationRequirementKey,
        ImplementationSelection, NamedTypeSymbolId, SymbolOrigin, TraitApplicationData,
        ConstantTermData, ConstantValueData, ConstantValueKind, TypeData,
    };

    use super::CODEGEN_PARTITION_REVISION;
    use crate::CancellationToken;

    const CONCRETE_GENERIC_SOURCE: &str = concat!(
        "module app;\n",
        "\n",
        "func entry()\n",
        "{\n",
        "    main();\n",
        "}\n",
        "\n",
        "func accept<T>(pos value: T) -> T\n",
        "{\n",
        "    return value;\n",
        "}\n",
        "\n",
        "func repeat<const count: usize>() -> usize\n",
        "{\n",
        "    return count;\n",
        "}\n",
        "\n",
        "func main()\n",
        "{\n",
        "    let accepted: i32 = accept<i32>(1);\n",
        "    let repeated: usize = repeat<2>();\n",
        "}\n",
    );

    #[test]
    fn concrete_generic_instances_realize_signatures_and_layouts() {
        let compilation =
            crate::test_support::compilation(CONCRETE_GENERIC_SOURCE);

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        let cancellation = CancellationToken::new();

        let target = compilation
            .selected_target()
            .target()
            .codegen_target()
            .unwrap_or_else(|error| panic!("test codegen target must validate: {error:?}"));

        let semantic = compilation
            .product_semantic_facts()
            .unwrap_or_else(|error| panic!("test product facts must resolve: {error:?}"));

        let roots = compilation
            .product_root_instances(semantic.value(), &target, &cancellation)
            .unwrap_or_else(|error| panic!("test roots must resolve: {error:?}"));

        let reachability = compilation
            .codegen_reachability(roots.clone(), None, &target, &cancellation)
            .unwrap_or_else(|error| panic!("generic reachability must close: {error:?}"));

        let reversed_reachability = compilation
            .codegen_reachability(
                roots.into_iter().rev(),
                None,
                &target,
                &cancellation,
            )
            .unwrap_or_else(|error| panic!("reversed reachability must close: {error:?}"));

        assert_eq!(reachability.graph(), reversed_reachability.graph());

        for instance in reachability.graph().instances() {
            assert_eq!(
                reachability.instance(instance.key()),
                reversed_reachability.instance(instance.key())
            );
        }

        let units = partition_codegen_units(
            CODEGEN_PARTITION_REVISION,
            reachability.graph(),
        )
        .unwrap_or_else(|error| panic!("generic units must partition: {error:?}"));

        let mut saw_concrete_generic_signature = false;
        let mut saw_const_specialization = false;

        for unit in units.iter() {
            let instance = &unit.instances()[0];

            let realization = reachability
                .instance(instance.key())
                .unwrap_or_else(|| panic!("reachable instance payload must be retained"));

            let signature = compilation
                .codegen_instance_signature(realization, &cancellation)
                .unwrap_or_else(|error| panic!("generic signature must realize: {error:?}"));

            let mappings = compilation
                .codegen_mappings_for_product(
                    unit,
                    None,
                    &target,
                    &reachability
                        .graph()
                        .roots()
                        .iter()
                        .cloned()
                        .collect(),
                    &reachability,
                    &cancellation,
                )
                .unwrap_or_else(|error| panic!("generic mappings must realize: {error:?}"));

            if instance
                .key()
                .specialization()
                .arguments()
                .iter()
                .any(|argument| matches!(argument, CodegenGenericArgument::Constant(_)))
            {
                saw_const_specialization = true;
            }

            let CodegenResultMapping::Direct { ty, .. } = signature.result() else {
                continue;
            };

            assert!(mappings.ty(*ty).is_some());

            let values = compilation
                .semantic_value_store()
                .unwrap_or_else(|error| panic!("semantic values must resolve: {error:?}"));

            let data = values
                .type_data(*ty)
                .unwrap_or_else(|error| panic!("signature result must resolve: {error:?}"));

            assert!(!matches!(data.as_ref(), TypeData::TypeParameter(_)));

            saw_concrete_generic_signature |= matches!(
                instance.key().specialization(),
                CodegenSpecialization::Generic(_)
            );
        }

        assert!(saw_concrete_generic_signature);
        assert!(saw_const_specialization);
    }

    #[test]
    fn concrete_generic_specializations_are_stable_across_store_order() {
        let first = crate::test_support::compilation(CONCRETE_GENERIC_SOURCE);
        let second = crate::test_support::compilation(CONCRETE_GENERIC_SOURCE);

        perturb_semantic_value_order(&second);

        let first = concrete_generic_specializations(&first);
        let second = concrete_generic_specializations(&second);

        assert_eq!(first, second);

        assert!(first.iter().any(|specialization| {
            specialization.arguments().iter().any(|argument| {
                matches!(argument, CodegenGenericArgument::Type(_))
            })
        }));

        assert!(first.iter().any(|specialization| {
            specialization.arguments().iter().any(|argument| {
                matches!(argument, CodegenGenericArgument::Constant(_))
            })
        }));
    }

    #[test]
    fn concrete_generic_instances_retain_exact_implementation_witnesses() {
        let compilation = crate::test_support::compilation(concat!(
            "module app;\n",
            "\n",
            "func entry()\n",
            "{\n",
            "}\n",
            "\n",
            "trait Converts<T>\n",
            "{\n",
            "}\n",
            "\n",
            "struct Wrapper<T>\n",
            "{\n",
            "}\n",
            "\n",
            "impl WrapperConverts = Wrapper<T>(Converts<T>) with(true)\n",
            "{\n",
            "}\n",
            "\n",
            "func target<T>()\n",
            "{\n",
            "}\n",
        ));

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        let cancellation = CancellationToken::new();

        let target = compilation
            .selected_target()
            .target()
            .codegen_target()
            .unwrap_or_else(|error| panic!("test codegen target must validate: {error:?}"));

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("test symbols must resolve: {error:?}"));

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("test values must resolve: {error:?}"));

        let wrapper = symbols
            .structures()
            .iter()
            .find(|symbol| symbol.origin() == SymbolOrigin::Source)
            .unwrap_or_else(|| panic!("test wrapper must be declared"));

        let conversion = symbols
            .traits()
            .iter()
            .find(|symbol| symbol.origin() == SymbolOrigin::Source)
            .unwrap_or_else(|| panic!("test trait must be declared"));

        let target_function = symbols
            .functions()
            .iter()
            .find(|symbol| {
                symbols
                    .member_name(symbol.id().into())
                    .is_some_and(|name| name.as_str() == "target")
            })
            .unwrap_or_else(|| panic!("test target must be declared"));

        let boolean_definition = compilation
            .available_compiler_known_symbols()
            .representation_symbol::<bray_symbols::StructSymbolId>(
                RepresentationRole::ScalarBool,
            )
            .unwrap_or_else(|| panic!("test bool representation must be available"));

        let boolean = named_test_type(values, boolean_definition, [], []);

        let wrapper_type = named_test_type(
            values,
            wrapper.id(),
            wrapper
                .generic_type_parameters()
                .iter()
                .copied()
                .map(GenericParameterSymbolId::Type),
            [GenericArgument::Type(boolean)],
        );

        let trait_substitution = test_substitution(
            values,
            conversion.id().into(),
            conversion
                .generic_type_parameters()
                .iter()
                .copied()
                .map(GenericParameterSymbolId::Type),
            [GenericArgument::Type(boolean)],
        );

        let trait_application = values
            .intern_trait_application(TraitApplicationData::new(
                conversion.id(),
                trait_substitution,
            ))
            .unwrap_or_else(|error| panic!("test trait application must intern: {error:?}"));

        let selection = compilation
            .implementation_selection_result(ImplementationRequirementKey::new(
                wrapper_type,
                trait_application,
            ))
            .unwrap_or_else(|error| panic!("test witness must select: {error:?}"));

        let ImplementationSelection::Selected(witness) = *selection.value() else {
            panic!("test generic implementation must be selected");
        };

        let callable_substitution = test_substitution(
            values,
            target_function.id().into(),
            target_function
                .generic_type_parameters()
                .iter()
                .copied()
                .map(GenericParameterSymbolId::Type),
            [GenericArgument::Type(boolean)],
        );

        let definition = CallableDefinitionId::try_new(target_function.id().into())
            .unwrap_or_else(|| panic!("test target must be callable"));

        let root = compilation
            .concrete_codegen_callable(
                CallableInstanceData::new(definition, callable_substitution),
                [witness],
                &target,
                &cancellation,
            )
            .unwrap_or_else(|error| panic!("test concrete root must realize: {error:?}"));

        assert!(
            super::ConcreteCodegenInstance::try_callable(
                root.key().clone(),
                root.callable_instance()
                    .unwrap_or_else(|| panic!("test root must retain its callable")),
                CodegenSpecialization::NonGeneric,
                [],
            )
            .is_none()
        );

        let reachability = compilation
            .codegen_reachability([root], None, &target, &cancellation)
            .unwrap_or_else(|error| panic!("generic reachability must close: {error:?}"));

        let [instance] = reachability.graph().instances() else {
            panic!("test root must be the only reachable instance");
        };

        assert!(matches!(
            instance.key().witnesses()[0].specialization(),
            CodegenSpecialization::Generic(_)
        ));

        let realization = reachability
            .instance(instance.key())
            .unwrap_or_else(|| panic!("test witness payload must be retained"));

        assert_eq!(realization.witness_instances(), [witness]);
    }

    fn named_test_type(
        values: &bray_symbols::SemanticValueStore,
        definition: bray_symbols::StructSymbolId,
        parameters: impl IntoIterator<Item = GenericParameterSymbolId>,
        arguments: impl IntoIterator<Item = GenericArgument>,
    ) -> bray_symbols::TypeId {
        let substitution = test_substitution(
            values,
            definition.into(),
            parameters,
            arguments,
        );

        values
            .intern_type(TypeData::Named {
                definition: NamedTypeSymbolId::Struct(definition),
                substitution,
            })
            .unwrap_or_else(|error| panic!("test named type must intern: {error:?}"))
    }

    fn test_substitution(
        values: &bray_symbols::SemanticValueStore,
        owner: bray_symbols::AnySymbolId,
        parameters: impl IntoIterator<Item = GenericParameterSymbolId>,
        arguments: impl IntoIterator<Item = GenericArgument>,
    ) -> bray_symbols::GenericSubstitutionId {
        let owner = GenericOwnerId::try_new(owner)
            .unwrap_or_else(|| panic!("test substitution owner must be generic"));

        let substitution = GenericSubstitutionData::try_new(owner, parameters, arguments)
            .unwrap_or_else(|error| panic!("test substitution must validate: {error:?}"));

        values
            .intern_generic_substitution(substitution)
            .unwrap_or_else(|error| panic!("test substitution must intern: {error:?}"))
    }

    fn perturb_semantic_value_order(compilation: &crate::Compilation) {
        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must resolve: {error:?}"));

        let mut ty = values
            .intern_type(TypeData::tuple([]))
            .unwrap_or_else(|error| panic!("noise tuple type must intern: {error:?}"));

        let mut value = values
            .intern_constant_value(ConstantValueData::new(
                ty,
                ConstantValueKind::Unit,
            ))
            .unwrap_or_else(|error| panic!("noise unit value must intern: {error:?}"));

        for _ in 0..4 {
            ty = values
                .intern_type(TypeData::Nullable(ty))
                .unwrap_or_else(|error| {
                    panic!("noise nullable type must intern: {error:?}")
                });

            value = values
                .intern_constant_value(ConstantValueData::new(
                    ty,
                    ConstantValueKind::NullablePresent(value),
                ))
                .unwrap_or_else(|error| {
                    panic!("noise nullable value must intern: {error:?}")
                });

            values
                .intern_constant_term(ConstantTermData::Value(value))
                .unwrap_or_else(|error| {
                    panic!("noise constant term must intern: {error:?}")
                });
        }
    }

    fn concrete_generic_specializations(
        compilation: &crate::Compilation,
    ) -> BTreeSet<CodegenSpecialization> {
        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        let cancellation = CancellationToken::new();

        let target = compilation
            .selected_target()
            .target()
            .codegen_target()
            .unwrap_or_else(|error| {
                panic!("test codegen target must validate: {error:?}")
            });

        let semantic = compilation
            .product_semantic_facts()
            .unwrap_or_else(|error| {
                panic!("test product facts must resolve: {error:?}")
            });

        let roots = compilation
            .product_root_instances(
                semantic.value(),
                &target,
                &cancellation,
            )
            .unwrap_or_else(|error| panic!("test roots must resolve: {error:?}"));

        compilation
            .codegen_reachability(roots, None, &target, &cancellation)
            .unwrap_or_else(|error| {
                panic!("generic reachability must close: {error:?}")
            })
            .graph()
            .instances()
            .iter()
            .filter_map(|instance| match instance.key().specialization() {
                CodegenSpecialization::Generic(_) => {
                    Some(instance.key().specialization().clone())
                }
                CodegenSpecialization::NonGeneric => None,
            })
            .collect()
    }
}
