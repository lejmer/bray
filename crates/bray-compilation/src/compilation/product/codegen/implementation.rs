use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_base::shared_slice;
use bray_binder::BinderFactContext;
use bray_codegen::{
    AssemblySyntaxKind, CodegenInstance, CodegenInstanceDependency, CodegenInstanceKey,
    CodegenPartitionPolicy, CodegenReachabilityBuilder, CodegenTarget, DebugInformationMode,
    DebugInformationOutputMode, LinkableArtifactKind, partition_codegen_units,
};
use bray_emitter::{BackendEmissionPolicy, EmissionBackend};
use bray_ir::{MirUnit, MirUnitId, MirUnitKey};
use bray_linker::Linker;
use bray_runtime_interface::{RuntimeArtifact, RuntimeCapability};
use bray_symbols::{
    AnySymbolId, CallableDefinitionId, CallableInstanceData, ProductIdentity, ProductKind,
};

use super::super::super::Compilation;
use super::super::super::binder::has_visible_generic_parameters;
use super::super::super::implementation::implementation_fulfillments;
use super::super::super::substitution::empty_substitution;
use super::super::specialization::{ConcreteCodegenInstance, ConcreteCodegenReachability};
use super::error::NativeProductFactError;
use super::facts::NativeProductFacts;
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError, NativeProductFactKey};

const GENERATED_HOST_UNIT: MirUnitId = MirUnitId::new(u32::MAX);

impl Compilation {
    /// Returns the native facts required to emit one selected product.
    pub fn native_product_facts(
        &self,
        product: ProductIdentity,
        configuration: crate::BuildConfiguration,
        runtime: Option<RuntimeArtifact>,
        required_capabilities: impl IntoIterator<Item = RuntimeCapability>,
        linker: Option<&Linker>,
    ) -> Result<Arc<NativeProductFacts>, Arc<NativeProductFactError>> {
        self.native_product_facts_with_cancellation(
            product,
            configuration,
            runtime,
            required_capabilities,
            linker,
            &self.state.cancellation,
        )
    }

    fn native_product_facts_with_cancellation(
        &self,
        product: ProductIdentity,
        configuration: crate::BuildConfiguration,
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
            configuration,
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
                            configuration,
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
        configuration: crate::BuildConfiguration,
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

        let options = configuration.codegen_options();

        let semantic = self.product_semantic_facts_with_cancellation(cancellation)?;

        let test_discovery = if semantic.value().kind() == ProductKind::Test {
            // Discovery owns the product identity used by its independently cached fact key.
            let discovery = self.test_discovery_with_cancellation(product.clone(), cancellation)?;

            Some(discovery)
        } else {
            None
        };

        let source_roots = self.product_root_instances(
            semantic.value(),
            test_discovery.as_deref().map(|discovery| discovery.value()),
            &target,
            cancellation,
        )?;

        // Native product facts retain the exact immutable catalog selected for this host.
        let test_catalog = test_discovery
            .as_ref()
            .map(|discovery| discovery.value().catalog().clone());

        let (host, units, mappings) = if source_roots.is_empty()
            && semantic.value().kind() != ProductKind::Test
        {
            (None, Arc::from([]), Vec::new())
        } else {
            let source_reachability = if source_roots.is_empty() {
                None
            } else {
                let reachability =
                    self.codegen_reachability(source_roots.clone(), None, &target, cancellation)?;

                Some(reachability)
            };

            let host = self.executable_host(
                &product,
                semantic.value().kind(),
                &source_roots,
                source_reachability
                    .as_ref()
                    .map(ConcreteCodegenReachability::graph),
                runtime.as_ref(),
                required_capabilities,
                &target,
                cancellation,
            )?;

            let reachability = match host.as_ref() {
                Some(host) => {
                    let host_target = source_roots.first().map_or_else(
                        || {
                            bray_ir::MirTargetFacts::new(
                                target.profile().clone(),
                                self.selected_target().target().runtime_abi(),
                            )
                        },
                        |root| root.key().target().clone(),
                    );

                    let host_mir = bray_lowering::lower_executable_host(
                        bray_lowering::ExecutableHostLoweringInput::new(
                            GENERATED_HOST_UNIT,
                            source_roots
                                .iter()
                                .map(|root| bound_template(root.key()))
                                .collect::<Result<Vec<_>, _>>()?,
                            host.clone(),
                            host_target,
                        ),
                    )
                    .map_err(NativeProductFactError::InvalidHostMir)?;

                    let host = ConcreteCodegenInstance::generated(CodegenInstanceKey::non_generic(
                        &host_mir,
                    ));

                    let reachability = self.codegen_reachability(
                        [host],
                        Some((host_mir, source_roots)),
                        &target,
                        cancellation,
                    )?;

                    reachability
                }
                None => source_reachability.ok_or(NativeProductFactError::MissingProductRoot)?,
            };

            let roots: BTreeSet<_> = reachability.graph().roots().iter().cloned().collect();

            let compatibility = reachability
                .graph()
                .instances()
                .iter()
                .map(|instance| {
                    self.codegen_partition_compatibility(
                        instance,
                        product.package(),
                        &roots,
                        cancellation,
                    )
                    // The lookup table owns the Arc-backed instance identity during partitioning.
                    .map(|compatibility| (instance.key().clone(), compatibility))
                })
                .collect::<Result<BTreeMap<_, _>, _>>()?;

            // The partitioner receives owned compatibility identities independent of the table.
            let units = partition_codegen_units(
                CodegenPartitionPolicy::NATIVE_BALANCED,
                reachability.graph(),
                |instance| compatibility.get(instance.key()).cloned(),
            )
            .map_err(NativeProductFactError::InvalidCodegenPartition)?;

            let mappings = units
                .iter()
                .map(|unit| {
                    self.codegen_mappings_for_product(
                        unit,
                        host.as_ref(),
                        &target,
                        &roots,
                        &reachability,
                        options.debug_information() != DebugInformationMode::None,
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

        let debug_output = match options.debug_information() {
            DebugInformationMode::None => DebugInformationOutputMode::Omit,
            DebugInformationMode::LineTables | DebugInformationMode::Full => {
                DebugInformationOutputMode::Embedded
            }
        };

        let policy = BackendEmissionPolicy::new(
            options.debug_information(),
            debug_output,
            Some(LinkableArtifactKind::RelocatableObject),
            bray_codegen::BackendSerializationOptions::new(AssemblySyntaxKind::TargetDefault),
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
                self.product_link_facts(
                    semantic.value().kind(),
                    host.as_ref(),
                    runtime,
                    linker,
                    &target,
                    configuration,
                )
            })
            .transpose()?;

        Ok(NativeProductFacts {
            backend,
            target,
            options,
            host,
            test_catalog,
            link,
            units,
            mappings: shared_slice(mappings),
        })
    }

    fn product_root_instances(
        &self,
        semantic: &bray_symbols::ProductSemanticFacts,
        test_discovery: Option<&super::super::super::testing::TestDiscovery>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<Vec<ConcreteCodegenInstance>, NativeProductFactError> {
        let facts = self.binder_facts(cancellation)?;

        let mut symbols: Vec<_> = match semantic.kind() {
            ProductKind::Executable => semantic
                .entrypoint()
                .map(AnySymbolId::from)
                .into_iter()
                .collect(),
            ProductKind::Test => {
                let discovery = test_discovery.ok_or(FactQueryError::InfrastructureFailure)?;

                discovery
                    .catalog()
                    .entries()
                    .iter()
                    .filter_map(|entry| discovery.function(entry.identity()))
                    .map(AnySymbolId::from)
                    .collect()
            }
            ProductKind::Library => semantic.public_symbols().to_vec(),
        };

        if semantic.kind() == ProductKind::Library {
            for implementation in semantic
                .public_symbols()
                .iter()
                .copied()
                .filter_map(bray_symbols::ImplementationSymbolId::try_from_any)
            {
                symbols.extend(
                    implementation_fulfillments(&facts, implementation)?
                        .callables
                        .iter()
                        .copied()
                        .map(AnySymbolId::from),
                );
            }
        }

        let mut roots = Vec::new();

        for symbol in symbols {
            if has_visible_generic_parameters(facts.symbols(), symbol) {
                continue;
            }

            let Some(definition) = CallableDefinitionId::try_new(symbol) else {
                continue;
            };

            let (callable, witnesses) =
                self.product_root_callable(symbol, definition, &facts, cancellation)?;

            let callable =
                self.concrete_codegen_callable(callable, witnesses, target, cancellation)?;

            if semantic.kind() == ProductKind::Library {
                roots.extend(self.concrete_codegen_callable_defaults(definition, &callable)?);
            }

            roots.push(callable);
        }

        if semantic.kind() != ProductKind::Test {
            roots.sort_unstable_by(|left, right| left.key().cmp(right.key()));
            roots.dedup_by(|left, right| left.key() == right.key());
        }

        if roots.is_empty() && semantic.kind() == ProductKind::Executable {
            return Err(NativeProductFactError::MissingProductRoot);
        }

        Ok(roots)
    }

    fn product_root_callable(
        &self,
        symbol: AnySymbolId,
        definition: CallableDefinitionId,
        facts: &super::super::super::binder::CompilationBinderFacts<'_>,
        cancellation: &CancellationToken,
    ) -> Result<
        (
            CallableInstanceData,
            Vec<bray_symbols::ImplementationInstanceId>,
        ),
        NativeProductFactError,
    > {
        let Some(implementation) = facts
            .symbols()
            .containing_symbol(symbol)
            .and_then(bray_symbols::ImplementationSymbolId::try_from_any)
        else {
            let substitution = empty_substitution(self.semantic_value_store()?, symbol)?;

            return Ok((
                CallableInstanceData::new(definition, substitution),
                Vec::new(),
            ));
        };

        let headers = self.implementation_header_index(cancellation)?;

        let header = headers
            .value()
            .header(implementation)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let values = self.semantic_value_store()?;

        let application = values
            .trait_application_data(header.trait_application())
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let implementation_substitution = empty_substitution(values, implementation.into_any())?;

        let callable = super::super::super::implementation::callable_instance(
            values,
            symbol,
            [application.substitution(), implementation_substitution],
        )
        .map_err(NativeProductFactError::from)?;

        let witness = values
            .intern_implementation_instance(bray_symbols::ImplementationInstanceData::new(
                implementation,
                implementation_substitution,
            ))
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        Ok((callable, vec![witness]))
    }

    fn codegen_reachability(
        &self,
        roots: impl IntoIterator<Item = ConcreteCodegenInstance>,
        generated_host: Option<(MirUnit, Vec<ConcreteCodegenInstance>)>,
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

        let generated_host = generated_host.map(|(mir, source_roots)| {
            for root in &source_roots {
                realizations.insert(root.key().clone(), root.clone());
            }

            (CodegenInstanceKey::non_generic(&mir), mir, source_roots)
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

                if matches!(
                    key.template(),
                    MirUnitKey::ExternalCallable(_) | MirUnitKey::ExternalRuntimeDefault(_)
                ) {
                    builder
                        .push_external(key.clone())
                        .map_err(NativeProductFactError::InvalidReachability)?;

                    continue;
                }

                let mir = if let Some((host_key, host_mir, _)) = &generated_host
                    && key == host_key
                {
                    host_mir.clone()
                } else {
                    match realization.generated_lifecycle_reference() {
                        Some(reference) => self.codegen_generated_lifecycle_mir(
                            key,
                            reference,
                            MirUnitId::new(0),
                            cancellation,
                        ),
                        None => {
                            self.codegen_mir_for_plan(key, MirUnitId::new(0), None, cancellation)
                        }
                    }?
                };

                let mut concrete_dependencies = self.concrete_codegen_dependencies_for_mir(
                    &realization,
                    &mir,
                    target,
                    cancellation,
                )?;

                if let Some((host_key, _, source_roots)) = &generated_host
                    && key == host_key
                {
                    for root in source_roots {
                        if !concrete_dependencies
                            .iter()
                            .any(|dependency| dependency.key() == root.key())
                        {
                            concrete_dependencies.push(root.clone());
                        }
                    }
                }

                concrete_dependencies.sort_unstable_by(|left, right| left.key().cmp(right.key()));
                concrete_dependencies.dedup_by(|left, right| left.key() == right.key());

                let dependencies = concrete_dependencies
                    .iter()
                    .map(|dependency| {
                        CodegenInstanceDependency::definition(dependency.key().clone())
                    })
                    .collect::<Vec<_>>();

                for dependency in concrete_dependencies {
                    match realizations.entry(dependency.key().clone()) {
                        std::collections::btree_map::Entry::Vacant(entry) => {
                            entry.insert(dependency);
                        }
                        std::collections::btree_map::Entry::Occupied(entry) => {
                            if entry.get() != &dependency {
                                return Err(FactQueryError::InfrastructureFailure.into());
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
    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;
    use std::sync::Arc;

    use bray_base::NonEmptySharedStr;
    use bray_codegen::{
        BackendArtifactId, BackendArtifactKind, BackendArtifactRequest,
        BackendArtifactRequestEntry, BackendArtifactRequirement, BackendSerializationOptions,
        CodeGenerator, CodeGeneratorRegistry, CodegenConfiguration, CodegenGenericArgument,
        CodegenPartitionPolicy, CodegenRequest, CodegenResultMapping, CodegenSpecialization,
        CodegenStatus, DebugInformationMode, LinkableArtifactKind, LinkableArtifactRequirement,
        OptimizationLevel, partition_codegen_units,
    };
    use bray_compiler_known::RepresentationRole;
    use bray_diagnostics::DiagnosticBag;
    use bray_ir::{
        MirHelperReference, MirHostOperation, MirOperationKind, MirUnitKey, MirUnitKind,
    };
    use bray_linker::{
        LinkFailure, LinkInputKind, LinkInputProvenance, LinkInputSource, LinkModel, LinkOutcome,
        LinkPlan, LinkedProductKind, Linker, LinkerDriver, LinkerDriverIdentity, LinkerDriverKind,
    };
    use bray_package_interface::{
        InterfaceExecutableTemplate, InterfaceLanguageRevision, InterfaceProductIdentity,
        InterfaceProductKind, InterfaceValidationLimits, InterfaceValidationPolicy,
        PackageImplementationArtifact, PackageInterfaceIdentity, ValidatedPackageInterface,
        encode_package_interface,
    };
    use bray_runtime_interface::{
        BinarySymbolName, ExecutableEntryResult, ExecutableHostContractBuildError,
        ProtectedFrameAbiVersions, ProtectedFrameOperation, RootExecution, RuntimeAbiRole,
        RuntimeAbiVersion, RuntimeArtifact, RuntimeArtifactDigest, RuntimeArtifactId,
        RuntimeArtifactMetadata, RuntimeCapability, RuntimeCompatibilityError, RuntimeContract,
        RuntimeIdentity, RuntimeRoleBinding, RuntimeRoleImplementation,
    };
    use bray_standard_library::{
        StandardLibraryArtifact, StandardLibraryArtifactKind, StandardLibraryBundleManifest,
        StandardLibraryRoot, StandardLibraryTargetArtifacts, encode_standard_library_manifest,
    };
    use bray_symbols::{
        CallableDefinitionId, CallableInstanceData, ConstantTermData, ConstantValueData,
        ConstantValueKind, GenericArgument, GenericOwnerId, GenericParameterSymbolId,
        GenericSubstitutionData, ImplementationRequirementKey, ImplementationSelection,
        NamedTypeSymbolId, NativeLinkKind, NativeLinkRequirement, ProductIdentity, ProductKind,
        SymbolOrigin, TraitApplicationData, TypeData,
    };
    use bray_target::NativeTarget;
    use bray_testing::TemporaryFile;

    use super::NativeProductFactError;
    use crate::compilation::CodegenFactError;
    use crate::{
        CancellationToken, CompilationOptions, CompilationRequest, DependencyInterfaceInput,
        PackageInterfaceExportRequest, SelectedTarget, WorkerBudget,
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
    fn native_products_allow_platform_runtime_dependencies() {
        for target in NativeTarget::ALL {
            assert_eq!(
                super::super::link::product_link_model(target.object_format()),
                LinkModel::Dynamic,
                "{target:?}"
            );
        }
    }

    #[test]
    fn executable_link_inputs_include_standard_library_archives_and_native_dependencies() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("fixture directory must exist: {error}"));

        let selected = SelectedTarget::baseline();
        let target = selected.profile().identity().clone();
        let runtime_abi = selected.runtime_abi();
        let archive_bytes = b"standard library archive";
        let platform_archive_bytes = b"platform ABI archive";

        let archive_path = format!(
            "targets/{}/{}.{}/libstd.a",
            target.as_str(),
            runtime_abi.major(),
            runtime_abi.minor()
        );

        let interface = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::PackageInterface,
            format!(
                "targets/{}/{}.{}/std.brayi",
                target.as_str(),
                runtime_abi.major(),
                runtime_abi.minor()
            ),
            b"interface",
        )
        .unwrap_or_else(|error| panic!("interface metadata must be valid: {error:?}"));

        let implementation = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::PackageImplementation,
            format!(
                "targets/{}/{}.{}/std.brayimpl",
                target.as_str(),
                runtime_abi.major(),
                runtime_abi.minor()
            ),
            b"implementation",
        )
        .unwrap_or_else(|error| panic!("implementation metadata must be valid: {error:?}"));

        let archive = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::StaticLibrary,
            archive_path,
            archive_bytes,
        )
        .unwrap_or_else(|error| panic!("archive metadata must be valid: {error:?}"));

        let platform_archive_path = format!(
            "targets/{}/{}.{}/libbray_platform_abi.a",
            target.as_str(),
            runtime_abi.major(),
            runtime_abi.minor()
        );

        let platform_archive = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::PlatformServiceLibrary,
            platform_archive_path,
            platform_archive_bytes,
        )
        .unwrap_or_else(|error| panic!("platform archive metadata must be valid: {error:?}"));

        let target_artifacts = StandardLibraryTargetArtifacts::try_new(
            target,
            runtime_abi,
            [
                interface.clone(),
                implementation.clone(),
                archive.clone(),
                platform_archive.clone(),
            ],
        )
        .map(|target| {
            target.with_native_links([NativeLinkRequirement::new(
                NonEmptySharedStr::try_new("c")
                    .unwrap_or_else(|| panic!("native library name must be valid")),
                NativeLinkKind::System,
            )])
        })
        .unwrap_or_else(|error| panic!("target metadata must be valid: {error:?}"));

        let manifest = StandardLibraryBundleManifest::try_new([target_artifacts])
            .unwrap_or_else(|error| panic!("manifest must be valid: {error:?}"));

        let archive_file = archive.beneath(directory.path());

        let archive_directory = archive_file
            .parent()
            .unwrap_or_else(|| panic!("archive must have a parent directory"));

        fs::create_dir_all(archive_directory)
            .unwrap_or_else(|error| panic!("archive directory must exist: {error}"));

        fs::write(&archive_file, archive_bytes)
            .unwrap_or_else(|error| panic!("archive must be written: {error}"));

        let platform_archive_file = platform_archive.beneath(directory.path());

        fs::write(&platform_archive_file, platform_archive_bytes)
            .unwrap_or_else(|error| panic!("platform archive must be written: {error}"));

        fs::write(interface.beneath(directory.path()), b"interface")
            .unwrap_or_else(|error| panic!("interface must be written: {error}"));

        fs::write(implementation.beneath(directory.path()), b"implementation")
            .unwrap_or_else(|error| panic!("implementation must be written: {error}"));

        let manifest_bytes = encode_standard_library_manifest(&manifest)
            .unwrap_or_else(|error| panic!("manifest must encode: {error:?}"));

        fs::write(directory.path().join("manifest.json"), manifest_bytes)
            .unwrap_or_else(|error| panic!("manifest must be written: {error}"));

        let root = StandardLibraryRoot::try_new(directory.path())
            .unwrap_or_else(|| panic!("temporary root must be absolute"));

        let request = CompilationRequest::with_options(
            crate::test_support::package_identity(),
            vec![crate::test_support::source_input(
                "module application;\n",
                0,
            )],
            CompilationOptions::new(WorkerBudget::serial(), ProductKind::Executable, selected),
        )
        .with_standard_library_root(root);

        let compilation = crate::Compilation::load(request)
            .unwrap_or_else(|error| panic!("compilation must load: {error:?}"));

        let inputs = compilation
            .standard_library_link_inputs(ProductKind::Executable, true)
            .unwrap_or_else(|error| panic!("standard library inputs must resolve: {error:?}"));

        assert_eq!(inputs.len(), 3);

        for path in [&archive_file, &platform_archive_file] {
            assert!(inputs.iter().any(|input| {
                input.as_ref().is_ok_and(|input| {
                    input.kind() == LinkInputKind::Archive
                        && input.source() == &LinkInputSource::file(path)
                        && matches!(
                            input.provenance(),
                            LinkInputProvenance::Package(package)
                                if package.as_str()
                                    == bray_standard_library::PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY
                        )
                })
            }));
        }

        assert!(inputs.iter().any(|input| {
            input.as_ref().is_ok_and(|input| {
                input.kind() == LinkInputKind::NativeLibrary
                    && input.source()
                        == &LinkInputSource::try_native_library("c")
                            .unwrap_or_else(|| panic!("native library name must be valid"))
            })
        }));

        let embedded_inputs = compilation
            .standard_library_link_inputs(ProductKind::Executable, false)
            .unwrap_or_else(|error| panic!("embedded platform inputs must resolve: {error:?}"));

        assert_eq!(embedded_inputs.len(), 1);

        assert!(embedded_inputs.iter().all(|input| {
            input
                .as_ref()
                .is_ok_and(|input| input.source() != &LinkInputSource::file(&platform_archive_file))
        }));

        assert!(
            compilation
                .standard_library_link_inputs(ProductKind::Library, true)
                .unwrap_or_else(|error| panic!("library inputs must resolve: {error:?}"))
                .is_empty()
        );
    }

    #[test]
    fn synchronous_i32_executable_hosts_emit_native_units() {
        let backend = Arc::new(
            bray_codegen_llvm::LlvmCodeGenerator::try_new()
                .unwrap_or_else(|error| panic!("LLVM backend must initialize: {error:?}")),
        );

        let registry =
            CodeGeneratorRegistry::try_new([Arc::clone(&backend) as Arc<dyn CodeGenerator>])
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

        let product = test_product_identity();

        let facts = compilation
            .native_product_facts(
                product.clone(),
                crate::BuildConfiguration::Development,
                None,
                [],
                Some(&test_linker()),
            )
            .unwrap_or_else(|error| panic!("native facts must resolve: {error:?}"));

        let release = compilation
            .native_product_facts(
                product,
                crate::BuildConfiguration::Release,
                None,
                [],
                Some(&test_linker()),
            )
            .unwrap_or_else(|error| panic!("release native facts must resolve: {error:?}"));

        assert_eq!(facts.options().optimization(), OptimizationLevel::Basic);

        assert_eq!(
            facts.options().debug_information(),
            DebugInformationMode::LineTables
        );

        assert_eq!(release.options().optimization(), OptimizationLevel::Full);

        assert_eq!(
            release.options().debug_information(),
            DebugInformationMode::None
        );

        assert!(!Arc::ptr_eq(&facts, &release));

        assert!(facts.mappings().iter().any(|mappings| {
            mappings
                .debug_locations()
                .iter()
                .any(|location| location.file().path() == "source-0" && location.line().get() > 1)
        }));

        assert!(
            release
                .mappings()
                .iter()
                .all(|mappings| mappings.debug_locations().is_empty())
        );

        let host = facts
            .executable_host()
            .unwrap_or_else(|| panic!("executable must own a host"));

        assert_eq!(host.entries()[0].root(), RootExecution::Synchronous);
        assert_eq!(host.entries()[0].result(), ExecutableEntryResult::I32);

        assert!(
            generated_artifacts(&backend, &facts)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }

    #[test]
    fn asynchronous_executable_hosts_emit_complete_deterministic_native_units() {
        let (backend, facts) = runtime_native_facts(include_str!(
            "../../../../../../xtask/fixtures/native-execution/async-i32.bray"
        ));

        let host = facts
            .executable_host()
            .unwrap_or_else(|| panic!("async executable must own a host"));

        let RootExecution::Asynchronous { frame } = host.entries()[0].root() else {
            panic!("async executable host must retain a protected root frame");
        };

        assert_eq!(host.entries()[0].result(), ExecutableEntryResult::I32);

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
            .find_map(|(unit, mappings)| (unit.key() == frame_unit.key()).then_some(mappings))
            .unwrap_or_else(|| panic!("root frame mappings must be retained"));

        let adapter = frame_mapping
            .symbol(&bray_codegen::CodegenSymbolKey::ProtectedFrame {
                frame,
                operation: ProtectedFrameOperation::MoveBeforeStart,
            })
            .map(bray_codegen::CodegenSymbolMapping::name);

        assert_eq!(host.entries()[0].root_frame_adapter(), adapter);

        for role in [
            RuntimeAbiRole::RootExecution,
            RuntimeAbiRole::RootCancellationRequest,
            RuntimeAbiRole::RootTerminalObservation,
            RuntimeAbiRole::RootCompletionResolution,
            RuntimeAbiRole::CleanupIncidentReporting,
            RuntimeAbiRole::PanicReporting,
            RuntimeAbiRole::EntryFailureReporting,
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
    fn asynchronous_unit_and_result_error_roots_emit_native_hosts() {
        let cases = [
            (
                include_str!("../../../../../../xtask/fixtures/native-execution/async-unit.bray"),
                false,
            ),
            (
                include_str!(
                    "../../../../../../xtask/fixtures/native-execution/async-result-error.bray"
                ),
                true,
            ),
        ];

        for (source, fallible) in cases {
            let (backend, facts) = runtime_native_facts(source);

            let host = facts
                .executable_host()
                .unwrap_or_else(|| panic!("async executable must own a host"));

            assert!(matches!(
                host.entries()[0].root(),
                RootExecution::Asynchronous { .. }
            ));

            if fallible {
                let ExecutableEntryResult::Fallible { error, .. } = host.entries()[0].result()
                else {
                    panic!("Result root must retain its concrete error type");
                };

                let helper_references = facts
                    .mappings()
                    .iter()
                    .flat_map(bray_codegen::CodegenMappings::operations)
                    .flat_map(bray_codegen::CodegenOperationMapping::helpers)
                    .map(bray_codegen::CodegenHelperMapping::reference)
                    .collect::<Vec<_>>();

                assert!(helper_references.contains(&&MirHelperReference::Finalize(error)));

                assert!(helper_references.contains(&&MirHelperReference::Destroy(error)));

                let ir =
                    generated_artifacts_of_kind(&backend, &facts, BackendArtifactKind::BackendIr)
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>();

                let ir = String::from_utf8(ir)
                    .unwrap_or_else(|error| panic!("LLVM IR must be UTF-8: {error}"));

                assert!(ir.contains("entry.failure"));
                assert!(ir.contains("bray_runtime_entry_failure_reporting_v1"));
            } else {
                assert_eq!(host.entries()[0].result(), ExecutableEntryResult::Unit);
            }

            assert!(
                generated_artifacts(&backend, &facts)
                    .iter()
                    .all(|artifact| !artifact.is_empty())
            );
        }
    }

    #[test]
    fn synchronous_panics_emit_a_runtime_owned_host_boundary() {
        let (backend, facts) = runtime_native_facts(include_str!(
            "../../../../../../xtask/fixtures/native-execution/sync-panic.bray"
        ));

        let host = facts
            .executable_host()
            .unwrap_or_else(|| panic!("executable must own a host"));

        assert_eq!(host.entries()[0].root(), RootExecution::Synchronous);

        for role in [
            RuntimeAbiRole::SynchronousRootExecution,
            RuntimeAbiRole::PanicReporting,
            RuntimeAbiRole::PanicPropagation,
        ] {
            assert_eq!(
                host.role_binding(role)
                    .map(RuntimeRoleBinding::implementation),
                Some(RuntimeRoleImplementation::BrayRuntime)
            );
        }

        assert!(
            generated_artifacts(&backend, &facts)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }

    #[test]
    fn demanded_runtime_roles_cannot_disappear_from_product_validation() {
        let (_, compilation) = codegen_compilation(include_str!(
            "../../../../../../xtask/fixtures/native-execution/sync-panic.bray"
        ));

        let archive = TemporaryFile::write("libbray_runtime.a", b"runtime archive");

        let roles = RuntimeAbiRole::ALL
            .into_iter()
            .filter(|role| *role != RuntimeAbiRole::PanicPropagation);

        let runtime = runtime_artifact_with_roles(&compilation, archive.path(), roles);

        let product = test_product_identity();

        let result = compilation.native_product_facts(
            product,
            crate::BuildConfiguration::Development,
            Some(runtime),
            [],
            Some(&test_linker()),
        );

        let Err(error) = result else {
            panic!("incomplete runtime must fail product validation");
        };

        assert!(
            matches!(
                error.as_ref(),
                NativeProductFactError::InvalidExecutableHost(
                    ExecutableHostContractBuildError::IncompatibleRuntime(
                        RuntimeCompatibilityError::MissingRole(RuntimeAbiRole::PanicPropagation)
                    )
                )
            ),
            "{error:?}"
        );
    }

    #[test]
    fn concrete_generic_instances_realize_signatures_and_layouts() {
        let compilation = crate::test_support::compilation(CONCRETE_GENERIC_SOURCE);

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
            .product_root_instances(semantic.value(), None, &target, &cancellation)
            .unwrap_or_else(|error| panic!("test roots must resolve: {error:?}"));

        let reachability = compilation
            .codegen_reachability(roots.clone(), None, &target, &cancellation)
            .unwrap_or_else(|error| panic!("generic reachability must close: {error:?}"));

        let reversed_reachability = compilation
            .codegen_reachability(
                roots.clone().into_iter().rev(),
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

        let roots: BTreeSet<_> = roots.into_iter().map(|root| root.key().clone()).collect();

        let compatibility = reachability
            .graph()
            .instances()
            .iter()
            .map(|instance| {
                compilation
                    .codegen_partition_compatibility(
                        instance,
                        compilation.package_identity(),
                        &roots,
                        &cancellation,
                    )
                    // The test lookup owns the Arc-backed identity during partitioning.
                    .map(|compatibility| (instance.key().clone(), compatibility))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()
            .unwrap_or_else(|error| panic!("partition compatibility must resolve: {error:?}"));

        let units = partition_codegen_units(
            CodegenPartitionPolicy::NATIVE_BALANCED,
            reachability.graph(),
            |instance| compatibility.get(instance.key()).cloned(),
        )
        .unwrap_or_else(|error| panic!("generic units must partition: {error:?}"));

        let mut saw_concrete_generic_signature = false;
        let mut saw_const_specialization = false;

        for unit in units.iter() {
            let mappings = compilation
                .codegen_mappings_for_product(
                    unit,
                    None,
                    &target,
                    &reachability.graph().roots().iter().cloned().collect(),
                    &reachability,
                    false,
                    &cancellation,
                )
                .unwrap_or_else(|error| panic!("generic mappings must realize: {error:?}"));

            for instance in unit.instances() {
                let realization = reachability
                    .instance(instance.key())
                    .unwrap_or_else(|| panic!("reachable instance payload must be retained"));

                let signature = compilation
                    .codegen_instance_signature(realization, &cancellation)
                    .unwrap_or_else(|error| panic!("generic signature must realize: {error:?}"));

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
        }

        assert!(saw_concrete_generic_signature);
        assert!(saw_const_specialization);
    }

    #[test]
    fn library_roots_exclude_members_of_open_generic_containers() {
        let compilation = crate::test_support::compilation_with_product(
            concat!(
                "module app;\n",
                "struct Holder<T>\n",
                "{\n",
                "    value: T;\n",
                "\n",
                "    destruct()\n",
                "    {\n",
                "    }\n",
                "}\n",
            ),
            ProductKind::Library,
        );

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
            .product_root_instances(semantic.value(), None, &target, &cancellation)
            .unwrap_or_else(|error| panic!("test roots must resolve: {error:?}"));

        assert!(roots.is_empty());
    }

    #[test]
    fn library_roots_include_public_implementation_fulfillments() {
        let compilation = crate::test_support::compilation_with_product(
            concat!(
                "module app;\n",
                "trait Equal<Other>\n",
                "{\n",
                "    func equals(pos other: &Other) -> bool;\n",
                "}\n",
                "\n",
                "struct Value\n",
                "{\n",
                "}\n",
                "\n",
                "impl ValueEqual = Value(Equal<Value>)\n",
                "{\n",
                "    func equals(pos other: &Value) -> bool\n",
                "    {\n",
                "        return true;\n",
                "    }\n",
                "}\n",
            ),
            ProductKind::Library,
        );

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
            .product_root_instances(semantic.value(), None, &target, &cancellation)
            .unwrap_or_else(|error| panic!("test roots must resolve: {error:?}"));

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("test symbols must resolve: {error:?}"));

        let fulfillment = symbols
            .trait_callable_fulfillments()
            .iter()
            .find(|fulfillment| fulfillment.origin() == SymbolOrigin::Source)
            .unwrap_or_else(|| panic!("source fulfillment must exist"));

        let fulfillment = CallableDefinitionId::try_new(fulfillment.id().into())
            .unwrap_or_else(|| panic!("trait callable fulfillment must be callable"));

        assert!(roots.iter().any(|root| {
            root.callable_instance()
                .is_some_and(|callable| callable.definition() == fulfillment)
        }));
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
            specialization
                .arguments()
                .iter()
                .any(|argument| matches!(argument, CodegenGenericArgument::Type(_)))
        }));

        assert!(first.iter().any(|specialization| {
            specialization
                .arguments()
                .iter()
                .any(|argument| matches!(argument, CodegenGenericArgument::Constant(_)))
        }));
    }

    #[test]
    fn imported_generic_templates_specialize_with_private_helpers_in_the_consumer() {
        let compilation = generic_consumer(generic_dependency(true));

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
            .unwrap_or_else(|error| panic!("consumer target must validate: {error:?}"));

        let semantic = compilation
            .product_semantic_facts()
            .unwrap_or_else(|error| panic!("consumer product facts must resolve: {error:?}"));

        let roots = compilation
            .product_root_instances(semantic.value(), None, &target, &cancellation)
            .unwrap_or_else(|error| panic!("consumer roots must resolve: {error:?}"));

        let reachability = compilation
            .codegen_reachability(roots, None, &target, &cancellation)
            .unwrap_or_else(|error| panic!("consumer reachability must close: {error:?}"));

        let imported = reachability
            .graph()
            .instances()
            .iter()
            .filter(|instance| {
                matches!(instance.key().template(), MirUnitKey::ImportedExecutable(_))
            })
            .collect::<Vec<_>>();

        assert_eq!(imported.len(), 2);

        assert!(imported.iter().all(|instance| {
            matches!(
                instance.key().specialization(),
                CodegenSpecialization::Generic(arguments)
                    if arguments.iter().any(|argument| {
                        matches!(argument, CodegenGenericArgument::Type(_))
                    })
            )
        }));

        assert!(imported.iter().any(|instance| {
            instance.mir().operations().iter().any(|operation| {
                matches!(
                    operation.kind(),
                    MirOperationKind::Call(call) if call.phase_behaviors().is_some()
                )
            })
        }));
    }

    #[test]
    fn imported_generic_templates_are_shared_by_concurrent_requests() {
        let compilation = generic_consumer(generic_dependency(true));
        let address = first_imported_function_address(&compilation);

        std::thread::scope(|scope| {
            let first = scope.spawn(|| {
                compilation.imported_executable_template_with_cancellation(
                    address,
                    &compilation.state.cancellation,
                )
            });

            let second = scope.spawn(|| {
                compilation.imported_executable_template_with_cancellation(
                    address,
                    &compilation.state.cancellation,
                )
            });

            let first = first
                .join()
                .unwrap_or_else(|_| panic!("first template query must not panic"))
                .unwrap_or_else(|error| panic!("first template query must complete: {error:?}"));

            let second = second
                .join()
                .unwrap_or_else(|_| panic!("second template query must not panic"))
                .unwrap_or_else(|error| panic!("second template query must complete: {error:?}"));

            assert!(Arc::ptr_eq(&first, &second));

            let first = first
                .value()
                .as_ref()
                .unwrap_or_else(|| panic!("first query must publish validated MIR"));

            let second = second
                .value()
                .as_ref()
                .unwrap_or_else(|| panic!("second query must publish validated MIR"));

            assert!(Arc::ptr_eq(first, second));
        });
    }

    #[test]
    fn imported_executable_templates_reject_a_different_target_contract() {
        let compilation = generic_consumer_for_target(
            generic_dependency(true),
            SelectedTarget::for_native(NativeTarget::X86_64WindowsMsvc),
        );

        let result = compilation
            .imported_executable_template_with_cancellation(
                first_imported_function_address(&compilation),
                &compilation.state.cancellation,
            )
            .unwrap_or_else(|error| panic!("target mismatch must be diagnosed: {error:?}"));

        assert!(result.value().is_none());

        assert_eq!(
            result
                .diagnostics()
                .by_kind(bray_diagnostics::DiagnosticKind::InterfaceExecutableTemplateUnavailable)
                .count(),
            1
        );
    }

    #[test]
    fn malformed_imported_executable_templates_publish_dependency_diagnostics() {
        let compilation = generic_consumer(generic_dependency_with_templates(true, true));

        let result = compilation
            .imported_executable_template_with_cancellation(
                first_imported_function_address(&compilation),
                &compilation.state.cancellation,
            )
            .unwrap_or_else(|error| panic!("malformed template must be diagnosed: {error:?}"));

        assert!(result.value().is_none());

        assert_eq!(
            result
                .diagnostics()
                .by_kind(bray_diagnostics::DiagnosticKind::InterfaceTruncated)
                .count(),
            1
        );
    }

    #[test]
    fn imported_generic_body_without_an_implementation_template_is_diagnosed() {
        let compilation = generic_consumer(generic_dependency(false));

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
            .unwrap_or_else(|error| panic!("consumer target must validate: {error:?}"));

        let semantic = compilation
            .product_semantic_facts()
            .unwrap_or_else(|error| panic!("consumer product facts must resolve: {error:?}"));

        let roots = compilation
            .product_root_instances(semantic.value(), None, &target, &cancellation)
            .unwrap_or_else(|error| panic!("consumer roots must resolve: {error:?}"));

        let error = match compilation.codegen_reachability(roots, None, &target, &cancellation) {
            Ok(_) => panic!("missing imported templates must stop code generation reachability"),
            Err(error) => error,
        };

        let NativeProductFactError::Codegen(CodegenFactError::Diagnostics(diagnostics)) = error
        else {
            panic!("missing imported template must preserve its diagnostics: {error:?}");
        };

        assert_eq!(
            diagnostics
                .by_kind(bray_diagnostics::DiagnosticKind::InterfaceExecutableTemplateUnavailable)
                .count(),
            1
        );
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
            .representation_symbol::<bray_symbols::StructSymbolId>(RepresentationRole::ScalarBool)
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

        assert_eq!(realization.implementation_witnesses(), [witness]);
    }

    fn runtime_native_facts(
        source: &str,
    ) -> (
        Arc<bray_codegen_llvm::LlvmCodeGenerator>,
        Arc<super::NativeProductFacts>,
    ) {
        let (backend, compilation) = codegen_compilation(source);

        let archive = TemporaryFile::write("libbray_runtime.a", b"runtime archive");
        let runtime = runtime_artifact(&compilation, archive.path());

        let product = test_product_identity();

        let facts = compilation
            .native_product_facts(
                product,
                crate::BuildConfiguration::Development,
                Some(runtime),
                [
                    RuntimeCapability::CooperativeExecution,
                    RuntimeCapability::MainThreadLane,
                ],
                Some(&test_linker()),
            )
            .unwrap_or_else(|error| panic!("runtime native facts must resolve: {error:?}"));

        (backend, facts)
    }

    fn codegen_compilation(
        source: &str,
    ) -> (
        Arc<bray_codegen_llvm::LlvmCodeGenerator>,
        crate::Compilation,
    ) {
        codegen_compilation_for_product(source, ProductKind::Executable)
    }

    #[test]
    fn native_test_products_emit_every_entry_in_catalog_order() {
        let source = concat!(
            "module app.tests;\n",
            "\n",
            "@test\n",
            "func alpha()\n",
            "{\n",
            "}\n",
            "\n",
            "@test\n",
            "async func gamma()\n",
            "{\n",
            "}\n",
            "\n",
            "@test(serial)\n",
            "func beta()\n",
            "{\n",
            "}\n",
        );

        let (backend, compilation) = codegen_compilation_for_product(source, ProductKind::Test);

        let archive = TemporaryFile::write("libbray_runtime.a", b"runtime archive");
        let runtime = runtime_artifact(&compilation, archive.path());

        let facts = compilation
            .native_product_facts(
                test_product_identity(),
                crate::BuildConfiguration::Development,
                Some(runtime),
                [
                    RuntimeCapability::CooperativeExecution,
                    RuntimeCapability::MainThreadLane,
                ],
                Some(&test_linker()),
            )
            .unwrap_or_else(|error| panic!("native test facts must resolve: {error:?}"));

        let catalog = facts
            .test_catalog()
            .unwrap_or_else(|| panic!("native test facts must retain their catalog"));

        assert_eq!(
            catalog
                .entries()
                .iter()
                .map(|entry| entry.identity().declaration().name().as_str())
                .collect::<Vec<_>>(),
            ["alpha", "beta", "gamma"]
        );

        let host = facts
            .executable_host()
            .unwrap_or_else(|| panic!("native test facts must retain their host"));

        assert_eq!(host.entries().len(), catalog.entries().len());
        assert_eq!(host.entries()[0].root(), RootExecution::Synchronous);
        assert_eq!(host.entries()[1].root(), RootExecution::Synchronous);

        assert!(matches!(
            host.entries()[2].root(),
            RootExecution::Asynchronous { .. }
        ));

        let host_mir = facts
            .units()
            .iter()
            .flat_map(bray_codegen::CodegenUnit::mir_units)
            .find(|unit| matches!(unit.kind(), MirUnitKind::ExecutableHost(_)))
            .unwrap_or_else(|| panic!("native test facts must retain generated host MIR"));

        let executed_entries = host_mir
            .operations()
            .iter()
            .filter_map(|operation| match operation.kind() {
                MirOperationKind::Host(MirHostOperation::ExecuteRoot { entry, .. }) => {
                    Some(entry.slot())
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(executed_entries, [0, 1, 2]);

        assert!(
            generated_artifacts(&backend, &facts)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }

    #[test]
    fn empty_native_test_products_emit_a_successful_host() {
        let (backend, compilation) =
            codegen_compilation_for_product("module app.tests;\n", ProductKind::Test);

        let facts = compilation
            .native_product_facts(
                test_product_identity(),
                crate::BuildConfiguration::Development,
                None,
                [],
                Some(&test_linker()),
            )
            .unwrap_or_else(|error| panic!("empty native test facts must resolve: {error:?}"));

        let catalog = facts
            .test_catalog()
            .unwrap_or_else(|| panic!("empty native test facts must retain their catalog"));

        assert!(catalog.entries().is_empty());

        let host = facts
            .executable_host()
            .unwrap_or_else(|| panic!("empty native test facts must retain their host"));

        assert!(host.entries().is_empty());

        let host_mir = facts
            .units()
            .iter()
            .flat_map(bray_codegen::CodegenUnit::mir_units)
            .find(|unit| matches!(unit.kind(), MirUnitKind::ExecutableHost(_)))
            .unwrap_or_else(|| panic!("empty native test facts must retain generated host MIR"));

        assert!(matches!(
            host_mir.operations(),
            [operation]
                if matches!(
                    operation.kind(),
                    MirOperationKind::Host(MirHostOperation::StructuredShutdown { .. })
                )
        ));

        assert!(
            generated_artifacts(&backend, &facts)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }

    fn codegen_compilation_for_product(
        source: &str,
        product_kind: ProductKind,
    ) -> (
        Arc<bray_codegen_llvm::LlvmCodeGenerator>,
        crate::Compilation,
    ) {
        let backend = Arc::new(
            bray_codegen_llvm::LlvmCodeGenerator::try_new()
                .unwrap_or_else(|error| panic!("LLVM backend must initialize: {error:?}")),
        );

        let registry =
            CodeGeneratorRegistry::try_new([Arc::clone(&backend) as Arc<dyn CodeGenerator>])
                .unwrap_or_else(|error| panic!("LLVM backend must register: {error:?}"));

        let codegen = CodegenConfiguration::try_new(registry, backend.identity().clone())
            .unwrap_or_else(|error| panic!("LLVM backend must select: {error:?}"));

        let request = CompilationRequest::with_options(
            crate::test_support::package_identity(),
            vec![crate::test_support::source_input(source, 0)],
            CompilationOptions::new(
                WorkerBudget::serial(),
                product_kind,
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

        (backend, compilation)
    }

    fn runtime_artifact(
        compilation: &crate::Compilation,
        archive: &std::path::Path,
    ) -> RuntimeArtifact {
        runtime_artifact_with_roles(compilation, archive, RuntimeAbiRole::ALL)
    }

    fn runtime_artifact_with_roles(
        compilation: &crate::Compilation,
        archive: &std::path::Path,
        roles: impl IntoIterator<Item = RuntimeAbiRole>,
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

        let bindings = roles.into_iter().map(|role| {
            let symbol = BinarySymbolName::try_new(format!("bray_runtime_{}_v1", role.as_str()))
                .unwrap_or_else(|| panic!("test runtime role symbol must be valid"));

            RuntimeRoleBinding::new(role, symbol, RuntimeRoleImplementation::BrayRuntime)
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

        let metadata = RuntimeArtifactMetadata::try_new(contract, "libbray_runtime.a", digest)
            .unwrap_or_else(|error| panic!("test runtime metadata must validate: {error:?}"));

        RuntimeArtifact::try_new(metadata, archive, digest)
            .unwrap_or_else(|error| panic!("test runtime artifact must validate: {error:?}"))
    }

    fn generated_artifacts(
        backend: &bray_codegen_llvm::LlvmCodeGenerator,
        facts: &super::NativeProductFacts,
    ) -> Vec<Vec<u8>> {
        generated_artifacts_of_kind(backend, facts, BackendArtifactKind::RelocatableObject)
    }

    fn generated_artifacts_of_kind(
        backend: &bray_codegen_llvm::LlvmCodeGenerator,
        facts: &super::NativeProductFacts,
        kind: BackendArtifactKind,
    ) -> Vec<Vec<u8>> {
        facts
            .units()
            .iter()
            .zip(facts.mappings())
            .map(|(unit, mappings)| {
                let artifact = BackendArtifactId::new(unit.key().clone(), kind, 0);

                let artifacts = BackendArtifactRequest::try_new(
                    unit.key().clone(),
                    [BackendArtifactRequestEntry::new(
                        artifact,
                        BackendArtifactRequirement::Required,
                    )],
                    facts.backend().policy().debug_output(),
                    (kind == BackendArtifactKind::RelocatableObject).then(|| {
                        LinkableArtifactRequirement::new(
                            LinkableArtifactKind::RelocatableObject,
                            BackendArtifactRequirement::Required,
                        )
                    }),
                    BackendSerializationOptions::new(
                        bray_codegen::AssemblySyntaxKind::TargetDefault,
                    ),
                )
                .unwrap_or_else(|error| panic!("test artifact request must validate: {error:?}"));

                let cancellation = CancellationToken::new();

                let request = CodegenRequest::try_new(
                    unit,
                    backend.identity(),
                    backend.capabilities().revision(),
                    ProductKind::Executable,
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
                    "{:?}: {:?}",
                    unit.key(),
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

    fn test_product_identity() -> ProductIdentity {
        ProductIdentity::try_new(crate::test_support::package_identity(), "application")
            .unwrap_or_else(|| panic!("test product identity must be valid"))
    }

    fn test_linker() -> Linker {
        Linker::try_new([Arc::new(TestLinkerDriver) as Arc<dyn LinkerDriver>])
            .unwrap_or_else(|error| panic!("test linker must validate: {error:?}"))
    }

    struct TestLinkerDriver;

    impl LinkerDriver for TestLinkerDriver {
        fn identity(&self) -> &LinkerDriverIdentity {
            static IDENTITY: std::sync::OnceLock<LinkerDriverIdentity> = std::sync::OnceLock::new();

            IDENTITY.get_or_init(|| {
                LinkerDriverIdentity::try_new(LinkerDriverKind::EmbeddedLld, "test-lld", "1", "22")
                    .unwrap_or_else(|| panic!("test linker identity must be valid"))
            })
        }

        fn supports(&self, _target: &bray_linker::LinkTarget, _product: LinkedProductKind) -> bool {
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
        let substitution = test_substitution(values, definition.into(), parameters, arguments);

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
            .intern_constant_value(ConstantValueData::new(ty, ConstantValueKind::Unit))
            .unwrap_or_else(|error| panic!("noise unit value must intern: {error:?}"));

        for _ in 0..4 {
            ty = values
                .intern_type(TypeData::Nullable(ty))
                .unwrap_or_else(|error| panic!("noise nullable type must intern: {error:?}"));

            value = values
                .intern_constant_value(ConstantValueData::new(
                    ty,
                    ConstantValueKind::NullablePresent(value),
                ))
                .unwrap_or_else(|error| panic!("noise nullable value must intern: {error:?}"));

            values
                .intern_constant_term(ConstantTermData::Value(value))
                .unwrap_or_else(|error| panic!("noise constant term must intern: {error:?}"));
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
            .unwrap_or_else(|error| panic!("test codegen target must validate: {error:?}"));

        let semantic = compilation
            .product_semantic_facts()
            .unwrap_or_else(|error| panic!("test product facts must resolve: {error:?}"));

        let roots = compilation
            .product_root_instances(semantic.value(), None, &target, &cancellation)
            .unwrap_or_else(|error| panic!("test roots must resolve: {error:?}"));

        compilation
            .codegen_reachability(roots, None, &target, &cancellation)
            .unwrap_or_else(|error| panic!("generic reachability must close: {error:?}"))
            .graph()
            .instances()
            .iter()
            .filter_map(|instance| match instance.key().specialization() {
                CodegenSpecialization::Generic(_) => Some(instance.key().specialization().clone()),
                CodegenSpecialization::NonGeneric => None,
            })
            .collect()
    }

    fn generic_consumer(dependency: DependencyInterfaceInput) -> crate::Compilation {
        generic_consumer_for_target(dependency, SelectedTarget::baseline())
    }

    fn generic_consumer_for_target(
        dependency: DependencyInterfaceInput,
        target: SelectedTarget,
    ) -> crate::Compilation {
        let request = CompilationRequest::with_options(
            crate::test_support::package_identity(),
            vec![crate::test_support::source_input(
                concat!(
                    "module application;\n",
                    "\n",
                    "using example.dependency.templates.identity;\n",
                    "\n",
                    "func main()\n",
                    "{\n",
                    "    let value: i32 = example.dependency.templates.identity<i32>(1);\n",
                    "}\n",
                ),
                0,
            )],
            CompilationOptions::new(WorkerBudget::serial(), ProductKind::Executable, target),
        )
        .with_dependency_interfaces([dependency]);

        crate::Compilation::load(request)
            .unwrap_or_else(|error| panic!("consumer compilation must load: {error:?}"))
    }

    fn generic_dependency(include_implementation: bool) -> DependencyInterfaceInput {
        generic_dependency_with_templates(include_implementation, false)
    }

    fn generic_dependency_with_templates(
        include_implementation: bool,
        malformed_templates: bool,
    ) -> DependencyInterfaceInput {
        let package = bray_symbols::PackageIdentity::try_new("example.dependency")
            .unwrap_or_else(|| panic!("dependency package identity must be valid"));

        let product = InterfaceProductIdentity::try_new("library")
            .unwrap_or_else(|| panic!("dependency product identity must be valid"));

        let identity = PackageInterfaceIdentity::try_new(
            package.clone(),
            crate::test_support::package_version(),
            product.clone(),
            InterfaceProductKind::Library,
            "public",
        )
        .unwrap_or_else(|| panic!("dependency interface identity must be valid"));

        let export =
            PackageInterfaceExportRequest::new(identity, InterfaceLanguageRevision::new(0));

        let request = CompilationRequest::with_options(
            package.clone(),
            vec![crate::test_support::source_input(
                concat!(
                    "module templates;\n",
                    "\n",
                    "func helper<T>(pos value: T) -> T\n",
                    "{\n",
                    "    return value;\n",
                    "}\n",
                    "\n",
                    "public func identity<T>(pos value: T) -> T\n",
                    "{\n",
                    "    return helper<T>(value);\n",
                    "}\n",
                ),
                0,
            )],
            CompilationOptions::new(
                WorkerBudget::serial(),
                ProductKind::Library,
                SelectedTarget::baseline(),
            ),
        )
        .with_package_interface_export(export);

        let compilation = crate::Compilation::load(request)
            .unwrap_or_else(|error| panic!("dependency compilation must load: {error:?}"));

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        let bundle = compilation
            .package_interface_export_bundle()
            .and_then(|result| result.as_ref().ok())
            .unwrap_or_else(|| panic!("dependency interface bundle must build"));

        let interface = encode_package_interface(bundle)
            .unwrap_or_else(|error| panic!("dependency interface must encode: {error:?}"));

        let policy = InterfaceValidationPolicy::new(InterfaceLanguageRevision::new(0));

        let validated = ValidatedPackageInterface::try_new(interface.bytes(), policy)
            .unwrap_or_else(|error| panic!("dependency interface must validate: {error:?}"));

        let templates = bundle
            .executable_templates()
            .iter()
            .map(|template| {
                if malformed_templates {
                    InterfaceExecutableTemplate::new(template.owner(), [0_u8])
                        .unwrap_or_else(|| panic!("malformed test payload must remain nonempty"))
                } else {
                    template.clone()
                }
            })
            .collect::<Vec<_>>();

        let implementation = PackageImplementationArtifact::try_new(
            &validated,
            bundle.surface(),
            bundle.semantic_facts(),
            [],
            templates,
            [],
            InterfaceValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("dependency implementation must encode: {error:?}"));

        assert_eq!(bundle.executable_templates().len(), 2);

        let dependency = DependencyInterfaceInput::new(
            package,
            product,
            "dependency.brayi",
            interface.shared_bytes(),
            policy,
        );

        if include_implementation {
            dependency.with_implementation_artifact("dependency.brayimpl", Arc::new(implementation))
        } else {
            dependency
        }
    }

    fn first_imported_function_address(
        compilation: &crate::Compilation,
    ) -> bray_symbols::ImportedSymbolFactAddress {
        let skeleton = compilation
            .imported_symbol_skeleton_result()
            .unwrap_or_else(|error| panic!("imported skeleton must load: {error:?}"));

        let skeleton = skeleton
            .value()
            .as_deref()
            .unwrap_or_else(|| panic!("valid dependency must publish a symbol skeleton"));

        skeleton
            .functions()
            .iter()
            .filter_map(|function| skeleton.imported_fact_address(function.id().into()))
            .next()
            .unwrap_or_else(|| panic!("imported generic function must have a fact address"))
    }
}
