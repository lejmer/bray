use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_binder::SymbolFactProvider;
use bray_base::shared_slice;
use bray_codegen::{
    AssemblySyntaxKind, CodegenInstance, CodegenInstanceDependency, CodegenInstanceKey,
    CodegenMappings, CodegenOptions, CodegenReachabilityBuilder, CodegenTarget, CodegenUnit,
    DebugInformationMode, DebugInformationOutputMode, LinkableArtifactKind,
    demanded_runtime_references, partition_codegen_units,
};
use bray_compiler_known::RepresentationRole;
use bray_emitter::{BackendEmissionPolicy, EmissionBackend, ProductLinkFacts};
use bray_ir::{MirUnit, MirUnitId, MirUnitKey};
use bray_linker::{
    DeadStripPolicy, DebugLinkPolicy, LinkInputKind, LinkInputMode, LinkInputProvenance,
    LinkInputSource, LinkInputSpec, LinkModel, LinkPolicy, LinkTarget, LinkedProductKind,
    Linker, SectionGarbageCollectionPolicy,
};
use bray_runtime_interface::{
    BinarySymbolName, ExecutableEntryResult, ExecutableHostContract,
    ExecutableHostContractBuilder, RootExecution, RuntimeAbiRole, RuntimeArtifact,
    RuntimeCapability, RuntimeRequirements, RuntimeRoleBinding, RuntimeRoleImplementation,
};
use bray_symbols::{
    AnySymbolId, CallableDefinitionId, CallableInstanceData,
    GenericDeclarationTemplateFact, GenericOwnerId, NativeLinkKind, ProductIdentity,
    ProductKind, SymbolFactRequest,
};

use super::super::specialization::{
    ConcreteCodegenInstance, ConcreteCodegenReachability,
};
use super::super::super::Compilation;
use super::super::super::substitution::empty_substitution;
use crate::fact::{
    CancellationToken, CompilationFactKey, FactQueryError, NativeProductFactKey,
};
use super::error::NativeProductFactError;

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

        let source_roots =
            self.product_root_instances(semantic.value(), &target, cancellation)?;

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
                .map_err(super::super::super::binder::binder_fact_error)?;

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

        let entry_result_type = root
            .mir()
            .frame_descriptor()
            .map(bray_ir::MirFrameDescriptor::result_type);

        let entry_result = self.executable_entry_result(
            root_realization,
            entry_result_type,
            cancellation,
        )?;

        let root_frame = root.protected_frame_identity();

        let root_execution = match (is_async, root_frame) {
            (false, _) => RootExecution::Synchronous,
            (true, Some(frame)) => RootExecution::Asynchronous { frame },
            (true, None) => return Err(NativeProductFactError::MissingProtectedRootFrame),
        };

        let runtime_contract = runtime.map(RuntimeArtifact::contract);

        if is_async && runtime_contract.is_none() {
            return Err(NativeProductFactError::MissingRuntime);
        }

        let mut runtime_roles: BTreeSet<_> = reachability
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

        if is_async {
            runtime_roles.extend([
                RuntimeAbiRole::RootExecution,
                RuntimeAbiRole::StructuredShutdown,
            ]);
        }

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

        builder.set_entry_result(entry_result);

        if let RootExecution::Asynchronous { frame } = root_execution {
            builder.set_root_frame_adapter(
                super::super::realization::generated_frame_symbol_name(
                    target,
                    frame,
                    bray_runtime_interface::ProtectedFrameOperation::MoveBeforeStart,
                )?,
            );
        }

        for role in [
            RuntimeAbiRole::RootExecution,
            RuntimeAbiRole::RootCancellationRequest,
            RuntimeAbiRole::CleanupIncidentReporting,
            RuntimeAbiRole::RootTerminalObservation,
            RuntimeAbiRole::StructuredShutdown,
        ] {
            if runtime_contract.is_some_and(|runtime| runtime.role_binding(role).is_some()) {
                continue;
            }

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

    fn executable_entry_result(
        &self,
        root: &ConcreteCodegenInstance,
        async_result: Option<bray_symbols::TypeId>,
        cancellation: &CancellationToken,
    ) -> Result<ExecutableEntryResult, NativeProductFactError> {
        let ty = match async_result {
            Some(ty) => ty,
            None => match self.codegen_instance_signature(root, cancellation)?.result() {
                bray_codegen::CodegenResultMapping::Void => {
                    return Ok(ExecutableEntryResult::Unit);
                }
                bray_codegen::CodegenResultMapping::Direct { ty, .. } => *ty,
                bray_codegen::CodegenResultMapping::Indirect { pointee, .. } => *pointee,
            },
        };

        let values = self.semantic_value_store()?;

        let data = values
            .type_data(ty)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let bray_symbols::TypeData::Named { definition, .. } = data.as_ref() else {
            return Err(NativeProductFactError::InvalidEntryResult);
        };

        let role =
            super::super::super::foreign::compiler_known_representation(self, *definition);

        match role {
            Some(RepresentationRole::ScalarI32) => Ok(ExecutableEntryResult::I32),
            Some(RepresentationRole::Result) => {
                let representation = self
                    .available_compiler_known_symbols()
                    .result_representation()
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                Ok(ExecutableEntryResult::Fallible {
                    ty,
                    success_variant: representation.success_variant(),
                })
            }
            _ => Err(NativeProductFactError::InvalidEntryResult),
        }
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use bray_codegen::{
        BackendArtifactId, BackendArtifactKind, BackendArtifactRequest,
        BackendArtifactRequestEntry, BackendArtifactRequirement,
        BackendSerializationOptions, CodeGenerator, CodeGeneratorRegistry,
        CodegenConfiguration, CodegenGenericArgument, CodegenRequest, CodegenResultMapping,
        CodegenSpecialization, CodegenStatus, DebugInformationOutputMode,
        LinkableArtifactKind, LinkableArtifactRequirement, partition_codegen_units,
    };
    use bray_compiler_known::RepresentationRole;
    use bray_diagnostics::DiagnosticBag;
    use bray_linker::{
        LinkFailure, LinkOutcome, LinkPlan, Linker, LinkerDriver, LinkerDriverIdentity,
        LinkerDriverKind, LinkedProductKind,
    };
    use bray_runtime_interface::{
        BinarySymbolName, ExecutableEntryResult, ProtectedFrameAbiVersions,
        ProtectedFrameOperation, RootExecution, RuntimeAbiRole, RuntimeAbiVersion,
        RuntimeArtifact, RuntimeArtifactDigest, RuntimeArtifactId, RuntimeArtifactMetadata,
        RuntimeCapability, RuntimeContract, RuntimeIdentity, RuntimeRoleBinding,
        RuntimeRoleImplementation,
    };
    use bray_symbols::{
        CallableDefinitionId, CallableInstanceData, ConstantTermData, ConstantValueData,
        ConstantValueKind, GenericArgument, GenericOwnerId, GenericParameterSymbolId,
        GenericSubstitutionData, ImplementationRequirementKey, ImplementationSelection,
        NamedTypeSymbolId, ProductIdentity, ProductKind, SymbolOrigin, TraitApplicationData,
        TypeData,
    };
    use bray_testing::TemporaryFile;

    use super::CODEGEN_PARTITION_REVISION;
    use crate::{
        CancellationToken, CompilationOptions, CompilationRequest, SelectedTarget,
        WorkerBudget,
    };

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
    fn synchronous_i32_executable_hosts_emit_native_units() {
        let backend = Arc::new(
            bray_codegen_llvm::LlvmCodeGenerator::try_new()
                .unwrap_or_else(|error| panic!("LLVM backend must initialize: {error:?}")),
        );

        let registry = CodeGeneratorRegistry::try_new([
            Arc::clone(&backend) as Arc<dyn CodeGenerator>
        ])
        .unwrap_or_else(|error| panic!("LLVM backend must register: {error:?}"));

        let codegen = CodegenConfiguration::try_new(registry, backend.identity().clone())
            .unwrap_or_else(|error| panic!("LLVM backend must select: {error:?}"));

        let request = CompilationRequest::with_options(
            crate::test_support::package_identity(),
            vec![crate::test_support::source_input(
                concat!(
                    "module app;\n",
                    "\n",
                    "func main() -> i32\n",
                    "{\n",
                    "    return 42;\n",
                    "}\n",
                ),
                0,
            )],
            CompilationOptions::new(
                WorkerBudget::serial(),
                ProductKind::Executable,
                SelectedTarget::baseline(),
            ),
        );

        let compilation = crate::Compilation::load_with_codegen(request, codegen)
            .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"));

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        let product = ProductIdentity::try_new(
            crate::test_support::package_identity(),
            "application",
        )
        .unwrap_or_else(|| panic!("test product identity must be valid"));

        let facts = compilation
            .native_product_facts(product, None, [], &test_linker())
            .unwrap_or_else(|error| panic!("native facts must resolve: {error:?}"));

        let host = facts
            .executable_host()
            .unwrap_or_else(|| panic!("executable must own a host"));

        assert_eq!(host.root(), RootExecution::Synchronous);
        assert_eq!(host.entry_result(), ExecutableEntryResult::I32);

        assert!(
            generated_artifacts(&backend, &facts)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }

    #[test]
    fn asynchronous_executable_hosts_emit_complete_deterministic_native_units() {
        let backend = Arc::new(
            bray_codegen_llvm::LlvmCodeGenerator::try_new()
                .unwrap_or_else(|error| panic!("LLVM backend must initialize: {error:?}")),
        );

        let selected = backend.identity().clone();

        let registry = CodeGeneratorRegistry::try_new([
            Arc::clone(&backend) as Arc<dyn CodeGenerator>
        ])
        .unwrap_or_else(|error| panic!("LLVM backend must register: {error:?}"));

        let codegen = CodegenConfiguration::try_new(registry, selected)
            .unwrap_or_else(|error| panic!("LLVM backend must select: {error:?}"));

        let request = CompilationRequest::with_options(
            crate::test_support::package_identity(),
            vec![crate::test_support::source_input(
                concat!(
                    "module app;\n",
                    "\n",
                    "async func main() -> i32\n",
                    "{\n",
                    "    return 42;\n",
                    "}\n",
                ),
                0,
            )],
            CompilationOptions::new(
                WorkerBudget::serial(),
                ProductKind::Executable,
                SelectedTarget::baseline(),
            ),
        );

        let compilation = crate::Compilation::load_with_codegen(request, codegen)
            .unwrap_or_else(|error| panic!("test compilation must load: {error:?}"));

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        let archive = TemporaryFile::write("libbray_runtime.a", b"runtime archive");
        let runtime = runtime_artifact(&compilation, archive.path());
        let linker = test_linker();

        let product = ProductIdentity::try_new(
            crate::test_support::package_identity(),
            "application",
        )
        .unwrap_or_else(|| panic!("test product identity must be valid"));

        let facts = compilation
            .native_product_facts(
                product,
                Some(runtime),
                [
                    RuntimeCapability::CooperativeExecution,
                    RuntimeCapability::MainThreadLane,
                ],
                &linker,
            )
            .unwrap_or_else(|error| panic!("async native facts must resolve: {error:?}"));

        let host = facts
            .executable_host()
            .unwrap_or_else(|| panic!("async executable must own a host"));

        let RootExecution::Asynchronous { frame } = host.root() else {
            panic!("async executable host must retain a protected root frame");
        };

        assert_eq!(host.entry_result(), ExecutableEntryResult::I32);

        let frame_unit = facts
            .units()
            .iter()
            .find(|unit| {
                unit.instances()
                    .iter()
                    .any(|instance| instance.protected_frame_identity() == Some(frame))
            })
            .unwrap_or_else(|| panic!("concrete root frame unit must be retained"));

        let frame_mapping = facts
            .units()
            .iter()
            .zip(facts.mappings())
            .find_map(|(unit, mappings)| {
                (unit.key() == frame_unit.key()).then_some(mappings)
            })
            .unwrap_or_else(|| panic!("root frame mappings must be retained"));

        let adapter = frame_mapping
            .symbol(&bray_codegen::CodegenSymbolKey::ProtectedFrame {
                frame,
                operation: ProtectedFrameOperation::MoveBeforeStart,
            })
            .map(bray_codegen::CodegenSymbolMapping::name);

        assert_eq!(host.root_frame_adapter(), adapter);

        for role in [
            RuntimeAbiRole::RootExecution,
            RuntimeAbiRole::StructuredShutdown,
        ] {
            assert_eq!(
                host.role_binding(role)
                    .map(RuntimeRoleBinding::implementation),
                Some(RuntimeRoleImplementation::BrayRuntime)
            );
        }

        let first = generated_artifacts(&backend, &facts);
        let second = generated_artifacts(&backend, &facts);

        assert_eq!(first, second);
        assert_eq!(first.len(), facts.units().len());
        assert!(first.iter().all(|artifact| !artifact.is_empty()));
    }

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

    fn runtime_artifact(
        compilation: &crate::Compilation,
        archive: &std::path::Path,
    ) -> RuntimeArtifact {
        let target = compilation
            .selected_target()
            .target()
            .codegen_target()
            .unwrap_or_else(|error| panic!("test target must validate: {error:?}"));

        let identity = RuntimeIdentity::try_new("bray.runtime.test")
            .unwrap_or_else(|| panic!("test runtime identity must be valid"));

        let artifact = RuntimeArtifactId::try_new("bray.runtime.test.x86_64")
            .unwrap_or_else(|| panic!("test runtime artifact identity must be valid"));

        let bindings = RuntimeAbiRole::ALL.into_iter().map(|role| {
            let symbol = BinarySymbolName::try_new(format!(
                "bray_runtime_{}_v1",
                role.as_str()
            ))
            .unwrap_or_else(|| panic!("test runtime role symbol must be valid"));

            RuntimeRoleBinding::new(
                role,
                symbol,
                RuntimeRoleImplementation::BrayRuntime,
            )
        });

        let version = RuntimeAbiVersion::new(1, 0);

        let contract = RuntimeContract::try_new(
            identity,
            artifact,
            version,
            ProtectedFrameAbiVersions::uniform(version),
            target.identity().clone(),
            target.panic_abi().clone(),
            [
                RuntimeCapability::CooperativeExecution,
                RuntimeCapability::LocalLanes,
                RuntimeCapability::MainThreadLane,
            ],
            bindings,
        )
        .unwrap_or_else(|error| panic!("test runtime contract must validate: {error:?}"));

        let digest = RuntimeArtifactDigest::new([11; 32]);

        let metadata = RuntimeArtifactMetadata::try_new(
            contract,
            "libbray_runtime.a",
            digest,
        )
        .unwrap_or_else(|error| panic!("test runtime metadata must validate: {error:?}"));

        RuntimeArtifact::try_new(metadata, archive, digest)
            .unwrap_or_else(|error| panic!("test runtime artifact must validate: {error:?}"))
    }

    fn generated_artifacts(
        backend: &bray_codegen_llvm::LlvmCodeGenerator,
        facts: &super::NativeProductFacts,
    ) -> Vec<Vec<u8>> {
        facts
            .units()
            .iter()
            .zip(facts.mappings())
            .map(|(unit, mappings)| {
                let artifact = BackendArtifactId::new(
                    unit.key().clone(),
                    BackendArtifactKind::RelocatableObject,
                    0,
                );

                let artifacts = BackendArtifactRequest::try_new(
                    unit.key().clone(),
                    [BackendArtifactRequestEntry::new(
                        artifact,
                        BackendArtifactRequirement::Required,
                    )],
                    DebugInformationOutputMode::Omit,
                    Some(LinkableArtifactRequirement::new(
                        LinkableArtifactKind::RelocatableObject,
                        BackendArtifactRequirement::Required,
                    )),
                    BackendSerializationOptions::new(
                        bray_codegen::AssemblySyntaxKind::TargetDefault,
                    ),
                )
                .unwrap_or_else(|error| panic!("test artifact request must validate: {error:?}"));

                let cancellation = CancellationToken::new();

                let request = CodegenRequest::try_new(
                    unit,
                    backend.identity(),
                    facts.target(),
                    mappings,
                    facts.options(),
                    &artifacts,
                    &cancellation,
                )
                .unwrap_or_else(|error| panic!("test codegen request must validate: {error:?}"));

                let outcome = backend.generate(request);

                assert!(
                    matches!(outcome.status(), CodegenStatus::Complete(_)),
                    "{:?}",
                    outcome.status()
                );

                let contribution = outcome
                    .artifacts()
                    .and_then(|artifacts| artifacts.contributions().first())
                    .unwrap_or_else(|| panic!("test codegen must publish one artifact"));

                match contribution.content().source() {
                    bray_codegen::ArtifactContentSource::Memory(bytes) => bytes.to_vec(),
                    bray_codegen::ArtifactContentSource::CompilerSpool(_) => {
                        panic!("test artifact must remain memory-backed");
                    }
                }
            })
            .collect()
    }

    fn test_linker() -> Linker {
        Linker::try_new([Arc::new(TestLinkerDriver) as Arc<dyn LinkerDriver>])
            .unwrap_or_else(|error| panic!("test linker must validate: {error:?}"))
    }

    struct TestLinkerDriver;

    impl LinkerDriver for TestLinkerDriver {
        fn identity(&self) -> &LinkerDriverIdentity {
            static IDENTITY: std::sync::OnceLock<LinkerDriverIdentity> =
                std::sync::OnceLock::new();

            IDENTITY.get_or_init(|| {
                LinkerDriverIdentity::try_new(
                    LinkerDriverKind::EmbeddedLld,
                    "test-lld",
                    "1",
                    "22",
                )
                .unwrap_or_else(|| panic!("test linker identity must be valid"))
            })
        }

        fn supports(
            &self,
            _target: &bray_linker::LinkTarget,
            _product: LinkedProductKind,
        ) -> bool {
            true
        }

        fn link(
            &self,
            _plan: &LinkPlan,
            _cancellation: &dyn bray_base::Cancellation,
        ) -> LinkOutcome {
            LinkOutcome::failed(LinkFailure::Invocation, DiagnosticBag::new())
        }
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
