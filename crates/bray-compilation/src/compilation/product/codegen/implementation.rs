use std::collections::BTreeSet;
use std::sync::Arc;

use bray_base::shared_slice;
use bray_codegen::{
    AssemblySyntaxKind, CodegenInstanceKey, CodegenTarget, DebugInformationMode,
    DebugInformationOutputMode, LinkableArtifactKind,
};
use bray_emitter::{BackendEmissionPolicy, EmissionBackend};
use bray_ir::{MirUnitId, MirUnitKey};
use bray_linker::Linker;
use bray_runtime_interface::{
    RuntimeArtifact, RuntimeArtifactPurpose, RuntimeArtifactSelection, RuntimeCapability,
};
use bray_symbols::{CallableDefinitionId, ProductIdentity, ProductKind};

use super::super::super::Compilation;
use super::error::NativeProductPlanningError;
use super::plan::NativeProductPlan;
use crate::fact::{CancellationToken, CompilationFactKey, NativeProductQueryKey};

pub(super) const GENERATED_HOST_UNIT: MirUnitId = MirUnitId::new(u32::MAX);

impl Compilation {
    /// Returns the native plan required to emit one selected product.
    pub fn native_product_plan(
        &self,
        product: ProductIdentity,
        configuration: crate::BuildConfiguration,
        runtime: Option<RuntimeArtifact>,
        required_capabilities: impl IntoIterator<Item = RuntimeCapability>,
        linker: Option<&Linker>,
    ) -> Result<Arc<NativeProductPlan>, Arc<NativeProductPlanningError>> {
        self.native_product_plan_with_cancellation(
            product,
            configuration,
            runtime,
            required_capabilities,
            linker,
            &self.state.cancellation,
        )
    }

    fn native_product_plan_with_cancellation(
        &self,
        product: ProductIdentity,
        configuration: crate::BuildConfiguration,
        runtime: Option<RuntimeArtifact>,
        required_capabilities: impl IntoIterator<Item = RuntimeCapability>,
        linker: Option<&Linker>,
        cancellation: &CancellationToken,
    ) -> Result<Arc<NativeProductPlan>, Arc<NativeProductPlanningError>> {
        let mut required_capabilities: Vec<_> = required_capabilities.into_iter().collect();

        if matches!(
            configuration,
            crate::BuildConfiguration::ObservedRelease
                | crate::BuildConfiguration::TimedRelease { .. }
        ) {
            required_capabilities.push(RuntimeCapability::PerformanceObservation);
        }

        required_capabilities.sort_unstable();
        required_capabilities.dedup();

        let key = NativeProductQueryKey::new(
            product.clone(),
            configuration,
            runtime.as_ref().map(|runtime| {
                runtime
                    .components()
                    .iter()
                    .map(|component| {
                        crate::fact::RuntimeComponentQueryIdentity::new(
                            component.metadata().identity().clone(),
                            component.metadata().purpose(),
                            component.metadata().archive_digest(),
                            component.archive().to_path_buf(),
                        )
                    })
                    .collect::<Vec<_>>()
                    .into()
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
            .map_err(|error| Arc::new(NativeProductPlanningError::from(error)))?;

        let result = cell
            .get_or_compute(
                &self.state.fact_runtime,
                CompilationFactKey::NativeProduct(key),
                cancellation,
                || {
                    Ok(self
                        .compute_native_product_plan(
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
            .map_err(|error| Arc::new(NativeProductPlanningError::from(error)))?;

        result.clone()
    }

    fn compute_native_product_plan(
        &self,
        product: ProductIdentity,
        configuration: crate::BuildConfiguration,
        runtime: Option<RuntimeArtifact>,
        required_capabilities: impl IntoIterator<Item = RuntimeCapability>,
        linker: Option<&Linker>,
        cancellation: &CancellationToken,
    ) -> Result<NativeProductPlan, NativeProductPlanningError> {
        let target = self
            .selected_target()
            .target()
            .codegen_target()
            .map_err(NativeProductPlanningError::InvalidCodegenTarget)?;

        let options = configuration.codegen_options();

        let semantic = self.product_semantics_with_cancellation(cancellation)?;

        let test_discovery = if semantic.value().kind() == ProductKind::Test {
            // Discovery owns the product identity used by its independently cached query key.
            let discovery = self.test_discovery_with_cancellation(product.clone(), cancellation)?;

            Some(discovery)
        } else {
            None
        };

        let entry_definitions = super::roots::product_entry_symbols(
            semantic.value(),
            test_discovery.as_deref().map(|discovery| discovery.value()),
        )?
        .into_iter()
        .filter_map(CallableDefinitionId::try_new)
        .collect::<BTreeSet<_>>();

        let source_roots = self.profile_native_product_operation(
            crate::profile::ProfileOperation::NativeRootSelection,
            || {
                self.product_root_instances(
                    semantic.value(),
                    test_discovery.as_deref().map(|discovery| discovery.value()),
                    &target,
                    cancellation,
                )
            },
        )?;

        let entry_roots = source_roots
            .iter()
            .filter(|root| {
                root.callable_instance()
                    .is_some_and(|callable| entry_definitions.contains(&callable.definition()))
            })
            .cloned()
            .collect::<Vec<_>>();

        // Native product plans retain the exact immutable catalog selected for this host.
        let test_catalog = test_discovery
            .as_ref()
            .map(|discovery| discovery.value().catalog().clone());

        let (host, units, mappings, host_statics) = self.prepare_native_codegen(
            &product,
            semantic.value().kind(),
            source_roots,
            &entry_roots,
            runtime.as_ref(),
            required_capabilities,
            &target,
            options.debug_information(),
            cancellation,
        )?;

        let product_host = self.profile_native_product_operation(
            crate::profile::ProfileOperation::NativePlanFinalization,
            || {
                self.codegen_product_host_mapping(
                    &product,
                    &units,
                    &mappings,
                    &host_statics,
                    host.as_ref(),
                    &target,
                )
            },
        )?;

        // Each unit mapping independently retains the Arc-backed product-host contract.
        let mappings = mappings
            .into_iter()
            .map(|mappings| match product_host.as_ref() {
                Some(product_host) => mappings.with_product_host(product_host.clone()),
                None => mappings,
            })
            .collect::<Vec<_>>();

        let codegen = self
            .state
            .codegen
            .as_ref()
            .ok_or(NativeProductPlanningError::CodegenUnavailable)?;

        let debug_output = match options.debug_information() {
            DebugInformationMode::None => DebugInformationOutputMode::Omit,
            DebugInformationMode::LineTables | DebugInformationMode::Full => {
                DebugInformationOutputMode::Embedded
            }
        };

        let linkable_artifact = if configuration.uses_thin_lto() {
            LinkableArtifactKind::BackendBitcode
        } else {
            LinkableArtifactKind::RelocatableObject
        };

        let serialization =
            bray_codegen::BackendSerializationOptions::new(AssemblySyntaxKind::TargetDefault)
                .with_bitcode_semantics(if configuration.uses_thin_lto() {
                    bray_codegen::BackendBitcodeSemantics::ThinLto
                } else {
                    bray_codegen::BackendBitcodeSemantics::Plain
                });

        let policy = BackendEmissionPolicy::new(
            options.debug_information(),
            debug_output,
            Some(linkable_artifact),
            serialization,
        );

        let backend = self.profile_native_product_operation(
            crate::profile::ProfileOperation::NativePlanFinalization,
            || {
                EmissionBackend::try_new(
                    codegen.selected().clone(),
                    codegen.selected_capabilities().clone(),
                    units.iter().map(|unit| unit.key().clone()),
                    policy,
                )
                .map_err(NativeProductPlanningError::InvalidEmissionBackend)
            },
        )?;

        let runtime = self.profile_native_product_operation(
            crate::profile::ProfileOperation::NativePlanFinalization,
            || {
                self.select_runtime(
                    semantic.value().kind(),
                    runtime,
                    host.as_ref(),
                    product_host.as_ref(),
                    &mappings,
                    host_statics.iter().any(
                        super::super::realization::ProductStaticHostEntry::requires_main_thread_cleanup,
                    ),
                    &target,
                )
            },
        )?;

        self.profile_runtime_selection(runtime.as_ref());

        let link = self.profile_native_product_operation(
            crate::profile::ProfileOperation::NativePlanFinalization,
            || {
                linker
                    .map(|_| {
                        self.product_link_inputs(
                            semantic.value().kind(),
                            host.as_ref(),
                            runtime,
                            &mappings,
                            product_host.as_ref(),
                            &target,
                            configuration,
                        )
                    })
                    .transpose()
            },
        )?;

        // The plan owns static instance identities independently of its mapping tables.
        let static_instances = mappings
            .iter()
            .flat_map(bray_codegen::CodegenMappings::static_storages)
            .map(|mapping| mapping.instance().clone())
            .collect::<BTreeSet<_>>();

        Ok(NativeProductPlan {
            backend,
            target,
            options,
            host,
            test_catalog,
            link,
            units,
            mappings: shared_slice(mappings),
            static_instances: shared_slice(static_instances),
            product_host,
        })
    }

    #[inline(always)]
    pub(super) fn profile_native_product_operation<T>(
        &self,
        operation: crate::profile::ProfileOperation,
        action: impl FnOnce() -> Result<T, NativeProductPlanningError>,
    ) -> Result<T, NativeProductPlanningError> {
        crate::profile::profile_operation(
            self.state.fact_runtime.profile(),
            operation,
            action,
            crate::profile::result_outcome,
        )
    }

    fn select_runtime(
        &self,
        kind: ProductKind,
        runtime: Option<RuntimeArtifact>,
        host: Option<&bray_runtime_interface::ExecutableHostContract>,
        product_host: Option<&bray_codegen::CodegenProductHostMapping>,
        mappings: &[bray_codegen::CodegenMappings],
        requires_main_thread_cleanup: bool,
        target: &CodegenTarget,
    ) -> Result<Option<RuntimeArtifactSelection>, NativeProductPlanningError> {
        if kind == ProductKind::Library && requires_main_thread_cleanup {
            return Err(NativeProductPlanningError::LibraryCleanupRequiresMainThread);
        }

        let Some(runtime) = runtime else {
            if host.is_some_and(|host| host.requirements().requires_implementation())
                || kind != ProductKind::Library && product_host.is_some()
            {
                return Err(NativeProductPlanningError::MissingRuntime);
            }

            return Ok(None);
        };

        let purpose = runtime_artifact_purpose(kind);

        let mapped_roles = mappings
            .iter()
            .flat_map(bray_codegen::CodegenMappings::symbols)
            .filter_map(|symbol| match symbol.key() {
                bray_codegen::CodegenSymbolKey::Runtime(reference) => Some(reference.role()),
                bray_codegen::CodegenSymbolKey::Instance(_)
                | bray_codegen::CodegenSymbolKey::CleanupFrameConstructor(_)
                | bray_codegen::CodegenSymbolKey::ProtectedFrame { .. } => None,
            })
            .chain(product_host.map(bray_codegen::CodegenProductHostMapping::control_role));

        let requirements = match host {
            Some(host) if host.requirements().requires_implementation() => {
                super::host::reachable_host_runtime_requirements(host, mapped_roles)
            }
            Some(_) | None => {
                let Some(product_host) = product_host else {
                    return Ok(None);
                };

                let mut roles = mapped_roles.collect::<Vec<_>>();

                roles.extend(bray_codegen::CLEANUP_RUNTIME_ROLES);

                if product_host.statics().iter().any(|entry| {
                    entry.duration() == bray_symbols::StaticStorageDuration::ExactThread
                }) {
                    roles.extend([
                        bray_runtime_interface::RuntimeAbiRole::ThreadAttachmentIdentity,
                        bray_runtime_interface::RuntimeAbiRole::ThreadStaticCleanupRegistration,
                    ]);
                }

                // Runtime selection owns the Arc-backed identities used after planning.
                bray_runtime_interface::RuntimeRequirements::new(
                    Some(runtime.contract().identity().clone()),
                    self.selected_target().target().runtime_abi(),
                    None,
                    target.identity().clone(),
                    target.panic_abi().clone(),
                    roles,
                    [],
                    [],
                )
            }
        };

        runtime
            .select(purpose, &requirements)
            .map(Some)
            .map_err(NativeProductPlanningError::InvalidRuntimeSelection)
    }

    fn profile_runtime_selection(&self, runtime: Option<&RuntimeArtifactSelection>) {
        let Some(profile) = self.state.fact_runtime.profile() else {
            return;
        };

        let Some(runtime) = runtime else {
            return;
        };

        profile.record_metric(
            crate::profile::ProfileMetricKind::RuntimeComponents,
            u64::try_from(runtime.components().len()).unwrap_or(u64::MAX),
        );

        let bytes = runtime
            .components()
            .iter()
            .filter_map(|component| component.archive().metadata().ok())
            .fold(0_u64, |total, metadata| {
                total.saturating_add(metadata.len())
            });

        for component in runtime.components() {
            let component_bytes = component
                .archive()
                .metadata()
                .map_or(0, |metadata| metadata.len());

            profile.add_runtime_artifact(component.metadata().identity().as_str(), component_bytes);
        }

        profile.record_metric(
            crate::profile::ProfileMetricKind::RuntimeArchiveBytes,
            bytes,
        );
    }
}

pub(super) const fn runtime_artifact_purpose(kind: ProductKind) -> RuntimeArtifactPurpose {
    match kind {
        ProductKind::Test => RuntimeArtifactPurpose::TestRunner,
        ProductKind::Executable | ProductKind::Library => RuntimeArtifactPurpose::Product,
    }
}

pub(super) fn bound_template(
    key: &CodegenInstanceKey,
) -> Result<bray_bound_tree::BoundUnitKey, NativeProductPlanningError> {
    let MirUnitKey::Bound(template) = key.template() else {
        return Err(NativeProductPlanningError::MissingProductRoot);
    };

    Ok(template.clone())
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;
    use std::sync::Arc;

    use bray_base::NonEmptySharedStr;
    use bray_bound_tree::BoundCallResult;
    use bray_codegen::{
        BackendArtifactId, BackendArtifactKind, BackendArtifactRequest,
        BackendArtifactRequestEntry, BackendArtifactRequirement, BackendSerializationOptions,
        CodeGenerator, CodeGeneratorRegistry, CodegenConfiguration, CodegenGenericArgument,
        CodegenLinkage, CodegenPartitionPolicy, CodegenRequest, CodegenResultMapping,
        CodegenSpecialization, CodegenStatus, DebugInformationMode, LinkableArtifactKind,
        LinkableArtifactRequirement, OptimizationLevel, partition_codegen_units,
    };
    use bray_compiler_known::RepresentationRole;
    use bray_ir::{
        MirCallTarget, MirHelperReference, MirHostOperation, MirOperand, MirOperationKind,
        MirProjectionKind, MirTerminatorKind, MirUnitKey, MirUnitKind,
    };
    use bray_linker::{
        LinkFailure, LinkInputKind, LinkInputProvenance, LinkInputSource, LinkModel, LinkOutcome,
        LinkPlan, Linker, LinkerDriver, LinkerDriverCapabilities, LinkerDriverIdentity,
        LinkerDriverKind,
    };
    use bray_package_interface::{
        ImportedSemanticRecord, InterfaceExecutableTemplate, InterfaceLanguageRevision,
        InterfaceProductIdentity, InterfaceProductKind, InterfaceSemanticRecordKind,
        InterfaceValidationLimits, InterfaceValidationPolicy, PackageImplementationArtifact,
        PackageInterfaceIdentity, ValidatedPackageInterface, encode_package_interface,
    };
    use bray_runtime_interface::{
        BinarySymbolName, ExecutableEntryResult, ExecutableHostContractBuildError,
        PlatformServiceBinding, PlatformServiceRole, ProtectedFrameAbiVersions,
        ProtectedFrameOperation, RootExecution, RuntimeAbiRole, RuntimeAbiVersion, RuntimeArtifact,
        RuntimeArtifactComponentMetadata, RuntimeArtifactDigest, RuntimeArtifactId,
        RuntimeArtifactMetadata, RuntimeArtifactPurpose, RuntimeCapability,
        RuntimeCompatibilityError, RuntimeContract, RuntimeIdentity, RuntimeRoleBinding,
        RuntimeRoleImplementation,
    };
    use bray_standard_library::{
        StandardLibraryArtifact, StandardLibraryArtifactKind, StandardLibraryBundleManifest,
        StandardLibraryRoot, encode_standard_library_manifest, target_artifacts_for_test,
    };
    use bray_symbols::{
        CallableDefinitionId, CallableInstanceData, ConstantTermData, ConstantValueData,
        ConstantValueKind, GenericArgument, GenericOwnerId, GenericParameterSymbolId,
        GenericSubstitutionData, ImplementationRequirementKey, ImplementationSelection,
        NamedTypeSymbolId, NativeLinkKind, NativeLinkRequirement, NativeSymbolBinding,
        PackageIdentity, ProductIdentity, ProductKind, StaticStorageDuration, SymbolOrigin,
        TraitApplicationData, TypeData,
    };
    use bray_target::{NativeTarget, TargetAddressSpaces, TargetProfile, TargetProperties};
    use bray_testing::TemporaryFile;

    use super::super::super::specialization::ConcreteCodegenInstance;
    use super::NativeProductPlanningError;
    use crate::compilation::CodegenPreparationError;
    use crate::{
        CancellationToken, CompilationOptions, CompilationRequest, DependencyInterfaceInput,
        ImportedSemanticRecordKey, PackageInterfaceExportRequest, SelectedTarget, WorkerBudget,
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

    const SCALAR_COMPARISON_CALL_SOURCE: &str = concat!(
        "module app;\n",
        "\n",
        "func compare_values<Value>(pos left: Value, pos right: Value) -> Ordering\n",
        "    with(Value: Comparable<Value>)\n",
        "{\n",
        "    let ordering: Ordering = left.compare(&right);\n",
        "\n",
        "    return ordering;\n",
        "}\n",
        "\n",
        "public func compare_u64(pos left: u64, pos right: u64) -> Ordering\n",
        "{\n",
        "    return compare_values<u64>(left, right);\n",
        "}\n",
    );

    const TARGET_FENCE_SOURCE: &str = r#"
    trusted module app;
    @copy public union FenceChoice
    {
        Acquire;
        Release;
        AcquireRelease;
        SequentiallyConsistent;
    }
    public trusted func fence(order: FenceChoice) uses(intrinsic)
    {
        match order
        {
            case FenceChoice.Acquire
            {
                trusted core.target.hardware_fence(MemoryOrder.Acquire);
            }
            case FenceChoice.Release
            {
                trusted core.target.hardware_fence(MemoryOrder.Release);
            }
            case FenceChoice.AcquireRelease
            {
                trusted core.target.hardware_fence(MemoryOrder.AcquireRelease);
            }
            case FenceChoice.SequentiallyConsistent
            {
                trusted core.target.hardware_fence( MemoryOrder.SequentiallyConsistent, );
            }
        }
    }
    "#;

    const ATOMIC_GENERIC_SOURCE: &str = concat!(
        "module app;\n",
        "\n",
        "func entry()\n",
        "{\n",
        "    main();\n",
        "}\n",
        "\n",
        "func initialize<T>(pos value: T) -> core.atomic.Atomic<T>\n",
        "{\n",
        "    return core.atomic.initialize<T>(value);\n",
        "}\n",
        "\n",
        "func main()\n",
        "{\n",
        "    let storage = initialize<u32>(1);\n",
        "}\n",
    );

    const ATOMIC_LIBRARY_SOURCE: &str = concat!(
        "module app;\n",
        "\n",
        "public func initialize<T>(pos value: T) -> core.atomic.Atomic<T>\n",
        "{\n",
        "    return core.atomic.initialize<T>(value);\n",
        "}\n",
    );

    const ATOMIC_FENCE_SOURCE: &str = concat!(
        "module app;\n",
        "\n",
        "@copy\n",
        "public union FenceOrder\n",
        "{\n",
        "    Acquire;\n",
        "    Release;\n",
        "    AcquireRelease;\n",
        "    SequentiallyConsistent;\n",
        "}\n",
        "\n",
        "public func fence(order: FenceOrder)\n",
        "{\n",
        "    match order\n",
        "    {\n",
        "        case FenceOrder.Acquire { core.atomic.fence<1>(); }\n",
        "        case FenceOrder.Release { core.atomic.fence<2>(); }\n",
        "        case FenceOrder.AcquireRelease { core.atomic.fence<3>(); }\n",
        "        case FenceOrder.SequentiallyConsistent { core.atomic.fence<4>(); }\n",
        "    }\n",
        "}\n",
    );

    const STRUCTURAL_ASSEMBLY_SOURCE: &str = r#"
    trusted module app;
    public trusted func assemble(pos value: i32) -> i32 uses(device_memory, intrinsic, raw_memory, unchecked_alias, unchecked_init)
    {
        let outputs: (i32, i32) = trusted core.target.assembly< (i32, i32, i32), (i32, i32), >( template = "", constraints = "+reg,=reg,reg,i", clobbers = "", features = "", options = 1, inputs = (value, value, 7), );
        return outputs.0;
    }
    func alternate() -> never
    {
        loop
        {
        }
    }
    func generic_alternate<T>(pos value: T) -> never
    {
        loop
        {
        }
    }
    public trusted func assemble_addresses(pos pointer: RawPointer<u8>) uses(device_memory, intrinsic, raw_memory, unchecked_alias, unchecked_init)
    {
        let ignored: (i32,) = trusted core.target.assembly< (func() -> never, func(pos value: i32) -> never, RawPointer<u8>), (i32,), >( template = "", constraints = "=reg,s,s,m", clobbers = "", features = "", options = 1, inputs = (alternate, generic_alternate, pointer), );
    }
    public trusted func branch(pos value: i32) -> i32 uses(device_memory, intrinsic, raw_memory, unchecked_alias, unchecked_init)
    {
        let output: (i32,) = trusted core.target.branching_assembly< (i32,), (i32,), (func() -> never,), >( template = "", constraints = "+reg,label", clobbers = "", features = "", options = 1, inputs = (value,), labels = (alternate,), );
        return output.0;
    }
    "#;

    const MEMORY_ASSEMBLY_SOURCE: &str = concat!(
        "trusted module memory_assembly;\n",
        "\n",
        "public trusted func assemble_memory(pos pointer: RawPointer<u8>)\n",
        "    uses(device_memory, intrinsic, raw_memory, unchecked_alias, unchecked_init)\n",
        "{\n",
        "    trusted core.target.assembly<(RawPointer<u8>,), unit>(\n",
        "        template = \"incb $0\",\n",
        "        constraints = \"m\",\n",
        "        clobbers = \"memory\",\n",
        "        features = \"\",\n",
        "        options = 0,\n",
        "        inputs = (pointer,),\n",
        "    );\n",
        "}\n",
    );

    const VOID_BRANCHING_ASSEMBLY_SOURCE: &str = concat!(
        "trusted module void_branching_assembly;\n",
        "\n",
        "func alternate() -> never\n",
        "{\n",
        "    loop {}\n",
        "}\n",
        "\n",
        "public trusted func branch_void(pos pointer: RawPointer<u8>)\n",
        "    uses(device_memory, intrinsic, raw_memory, unchecked_alias, unchecked_init)\n",
        "{\n",
        "    trusted core.target.branching_assembly<\n",
        "        (RawPointer<u8>,),\n",
        "        unit,\n",
        "        (func() -> never,),\n",
        "    >(\n",
        "        template = \"\",\n",
        "        constraints = \"m,label\",\n",
        "        clobbers = \"\",\n",
        "        features = \"\",\n",
        "        options = 0,\n",
        "        inputs = (pointer,),\n",
        "        labels = (alternate,),\n",
        "    );\n",
        "}\n",
    );

    const DIVERGING_ASSEMBLY_SOURCE: &str = concat!(
        "trusted module diverging_assembly;\n",
        "\n",
        "public trusted func diverge() -> never\n",
        "    uses(device_memory, intrinsic, raw_memory, unchecked_alias, unchecked_init)\n",
        "{\n",
        "    trusted core.target.diverging_assembly<(i32,)>(\n",
        "        template = \"ud2\",\n",
        "        constraints = \"reg\",\n",
        "        clobbers = \"\",\n",
        "        features = \"\",\n",
        "        options = 0,\n",
        "        inputs = (0,),\n",
        "    );\n",
        "}\n",
    );

    const DEVICE_VOLATILE_CONTRACT_SOURCE: &str = concat!(
        "trusted module device_contract;\n",
        "\n",
        "trusted func device_roundtrip(pos pointer: DevicePointer<u8>, pos value: u8) -> u8\n",
        "    uses(device_memory, intrinsic, raw_memory, unchecked_alias, unchecked_init)\n",
        "{\n",
        "    trusted core.target.device_volatile_store<u8>(pointer, value);\n",
        "    trusted core.target.assembly<(DevicePointer<u8>,), unit>(\n",
        "        template = \"\",\n",
        "        constraints = \"m\",\n",
        "        clobbers = \"memory\",\n",
        "        features = \"\",\n",
        "        options = 0,\n",
        "        inputs = (pointer,),\n",
        "    );\n",
        "    return trusted core.target.device_volatile_load<u8>(pointer);\n",
        "}\n",
    );

    const DIRECT_CALLABLE_TUPLE_INFERENCE_SOURCE: &str = concat!(
        "module app;\n",
        "\n",
        "func alternate() -> never\n",
        "{\n",
        "    loop {}\n",
        "}\n",
        "\n",
        "func identity<T>(pos value: T) -> T\n",
        "{\n",
        "    return value;\n",
        "}\n",
        "\n",
        "public func exercise() -> func() -> never\n",
        "{\n",
        "    return identity<(func() -> never,)>(value = (alternate,)).0;\n",
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
    fn reachable_native_symbols_select_only_their_platform_roles() {
        let available_services = [
            PlatformServiceRole::StandardOutputWrite,
            PlatformServiceRole::FileRead,
        ];

        let selected = super::super::link::platform_services_for_imported_symbols(
            &available_services,
            [PlatformServiceRole::StandardOutputWrite.native_symbol()],
        );

        assert_eq!(
            selected,
            BTreeSet::from([PlatformServiceRole::StandardOutputWrite,])
        );
    }

    #[test]
    fn source_authority_link_inputs_include_standard_library_providers() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("fixture directory must exist: {error}"));

        let selected = SelectedTarget::baseline();
        let target = selected.profile().identity().clone();
        let runtime_abi = selected.runtime_abi();
        let archive_bytes = b"standard library archive";
        let platform_archive_bytes = b"standard stream provider archive";
        let filesystem_archive_bytes = b"filesystem provider archive";
        let process_archive_bytes = b"process provider archive";

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
            "targets/{}/{}.{}/libbray_platform_standard_streams.a",
            target.as_str(),
            runtime_abi.major(),
            runtime_abi.minor()
        );

        let platform_archive = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::PlatformServiceLibrary,
            platform_archive_path,
            platform_archive_bytes,
        )
        .map(|artifact| artifact.with_platform_services([PlatformServiceRole::StandardOutputWrite]))
        .map(|artifact| {
            artifact.with_native_links([NativeLinkRequirement::new(
                NonEmptySharedStr::try_new("c")
                    .unwrap_or_else(|| panic!("native library name must be valid")),
                NativeLinkKind::System,
            )])
        })
        .unwrap_or_else(|error| panic!("platform archive metadata must be valid: {error:?}"));

        let filesystem_archive = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::PlatformServiceLibrary,
            format!(
                "targets/{}/{}.{}/libbray_platform_filesystem.a",
                target.as_str(),
                runtime_abi.major(),
                runtime_abi.minor()
            ),
            filesystem_archive_bytes,
        )
        .map(|artifact| artifact.with_platform_services([PlatformServiceRole::FileRead]))
        .map(|artifact| {
            artifact.with_native_links([NativeLinkRequirement::new(
                NonEmptySharedStr::try_new("filesystem")
                    .unwrap_or_else(|| panic!("native library name must be valid")),
                NativeLinkKind::System,
            )])
        })
        .unwrap_or_else(|error| panic!("filesystem metadata must be valid: {error:?}"));

        let process_archive = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::PlatformServiceLibrary,
            format!(
                "targets/{}/{}.{}/libbray_platform_process.a",
                target.as_str(),
                runtime_abi.major(),
                runtime_abi.minor()
            ),
            process_archive_bytes,
        )
        .map(|artifact| artifact.with_platform_services([PlatformServiceRole::ChildSpawn]))
        .map(|artifact| {
            artifact.with_native_links([NativeLinkRequirement::new(
                NonEmptySharedStr::try_new("process")
                    .unwrap_or_else(|| panic!("native library name must be valid")),
                NativeLinkKind::System,
            )])
        })
        .unwrap_or_else(|error| panic!("process metadata must be valid: {error:?}"));

        let target_artifacts = target_artifacts_for_test(
            target,
            runtime_abi,
            vec![
                interface.clone(),
                implementation.clone(),
                archive.clone(),
                platform_archive.clone(),
                filesystem_archive.clone(),
                process_archive.clone(),
            ],
        )
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
        let filesystem_archive_file = filesystem_archive.beneath(directory.path());
        let process_archive_file = process_archive.beneath(directory.path());

        fs::write(&platform_archive_file, platform_archive_bytes)
            .unwrap_or_else(|error| panic!("platform archive must be written: {error}"));

        fs::write(&filesystem_archive_file, filesystem_archive_bytes)
            .unwrap_or_else(|error| panic!("filesystem archive must be written: {error}"));

        // The unrelated process provider stays absent so selected plans cannot resolve it eagerly.
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

        let package = PackageIdentity::try_new("std.tests.api")
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        let request = CompilationRequest::with_options(
            package,
            vec![crate::test_support::source_input(
                "module application;\n",
                0,
            )],
            CompilationOptions::new(WorkerBudget::serial(), ProductKind::Executable, selected),
        )
        .with_standard_library_provider_root(root)
        .with_standard_library_source_authority();

        let compilation = crate::Compilation::load(request)
            .unwrap_or_else(|error| panic!("compilation must load: {error:?}"));

        assert!(compilation.dependency_interfaces().is_empty());

        let inputs = compilation
            .standard_library_link_inputs(
                ProductKind::Executable,
                &BTreeSet::from([PlatformServiceRole::StandardOutputWrite.native_symbol()]),
                &BTreeSet::new(),
            )
            .unwrap_or_else(|error| panic!("standard library inputs must resolve: {error:?}"));

        assert_eq!(inputs.len(), 3);

        let package_provenance = |provenance: &LinkInputProvenance| {
            matches!(
                provenance,
                LinkInputProvenance::Package(package)
                    if package.as_str()
                        == bray_standard_library::PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY
            )
        };

        let platform_provenance = |provenance: &LinkInputProvenance| {
            matches!(
                provenance,
                LinkInputProvenance::PlatformProvider(package)
                    if package.as_str()
                        == bray_standard_library::PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY
            )
        };

        for (path, provenance) in [
            (
                &archive_file,
                package_provenance as fn(&LinkInputProvenance) -> bool,
            ),
            (
                &platform_archive_file,
                platform_provenance as fn(&LinkInputProvenance) -> bool,
            ),
        ] {
            assert!(inputs.iter().any(|input| {
                input.as_ref().is_ok_and(|input| {
                    input.kind() == LinkInputKind::Archive
                        && input.source() == &LinkInputSource::file(path)
                        && provenance(input.provenance())
                })
            }));
        }

        assert!(inputs.iter().any(|input| {
            input.as_ref().is_ok_and(|input| {
                input.kind() == LinkInputKind::NativeLibrary
                    && input.source()
                        == &LinkInputSource::try_native_library("c")
                            .unwrap_or_else(|| panic!("native library name must be valid"))
                    && platform_provenance(input.provenance())
            })
        }));

        for forbidden in [&filesystem_archive_file, &process_archive_file] {
            assert!(!inputs.iter().any(|input| {
                input
                    .as_ref()
                    .is_ok_and(|input| input.source() == &LinkInputSource::file(forbidden))
            }));
        }

        let filesystem_inputs = compilation
            .standard_library_link_inputs(
                ProductKind::Executable,
                &BTreeSet::from([PlatformServiceRole::FileRead.native_symbol()]),
                &BTreeSet::new(),
            )
            .unwrap_or_else(|error| panic!("filesystem inputs must resolve: {error:?}"));

        assert!(filesystem_inputs.iter().any(|input| {
            input.as_ref().is_ok_and(|input| {
                input.source() == &LinkInputSource::file(&filesystem_archive_file)
                    && platform_provenance(input.provenance())
            })
        }));

        assert!(!filesystem_inputs.iter().any(|input| {
            input.as_ref().is_ok_and(|input| {
                input.source() == &LinkInputSource::file(&platform_archive_file)
                    || input.source() == &LinkInputSource::file(&process_archive_file)
            })
        }));

        assert!(filesystem_inputs.iter().any(|input| {
            input.as_ref().is_ok_and(|input| {
                input.source()
                    == &LinkInputSource::try_native_library("filesystem")
                        .unwrap_or_else(|| panic!("native library name must be valid"))
                    && platform_provenance(input.provenance())
            })
        }));

        assert!(!filesystem_inputs.iter().any(|input| {
            input.as_ref().is_ok_and(|input| {
                ["c", "process"].iter().any(|name| {
                    input.source()
                        == &LinkInputSource::try_native_library(*name)
                            .unwrap_or_else(|| panic!("native library name must be valid"))
                })
            })
        }));

        let overridden_inputs = compilation
            .standard_library_link_inputs(
                ProductKind::Test,
                &BTreeSet::from([PlatformServiceRole::StandardOutputWrite.native_symbol()]),
                &BTreeSet::from([PlatformServiceRole::StandardOutputWrite]),
            )
            .unwrap_or_else(|error| panic!("overridden inputs must resolve: {error:?}"));

        assert_eq!(overridden_inputs.len(), 1);

        assert!(overridden_inputs[0].as_ref().is_ok_and(|input| {
            input.source() == &LinkInputSource::file(&archive_file)
                && package_provenance(input.provenance())
        }));

        assert!(
            compilation
                .standard_library_link_inputs(
                    ProductKind::Library,
                    &BTreeSet::new(),
                    &BTreeSet::new(),
                )
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
        let archive = TemporaryFile::write("libbray_runtime.a", b"!<arch>\n");
        let available_runtime = runtime_artifact(&compilation, archive.path());

        let plan = compilation
            .native_product_plan(
                product.clone(),
                crate::BuildConfiguration::Development,
                Some(available_runtime.clone()),
                [],
                Some(&test_linker()),
            )
            .unwrap_or_else(|error| panic!("native plan must resolve: {error:?}"));

        let release = compilation
            .native_product_plan(
                product.clone(),
                crate::BuildConfiguration::Release,
                Some(available_runtime.clone()),
                [],
                Some(&test_linker()),
            )
            .unwrap_or_else(|error| panic!("release native plan must resolve: {error:?}"));

        let host = plan
            .executable_host()
            .unwrap_or_else(|| panic!("executable must retain its host"));

        assert!(host.requirements().requires_implementation());
        assert!(host.runtime_artifact().is_some());

        compilation
            .native_product_plan(
                product,
                crate::BuildConfiguration::Development,
                Some(available_runtime),
                [],
                Some(&test_linker()),
            )
            .unwrap_or_else(|error| panic!("available runtime must remain reusable: {error:?}"));

        assert_eq!(plan.options().optimization(), OptimizationLevel::Basic);

        assert_eq!(
            plan.options().debug_information(),
            DebugInformationMode::LineTables
        );

        assert_eq!(release.options().optimization(), OptimizationLevel::Full);

        assert_eq!(
            release.options().debug_information(),
            DebugInformationMode::None
        );

        assert!(!Arc::ptr_eq(&plan, &release));

        assert!(plan.mappings().iter().any(|mappings| {
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

        let host = plan
            .executable_host()
            .unwrap_or_else(|| panic!("executable must own a host"));

        assert_eq!(host.entries()[0].root(), RootExecution::Synchronous);
        assert_eq!(host.entries()[0].result(), ExecutableEntryResult::I32);

        assert!(
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }

    #[test]
    fn borrowed_literal_identity_remains_reachable_for_every_representation() {
        let (_, compilation) = codegen_compilation_for_product(
            concat!(
                "module app;\n",
                "\n",
                "public func owned() -> string\n",
                "{\n",
                "    return \"shared literal\";\n",
                "}\n",
                "\n",
                "public func borrowed() -> &string\n",
                "{\n",
                "    return &\"shared literal\";\n",
                "}\n",
            ),
            ProductKind::Library,
        );

        let plan = compilation
            .native_product_plan(
                test_product_identity(),
                crate::BuildConfiguration::Development,
                None,
                [],
                None,
            )
            .unwrap_or_else(|error| panic!("borrowed literal must remain reachable: {error:?}"));

        let mappings = plan
            .mappings()
            .iter()
            .flat_map(bray_codegen::CodegenMappings::constants)
            .filter(|mapping| {
                matches!(
                    mapping.data().kind(),
                    ConstantValueKind::String(text) if text.as_ref() == "shared literal"
                )
            })
            .collect::<Vec<_>>();

        let borrowed = mappings
            .iter()
            .copied()
            .find(|mapping| mapping.semantic_type() != mapping.representation())
            .unwrap_or_else(|| panic!("borrow representation must be materialized"));

        assert!(mappings.iter().any(|mapping| {
            mapping.value() == borrowed.value()
                && mapping.semantic_type() == mapping.representation()
        }));
    }

    #[test]
    fn target_fence_wrapper_emits_valid_native_units() {
        let (backend, compilation) =
            codegen_compilation_for_product(TARGET_FENCE_SOURCE, ProductKind::Library);

        let plan = compilation
            .native_product_plan(
                test_product_identity(),
                crate::BuildConfiguration::Development,
                None,
                [],
                None,
            )
            .unwrap_or_else(|error| panic!("target fence wrapper must realize: {error:?}"));

        assert!(
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }

    #[test]
    fn structural_assembly_emits_valid_native_units() {
        let (backend, compilation) =
            codegen_compilation_for_product(STRUCTURAL_ASSEMBLY_SOURCE, ProductKind::Library);

        let lowered = compilation
            .lowered_unit(crate::test_support::source_function_body_key(
                &compilation,
                "branch",
            ))
            .unwrap_or_else(|error| panic!("branching assembly must lower: {error:?}"));

        let mir = lowered
            .value()
            .as_ref()
            .and_then(bray_lowering::LoweredUnit::mir)
            .unwrap_or_else(|| panic!("branching assembly must produce MIR: {lowered:#?}"));

        let assembly = mir
            .blocks()
            .iter()
            .find_map(|block| match block.terminator().kind() {
                MirTerminatorKind::InlineAssembly(assembly) => Some(assembly),
                _ => None,
            })
            .unwrap_or_else(|| panic!("branching assembly must lower as a terminator"));

        let [alternate] = assembly.alternates() else {
            panic!("one checked label must produce one alternate trampoline");
        };

        let alternate = mir
            .block(*alternate)
            .unwrap_or_else(|| panic!("alternate trampoline must exist"));

        let calls = alternate
            .operations()
            .iter()
            .filter_map(|id| mir.operation(*id))
            .filter_map(|operation| match operation.kind() {
                MirOperationKind::Call(call) => Some(call),
                _ => None,
            })
            .collect::<Vec<_>>();

        let [call] = calls.as_slice() else {
            panic!("alternate trampoline must contain one callback call");
        };

        let MirCallTarget::Indirect { .. } = call.target() else {
            panic!("assembly labels must remain runtime callable values");
        };

        let never = compilation
            .available_compiler_known_symbols()
            .representation_symbol::<bray_symbols::StructSymbolId>(RepresentationRole::Never)
            .and_then(|definition| {
                compilation.semantic_value_store().ok().and_then(|values| {
                    crate::compilation::substitution::named_type(
                        values,
                        NamedTypeSymbolId::Struct(definition),
                    )
                    .ok()
                })
            })
            .unwrap_or_else(|| panic!("never representation must resolve"));

        assert!(call.arguments().is_empty());
        assert_eq!(call.result(), BoundCallResult::Immediate(never));

        let MirTerminatorKind::CheckCallOutcome { completed, .. } = alternate.terminator().kind()
        else {
            panic!("a nonreturning Bray callback must still forward panic and cancellation");
        };

        assert!(matches!(
            mir.block(completed.target()).unwrap().terminator().kind(),
            MirTerminatorKind::Unreachable
        ));

        let plan = compilation
            .native_product_plan(
                test_product_identity(),
                crate::BuildConfiguration::Development,
                None,
                [],
                None,
            )
            .unwrap_or_else(|error| panic!("structural assembly must realize: {error:?}"));

        assert!(
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }

    #[test]
    fn native_frame_metadata_uses_concrete_identities_for_generic_instances() {
        let source = r#"
        module app;
        async func identity<T>(pos value: T) -> T
        {
            return value;
        }
        async func main()
        {
            let first: i32 = await identity<i32>(1);
            let second: u32 = await identity<u32>(2);
        }
        "#;

        for target in NativeTarget::ALL {
            let (backend, plan) = runtime_native_plan_for_sources_target(
                &[source],
                ProductKind::Executable,
                SelectedTarget::for_native(target),
                &[],
            );

            let artifacts =
                generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendIr);

            let mut identities =
                std::collections::BTreeMap::<_, std::collections::BTreeSet<_>>::new();

            for (unit, artifact) in plan.units().iter().zip(&artifacts) {
                let ir = std::str::from_utf8(artifact).unwrap();

                for instance in unit.instances() {
                    let Some(descriptor) = instance.mir().frame_descriptor() else {
                        continue;
                    };

                    let identity = instance.protected_frame_identity().unwrap();

                    identities
                        .entry(descriptor.frame())
                        .or_default()
                        .insert(identity);

                    let encoded: String = identity
                        .digest()
                        .iter()
                        .map(|byte| {
                            if *byte == b'\\' {
                                "\\\\".to_owned()
                            } else if (0x20..=0x7e).contains(byte) && *byte != b'"' {
                                char::from(*byte).to_string()
                            } else {
                                format!("\\{byte:02X}")
                            }
                        })
                        .collect();

                    assert!(
                        ir.contains(&format!(r#"[32 x i8] c"{encoded}""#)),
                        "{target:?} must emit the concrete frame identity {identity:?}. Expected {encoded:?}. Metadata: {:?}",
                        ir.lines()
                            .filter(|line| line.starts_with("@frame.metadata"))
                            .collect::<Vec<_>>()
                    );
                }
            }

            assert!(
                identities.values().any(|instances| instances.len() >= 2),
                "{target:?} must exercise distinct specializations of the same frame template"
            );
        }
    }

    #[test]
    fn indirect_memory_assembly_emits_valid_native_units() {
        let (backend, compilation) =
            codegen_compilation_for_product(MEMORY_ASSEMBLY_SOURCE, ProductKind::Library);

        let plan = compilation
            .native_product_plan(
                test_product_identity(),
                crate::BuildConfiguration::Development,
                None,
                [],
                None,
            )
            .unwrap_or_else(|error| panic!("memory assembly must realize: {error:?}"));

        let artifacts =
            generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendIr);

        assert!(artifacts.iter().any(|artifact| {
            std::str::from_utf8(artifact)
                .is_ok_and(|artifact| artifact.contains("ptr elementtype(i8)"))
        }));
    }

    #[test]
    fn void_branching_assembly_emits_valid_native_units() {
        assert_source_emits_valid_native_units(
            VOID_BRANCHING_ASSEMBLY_SOURCE,
            crate::BuildConfiguration::Development,
        );
    }

    #[test]
    fn impure_diverging_assembly_survives_optimized_native_codegen() {
        let (backend, compilation) =
            codegen_compilation_for_product(DIVERGING_ASSEMBLY_SOURCE, ProductKind::Library);

        let plan = compilation
            .native_product_plan(
                test_product_identity(),
                crate::BuildConfiguration::Release,
                None,
                [],
                None,
            )
            .unwrap_or_else(|error| panic!("diverging assembly must realize: {error:?}"));

        let artifacts =
            generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendIr);

        assert!(artifacts.iter().any(|artifact| {
            std::str::from_utf8(artifact)
                .is_ok_and(|artifact| artifact.contains("asm sideeffect \"ud2\""))
        }));
    }

    #[test]
    fn device_volatile_contracts_accept_device_pointer_storage_contracts() {
        let native = NativeTarget::X86_64LinuxGnu.profile();
        let baseline = native.properties();

        let address_spaces = TargetAddressSpaces::try_new(true, true)
            .unwrap_or_else(|| panic!("test target must expose host and device address spaces"));

        let properties = TargetProperties::new(
            baseline.identity().clone(),
            baseline.scalars(),
            baseline.atomics(),
            baseline.abis(),
            baseline.c_abi(),
            address_spaces,
            baseline.alignments(),
            baseline.operations(),
        );

        let profile = TargetProfile::try_new(
            native.identity().clone(),
            native.machine().clone(),
            properties,
        )
        .unwrap_or_else(|error| panic!("device-capable target profile must validate: {error:?}"));

        let request = CompilationRequest::with_options(
            crate::test_support::package_identity(),
            vec![crate::test_support::source_input(
                DEVICE_VOLATILE_CONTRACT_SOURCE,
                0,
            )],
            CompilationOptions::new(
                WorkerBudget::serial(),
                ProductKind::Library,
                SelectedTarget::new(profile, RuntimeAbiVersion::new(1, 0)),
            ),
        );

        let compilation = crate::Compilation::load(request)
            .unwrap_or_else(|error| panic!("device contract compilation must load: {error:?}"));

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );
    }

    fn assert_source_emits_valid_native_units(
        source: &str,
        configuration: crate::BuildConfiguration,
    ) {
        let (backend, compilation) = codegen_compilation_for_product(source, ProductKind::Library);

        let plan = compilation
            .native_product_plan(test_product_identity(), configuration, None, [], None)
            .unwrap_or_else(|error| panic!("target-control source must realize: {error:?}"));

        assert!(
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }

    #[test]
    fn explicit_generic_tuple_expectations_reach_direct_callable_elements() {
        let _ = codegen_compilation_for_product(
            DIRECT_CALLABLE_TUPLE_INFERENCE_SOURCE,
            ProductKind::Library,
        );
    }

    #[test]
    fn callable_contract_adaptations_emit_native_units_in_debug_and_release() {
        for configuration in [
            crate::BuildConfiguration::Development,
            crate::BuildConfiguration::Release,
        ] {
            assert_source_emits_valid_native_units(
                r#"
                module app;

                func weaken(operation: func()
                        executes(pure, total)
                    ) -> func()
                    {
                        return operation;
                    }

                    func convert(operation: func()
                            executes(pure, total)
                        ) -> func()
                        {
                            return operation as func();
                        }
                "#,
                configuration,
            );
        }
    }

    #[test]
    fn asynchronous_executable_hosts_emit_complete_deterministic_native_units() {
        let (backend, plan) = runtime_native_plan(include_str!(
            "../../../../../../xtask/fixtures/native-execution/async-i32.bray"
        ));

        let host = plan
            .executable_host()
            .unwrap_or_else(|| panic!("async executable must own a host"));

        let RootExecution::Asynchronous { frame } = host.entries()[0].root() else {
            panic!("async executable host must retain a protected root frame");
        };

        let product_host = plan
            .product_host()
            .expect("native root must retain its provider even without statics");

        assert!(product_host.statics().is_empty());

        let entry_unit = plan
            .units()
            .iter()
            .flat_map(|unit| unit.instances())
            .find(|instance| {
                matches!(
                    instance.mir().kind(),
                    bray_ir::MirUnitKind::ExecutableHost(_)
                )
            })
            .expect("host MIR must exist");

        let entry = entry_unit.mir().block(entry_unit.mir().entry()).unwrap();
        let first = entry_unit.mir().operation(entry.operations()[0]).unwrap();

        assert!(matches!(
            first.kind(),
            bray_ir::MirOperationKind::Host(bray_ir::MirHostOperation::BeginExecution { .. })
        ));

        assert_eq!(host.entries()[0].result(), ExecutableEntryResult::I32);

        let frame_unit = plan
            .units()
            .iter()
            .find(|unit| {
                unit.instances()
                    .iter()
                    .any(|instance| instance.protected_frame_identity() == Some(frame))
            })
            .unwrap_or_else(|| panic!("concrete root frame unit must be retained"));

        let frame_mapping = plan
            .units()
            .iter()
            .zip(plan.mappings())
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
            RuntimeAbiRole::MainThreadLaneStartup,
            RuntimeAbiRole::ProductHostControl,
            RuntimeAbiRole::RootExecution,
            RuntimeAbiRole::RootCancellationRequest,
            RuntimeAbiRole::RootTerminalObservation,
            RuntimeAbiRole::RootCompletionResolution,
            RuntimeAbiRole::CleanupIncidentReporting,
            RuntimeAbiRole::PanicReporting,
            RuntimeAbiRole::EntryFailureResolution,
            RuntimeAbiRole::StructuredShutdown,
        ] {
            assert_eq!(
                host.role_binding(role)
                    .map(RuntimeRoleBinding::implementation),
                Some(RuntimeRoleImplementation::BrayRuntime)
            );
        }

        let first = generated_artifacts(&backend, &plan);
        let second = generated_artifacts(&backend, &plan);

        assert_eq!(first, second);
        assert_eq!(first.len(), plan.units().len());
        assert!(first.iter().all(|artifact| !artifact.is_empty()));
    }

    #[test]
    fn native_preparation_is_deterministic_across_worker_budgets() {
        let (serial_backend, serial_compilation) =
            codegen_compilation_for_product_with_worker_budget(
                CONCRETE_GENERIC_SOURCE,
                ProductKind::Library,
                WorkerBudget::serial(),
            );

        let parallel_budget = WorkerBudget::new(4)
            .unwrap_or_else(|error| panic!("parallel worker budget must validate: {error:?}"));

        let (parallel_backend, parallel_compilation) =
            codegen_compilation_for_product_with_worker_budget(
                CONCRETE_GENERIC_SOURCE,
                ProductKind::Library,
                parallel_budget,
            );

        let serial = serial_compilation
            .native_product_plan(
                test_product_identity(),
                crate::BuildConfiguration::Development,
                None,
                [],
                None,
            )
            .unwrap_or_else(|error| panic!("serial native plan must prepare: {error:?}"));

        let parallel = parallel_compilation
            .native_product_plan(
                test_product_identity(),
                crate::BuildConfiguration::Development,
                None,
                [],
                None,
            )
            .unwrap_or_else(|error| panic!("parallel native plan must prepare: {error:?}"));

        assert_eq!(
            native_partition_recipe(&serial),
            native_partition_recipe(&parallel)
        );

        assert_eq!(serial.executable_host(), parallel.executable_host());
        assert_eq!(serial.product_host(), parallel.product_host());
        assert_eq!(serial.static_instances(), parallel.static_instances());

        assert_eq!(
            generated_artifacts(&serial_backend, &serial),
            generated_artifacts(&parallel_backend, &parallel)
        );
    }

    #[test]
    fn compile_only_emission_preserves_specialized_cleanup_mir() {
        let source = r#"
            module app;
            struct Guard
            {
                async finalize() {}
            }
            async func dispose<T>(pos value: T) -> i32
            {
                return 7;
            }
            async func main()
            {
                await dispose<[Guard; 2]>([Guard {}, Guard {}]);
            }
        "#;

        let (_, compilation) = codegen_compilation_for_product(source, ProductKind::Executable);

        let archive = TemporaryFile::write("libbray_runtime.a", b"!<arch>\n");
        let runtime = runtime_artifact(&compilation, archive.path());

        let plan = compilation
            .native_product_plan(
                test_product_identity(),
                crate::BuildConfiguration::Development,
                Some(runtime),
                [],
                Some(&test_linker()),
            )
            .expect("generic cleanup native plan must prepare");

        let profile = compilation.selected_target().target().profile().clone();

        let name = bray_target::TargetOutputName::try_new(
            bray_target::TargetOutputKind::BackendIr,
            "",
            ".ll",
        )
        .expect("backend IR output name must validate");

        let outputs = bray_target::TargetOutputDescription::try_new(profile.clone(), [name])
            .expect("backend IR outputs must validate");

        let destination = tempfile::tempdir().expect("output directory must exist");

        let request = bray_emitter::EmissionRequest::try_new(
            test_product_identity(),
            ProductKind::Executable,
            plan.executable_host().cloned(),
            profile.identity().clone(),
            bray_emitter::RequestedArtifactDestination::FilesystemDirectory(
                destination.path().into(),
            ),
            [bray_emitter::RequestedArtifact::new(
                bray_emitter::ArtifactKind::BackendIr,
                bray_emitter::ArtifactRequirement::Required,
            )],
            bray_emitter::ReplacementPolicy::RequireAbsent,
        )
        .expect("compile-only request must validate");

        let linker = test_linker();
        let linking = plan.link().expect("native plan must retain link inputs");

        let composed = crate::ProductEmissionInputs::new(&outputs)
            .with_native_codegen(&plan)
            .with_linking(&linker, linking);

        let error = compilation
            .emit_product(request.clone(), composed)
            .expect_err("compile-only requests must reject explicitly supplied linking");

        assert!(
            matches!(
                error.kind(),
                crate::ProductEmissionErrorKind::UnexpectedLinker
            ),
            "builder composition must preserve codegen and linking: {error:?}",
        );

        let outcome = compilation
            .emit_product(
                request.clone(),
                crate::ProductEmissionInputs::new(&outputs).with_native_codegen(&plan),
            )
            .expect("compile-only emission must consume specialized cleanup MIR");

        assert!(
            matches!(outcome.status(), bray_emitter::EmissionStatus::Complete),
            "status: {:?}, diagnostics: {:?}",
            outcome.status(),
            outcome.diagnostics()
        );

        assert!(!outcome.artifacts().artifacts().is_empty());

        assert!(
            outcome
                .artifacts()
                .artifacts()
                .iter()
                .all(|artifact| { artifact.id().kind() == bray_emitter::ArtifactKind::BackendIr })
        );
    }

    #[test]
    fn cancelled_native_preparation_publishes_no_partial_plan() {
        let (_, compilation) = codegen_compilation_for_product_with_worker_budget(
            CONCRETE_GENERIC_SOURCE,
            ProductKind::Library,
            WorkerBudget::new(4)
                .unwrap_or_else(|error| panic!("parallel worker budget must validate: {error:?}")),
        );

        let cancellation = CancellationToken::new();

        cancellation.cancel();

        let cancelled = compilation.native_product_plan_with_cancellation(
            test_product_identity(),
            crate::BuildConfiguration::Development,
            None,
            [],
            None,
            &cancellation,
        );

        assert!(cancelled.is_err_and(|error| error.is_cancelled()));

        assert!(
            compilation
                .native_product_plan(
                    test_product_identity(),
                    crate::BuildConfiguration::Development,
                    None,
                    [],
                    None,
                )
                .is_ok()
        );
    }

    #[test]
    fn independently_started_tasks_emit_native_units() {
        let source = concat!(
            "module async_tasks;\n",
            "\n",
            "async func complete()\n",
            "{\n",
            "    return unit;\n",
            "}\n",
            "\n",
            "async func main()\n",
            "{\n",
            "    let first: Task<unit> = complete().start();\n",
            "    let second: Task<unit> = complete().start();\n",
            "\n",
            "    try await first.join();\n",
            "    try await second.join();\n",
            "}\n",
        );

        let (backend, compilation) =
            codegen_compilation_for_product(source, ProductKind::Executable);

        let archive = TemporaryFile::write("libbray_runtime.a", b"!<arch>\n");
        let runtime = runtime_artifact(&compilation, archive.path());

        let plan = compilation
            .native_product_plan(
                test_product_identity(),
                crate::BuildConfiguration::Development,
                Some(runtime),
                [],
                Some(&test_linker()),
            )
            .unwrap_or_else(|error| panic!("started tasks must realize: {error:?}"));

        assert!(
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }

    #[test]
    fn task_outcome_transfers_use_concrete_callbacks_on_every_native_target() {
        let source = r#"
        module app;

        @layout(c, align = 32)
        struct Payload
        {
            first: u64;
            second: u64;
            third: u64;
        }

        async func empty() {}

        async func integer() -> u64
        {
            return 42;
        }

        async func aggregate() -> Payload
        {
            return Payload
            {
                first = 17,
                second = 29,
                third = 41
            };
        }

        async func main()
        {
            let first: Task<unit> = empty().start();
            let second: Task<u64> = integer().start();
            let third: Task<Payload> = aggregate().start();

            try await first.join();

            let integer_result: u64 = try await second.join();
            let aggregate_result: Payload = try await third.join();
        }
        "#;

        for target in NativeTarget::ALL {
            let (backend, plan) = runtime_native_plan_for_sources_target(
                &[source],
                ProductKind::Executable,
                SelectedTarget::for_native(target),
                &[],
            );

            let artifacts =
                generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendIr);

            let callbacks = artifacts
                .iter()
                .map(|artifact| {
                    String::from_utf8_lossy(artifact)
                        .lines()
                        .filter(|line| {
                            line.starts_with("define ") && line.contains(".run_result.transfer.")
                        })
                        .count()
                })
                .sum::<usize>();

            assert!(
                callbacks >= 3,
                "{target:?} must retain unit, scalar, and over-aligned aggregate transfers"
            );

            assert!(
                generated_artifacts(&backend, &plan)
                    .iter()
                    .all(|artifact| !artifact.is_empty())
            );
        }
    }

    #[test]
    fn entry_errors_keep_async_cleanup_owned_through_host_resolution() {
        for execution in ["", "async "] {
            let source = format!(
                r#"
                module app;
                async func suspend()
                {{
                }}
                struct Failure
                {{
                    mut complete: bool;
                    async finalize()
                    {{
                        await suspend();
                        self.complete = true;
                    }}
                    destruct()
                    {{
                        assert(self.complete);
                    }}
                }}
                {execution}func main() -> Result<unit, Failure>
                {{
                    return Error(Failure
                    {{
                        complete = false
                    }});
                }}
                "#
            );

            for target in [
                SelectedTarget::baseline(),
                SelectedTarget::for_native(NativeTarget::X86_64WindowsMsvc),
            ] {
                let (backend, compilation) = codegen_compilation_for_sources_target(
                    &[source.as_str()],
                    ProductKind::Executable,
                    target,
                    &[],
                );

                let archive = TemporaryFile::write("libbray_runtime.a", b"!<arch>\n");
                let runtime = runtime_artifact(&compilation, archive.path());

                let plan = compilation
                    .native_product_plan(
                        test_product_identity(),
                        crate::BuildConfiguration::Development,
                        Some(runtime),
                        [],
                        Some(&test_linker()),
                    )
                    .expect("host-owned cleanup must select its runtime dependencies");

                let incomplete_runtime = runtime_artifact_with_roles(
                    &compilation,
                    archive.path(),
                    RuntimeAbiRole::ALL
                        .into_iter()
                        .filter(|role| *role != RuntimeAbiRole::AwaitedFrameComposition),
                );

                let error = compilation
                    .select_runtime(
                        ProductKind::Executable,
                        Some(incomplete_runtime),
                        plan.executable_host(),
                        plan.product_host(),
                        plan.mappings(),
                        false,
                        plan.target(),
                    )
                    .expect_err(
                        "generated cleanup dependencies must participate in final selection",
                    );

                assert!(matches!(
                    error,
                    NativeProductPlanningError::InvalidRuntimeSelection(
                        bray_runtime_interface::RuntimeArtifactSelectionError::IncompatibleRuntime(
                            RuntimeCompatibilityError::MissingRole(
                                RuntimeAbiRole::AwaitedFrameComposition
                            )
                        )
                    )
                ));

                let artifacts =
                    generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendIr);

                let ir = artifacts
                    .iter()
                    .map(|artifact| String::from_utf8_lossy(artifact))
                    .collect::<Vec<_>>()
                    .join("\n");

                assert!(ir.contains("bray_runtime_entry_failure_resolution"));
                assert!(ir.contains("value_cleanup.descriptor"));
                assert!(ir.contains("cleanup.inactive"));

                assert!(ir.lines().any(|line| line.contains("call")
                    && line.contains("bray_runtime_entry_result_admission")
                    && line.contains("value_cleanup.descriptor")));

                assert!(ir.lines().any(|line| line.contains("call")
                    && line.contains("bray_runtime_entry_failure_resolution")
                    && line.contains("ptr null, ptr null")));

                assert!(ir.lines().any(|line| line.contains("call")
                    && line.contains("bray_runtime_entry_result_resolution")));

                assert!(ir.contains("entry.result.admitted"));
                assert!(ir.contains("entry.result.resolved"));
            }
        }
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
            let (backend, plan) = runtime_native_plan(source);

            let host = plan
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

                let helper_references = plan
                    .mappings()
                    .iter()
                    .flat_map(bray_codegen::CodegenMappings::operations)
                    .flat_map(bray_codegen::CodegenOperationMapping::helpers)
                    .map(bray_codegen::CodegenHelperMapping::reference)
                    .collect::<Vec<_>>();

                for phase in [
                    bray_ir::MirCleanupPhase::TaskCancellation,
                    bray_ir::MirCleanupPhase::LifecycleResolution,
                ] {
                    assert!(
                        helper_references
                            .contains(&&MirHelperReference::Cleanup { phase, ty: error })
                    );
                }

                let ir =
                    generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendIr)
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>();

                let ir = String::from_utf8(ir)
                    .unwrap_or_else(|error| panic!("LLVM IR must be UTF-8: {error}"));

                assert!(ir.contains("entry.failure"));
                assert!(ir.contains("bray_runtime_entry_failure_resolution"));
                assert!(ir.contains("entry.skip.error"));

                let identities = plan
                    .mappings()
                    .iter()
                    .flat_map(bray_codegen::CodegenMappings::operations)
                    .filter_map(bray_codegen::CodegenOperationMapping::returned_error_identity)
                    .collect::<Vec<_>>();

                assert_eq!(identities.len(), 1);
                assert_ne!(identities[0], [0; 32]);

                let reports = ir
                    .lines()
                    .filter(|line| {
                        line.contains("call")
                            && line.contains("@bray_runtime_entry_failure_resolution")
                    })
                    .collect::<Vec<_>>();

                assert!(!reports.is_empty());

                assert!(
                    reports
                        .iter()
                        .all(|line| line.contains("error_type") && line.contains("error_source"))
                );
            } else {
                assert_eq!(host.entries()[0].result(), ExecutableEntryResult::Unit);
            }

            assert!(
                generated_artifacts(&backend, &plan)
                    .iter()
                    .all(|artifact| !artifact.is_empty())
            );
        }
    }

    #[test]
    fn synchronous_panics_emit_a_runtime_owned_host_boundary() {
        for target in [
            SelectedTarget::baseline(),
            SelectedTarget::for_native(NativeTarget::X86_64WindowsMsvc),
        ] {
            let (backend, plan) = runtime_native_plan_for_sources_target(
                &[include_str!(
                    "../../../../../../xtask/fixtures/native-execution/sync-panic.bray"
                )],
                ProductKind::Executable,
                target,
                &[],
            );

            let host = plan
                .executable_host()
                .unwrap_or_else(|| panic!("executable must own a host"));

            assert!(
                !host
                    .requirements()
                    .requires_role(RuntimeAbiRole::MainThreadLaneStartup)
            );

            assert!(plan.product_host().is_none());

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
                generated_artifacts(&backend, &plan)
                    .iter()
                    .all(|artifact| !artifact.is_empty())
            );
        }
    }

    #[test]
    fn demanded_runtime_roles_cannot_disappear_from_product_validation() {
        let (_, compilation) = codegen_compilation(include_str!(
            "../../../../../../xtask/fixtures/native-execution/sync-panic.bray"
        ));

        let archive = TemporaryFile::write("libbray_runtime.a", b"!<arch>\n");

        let roles = RuntimeAbiRole::ALL
            .into_iter()
            .filter(|role| *role != RuntimeAbiRole::PanicPropagation);

        let runtime = runtime_artifact_with_roles(&compilation, archive.path(), roles);

        let product = test_product_identity();

        let result = compilation.native_product_plan(
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
                NativeProductPlanningError::InvalidExecutableHost(
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
            .product_semantics()
            .unwrap_or_else(|error| panic!("test product plan must resolve: {error:?}"));

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
        let mut saw_product_local_symbol = false;
        let primary_product = test_product_identity();

        let alternate_product =
            ProductIdentity::try_new(primary_product.package().clone(), "alternate-application")
                .unwrap_or_else(|| panic!("alternate test product identity must validate"));

        for unit in units.iter() {
            let mappings = compilation
                .codegen_mappings_for_product(
                    &primary_product,
                    unit,
                    None,
                    &BTreeSet::new(),
                    &target,
                    &reachability.graph().roots().iter().cloned().collect(),
                    &reachability,
                    false,
                    &cancellation,
                )
                .unwrap_or_else(|error| panic!("generic mappings must realize: {error:?}"));

            let alternate_mappings = compilation
                .codegen_mappings_for_product(
                    &alternate_product,
                    unit,
                    None,
                    &BTreeSet::new(),
                    &target,
                    &reachability.graph().roots().iter().cloned().collect(),
                    &reachability,
                    false,
                    &cancellation,
                )
                .unwrap_or_else(|error| {
                    panic!("alternate-product mappings must realize: {error:?}")
                });

            for symbol in mappings
                .symbols()
                .iter()
                .filter(|symbol| symbol.linkage() == CodegenLinkage::Internal)
            {
                let alternate = alternate_mappings
                    .symbols()
                    .iter()
                    .find(|candidate| candidate.key() == symbol.key())
                    .unwrap_or_else(|| panic!("alternate mapping must retain every local symbol"));

                assert_ne!(symbol.name(), alternate.name());
                saw_product_local_symbol = true;
            }

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
        assert!(saw_product_local_symbol);
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
            .product_semantics()
            .unwrap_or_else(|error| panic!("test product plan must resolve: {error:?}"));

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
            .product_semantics()
            .unwrap_or_else(|error| panic!("test product plan must resolve: {error:?}"));

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
    fn generic_method_arguments_specialize_fulfillments_and_trait_defaults() {
        for (required_parameter, provided_parameter, required_type, provided_type, argument) in [
            ("Value", "Output", "Value", "Output", "bool"),
            (
                "const COUNT: usize",
                "const LENGTH: usize",
                "bool",
                "bool",
                "3",
            ),
        ] {
            for default in [false, true] {
                let required_body = if default { "{ return value; }" } else { ";" };

                let fulfillment = if default {
                    String::new()
                } else {
                    format!(
                        r#"
                        func read<{provided_parameter}>(pos value: {provided_type}) -> {provided_type}
                        {{
                            return value;
                        }}
                        "#
                    )
                };

                let source = format!(
                    r#"
                    module app;
                    trait Reader
                    {{
                        func read<{required_parameter}>(pos value: {required_type}) -> {required_type} {required_body}
                    }}
                    struct ReaderValue
                    {{
                    }}
                    impl ReaderValue(Reader)
                    {{
                        {fulfillment}
                    }}
                    func apply<T>(pos reader: &T) -> bool with(T: Reader)
                    {{
                        return reader.read<{argument}>(true);
                    }}
                    public func root() -> bool
                    {{
                        let reader = ReaderValue
                        {{
                        }};
                        return apply<ReaderValue>(&reader);
                    }}
                    "#
                );

                let (backend, compilation) =
                    codegen_compilation_for_product(&source, ProductKind::Library);

                let diagnostics = compilation.check_diagnostics();

                assert!(!diagnostics.has_errors(), "{source}: {diagnostics:?}");

                let plan = compilation
                    .native_product_plan(
                        test_product_identity(),
                        crate::BuildConfiguration::Development,
                        None,
                        [],
                        None,
                    )
                    .unwrap();

                assert!(
                    generated_artifacts(&backend, &plan)
                        .iter()
                        .all(|artifact| !artifact.is_empty())
                );
            }
        }
    }

    #[test]
    fn trait_owned_default_bodies_specialize_with_the_selected_implementation() {
        let source = concat!(
            "module app;\n",
            "trait Counter\n",
            "{\n",
            "    func count() -> i32;\n",
            "    func doubled() -> i32\n",
            "    {\n",
            "        return self.count() + self.count();\n",
            "    }\n",
            "    func quadrupled() -> i32\n",
            "    {\n",
            "        return self.doubled() + self.doubled();\n",
            "    }\n",
            "}\n",
            "struct Value\n",
            "{\n",
            "    count: i32;\n",
            "}\n",
            "impl ValueCounter = Value(Counter)\n",
            "{\n",
            "    func count() -> i32\n",
            "    {\n",
            "        return self.count;\n",
            "    }\n",
            "}\n",
            "func read_generic<T>(pos value: &T) -> i32\n",
            "    with(T: Counter)\n",
            "{\n",
            "    return value.quadrupled();\n",
            "}\n",
            "public func read() -> i32\n",
            "{\n",
            "    let value: Value = { count = 3 };\n",
            "    return value.quadrupled() + read_generic<Value>(&value);\n",
            "}\n",
        );

        let (backend, compilation) = codegen_compilation_for_product(source, ProductKind::Library);

        let cancellation = CancellationToken::new();

        let target = compilation
            .selected_target()
            .target()
            .codegen_target()
            .unwrap_or_else(|error| panic!("test codegen target must validate: {error:?}"));

        let semantic = compilation.product_semantics().unwrap_or_else(|error| {
            panic!("trait default product semantics must resolve: {error:?}")
        });

        let roots = compilation
            .product_root_instances(semantic.value(), None, &target, &cancellation)
            .unwrap_or_else(|error| panic!("trait default roots must resolve: {error:?}"));

        compilation
            .codegen_reachability(roots, None, &target, &cancellation)
            .unwrap_or_else(|error| panic!("trait default reachability must close: {error:?}"));

        let plan = compilation
            .native_product_plan(
                test_product_identity(),
                crate::BuildConfiguration::Development,
                None,
                [],
                None,
            )
            .unwrap_or_else(|error| panic!("trait default body must realize: {error:?}"));

        assert!(
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }

    #[test]
    fn constrained_trait_owned_default_bodies_dispatch_required_members() {
        let source = concat!(
            "module app;\n",
            "trait Numeric\n",
            "{\n",
            "    func bounds() -> (Self, Self);\n",
            "    func increase(pos amount: Self) -> Self\n",
            "        with(Self: Add<Self>, Self(Add<Self>).Output == Self, Self: Copyable)\n",
            "    {\n",
            "        let bounds: (Self, Self) = self.bounds();\n",
            "        return identity<Self>(bounds.0 + amount);\n",
            "    }\n",
            "}\n",
            "impl I32Numeric = i32(Numeric)\n",
            "{\n",
            "    func bounds() -> (i32, i32)\n",
            "    {\n",
            "        return (1, 10);\n",
            "    }\n",
            "}\n",
            "func identity<Value>(pos value: Value) -> Value\n",
            "{\n",
            "    return value;\n",
            "}\n",
            "public func read() -> i32\n",
            "{\n",
            "    return 1.increase(2);\n",
            "}\n",
        );

        let (backend, compilation) = codegen_compilation_for_product(source, ProductKind::Library);

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
            .product_semantics()
            .unwrap_or_else(|error| panic!("constrained trait semantics must resolve: {error:?}"));

        let roots = compilation
            .product_root_instances(semantic.value(), None, &target, &cancellation)
            .unwrap_or_else(|error| panic!("constrained trait roots must resolve: {error:?}"));

        compilation
            .codegen_reachability(roots, None, &target, &cancellation)
            .unwrap_or_else(|error| panic!("constrained trait reachability must close: {error:?}"));

        let plan = compilation
            .native_product_plan(
                test_product_identity(),
                crate::BuildConfiguration::Development,
                None,
                [],
                None,
            )
            .unwrap_or_else(|error| {
                panic!("constrained trait default body must realize: {error:?}")
            });

        assert!(
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
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
    fn supported_atomic_generic_specializations_reach_codegen() {
        let compilation = crate::test_support::compilation(ATOMIC_GENERIC_SOURCE);
        let specializations = concrete_generic_specializations(&compilation);

        assert!(specializations.iter().any(|specialization| {
            matches!(specialization, CodegenSpecialization::Generic(arguments) if !arguments.is_empty())
        }));
    }

    #[test]
    fn library_atomic_roots_exclude_open_generic_wrappers() {
        let compilation = crate::test_support::compilation_with_product(
            ATOMIC_LIBRARY_SOURCE,
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
            .product_semantics()
            .unwrap_or_else(|error| panic!("test product plan must resolve: {error:?}"));

        let roots = compilation
            .product_root_instances(semantic.value(), None, &target, &cancellation)
            .unwrap_or_else(|error| panic!("test roots must resolve: {error:?}"));

        assert!(roots.is_empty());
    }

    #[test]
    fn atomic_fence_wrapper_emits_valid_native_units() {
        let (backend, compilation) =
            codegen_compilation_for_product(ATOMIC_FENCE_SOURCE, ProductKind::Library);

        let plan = compilation
            .native_product_plan(
                test_product_identity(),
                crate::BuildConfiguration::Development,
                None,
                [],
                None,
            )
            .unwrap_or_else(|error| panic!("atomic fence plan must resolve: {error:?}"));

        assert!(
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }

    #[test]
    fn imported_callable_contract_adaptations_round_trip_in_executable_templates() {
        let dependency = generic_dependency_from_fixture(
            true,
            false,
            GenericDependencyFixture {
                source: r#"
                module templates;

                public func weaken<T>(operation: func(pos value: T) -> T
                        executes(pure, total)
                    ) -> func(pos value: T) -> T
                    {
                        return operation;
                    }
                "#,
                runtime_frames: None,
                executable_templates: 1,
                platform_service: None,
            },
        );

        let compilation = generic_consumer_for_target_with_source(
            dependency,
            SelectedTarget::baseline(),
            r#"
            module application;

            using example.dependency.templates.weaken;

            func main()
            {
                let operation = lambda(pos value: i32) -> i32
                    executes(pure, total)
                {
                    return value;
                };

                let erased = example.dependency.templates.weaken<i32>(operation = operation);
                let result = erased(1);
            }
            "#,
        );

        assert_eq!(concrete_generic_specializations(&compilation).len(), 1);
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
            .product_semantics()
            .unwrap_or_else(|error| panic!("consumer product plan must resolve: {error:?}"));

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

        assert_eq!(imported.len(), 3);

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

        realize_codegen_mappings(&compilation, &target, &reachability, &cancellation);
    }

    #[test]
    fn imported_platform_service_templates_retain_their_role_during_specialization() {
        let role = PlatformServiceRole::StandardOutputFlush;

        let fixture = GenericDependencyFixture {
            source: r#"trusted module templates;

@layout(c)
internal struct PlatformStatus
{
    category: u32;
    reserved: u32;
    native_code: i64;
}

@abi(c)
trusted internal func flush() -> PlatformStatus
{
    return { category = 0, reserved = 0, native_code = 0 };
}

public func invoke<T>(pos value: T)
{
    let _: PlatformStatus = trusted flush();
}
"#,
            runtime_frames: None,
            executable_templates: 2,
            platform_service: Some((role, "templates.flush")),
        };

        let compilation = generic_consumer_for_target_with_source(
            generic_dependency_from_fixture(true, false, fixture),
            SelectedTarget::baseline(),
            concat!(
                "module application;\n",
                "using example.dependency.templates.invoke;\n",
                "func main() { example.dependency.templates.invoke<i32>(1); }\n",
            ),
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
            .unwrap_or_else(|error| panic!("consumer target must validate: {error:?}"));

        let semantic = compilation
            .product_semantics()
            .unwrap_or_else(|error| panic!("consumer product plan must resolve: {error:?}"));

        let roots = compilation
            .product_root_instances(semantic.value(), None, &target, &cancellation)
            .unwrap_or_else(|error| panic!("consumer roots must resolve: {error:?}"));

        let reachability = compilation
            .codegen_reachability(roots, None, &target, &cancellation)
            .unwrap_or_else(|error| panic!("consumer reachability must close: {error:?}"));

        let roles = reachability
            .graph()
            .instances()
            .iter()
            .filter_map(|instance| match instance.key().template() {
                MirUnitKey::ImportedExecutable(key) => key.platform_service(),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(roles, [role]);

        realize_codegen_mappings(&compilation, &target, &reachability, &cancellation);
    }

    #[test]
    fn scalar_comparison_callable_is_lowered_as_native_intrinsic() {
        assert_source_emits_valid_native_units(
            SCALAR_COMPARISON_CALL_SOURCE,
            crate::BuildConfiguration::Development,
        );
    }

    #[test]
    fn imported_trait_default_bodies_specialize_with_the_consumer_implementation() {
        let compilation = generic_consumer_for_target_with_source(
            trait_default_dependency(),
            SelectedTarget::baseline(),
            TRAIT_DEFAULT_CONSUMER_SOURCE,
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
            .unwrap_or_else(|error| panic!("consumer target must validate: {error:?}"));

        let semantic = compilation
            .product_semantics()
            .unwrap_or_else(|error| panic!("consumer product plan must resolve: {error:?}"));

        let roots = compilation
            .product_root_instances(semantic.value(), None, &target, &cancellation)
            .unwrap_or_else(|error| panic!("consumer roots must resolve: {error:?}"));

        let reachability = compilation
            .codegen_reachability(roots, None, &target, &cancellation)
            .unwrap_or_else(|error| panic!("consumer reachability must close: {error:?}"));

        let imported_defaults = reachability
            .graph()
            .instances()
            .iter()
            .filter(|instance| {
                matches!(instance.key().template(), MirUnitKey::ImportedExecutable(_))
                    && instance.key().contextual_self_witness().is_some()
            })
            .count();

        assert_eq!(imported_defaults, 4);

        realize_codegen_mappings(&compilation, &target, &reachability, &cancellation);
    }

    #[test]
    fn imported_generic_template_families_merge_protected_frame_requirements() {
        let compilation = async_generic_consumer(generic_async_dependency());

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        let address = imported_nested_template_address(&compilation).symbol();

        let runtime = compilation
            .imported_semantics(ImportedSemanticRecordKey::new(
                address.interface(),
                address.symbol(),
                InterfaceSemanticRecordKind::Runtime,
            ))
            .unwrap_or_else(|error| panic!("runtime requirement must resolve: {error:?}"));

        let [ImportedSemanticRecord::Runtime(runtime)] = runtime.value().as_ref() else {
            panic!("generic callable must import one family runtime requirement");
        };

        assert_eq!(runtime.frames().len(), 2);
        assert!(runtime.requirements().frame_abi().is_some());

        let target = compilation
            .selected_target()
            .target()
            .codegen_target()
            .unwrap_or_else(|error| panic!("consumer target must validate: {error:?}"));

        let semantic = compilation
            .product_semantics()
            .unwrap_or_else(|error| panic!("consumer product plan must resolve: {error:?}"));

        let roots = compilation
            .product_root_instances(
                semantic.value(),
                None,
                &target,
                &compilation.state.cancellation,
            )
            .unwrap_or_else(|error| panic!("consumer roots must resolve: {error:?}"));

        let reachability = compilation
            .codegen_reachability(roots, None, &target, &compilation.state.cancellation)
            .unwrap_or_else(|error| panic!("consumer reachability must close: {error:?}"));

        let frames = reachability
            .graph()
            .instances()
            .iter()
            .filter(|instance| {
                matches!(instance.key().template(), MirUnitKey::ImportedExecutable(_))
            })
            .filter_map(|instance| instance.mir().frame_descriptor())
            .map(bray_ir::MirFrameDescriptor::frame)
            .collect::<BTreeSet<_>>();

        assert_eq!(frames.iter().copied().collect::<Vec<_>>(), runtime.frames());
    }

    #[test]
    fn imported_generic_templates_are_shared_by_concurrent_requests() {
        let compilation = generic_consumer(generic_dependency(true));
        let address = imported_nested_template_address(&compilation);

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
                crate::fact::ImportedExecutableTemplateAddress::root(
                    first_imported_function_address(&compilation),
                ),
                &compilation.state.cancellation,
            )
            .unwrap_or_else(|error| panic!("target mismatch must be diagnosed: {error:?}"));

        assert!(result.value().is_none());

        assert_eq!(
            result
                .diagnostics()
                .by_kind(bray_diagnostics::DiagnosticKind::InterfaceValidationFailed)
                .count(),
            1
        );
    }

    #[test]
    fn malformed_imported_executable_templates_publish_dependency_diagnostics() {
        let compilation = generic_consumer(generic_dependency_with_templates(true, true));

        let result = compilation
            .imported_executable_template_with_cancellation(
                crate::fact::ImportedExecutableTemplateAddress::root(
                    first_imported_function_address(&compilation),
                ),
                &compilation.state.cancellation,
            )
            .unwrap_or_else(|error| panic!("malformed template must be diagnosed: {error:?}"));

        assert!(result.value().is_none());

        assert_eq!(
            result
                .diagnostics()
                .by_kind(bray_diagnostics::DiagnosticKind::InterfaceValidationFailed)
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
            .product_semantics()
            .unwrap_or_else(|error| panic!("consumer product plan must resolve: {error:?}"));

        let roots = compilation
            .product_root_instances(semantic.value(), None, &target, &cancellation)
            .unwrap_or_else(|error| panic!("consumer roots must resolve: {error:?}"));

        let error = match compilation.codegen_reachability(roots, None, &target, &cancellation) {
            Ok(_) => panic!("missing imported templates must stop code generation reachability"),
            Err(error) => error,
        };

        let NativeProductPlanningError::Codegen(CodegenPreparationError::Diagnostics(diagnostics)) =
            error
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
            ConcreteCodegenInstance::try_callable(
                root.key().clone(),
                root.callable_instance()
                    .unwrap_or_else(|| panic!("test root must retain its callable")),
                CodegenSpecialization::NonGeneric,
                [],
                None,
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

    #[test]
    fn replacement_cleanup_helpers_emit_checked_outcomes() {
        for lifecycle in ["destruct() {}", "finalize() {} destruct() {}"] {
            let source = format!(
                "module app; struct Resource {{ value: i32; {lifecycle} }} \
                 func main() {{ let mut value: Resource = Resource {{ value = 1 }}; \
                 value = Resource {{ value = 2 }}; }}"
            );

            let (backend, plan) = runtime_native_plan(&source);

            for mir in plan
                .units()
                .iter()
                .flat_map(bray_codegen::CodegenUnit::mir_units)
            {
                if !matches!(mir.key(), MirUnitKey::GeneratedLifecycle(_)) {
                    continue;
                }

                for block in mir.blocks() {
                    let Some(operation) =
                        block.operations().last().and_then(|id| mir.operation(*id))
                    else {
                        continue;
                    };

                    if matches!(operation.kind(), bray_ir::MirOperationKind::Call(call) if call.may_propagate_panic())
                    {
                        assert!(matches!(
                            block.terminator().kind(),
                            bray_ir::MirTerminatorKind::CheckCallOutcome { .. }
                        ));
                    }
                }
            }

            for symbol in plan
                .mappings()
                .iter()
                .flat_map(bray_codegen::CodegenMappings::symbols)
            {
                if matches!(symbol.key(), bray_codegen::CodegenSymbolKey::Instance(instance)
                    if matches!(instance.template(), MirUnitKey::GeneratedLifecycle(_)))
                {
                    assert!(symbol.signature().has_panic_report_context());
                }
            }

            assert!(!generated_artifacts(&backend, &plan).is_empty());
        }
    }

    #[test]
    fn library_source_bodies_are_not_skipped_by_compiler_known_names() {
        for module in ["app", "std.memory"] {
            let source = format!("module {module}; func allocate() -> i32 {{ return 3; }}");

            let (backend, plan) = runtime_native_plan_for_product(&source, ProductKind::Library);

            assert!(!plan.units().is_empty());
            assert!(!generated_artifacts(&backend, &plan).is_empty());
        }
    }

    fn runtime_native_plan(
        source: &str,
    ) -> (
        Arc<bray_codegen_llvm::LlvmCodeGenerator>,
        Arc<super::NativeProductPlan>,
    ) {
        runtime_native_plan_for_product(source, ProductKind::Executable)
    }

    fn runtime_native_plan_for_product(
        source: &str,
        product_kind: ProductKind,
    ) -> (
        Arc<bray_codegen_llvm::LlvmCodeGenerator>,
        Arc<super::NativeProductPlan>,
    ) {
        runtime_native_plan_for_target(source, product_kind, SelectedTarget::baseline())
    }

    fn runtime_native_plan_for_target(
        source: &str,
        product_kind: ProductKind,
        target: SelectedTarget,
    ) -> (
        Arc<bray_codegen_llvm::LlvmCodeGenerator>,
        Arc<super::NativeProductPlan>,
    ) {
        runtime_native_plan_for_sources_target(&[source], product_kind, target, &[])
    }

    fn runtime_native_plan_for_sources_target(
        sources: &[&str],
        product_kind: ProductKind,
        target: SelectedTarget,
        native_link_inputs: &[NativeLinkRequirement],
    ) -> (
        Arc<bray_codegen_llvm::LlvmCodeGenerator>,
        Arc<super::NativeProductPlan>,
    ) {
        runtime_native_plan_for_sources_target_with_platform_services(
            sources,
            product_kind,
            target,
            native_link_inputs,
            [],
        )
    }

    fn runtime_native_plan_for_sources_target_with_platform_services(
        sources: &[&str],
        product_kind: ProductKind,
        target: SelectedTarget,
        native_link_inputs: &[NativeLinkRequirement],
        platform_services: impl IntoIterator<Item = PlatformServiceBinding>,
    ) -> (
        Arc<bray_codegen_llvm::LlvmCodeGenerator>,
        Arc<super::NativeProductPlan>,
    ) {
        runtime_native_plan_for_sources_target_with_platform_overrides(
            sources,
            product_kind,
            target,
            native_link_inputs,
            platform_services,
            [],
        )
    }

    fn runtime_native_plan_for_sources_target_with_platform_overrides(
        sources: &[&str],
        product_kind: ProductKind,
        target: SelectedTarget,
        native_link_inputs: &[NativeLinkRequirement],
        platform_services: impl IntoIterator<Item = PlatformServiceBinding>,
        runtime_platform_services: impl IntoIterator<Item = PlatformServiceRole>,
    ) -> (
        Arc<bray_codegen_llvm::LlvmCodeGenerator>,
        Arc<super::NativeProductPlan>,
    ) {
        let (backend, compilation) = codegen_compilation_for_sources_target_with_platform_services(
            sources,
            product_kind,
            target,
            native_link_inputs,
            platform_services,
        );

        let archive = TemporaryFile::write("libbray_runtime.a", b"!<arch>\n");

        let runtime = runtime_artifact_with_roles_and_platform_services(
            &compilation,
            archive.path(),
            RuntimeAbiRole::ALL,
            runtime_platform_services,
        );

        let product = test_product_identity();

        let plan = compilation
            .native_product_plan(
                product,
                crate::BuildConfiguration::Development,
                Some(runtime),
                [
                    RuntimeCapability::CooperativeExecution,
                    RuntimeCapability::MainThreadLane,
                ],
                Some(&test_linker()),
            )
            .unwrap_or_else(|error| panic!("runtime native plan must resolve: {error:?}"));

        (backend, plan)
    }

    fn runtime_native_plan_with_source_roles(
        sources: &[&str],
        product_kind: ProductKind,
        runtime_roles: impl IntoIterator<Item = bray_runtime_interface::RuntimeRoleSourceBinding>,
    ) -> (
        Arc<bray_codegen_llvm::LlvmCodeGenerator>,
        Arc<super::NativeProductPlan>,
    ) {
        let (backend, compilation) = codegen_compilation_for_sources_target_with_source_roles(
            sources,
            product_kind,
            SelectedTarget::baseline(),
            &[],
            [],
            runtime_roles,
            WorkerBudget::serial(),
        );

        let archive = TemporaryFile::write("libbray_runtime.a", b"!<arch>\n");
        let runtime = runtime_artifact(&compilation, archive.path());

        let plan = compilation
            .native_product_plan(
                test_product_identity(),
                crate::BuildConfiguration::Development,
                Some(runtime),
                [],
                Some(&test_linker()),
            )
            .unwrap_or_else(|error| panic!("runtime source-role plan must resolve: {error:?}"));

        (backend, plan)
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

        let archive = TemporaryFile::write("libbray_runtime.a", b"!<arch>\n");
        let runtime = runtime_artifact(&compilation, archive.path());

        let plan = compilation
            .native_product_plan(
                test_product_identity(),
                crate::BuildConfiguration::Development,
                Some(runtime),
                [
                    RuntimeCapability::CooperativeExecution,
                    RuntimeCapability::MainThreadLane,
                ],
                Some(&test_linker()),
            )
            .unwrap_or_else(|error| panic!("native test plan must resolve: {error:?}"));

        let catalog = plan
            .test_catalog()
            .unwrap_or_else(|| panic!("native test plan must retain their catalog"));

        assert_eq!(
            catalog
                .entries()
                .iter()
                .map(|entry| entry.identity().declaration().name().as_str())
                .collect::<Vec<_>>(),
            ["alpha", "beta", "gamma"]
        );

        let host = plan
            .executable_host()
            .unwrap_or_else(|| panic!("native test plan must retain their host"));

        assert_eq!(host.entries().len(), catalog.entries().len());
        assert_eq!(host.entries()[0].root(), RootExecution::Synchronous);
        assert_eq!(host.entries()[1].root(), RootExecution::Synchronous);

        assert!(matches!(
            host.entries()[2].root(),
            RootExecution::Asynchronous { .. }
        ));

        let host_mir = plan
            .units()
            .iter()
            .flat_map(bray_codegen::CodegenUnit::mir_units)
            .find(|unit| matches!(unit.kind(), MirUnitKind::ExecutableHost(_)))
            .unwrap_or_else(|| panic!("native test plan must retain generated host MIR"));

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
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }

    #[test]
    fn empty_native_test_products_emit_a_successful_host() {
        let (backend, compilation) =
            codegen_compilation_for_product("module app.tests;\n", ProductKind::Test);

        let plan = compilation
            .native_product_plan(
                test_product_identity(),
                crate::BuildConfiguration::Development,
                None,
                [],
                Some(&test_linker()),
            )
            .unwrap_or_else(|error| panic!("empty native test plan must resolve: {error:?}"));

        let catalog = plan
            .test_catalog()
            .unwrap_or_else(|| panic!("empty native test plan must retain their catalog"));

        assert!(catalog.entries().is_empty());

        let host = plan
            .executable_host()
            .unwrap_or_else(|| panic!("empty native test plan must retain their host"));

        assert!(host.entries().is_empty());

        let host_mir = plan
            .units()
            .iter()
            .flat_map(bray_codegen::CodegenUnit::mir_units)
            .find(|unit| matches!(unit.kind(), MirUnitKind::ExecutableHost(_)))
            .unwrap_or_else(|| panic!("empty native test plan must retain generated host MIR"));

        assert!(matches!(
            host_mir.operations(),
            [begin, report, shutdown]
                if matches!(
                    begin.kind(),
                    MirOperationKind::Host(MirHostOperation::BeginStaticCleanup)
                ) && matches!(
                    report.kind(),
                    MirOperationKind::Host(MirHostOperation::ReportCleanupIncidents { .. })
                ) && matches!(
                    shutdown.kind(),
                    MirOperationKind::Host(MirHostOperation::StructuredShutdown { .. })
                )
        ));

        assert!(
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }

    #[test]
    fn generic_static_instances_have_distinct_realizations_and_one_host_owner() {
        let source = concat!(
            "module app;\n",
            "\n",
            "static GENERIC_VALUE<const N: i32>: i32 = N;\n",
            "\n",
            "func main() -> i32\n",
            "{\n",
            "    return GENERIC_VALUE<1> + GENERIC_VALUE<2> - 3;\n",
            "}\n",
        );

        let (backend, plan) = runtime_native_plan(source);

        let static_mappings = plan
            .mappings()
            .iter()
            .flat_map(bray_codegen::CodegenMappings::static_storages)
            .collect::<Vec<_>>();

        assert!(static_mappings.len() >= 4);

        let instances = static_mappings
            .iter()
            .map(|mapping| mapping.instance())
            .collect::<BTreeSet<_>>();

        let symbols = static_mappings
            .iter()
            .map(|mapping| mapping.symbol())
            .collect::<BTreeSet<_>>();

        assert_eq!(instances.len(), 2);
        assert_eq!(symbols.len(), instances.len());

        let product_instances = instances
            .iter()
            .filter(|instance| instance.duration() == StaticStorageDuration::Product)
            .count();

        let thread_instances = instances
            .iter()
            .filter(|instance| instance.duration() == StaticStorageDuration::ExactThread)
            .count();

        assert_eq!(product_instances, 2);
        assert_eq!(thread_instances, 0);

        let host = plan
            .units()
            .iter()
            .flat_map(bray_codegen::CodegenUnit::mir_units)
            .find(|unit| matches!(unit.kind(), MirUnitKind::ExecutableHost(_)))
            .unwrap_or_else(|| panic!("static native plan must retain host MIR"));

        assert_eq!(
            host.operations()
                .iter()
                .filter(|operation| matches!(
                    operation.kind(),
                    MirOperationKind::Host(MirHostOperation::MaterializeStatic { .. })
                ))
                .count(),
            product_instances
        );

        assert_eq!(
            host.operations()
                .iter()
                .filter(|operation| matches!(
                    operation.kind(),
                    MirOperationKind::Cleanup {
                        phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                        ..
                    }
                ))
                .count(),
            0,
            "the runtime product owner must execute static callbacks"
        );

        assert!(
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }

    #[test]
    fn exact_thread_static_emits_attachment_owned_cleanup() {
        let source = concat!(
            "module app;\n",
            "@thread_local static THREAD_ANSWER: i32 = 42;\n",
            "func main() -> i32\n",
            "{\n",
            "    return THREAD_ANSWER;\n",
            "}\n",
        );

        let (backend, plan) = runtime_native_plan(source);

        let mappings = plan
            .mappings()
            .iter()
            .flat_map(bray_codegen::CodegenMappings::static_storages)
            .filter(|mapping| mapping.instance().duration() == StaticStorageDuration::ExactThread)
            .collect::<Vec<_>>();

        assert!(!mappings.is_empty());

        assert!(
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }

    #[test]
    fn repeated_static_references_emit_one_native_instance() {
        let source = concat!(
            "module app;\n",
            "static ANSWER: i32 = 42;\n",
            "func answer_address() -> RawPointer<i32>\n",
            "{\n",
            "    return core.memory.address_of<i32>(&ANSWER);\n",
            "}\n",
            "func main() -> i32\n",
            "{\n",
            "    let first = core.memory.address_of<i32>(&ANSWER);\n",
            "    let second = answer_address();\n",
            "    return ANSWER;\n",
            "}\n",
        );

        let (backend, plan) = runtime_native_plan(source);

        assert_eq!(
            plan.mappings()
                .iter()
                .flat_map(bray_codegen::CodegenMappings::static_storages)
                .map(|mapping| mapping.instance().clone())
                .collect::<BTreeSet<_>>()
                .len(),
            1
        );

        assert!(
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }

    #[test]
    fn const_generic_static_specializations_emit_distinct_native_instances() {
        let source = concat!(
            "module app;\n",
            "@thread_local static VALUE<const N: u64>: u64 = N;\n",
            "func values() -> (u64, u64)\n",
            "{\n",
            "    return(VALUE<7>, VALUE<11>);\n",
            "}\n",
            "func main() -> i32\n",
            "{\n",
            "    let _: (u64, u64) = values();\n",
            "    return 0;\n",
            "}\n",
        );

        let (_, plan) = runtime_native_plan_for_target(
            source,
            ProductKind::Executable,
            SelectedTarget::for_native(NativeTarget::X86_64WindowsMsvc),
        );

        let mappings = plan
            .mappings()
            .iter()
            .flat_map(bray_codegen::CodegenMappings::static_storages)
            .collect::<Vec<_>>();

        assert_eq!(
            mappings
                .iter()
                .map(|mapping| mapping.instance())
                .collect::<BTreeSet<_>>()
                .len(),
            2
        );

        assert_eq!(
            mappings
                .iter()
                .map(|mapping| mapping.symbol())
                .collect::<BTreeSet<_>>()
                .len(),
            2
        );

        assert_eq!(
            mappings
                .iter()
                .map(|mapping| mapping.initial_value())
                .collect::<BTreeSet<_>>()
                .len(),
            2
        );
    }

    #[test]
    fn tuple_destructuring_projects_each_initializer_field_once() {
        let (_, compilation) = codegen_compilation_for_product(
            concat!(
                "module app;\n",
                "public func sum(pos value: (u64, u64)) -> u64\n",
                "{\n",
                "    let(first, second) = value;\n",
                "    return first + second;\n",
                "}\n",
            ),
            ProductKind::Library,
        );

        let lowered = compilation
            .lowered_unit(crate::test_support::source_function_body_key(
                &compilation,
                "sum",
            ))
            .unwrap_or_else(|error| panic!("tuple destructuring must lower: {error:?}"));

        let mir = lowered
            .value()
            .as_ref()
            .and_then(bray_lowering::LoweredUnit::mir)
            .unwrap_or_else(|| panic!("tuple destructuring must produce MIR: {lowered:#?}"));

        let fields = mir
            .operations()
            .iter()
            .filter_map(|operation| match operation.kind() {
                MirOperationKind::Store {
                    value: MirOperand::Copy(place),
                    ..
                } => match place.projections() {
                    [projection] => match projection.kind() {
                        MirProjectionKind::TupleField(field) => Some(*field),
                        _ => None,
                    },
                    _ => None,
                },
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(fields, [0, 1]);
    }

    #[test]
    fn public_library_static_contributes_a_linked_host_table_entry() {
        let source = concat!(
            "module app;\n",
            "\n",
            "public static EXPORTED_VALUE: i32 = 42;\n",
        );

        let (backend, compilation) = codegen_compilation_for_product(source, ProductKind::Library);

        let plan = compilation
            .native_product_plan(
                test_product_identity(),
                crate::BuildConfiguration::Development,
                None,
                [],
                Some(&test_linker()),
            )
            .unwrap_or_else(|error| panic!("static library plan must resolve: {error:?}"));

        let mappings = plan
            .mappings()
            .iter()
            .flat_map(bray_codegen::CodegenMappings::static_storages)
            .collect::<Vec<_>>();

        assert_eq!(mappings.len(), 1);

        assert_eq!(
            mappings[0].instance().duration(),
            StaticStorageDuration::Product
        );

        assert_eq!(plan.static_instances(), [mappings[0].instance().clone()]);

        let marker = b"bray.static.host.";

        assert!(generated_artifacts(&backend, &plan).iter().any(|artifact| {
            artifact
                .windows(marker.len())
                .any(|candidate| candidate == marker)
        }));
    }

    #[test]
    fn private_lifecycle_static_is_a_retained_product_root() {
        let source = concat!(
            "module app;\n",
            "internal struct Resource {}\n",
            "impl Resource\n",
            "{\n",
            "    finalize()\n",
            "    {\n",
            "    }\n",
            "}\n",
            "internal static HIDDEN_RESOURCE: Resource = Resource {};\n",
            "func main()\n",
            "{\n",
            "}\n",
        );

        let compilation =
            crate::test_support::compilation_with_product(source, ProductKind::Executable);

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
            .product_semantics()
            .unwrap_or_else(|error| panic!("test product plan must resolve: {error:?}"));

        let roots = compilation
            .product_root_instances(semantic.value(), None, &target, &cancellation)
            .unwrap_or_else(|error| panic!("test roots must resolve: {error:?}"));

        assert_eq!(roots.len(), 2);
    }

    #[test]
    fn static_mapping_retains_selected_lifecycle_helpers() {
        let source = concat!(
            "module app;\n",
            "struct Resource { mut state: i32; }\n",
            "impl Resource\n",
            "{\n",
            "    finalize() { self.state = 2; }\n",
            "    destruct() { self.state = 3; }\n",
            "}\n",
            "static RESOURCE: Resource = Resource { state = 1 };\n",
            "func main() {}\n",
        );

        let (backend, plan) = runtime_native_plan(source);

        let mapping = plan
            .mappings()
            .iter()
            .flat_map(bray_codegen::CodegenMappings::static_storages)
            .find(|mapping| mapping.finalization().is_some() || mapping.destroy().is_some())
            .unwrap_or_else(|| panic!("lifecycle-bearing static mapping must be retained"));

        assert!(mapping.finalization().is_some());
        assert!(mapping.destroy().is_some());

        let lifecycle_symbols = plan
            .mappings()
            .iter()
            .flat_map(bray_codegen::CodegenMappings::symbols)
            .filter(|symbol| {
                matches!(
                    symbol.key(),
                    bray_codegen::CodegenSymbolKey::Instance(instance)
                        if matches!(instance.template(), MirUnitKey::GeneratedLifecycle(_))
                )
            })
            .collect::<Vec<_>>();

        assert!(!lifecycle_symbols.is_empty());

        assert!(
            lifecycle_symbols
                .iter()
                .any(|symbol| symbol.linkage() == CodegenLinkage::LinkOnce)
        );

        assert!(lifecycle_symbols.iter().all(|symbol| {
            matches!(
                symbol.linkage(),
                CodegenLinkage::LinkOnce | CodegenLinkage::Import
            )
        }));

        assert!(!generated_artifacts(&backend, &plan).is_empty());
    }

    #[test]
    fn static_host_orders_dependencies_reached_only_by_finalization() {
        let source = concat!(
            "module app;\n",
            "struct Provider { mut state: i32; }\n",
            "impl Provider\n",
            "{\n",
            "    finalize() {}\n",
            "}\n",
            "static PROVIDER: Provider = Provider { state = 1 };\n",
            "struct Consumer {}\n",
            "impl Consumer\n",
            "{\n",
            "    finalize() { if PROVIDER.state == 1 {} }\n",
            "}\n",
            "static CONSUMER: Consumer = Consumer {};\n",
            "func main() {}\n",
        );

        let (_, plan) = runtime_native_plan(source);

        let host = plan
            .product_host()
            .unwrap_or_else(|| panic!("lifecycle-bearing statics must retain a product host"));

        let consumer = host
            .statics()
            .iter()
            .find(|entry| !entry.dependencies().is_empty())
            .unwrap_or_else(|| panic!("finalizer-only static dependency must be retained"));

        let [provider] = consumer.dependencies() else {
            panic!("consumer must retain exactly one finalizer-only provider");
        };

        let provider = host
            .statics()
            .iter()
            .find(|entry| entry.identity() == *provider)
            .unwrap_or_else(|| panic!("provider must remain in the product host"));

        assert!(consumer.order() < provider.order());
    }

    #[test]
    fn concrete_generic_cleanup_selects_suspension_after_substitution() {
        for (ty, value) in [
            ("Guard", "Guard {}"),
            ("[Guard; 2]", "[Guard {}, Guard {}]"),
            (
                "[[Guard; 2]; 2]",
                "[[Guard {}, Guard {}], [Guard {}, Guard {}]]",
            ),
        ] {
            for asynchronous in [false, true] {
                let execution = if asynchronous { "async" } else { "" };

                let source = format!(
                    r#"
                module app;
                struct Guard
                {{
                    {execution} finalize()
                    {{
                    }}
                }}
                async func dispose<T>(pos value: T) -> i32
                {{
                    return 7;
                }}
                async func main()
                {{
                    await dispose<{ty}>({value});
                }}
                "#
                );

                let (backend, plan) = runtime_native_plan(&source);

                let instances = plan
                    .units()
                    .iter()
                    .flat_map(bray_codegen::CodegenUnit::instances)
                    .filter(|instance| {
                        matches!(instance.key().template(), bray_ir::MirUnitKey::Bound(_))
                            && !instance.key().specialization().arguments().is_empty()
                    })
                    .collect::<Vec<_>>();

                assert_eq!(instances.len(), 1);

                let mir = instances[0].mir();
                let frame = mir.frame_descriptor().unwrap();

                let suspended = mir.blocks().iter().any(|block| {
                    matches!(
                        block.terminator().kind(),
                        bray_ir::MirTerminatorKind::Suspend { .. }
                    )
                });

                assert_eq!(suspended, asynchronous);
                assert_eq!(frame.states().len() > 1, asynchronous);
                assert!(frame.inactive_cleanup().is_some());

                let pending = mir
                    .storages_with_ids()
                    .find_map(|(id, storage)| {
                        (matches!(storage.kind(), bray_ir::MirStorageKind::Temporary)
                            && storage.ty() == frame.result_type())
                        .then_some(id)
                    })
                    .expect("generic return must retain its value during cleanup");

                if asynchronous {
                    assert_cleanup_storage_retained(mir, pending);

                    if ty.starts_with('[') {
                        let source_counters = mir
                            .operations()
                            .iter()
                            .filter_map(|operation| match operation.kind() {
                                bray_ir::MirOperationKind::Binary {
                                    operator: bray_ir::MirBinaryOperator::Subtract,
                                    left: bray_ir::MirOperand::Copy(counter),
                                    ..
                                } => Some(counter.storage()),
                                _ => None,
                            })
                            .collect::<Vec<_>>();

                        assert!(
                            !source_counters.is_empty(),
                            "specialization must compose finite arrays inside the source frame"
                        );

                        for counter in source_counters {
                            assert_cleanup_storage_retained(mir, counter);
                        }
                    }
                }

                assert!(
                    generated_artifacts(&backend, &plan)
                        .iter()
                        .all(|artifact| !artifact.is_empty())
                );
            }
        }
    }

    #[test]
    fn generic_partial_array_cleanup_retains_loop_counters() {
        let source = r#"
            module app;
            struct Guard
            {
                async finalize() {}
                destruct() {}
            }
            async func take<T>(pos value: T) {}
            async func partial<T>(pos values: [[T; 2]; 2], pos index: usize) -> i32
            {
                await take<T>(values[index][0]);
                return 7;
            }
            async func main()
            {
                await partial<Guard>([[Guard {}, Guard {}], [Guard {}, Guard {}]], 1);
            }
        "#;

        let (backend, plan) = runtime_native_plan(source);

        let retains_nested_counters = plan
            .units()
            .iter()
            .flat_map(bray_codegen::CodegenUnit::instances)
            .filter(|instance| {
                matches!(instance.key().template(), bray_ir::MirUnitKey::Bound(_))
                    && !instance.key().specialization().arguments().is_empty()
            })
            .any(|instance| {
                let mir = instance.mir();

                let counters = mir
                    .operations()
                    .iter()
                    .filter_map(|operation| match operation.kind() {
                        bray_ir::MirOperationKind::Binary {
                            operator: bray_ir::MirBinaryOperator::Subtract,
                            left: bray_ir::MirOperand::Copy(counter),
                            ..
                        } => Some(counter.storage()),
                        _ => None,
                    })
                    .collect::<std::collections::BTreeSet<_>>();

                mir.frame_descriptor()
                    .unwrap()
                    .states()
                    .iter()
                    .any(|state| {
                        state.state().raw() != 0
                            && state
                                .execution()
                                .retained_storages()
                                .iter()
                                .filter(|storage| counters.contains(storage))
                                .count()
                                >= 2
                    })
            });

        assert!(
            retains_nested_counters,
            "nested cleanup must retain both active array counters at suspension"
        );

        assert!(
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }

    fn assert_cleanup_storage_retained(mir: &bray_ir::MirUnit, storage: bray_ir::MirStorageId) {
        assert!(
            mir.frame_descriptor()
                .unwrap()
                .states()
                .iter()
                .any(|state| {
                    state.state().raw() != 0
                        && state.execution().retained_storages().contains(&storage)
                }),
            "cleanup storage {storage:?} must survive a specialized suspension"
        );
    }

    #[test]
    fn generic_owner_cleanup_reuses_existing_storage_after_substitution() {
        for owner_kind in ["Future", "Task"] {
            for (completion, value) in [
                ("i32", "7"),
                ("Future<i32>", "integer()"),
                ("Task<i32>", "integer().start()"),
                ("Guard", "Guard {}"),
            ] {
                let start = if owner_kind == "Task" { ".start()" } else { "" };

                let source = format!(
                    r#"
                module app;
                struct Guard {{ async finalize() {{}} }}
                async func integer() -> i32 {{ return 7; }}
                async func produce() -> {completion} {{ return {value}; }}
                async func dispose<T>(pos value: T) {{}}
                async func main()
                {{
                    await dispose<{owner_kind}<{completion}>>(produce(){start});
                }}
                "#
                );

                let (backend, plan) = runtime_native_plan(&source);

                let instances = plan
                    .units()
                    .iter()
                    .flat_map(bray_codegen::CodegenUnit::instances)
                    .filter(|instance| {
                        matches!(instance.key().template(), bray_ir::MirUnitKey::Bound(_))
                            && !instance.key().specialization().arguments().is_empty()
                    })
                    .collect::<Vec<_>>();

                assert_eq!(instances.len(), 1);

                let mir = instances[0].mir();

                let owner = mir
                    .storages()
                    .iter()
                    .find(|storage| matches!(storage.kind(), bray_ir::MirStorageKind::Parameter(0)))
                    .unwrap()
                    .ty();

                if owner_kind == "Task" {
                    assert!(
                        mir.blocks().iter().any(|block| matches!(
                            block.terminator().kind(),
                            bray_ir::MirTerminatorKind::Suspend {
                                kind: bray_ir::MirSuspensionKind::TaskCompletion,
                                ..
                            }
                        )),
                        "specialized Task<{completion}> must wait directly"
                    );

                    assert!(
                        mir.operations().iter().any(|operation| matches!(
                            operation.kind(),
                            bray_ir::MirOperationKind::Async(
                                bray_ir::MirAsyncOperation::BorrowTaskCompletion { .. }
                            )
                        )),
                        "specialized Task<{completion}> must borrow completion directly"
                    );

                    assert!(
                        mir.operations().iter().any(|operation| matches!(
                            operation.kind(),
                            bray_ir::MirOperationKind::Async(
                                bray_ir::MirAsyncOperation::ResolveTask { .. }
                            )
                        )),
                        "specialized Task<{completion}> must resolve completion directly"
                    );
                } else {
                    for expected in [
                        bray_ir::MirFrameEntry::CaptureCleanup,
                        bray_ir::MirFrameEntry::CaptureQuiescence,
                    ] {
                        assert!(mir.operations().iter().any(|operation| matches!(operation.kind(),
                    bray_ir::MirOperationKind::Async(bray_ir::MirAsyncOperation::ComposeAwaitedFrame { entry, .. })
                        if *entry == expected
                )), "specialized Future<{completion}> must enter {expected:?} directly");
                    }
                }

                assert!(
                !mir.operations()
                    .iter()
                    .any(|operation| matches!(operation.kind(),
                        bray_ir::MirOperationKind::Async(bray_ir::MirAsyncOperation::CreateFrame {
                            initializer: bray_ir::MirFrameInitializer::Lifecycle { ty, .. }, ..
                        }) if *ty == owner || completion != "Guard"
                    )),
                "specialized {owner_kind}<{completion}> must not allocate a lifecycle wrapper"
            );

                let resolved_values = mir
                    .operations()
                    .iter()
                    .filter_map(|operation| match operation.kind() {
                        bray_ir::MirOperationKind::Async(
                            bray_ir::MirAsyncOperation::ResolveAwaitedFrame { .. }
                            | bray_ir::MirAsyncOperation::ResolveTask { .. }
                            | bray_ir::MirAsyncOperation::BorrowTaskCompletion { .. },
                        ) => operation.result(),
                        _ => None,
                    })
                    .collect::<std::collections::BTreeSet<_>>();

                assert!(!resolved_values.is_empty());

                let mut retained_results = 0;

                for operation in mir.operations() {
                    if let bray_ir::MirOperationKind::Store {
                        destination,
                        value: bray_ir::MirOperand::Value(value),
                        ..
                    } = operation.kind()
                        && resolved_values.contains(value)
                    {
                        assert_cleanup_storage_retained(mir, destination.storage());
                        retained_results += 1;
                    }
                }

                assert_eq!(retained_results, resolved_values.len());

                let frame = mir.frame_descriptor().unwrap();

                for block in mir.blocks() {
                    if let bray_ir::MirTerminatorKind::Suspend {
                        resume_state,
                        resume,
                        ..
                    } = block.terminator().kind()
                    {
                        if let bray_ir::MirTerminatorKind::Suspend {
                            kind: bray_ir::MirSuspensionKind::TaskCompletion,
                            payload: Some(bray_ir::MirOperand::Copy(task)),
                            ..
                        } = block.terminator().kind()
                        {
                            let state = frame
                                .states()
                                .iter()
                                .find(|state| state.state() == *resume_state)
                                .unwrap();

                            assert!(
                                state
                                    .execution()
                                    .retained_storages()
                                    .contains(&task.storage())
                            );
                        }

                        assert!(
                            frame
                                .states()
                                .iter()
                                .any(|state| state.state() == *resume_state
                                    && state.entry() == resume.target())
                        );
                    }
                }

                assert!(
                    generated_artifacts(&backend, &plan)
                        .iter()
                        .all(|artifact| !artifact.is_empty())
                );
            }
        }
    }

    #[test]
    fn abandonment_specializes_source_destructors_without_reintroducing_finalization() {
        use bray_ir::{
            MirAbandonmentAction, MirGeneratedLifecycleRole, MirHelperReference, MirUnitKey,
        };

        for generic in [false, true] {
            for asynchronous in [false, true] {
                let parameters = if generic { "<T>" } else { "" };
                let field = if generic { "T" } else { "Leaf" };
                let execution = if asynchronous { "async " } else { "" };

                let source = format!(
                    r#"
                    module app;
                    struct Owner{parameters}
                    {{
                        leaf: {field};
                        mut flag: i32;
                        destruct()
                        {{
                            self.flag = 7;
                        }}
                    }}
                    struct Leaf
                    {{
                        {execution}finalize()
                        {{
                        }}
                        destruct()
                        {{
                        }}
                    }}
                    func main()
                    {{
                    }}
                    "#,
                    parameters = parameters,
                    field = field,
                    execution = execution
                );

                let compilation = crate::test_support::compilation(&source);

                assert!(
                    compilation.check_diagnostics().is_empty(),
                    "{:#?}",
                    compilation.check_diagnostics()
                );

                let symbols = compilation.symbol_graph().unwrap();
                let values = compilation.semantic_value_store().unwrap();

                let structure = |name| {
                    symbols
                        .structures()
                        .iter()
                        .find(|symbol| {
                            symbols
                                .member_name(symbol.id().into())
                                .is_some_and(|found| found.as_str() == name)
                        })
                        .unwrap()
                };

                let leaf = named_test_type(values, structure("Leaf").id(), [], []);
                let owner = structure("Owner");

                let owner = named_test_type(
                    values,
                    owner.id(),
                    owner
                        .generic_type_parameters()
                        .iter()
                        .copied()
                        .map(GenericParameterSymbolId::Type),
                    generic.then_some(GenericArgument::Type(leaf)),
                );

                let target = compilation
                    .selected_target()
                    .target()
                    .codegen_target()
                    .unwrap();

                let root = compilation
                    .concrete_codegen_lifecycle(
                        MirHelperReference::Abandon {
                            action: MirAbandonmentAction::Destroy,
                            ty: owner,
                        },
                        &target,
                    )
                    .unwrap();

                let reachability = compilation
                    .codegen_reachability([root], None, &target, &CancellationToken::new())
                    .unwrap_or_else(|error| {
                        panic!("generic={generic}, async leaf={asynchronous}: {error:?}")
                    });

                let destructors = reachability.graph().instances().iter().filter(|instance| {
                    matches!(instance.key().template(), MirUnitKey::GeneratedLifecycle(key)
                        if key.role() == MirGeneratedLifecycleRole::Abandon(MirAbandonmentAction::Destructor))
                }).collect::<Vec<_>>();

                assert_eq!(destructors.len(), 2);

                assert!(
                    destructors
                        .iter()
                        .any(|instance| instance.mir().operations().iter().any(
                            |operation| matches!(
                                operation.kind(),
                                bray_ir::MirOperationKind::Store { .. }
                            )
                        ))
                );

                assert!(
                    destructors
                        .iter()
                        .all(|instance| instance.mir().frame_descriptor().is_none())
                );

                assert!(
                    destructors.iter().any(|instance| !instance
                        .key()
                        .specialization()
                        .arguments()
                        .is_empty())
                        == generic
                );

                for instance in reachability.graph().instances() {
                    assert!(
                        instance
                            .mir()
                            .operations()
                            .iter()
                            .all(|operation| !matches!(
                                operation.kind(),
                                bray_ir::MirOperationKind::Finalize(_)
                                    | bray_ir::MirOperationKind::DestructorRemainder { .. }
                            )),
                        "abandoned parts must not regain graceful finalization: {:?}",
                        instance.key()
                    );
                }

                let root = compilation
                    .concrete_codegen_lifecycle(MirHelperReference::Destroy(owner), &target)
                    .unwrap();

                let key = root.key().clone();

                let ordinary = compilation
                    .codegen_reachability([root], None, &target, &CancellationToken::new())
                    .unwrap_or_else(|error| {
                        panic!("ordinary generic={generic}, async leaf={asynchronous}: {error:?}")
                    });

                let destructor = ordinary
                    .graph()
                    .instances()
                    .iter()
                    .find(|instance| instance.key() == &key)
                    .unwrap();

                assert_eq!(destructor.mir().frame_descriptor().is_some(), asynchronous);

                assert!(
                    destructor
                        .mir()
                        .operations()
                        .iter()
                        .any(|operation| matches!(
                            operation.kind(),
                            bray_ir::MirOperationKind::Store { .. }
                        ))
                );

                assert!(ordinary.graph().instances().iter().all(|instance| {
                    instance.mir().operations().iter().all(|operation| {
                        !matches!(
                            operation.kind(),
                            bray_ir::MirOperationKind::DestructorRemainder { .. }
                        )
                    })
                }));
            }
        }
    }

    #[test]
    fn destructor_allowance_discharge_follows_suspended_remainder_settlement() {
        for body in ["", "panic(\"remainder failure\");"] {
            let source = format!(
                r#"
                module app;
                struct Leaf {{ async finalize() {{ {body} }} destruct() {{}} }}
                struct Owner {{
                    leaf: Leaf;
                    mut flag: i32;
                    destruct() {{ self.flag = 7; }}
                }}
                async func main() {{ let owner = Owner {{ leaf = Leaf {{}}, flag = 0 }}; }}
            "#
            );

            let (_, plan) = runtime_native_plan(&source);

            let mir = plan.units().iter().flat_map(bray_codegen::CodegenUnit::mir_units).find(|mir|
                matches!(mir.key(), MirUnitKey::GeneratedLifecycle(key) if key.role() == bray_ir::MirGeneratedLifecycleRole::Destroy)
                && mir.frame_descriptor().is_some()
            ).expect("the destructor remainder must require an activation");

            let discharge_blocks = mir
                .blocks_with_ids()
                .filter_map(|(id, block)| {
                    block
                        .operations()
                        .iter()
                        .any(|operation| {
                            matches!(
                                mir.operation(*operation).unwrap().kind(),
                                MirOperationKind::DischargeCleanup(_)
                            )
                        })
                        .then_some(id)
                })
                .collect::<Vec<_>>();

            assert!(!discharge_blocks.is_empty());

            for block in discharge_blocks {
                assert!(
                    mir.reachable_blocks([block]).iter().all(|id| !matches!(
                        mir.block(*id).unwrap().terminator().kind(),
                        bray_ir::MirTerminatorKind::Suspend { .. }
                    )),
                    "discharge must follow every remainder suspension: {body}"
                );

                assert!(
                    mir.frame_descriptor()
                        .unwrap()
                        .states()
                        .iter()
                        .any(|state| state.state().raw() != 0
                            && mir.reachable_blocks([state.entry()]).contains(&block)),
                    "resumed cleanup must settle before discharge: {body}"
                );
            }
        }
    }

    #[test]
    fn completed_fallible_finalizer_still_settles_its_local_allowance() {
        let (_, plan) = runtime_native_plan(
            r#"
            module app;
            struct Resource {
                finalize() -> Result<unit, unit>
                    executes(pure, total)
                    ensures(result matches Ok(_))
                { return Ok(unit); }
            }
            func main() { let resource = Resource {}; }
        "#,
        );

        let mir = plan.units().iter().flat_map(bray_codegen::CodegenUnit::mir_units).find(|mir|
            matches!(mir.key(), MirUnitKey::GeneratedLifecycle(key) if key.role() == bray_ir::MirGeneratedLifecycleRole::Destroy)
        ).expect("completion must retain whole-owner destruction");

        let reachable = mir.reachable_blocks([mir.entry()]);

        assert!(
            reachable
                .iter()
                .any(|id| mir
                    .block(*id)
                    .unwrap()
                    .operations()
                    .iter()
                    .any(|operation| matches!(
                        mir.operation(*operation).unwrap().kind(),
                        MirOperationKind::DischargeCleanup(_)
                    ))),
            "a completed Result finalizer still owns its uniform allowance"
        );
    }

    #[test]
    fn source_destructor_remainder_suspends_after_its_synchronous_body() {
        let source = r#"
        module app;

        struct Owner
        {
            leaf: Leaf;
            mut flag: i32;

            destruct()
                requires(blocking_execution())
            {
                self.flag = 7;
            }
        }

        struct Leaf
        {
            async finalize() {}

            destruct() {}
        }

        async func main()
            requires(blocking_execution())
        {
            let owner = Owner
            {
                leaf = Leaf
                {
                },
                flag = 0
            };
        }
        "#;

        let (backend, plan) = runtime_native_plan(source);

        let adapted = plan
            .units()
            .iter()
            .flat_map(bray_codegen::CodegenUnit::mir_units)
            .filter(|mir| {
                matches!(mir.key(), bray_ir::MirUnitKey::GeneratedLifecycle(key)
                if key.role() == bray_ir::MirGeneratedLifecycleRole::Destroy)
                    && mir.frame_descriptor().is_some()
            })
            .collect::<Vec<_>>();

        assert!(!adapted.is_empty());

        for mir in adapted {
            let receiver = mir
                .storages_with_ids()
                .find_map(|(id, storage)| {
                    matches!(storage.kind(), bray_ir::MirStorageKind::Parameter(0)).then_some(id)
                })
                .unwrap();

            let frame = mir.frame_descriptor().unwrap();

            assert!(frame.states().len() > 1);
            assert!(frame.states().iter().any(|state| state.state().raw() == 0));

            for state in frame.states() {
                assert_eq!(
                    state.execution().lane_requirements(),
                    [bray_runtime_interface::ExecutionLaneRequirement::Blocking]
                );

                assert_eq!(
                    state.execution().affinity(),
                    bray_runtime_interface::ProtectedFrameAffinity::OriginThread
                );

                assert!(state.execution().retained_storages().contains(&receiver));
            }
        }

        assert!(
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }

    #[test]
    fn source_destructors_require_completion_of_new_async_obligations() {
        for body in ["self.leaf = Leaf {};", "let fresh = Leaf {};"] {
            let source = format!(
                r#"
                module app;
                struct Owner
                {{
                    mut leaf: Leaf;
                    destruct()
                    {{
                        {body}
                    }}
                }}
                struct Leaf
                {{
                    async finalize()
                    {{
                    }}
                }}
                func main()
                {{
                }}
                "#,
                body = body
            );

            let compilation = crate::test_support::compilation(&source);

            assert!(compilation.check_diagnostics().iter().any(|diagnostic|
                diagnostic.kind() == bray_diagnostics::DiagnosticKind::CheckingAsyncFinalizationInSynchronousContext),
                "{body}: {:#?}", compilation.check_diagnostics());
        }
    }

    #[test]
    fn source_destructor_preserves_proven_no_work_remainder() {
        let source = r#"
        module app;

        struct Owner
        {
            leaf: Leaf;

            destruct()
                executes(pure, total) {}
        }

        struct Leaf
        {
            async finalize()
                executes(pure, total) {}

            destruct()
                executes(pure, total) {}
        }

        func main()
        {
            let owner = Owner
            {
                leaf = Leaf
                {
                }
            };
        }
        "#;

        let (backend, plan) = runtime_native_plan(source);

        assert!(
            plan.units()
                .iter()
                .flat_map(bray_codegen::CodegenUnit::mir_units)
                .all(|unit| unit.frame_descriptor().is_none())
        );

        assert!(
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }

    #[test]
    fn indirect_async_construction_uses_the_selected_callable_and_checks_allocation() {
        let source = r#"
        module app;

        async func produce(pos value: i32) -> i32
        {
            return value;
        }

        async func main() -> i32
        {
            let producer: async func(pos value: i32) -> i32 = produce;

            return await producer(42);
        }
        "#;

        let (backend, plan) = runtime_native_plan(source);

        let creation = plan
            .units()
            .iter()
            .flat_map(bray_codegen::CodegenUnit::mir_units)
            .flat_map(bray_ir::MirUnit::operations)
            .find(|operation| {
                matches!(operation.kind(),
                    bray_ir::MirOperationKind::Async(bray_ir::MirAsyncOperation::CreateFrame {
                        initializer: bray_ir::MirFrameInitializer::Callable(call), ..
                    }) if matches!(call.target(), bray_ir::MirCallTarget::Indirect { .. })
                )
            })
            .expect("indirect invocation must construct its selected frame");

        assert!(creation.kind().helper_references().is_empty());

        assert!(
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }

    #[test]
    fn inactive_future_destruction_composes_capture_cleanup() {
        let source = r#"
        module app;

        struct Guard
        {
            async finalize() {}
        }

        async func produce(pos guard: Guard) -> i32
        {
            return 42;
        }

        async func main()
        {
            let pending = produce(
                Guard
                {
                }
            );
        }
        "#;

        let (backend, plan) = runtime_native_plan(source);

        let composed = plan
            .units()
            .iter()
            .flat_map(bray_codegen::CodegenUnit::mir_units)
            .flat_map(bray_ir::MirUnit::operations)
            .filter(|operation| {
                matches!(
                    operation.kind(),
                    bray_ir::MirOperationKind::Async(
                        bray_ir::MirAsyncOperation::ComposeAwaitedFrame {
                            entry: bray_ir::MirFrameEntry::CaptureCleanup,
                            ..
                        }
                    )
                )
            })
            .count();

        assert!(composed > 0);

        assert!(
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }

    #[test]
    fn inactive_task_observers_retain_owned_result_cleanup() {
        for operation in ["join", "cancel"] {
            for asynchronous in ["", "async "] {
                let source = format!(
                    r#"
                    module app;
                    struct Guard
                    {{
                        {asynchronous}finalize()
                        {{
                        }}
                    }}
                    async func produce(pos guard: Guard) -> Guard
                    {{
                        return guard;
                    }}
                    async func main()
                    {{
                        let task = produce(Guard
                        {{
                        }}).start();
                        let pending = task.{}();
                    }}
                    "#,
                    operation,
                    asynchronous = asynchronous
                );

                let (backend, plan) = runtime_native_plan(&source);

                let roles = plan
                    .units()
                    .iter()
                    .flat_map(bray_codegen::CodegenUnit::mir_units)
                    .flat_map(bray_codegen::demanded_runtime_references_for_mir)
                    .map(|reference| reference.role())
                    .collect::<std::collections::BTreeSet<_>>();

                assert!(roles.contains(&bray_runtime_interface::RuntimeAbiRole::TaskResolution));

                assert!(
                    roles
                        .contains(&bray_runtime_interface::RuntimeAbiRole::TaskCancellationRequest)
                );

                assert!(
                    plan.units()
                        .iter()
                        .flat_map(bray_codegen::CodegenUnit::mir_units)
                        .flat_map(bray_ir::MirUnit::blocks)
                        .any(|block| matches!(
                            block.terminator().kind(),
                            bray_ir::MirTerminatorKind::Suspend {
                                kind: bray_ir::MirSuspensionKind::TaskCompletion,
                                cancellation: None,
                                ..
                            }
                        ))
                );

                assert!(
                    generated_artifacts(&backend, &plan)
                        .iter()
                        .all(|artifact| !artifact.is_empty())
                );
            }
        }
    }

    #[test]
    fn static_mapping_retains_fallible_finalizers() {
        for asynchronous in [false, true] {
            let source = r#"
            module app;

            struct Resource
            {
                mut state: i32;
            }

            impl Resource
            {
                async finalize() -> Result<unit, i32>
                {
                    self.state = 2;
                    return await finish();
                }

                destruct()
                {
                    self.state = 3;
                }
            }

            async func finish() -> Result<unit, i32>
            {
                return Error(42);
            }

            static RESOURCE: Resource = Resource
            {
                state = 1
            };

            func main() {}
            "#;

            let source = if asynchronous {
                source.to_owned()
            } else {
                source.replace("async ", "").replace("await ", "")
            };

            let (backend, plan) = runtime_native_plan(&source);

            assert_eq!(
                plan.units()
                    .iter()
                    .flat_map(bray_codegen::CodegenUnit::mir_units)
                    .any(|mir| matches!(
                        mir.source(),
                        bray_ir::MirSourceOrigin::GeneratedLifecycle(MirHelperReference::Finalize(
                            _
                        ))
                    )),
                asynchronous,
            );

            let finalization = plan
                .mappings()
                .iter()
                .flat_map(bray_codegen::CodegenMappings::static_storages)
                .find_map(bray_codegen::CodegenStaticStorageMapping::finalization)
                .unwrap_or_else(|| panic!("static finalizer must be retained"));

            assert_eq!(
                finalization.execution(),
                if asynchronous {
                    bray_symbols::CallableExecution::Asynchronous
                } else {
                    bray_symbols::CallableExecution::Synchronous
                }
            );

            assert!(
                generated_artifacts(&backend, &plan)
                    .iter()
                    .all(|artifact| !artifact.is_empty())
            );

            let (windows_backend, windows_plan) = runtime_native_plan_for_target(
                &source,
                ProductKind::Executable,
                SelectedTarget::for_native(NativeTarget::X86_64WindowsMsvc),
            );

            assert!(
                generated_artifacts(&windows_backend, &windows_plan)
                    .iter()
                    .all(|artifact| !artifact.is_empty())
            );
        }
    }

    #[test]
    fn async_finalizer_composition_preserves_direct_invocation_and_static_wrapper() {
        let (_, plan) = runtime_native_plan(
            r#"
            module app;
            struct Resource
            {
                async finalize() -> Result<unit, unit> { return Error(unit); }
            }
            static RESOURCE: Resource = Resource {};
            async func main()
            {
                let resource = Resource {};
                panic("abnormal exit");
            }
            "#,
        );

        let instances = plan
            .units()
            .iter()
            .flat_map(bray_codegen::CodegenUnit::instances)
            .collect::<Vec<_>>();

        let source = instances
            .iter()
            .find_map(|instance| {
                if !matches!(instance.key().template(), MirUnitKey::Bound(_)) {
                    return None;
                }

                instance
                    .mir()
                    .operations()
                    .iter()
                    .any(|operation| {
                        matches!(
                            operation.kind(),
                            MirOperationKind::Async(
                                bray_ir::MirAsyncOperation::TransferCleanupIncident { .. }
                            )
                        )
                    })
                    .then_some(instance.mir())
            })
            .expect("source cleanup must retain the directly awaited finalizer error");

        for operation in source.operations() {
            if let MirOperationKind::Async(bray_ir::MirAsyncOperation::TransferCleanupIncident {
                invocation,
                ..
            }) = operation.kind()
            {
                assert!(
                    matches!(source.operation(*invocation).unwrap().kind(),
                        MirOperationKind::Async(bray_ir::MirAsyncOperation::CreateFrame {
                            initializer: bray_ir::MirFrameInitializer::Callable(call),
                            storage: bray_ir::MirFrameStorageSource::Fresh,
                            ..
                        }) if matches!(call.target(), bray_ir::MirCallTarget::Direct(_))
                    ),
                    "the incident must identify the declared finalizer's frame construction"
                );
            }
        }

        assert!(
            !source.operations().iter().any(|operation| matches!(
                operation.kind(),
                MirOperationKind::Async(bray_ir::MirAsyncOperation::CreateFrame {
                    initializer: bray_ir::MirFrameInitializer::Lifecycle {
                        role: bray_ir::MirGeneratedLifecycleRole::Finalize,
                        ..
                    },
                    ..
                })
            )),
            "source cleanup must not allocate a generated Finalize wrapper"
        );

        let static_wrapper = instances
            .iter()
            .find(|instance| {
                matches!(
                    instance.key().template(), MirUnitKey::GeneratedLifecycle(key)
                        if key.role() == bray_ir::MirGeneratedLifecycleRole::StaticFinalize
                )
            })
            .expect("static finalization must retain its construction wrapper");

        assert!(
            static_wrapper
                .mir()
                .operations()
                .iter()
                .any(|operation| matches!(
                    operation.kind(),
                    MirOperationKind::Async(bray_ir::MirAsyncOperation::CreateFrame {
                        initializer: bray_ir::MirFrameInitializer::Lifecycle {
                            role: bray_ir::MirGeneratedLifecycleRole::Finalize,
                            ..
                        },
                        ..
                    })
                )),
            "the static wrapper must still return its generated Finalize frame"
        );
    }

    #[test]
    fn fallible_value_finalizers_transfer_owned_errors_after_successful_completion() {
        for asynchronous in ["", "async "] {
            let source = format!(
                r#"
                module app;
                @layout(c) struct Failure
                {{
                    category: u8;
                    code: i64;
                    destruct()
                    {{
                    }}
                }}
                struct Resource
                {{
                    {asynchronous}finalize() -> Result<unit, Failure>
                    {{
                        return Error(Failure
                        {{
                            category = 1, code = 42
                        }});
                    }}
                }}
                {asynchronous}func main()
                {{
                    let value = Resource
                    {{
                    }};
                    panic("abnormal exit");
                }}
                "#,
                asynchronous = asynchronous
            );

            let (backend, plan) = runtime_native_plan(&source);

            let transfers = plan
                .units()
                .iter()
                .flat_map(bray_codegen::CodegenUnit::instances)
                .flat_map(|instance| {
                    instance
                        .mir()
                        .operations_with_ids()
                        .filter_map(move |(id, operation)| {
                            matches!(
                                operation.kind(),
                                bray_ir::MirOperationKind::Async(
                                    bray_ir::MirAsyncOperation::TransferCleanupIncident { .. }
                                )
                            )
                            .then_some((instance.key(), id))
                        })
                })
                .collect::<Vec<_>>();

            assert!(
                !transfers.is_empty(),
                "{asynchronous}finalization must retain its error payload"
            );

            let requirements = plan.executable_host().unwrap().requirements();

            assert!(requirements.requires_role(RuntimeAbiRole::CleanupIncidentDetailReporting));

            for (owner, operation) in transfers {
                let incident = plan
                    .mappings()
                    .iter()
                    .find_map(|mappings| mappings.operation(owner, operation))
                    .and_then(bray_codegen::CodegenOperationMapping::incident)
                    .unwrap();

                assert!(matches!(
                    incident.source(),
                    Some(bray_ir::MirSourceAnchor::Source(_))
                ));

                assert!(
                    matches!(incident.cleanup().template(), MirUnitKey::GeneratedLifecycle(key)
                    if key.role() == bray_ir::MirGeneratedLifecycleRole::Abandon(bray_ir::MirAbandonmentAction::Destroy))
                );

                assert!(
                    incident
                        .dependencies()
                        .iter()
                        .all(|dependency| plan.units().iter().any(|unit| unit
                            .instances()
                            .iter()
                            .any(|instance| instance.key() == *dependency)
                            || unit.external_instances().contains(dependency)))
                );
            }

            assert!(
                generated_artifacts(&backend, &plan)
                    .iter()
                    .all(|artifact| !artifact.is_empty())
            );
        }
    }

    #[test]
    fn synchronous_entry_starts_execution_only_for_async_static_cleanup() {
        for asynchronous in [false, true] {
            let qualifier = if asynchronous { "async " } else { "" };

            let body = if asynchronous {
                "await complete();"
            } else {
                ""
            };

            let source = format!(
                r#"
                module static_cleanup_admission;
                async func complete() {{}}
                struct Probe {{
                    value: i32;
                    {qualifier}finalize() {{ {body} }}
                }}
                static PROBE: Probe = Probe {{ value = 7 }};
                func main() -> i32 {{ return PROBE.value - 7; }}
            "#
            );

            let (backend, plan) = runtime_native_plan_for_sources_target(
                &[&source],
                ProductKind::Executable,
                SelectedTarget::baseline(),
                &[],
            );

            assert_eq!(
                plan.mappings()
                    .iter()
                    .flat_map(|mapping| mapping.static_storages())
                    .filter_map(|storage| storage.finalization())
                    .any(|finalizer| finalizer.execution()
                        == bray_symbols::CallableExecution::Asynchronous),
                asynchronous,
            );

            let host = plan.executable_host().unwrap();
            assert_eq!(host.entries()[0].root(), RootExecution::Synchronous);

            assert_eq!(
                host.requirements()
                    .requires_role(RuntimeAbiRole::MainThreadLaneStartup),
                asynchronous
            );

            assert_eq!(
                plan.product_host().unwrap().control_role(),
                if asynchronous {
                    RuntimeAbiRole::AsynchronousProductHostControl
                } else {
                    RuntimeAbiRole::ProductHostControl
                }
            );

            let host_mir = plan
                .units()
                .iter()
                .flat_map(|unit| unit.instances())
                .map(|instance| instance.mir())
                .find(|mir| matches!(mir.kind(), bray_ir::MirUnitKind::ExecutableHost(_)))
                .unwrap();

            let operations = host_mir.block(host_mir.entry()).unwrap().operations();

            let startup = operations.iter().position(|id| {
                matches!(
                    host_mir.operation(*id).unwrap().kind(),
                    bray_ir::MirOperationKind::Host(
                        bray_ir::MirHostOperation::BeginExecution { .. }
                    )
                )
            });

            let materialize = operations
                .iter()
                .position(|id| {
                    matches!(
                        host_mir.operation(*id).unwrap().kind(),
                        bray_ir::MirOperationKind::Host(
                            bray_ir::MirHostOperation::MaterializeStatic { .. }
                        )
                    )
                })
                .unwrap();

            assert_eq!(startup.is_some(), asynchronous);

            if let Some(startup) = startup {
                assert!(startup < materialize);
            }

            assert!(plan.product_host().is_some());

            assert!(
                generated_artifacts(&backend, &plan)
                    .iter()
                    .all(|artifact| !artifact.is_empty())
            );
        }
    }

    #[test]
    fn worker_static_finalization_with_atomic_borrows_generates_for_windows() {
        let source = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../xtask/fixtures/native-execution/worker-static-cleanup.bray"
        ))
        .replace("using std.task;", "")
        .replace("std.task.yield_now()", "complete()");

        let source = format!("{source}\nasync func complete() {{}}\n");

        let (backend, plan) = runtime_native_plan_for_sources_target(
            &[&source],
            ProductKind::Executable,
            SelectedTarget::for_native(NativeTarget::X86_64WindowsMsvc),
            &[],
        );

        assert!(
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }

    #[test]
    fn native_static_storage_fixture_prepares_for_windows() {
        let sources = [
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../xtask/fixtures/native-execution/static_storage.bray"
            )),
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../xtask/fixtures/native-execution/static_storage_contribution.bray"
            )),
        ];

        let (backend, plan) = runtime_native_plan_for_sources_target(
            &sources,
            ProductKind::Executable,
            SelectedTarget::for_native(NativeTarget::X86_64WindowsMsvc),
            &[],
        );

        let requirements = plan
            .executable_host()
            .unwrap_or_else(|| panic!("fixture must retain an executable host"))
            .requirements();

        assert!(requirements.requires_role(RuntimeAbiRole::AwaitedFrameComposition));
        assert!(requirements.requires_role(RuntimeAbiRole::AwaitedFrameResolution));

        assert!(
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }

    #[test]
    fn native_exports_and_opaque_storage_survive_reachability_and_codegen() {
        let source = r#"
        trusted module app;

        @layout(c, size = 40, align = 8)
        struct NativeMutex;

        @link(name = "native")
        @symbol(name = "native_mutex")
        extern trusted static NATIVE_MUTEX_STORAGE: NativeMutex;

        @link(name = "native")
        @symbol(name = "native_pointer")
        extern trusted static mut NATIVE_POINTER: RawPointer<u8>;

        @symbol(name = "unused_export")
        static UNUSED_EXPORT: i32 = 7;

        @symbol(name = "weak_export", binding = weak)
        @abi(c)
        func weak_export() -> i32
        {
            return 1;
        }

        func main()
        {
            let value: i32 = weak_export();
            let pointer: RawPointer<NativeMutex> = NATIVE_MUTEX_STORAGE;
            let pointer_storage: RawPointer<RawPointer<u8>> = NATIVE_POINTER;
        }
        "#;

        let native_link = NativeLinkRequirement::new(
            NonEmptySharedStr::try_new("native")
                .unwrap_or_else(|| panic!("native link name must be valid")),
            NativeLinkKind::Dynamic,
        );

        let (backend, plan) = runtime_native_plan_for_sources_target(
            &[source],
            ProductKind::Executable,
            SelectedTarget::for_native(NativeTarget::X86_64WindowsMsvc),
            &[native_link],
        );

        let host = plan
            .executable_host()
            .unwrap_or_else(|| panic!("executable plan must retain its host"));

        assert_eq!(host.entries().len(), 1);

        let callback_body =
            assert_native_callback_entry(&plan, "weak_export", CodegenLinkage::Weak);

        assert!(plan.mappings().iter().any(|mappings| {
            mappings.static_storages().iter().any(|mapping| {
                mapping.symbol().as_str() == "unused_export"
                    && mapping.native_binding() == Some(NativeSymbolBinding::Strong)
                    && mapping.defines_storage()
            })
        }));

        assert!(plan.mappings().iter().any(|mappings| {
            mappings.types().iter().any(|mapping| {
                mapping
                    .layout()
                    .is_some_and(|layout| layout.size() == 40 && layout.alignment().get() == 8)
            })
        }));

        let backend_ir =
            generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendIr);

        assert!(
            backend_ir
                .iter()
                .any(|artifact| { String::from_utf8_lossy(artifact).contains("@unused_export =") })
        );

        assert!(backend_ir.iter().any(|artifact| {
            String::from_utf8_lossy(artifact)
                .lines()
                .any(|line| line.contains(" call ") && line.contains(&callback_body))
        }));

        assert!(
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }

    #[test]
    fn direct_platform_bindings_and_callback_entries_cover_every_native_target() {
        let callback_source = concat!(
            "module app;\n",
            "@symbol(name = \"native_callback\")\n",
            "@abi(c)\n",
            "func callback(pos value: i32) -> i32\n",
            "{\n",
            "    return value + 1;\n",
            "}\n",
            "func main()\n",
            "{\n",
            "    let result: i32 = callback(1);\n",
            "}\n",
        );

        let platform_source = concat!(
            "trusted module app;\n",
            "@layout(c)\n",
            "internal struct PlatformStatus\n",
            "{\n",
            "    category: u32;\n",
            "    reserved: u32;\n",
            "    native_code: i64;\n",
            "}\n",
            "@abi(c)\n",
            "trusted internal func flush() -> PlatformStatus\n",
            "{\n",
            "    return { category = 0, reserved = 0, native_code = 0 };\n",
            "}\n",
            "func main()\n",
            "{\n",
            "    let status: PlatformStatus = trusted flush();\n",
            "}\n",
        );

        let binding =
            PlatformServiceBinding::try_new(PlatformServiceRole::StandardOutputFlush, "app.flush")
                .unwrap_or_else(|| panic!("platform service binding must validate"));

        let platform_symbol = PlatformServiceRole::StandardOutputFlush.native_symbol();

        for target in NativeTarget::ALL {
            let selected = SelectedTarget::for_native(target);

            let (backend, callback_plan) = runtime_native_plan_for_sources_target(
                &[callback_source],
                ProductKind::Executable,
                selected.clone(),
                &[],
            );

            let callback_body = assert_native_callback_entry(
                &callback_plan,
                "native_callback",
                CodegenLinkage::Export,
            );

            let backend_ir = generated_artifacts_of_kind(
                &backend,
                &callback_plan,
                BackendArtifactKind::BackendIr,
            )
            .into_iter()
            .map(|artifact| String::from_utf8_lossy(&artifact).into_owned())
            .collect::<String>();

            assert!(
                backend_ir
                    .lines()
                    .any(|line| line.contains(" call ") && line.contains(&callback_body)),
                "{target:?}"
            );

            let (_, platform_plan) = runtime_native_plan_for_sources_target_with_platform_services(
                &[platform_source],
                ProductKind::Executable,
                selected,
                &[],
                [binding.clone()],
            );

            assert_direct_platform_service(&platform_plan, platform_symbol);
        }
    }

    #[test]
    fn bray_platform_service_implementations_are_native_fallbacks() {
        let source = concat!(
            "trusted module app;\n",
            "@layout(c)\n",
            "internal struct PlatformStatus\n",
            "{\n",
            "    category: u32;\n",
            "    reserved: u32;\n",
            "    native_code: i64;\n",
            "}\n",
            "@abi(c)\n",
            "trusted internal func flush() -> PlatformStatus\n",
            "{\n",
            "    return { category = 0, reserved = 0, native_code = 0 };\n",
            "}\n",
            "func main()\n",
            "{\n",
            "    let status: PlatformStatus = trusted flush();\n",
            "}\n",
            "@test\n",
            "func platform_service_test()\n",
            "{\n",
            "    let status: PlatformStatus = trusted flush();\n",
            "}\n",
        );

        let binding =
            PlatformServiceBinding::try_new(PlatformServiceRole::StandardOutputFlush, "app.flush")
                .unwrap_or_else(|| panic!("platform service binding must validate"));

        let (backend, plan) = runtime_native_plan_for_sources_target_with_platform_services(
            &[source],
            ProductKind::Executable,
            SelectedTarget::baseline(),
            &[],
            [binding.clone()],
        );

        let symbol = PlatformServiceRole::StandardOutputFlush.native_symbol();

        assert_direct_platform_service(&plan, symbol);

        assert!(
            plan.preservation_roots()
                .any(|root| root.as_str() == symbol)
        );

        assert!(
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );

        let (backend, plan) = runtime_native_plan_for_sources_target_with_platform_overrides(
            &[source],
            ProductKind::Test,
            SelectedTarget::baseline(),
            &[],
            [binding],
            [PlatformServiceRole::StandardOutputFlush],
        );

        let backend_ir =
            generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendIr)
                .into_iter()
                .map(|artifact| String::from_utf8_lossy(&artifact).into_owned())
                .collect::<String>();

        assert!(plan.mappings().iter().any(|mappings| {
            mappings.symbols().iter().any(|mapping| {
                mapping.name().as_str() == symbol && mapping.linkage() == CodegenLinkage::Import
            })
        }));

        assert!(
            backend_ir
                .lines()
                .any(|line| line.starts_with("declare ") && line.contains(symbol))
        );

        assert!(
            !backend_ir
                .lines()
                .any(|line| line.starts_with("define ") && line.contains(symbol))
        );
    }

    fn assert_native_callback_entry(
        plan: &super::NativeProductPlan,
        entry_name: &str,
        entry_linkage: CodegenLinkage,
    ) -> String {
        let callback = plan
            .mappings()
            .iter()
            .flat_map(bray_codegen::CodegenMappings::symbols)
            .find(|mapping| {
                mapping.native_entry().is_some_and(|entry| {
                    entry.name().as_str() == entry_name && entry.linkage() == entry_linkage
                })
            })
            .unwrap_or_else(|| panic!("native callback entry must be mapped"));

        assert_eq!(callback.linkage(), CodegenLinkage::LinkOnce);
        assert_ne!(callback.name().as_str(), entry_name);

        let bray_codegen::CodegenSymbolKey::Instance(instance) = callback.key() else {
            panic!("native callback entry must map a concrete instance");
        };

        let compatibility = plan
            .units()
            .iter()
            .find_map(|unit| unit.key().compatibility(instance))
            .unwrap_or_else(|| panic!("callback instance must retain partition compatibility"));

        assert_eq!(compatibility.linkage(), CodegenLinkage::LinkOnce);

        assert_eq!(
            compatibility.visibility(),
            bray_codegen::CodegenDefinitionVisibility::Product
        );

        let host = plan
            .executable_host()
            .unwrap_or_else(|| panic!("callback plan must retain an executable host"));

        assert!(
            host.requirements()
                .requires_role(RuntimeAbiRole::ForeignCallbackExecution)
        );

        callback.name().as_str().to_owned()
    }

    fn assert_direct_platform_service(plan: &super::NativeProductPlan, symbol: &str) {
        assert!(plan.mappings().iter().any(|mappings| {
            mappings.symbols().iter().any(|mapping| {
                mapping.name().as_str() == symbol
                    && mapping.linkage() == CodegenLinkage::Fallback
                    && mapping.native_entry().is_none()
            })
        }));

        let host = plan
            .executable_host()
            .unwrap_or_else(|| panic!("platform plan must retain an executable host"));

        assert!(
            !host
                .requirements()
                .requires_role(RuntimeAbiRole::ForeignCallbackExecution)
        );
    }

    fn codegen_compilation_for_product(
        source: &str,
        product_kind: ProductKind,
    ) -> (
        Arc<bray_codegen_llvm::LlvmCodeGenerator>,
        crate::Compilation,
    ) {
        codegen_compilation_for_product_target(source, product_kind, SelectedTarget::baseline())
    }

    fn codegen_compilation_for_product_with_worker_budget(
        source: &str,
        product_kind: ProductKind,
        worker_budget: WorkerBudget,
    ) -> (
        Arc<bray_codegen_llvm::LlvmCodeGenerator>,
        crate::Compilation,
    ) {
        codegen_compilation_for_sources_target_with_worker_budget_and_platform_services(
            &[source],
            product_kind,
            SelectedTarget::baseline(),
            &[],
            [],
            worker_budget,
        )
    }

    fn codegen_compilation_for_product_target(
        source: &str,
        product_kind: ProductKind,
        target: SelectedTarget,
    ) -> (
        Arc<bray_codegen_llvm::LlvmCodeGenerator>,
        crate::Compilation,
    ) {
        codegen_compilation_for_sources_target(&[source], product_kind, target, &[])
    }

    fn codegen_compilation_for_sources_target(
        sources: &[&str],
        product_kind: ProductKind,
        target: SelectedTarget,
        native_link_inputs: &[NativeLinkRequirement],
    ) -> (
        Arc<bray_codegen_llvm::LlvmCodeGenerator>,
        crate::Compilation,
    ) {
        codegen_compilation_for_sources_target_with_platform_services(
            sources,
            product_kind,
            target,
            native_link_inputs,
            [],
        )
    }

    fn codegen_compilation_for_sources_target_with_platform_services(
        sources: &[&str],
        product_kind: ProductKind,
        target: SelectedTarget,
        native_link_inputs: &[NativeLinkRequirement],
        platform_services: impl IntoIterator<Item = PlatformServiceBinding>,
    ) -> (
        Arc<bray_codegen_llvm::LlvmCodeGenerator>,
        crate::Compilation,
    ) {
        codegen_compilation_for_sources_target_with_worker_budget_and_platform_services(
            sources,
            product_kind,
            target,
            native_link_inputs,
            platform_services,
            WorkerBudget::serial(),
        )
    }

    fn codegen_compilation_for_sources_target_with_worker_budget_and_platform_services(
        sources: &[&str],
        product_kind: ProductKind,
        target: SelectedTarget,
        native_link_inputs: &[NativeLinkRequirement],
        platform_services: impl IntoIterator<Item = PlatformServiceBinding>,
        worker_budget: WorkerBudget,
    ) -> (
        Arc<bray_codegen_llvm::LlvmCodeGenerator>,
        crate::Compilation,
    ) {
        codegen_compilation_for_sources_target_with_source_roles(
            sources,
            product_kind,
            target,
            native_link_inputs,
            platform_services,
            [],
            worker_budget,
        )
    }

    #[test]
    fn runtime_source_bindings_export_and_retain_the_canonical_role_symbol() {
        let source = concat!(
            "trusted module app;\n",
            "@abi(c)\n",
            "trusted internal func attachment_identity(pos descriptor: RawPointer<u8>) -> u64\n",
            "{\n",
            "    let _: RawPointer<u8> = descriptor;\n",
            "    return 7;\n",
            "}\n",
        );

        let role = RuntimeAbiRole::ThreadAttachmentIdentity;

        let binding = bray_runtime_interface::RuntimeRoleSourceBinding::try_new(
            role,
            "app.attachment_identity",
        )
        .unwrap_or_else(|| panic!("runtime source binding must validate"));

        let (backend, plan) =
            runtime_native_plan_with_source_roles(&[source], ProductKind::Library, [binding]);

        let symbol = role
            .native_symbol()
            .unwrap_or_else(|| panic!("runtime role must have a native symbol"));

        assert!(plan.mappings().iter().any(|mappings| {
            mappings.symbols().iter().any(|mapping| {
                mapping.name().as_str() == symbol && mapping.linkage() == CodegenLinkage::Export
            })
        }));

        assert!(
            plan.preservation_roots()
                .any(|root| root.as_str() == symbol)
        );

        assert!(
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }

    #[test]
    fn platform_source_bindings_export_and_retain_the_canonical_role_symbol() {
        let source = concat!(
            "trusted module app;\n",
            "@layout(c)\n",
            "internal struct PlatformStatus\n",
            "{\n",
            "    category: u32;\n",
            "    reserved: u32;\n",
            "    native_code: i64;\n",
            "}\n",
            "@abi(c)\n",
            "trusted internal func flush() -> PlatformStatus\n",
            "{\n",
            "    return { category = 0, reserved = 0, native_code = 0 };\n",
            "}\n",
        );

        let role = PlatformServiceRole::StandardOutputFlush;

        let binding = PlatformServiceBinding::try_new(role, "app.flush")
            .unwrap_or_else(|| panic!("platform source binding must validate"));

        let (backend, plan) = runtime_native_plan_for_sources_target_with_platform_services(
            &[source],
            ProductKind::Library,
            SelectedTarget::baseline(),
            &[],
            [binding],
        );

        let symbol = role.native_symbol();

        assert!(plan.mappings().iter().any(|mappings| {
            mappings.symbols().iter().any(|mapping| {
                mapping.name().as_str() == symbol && mapping.linkage() == CodegenLinkage::Fallback
            })
        }));

        assert!(
            plan.preservation_roots()
                .any(|root| root.as_str() == symbol)
        );

        assert!(
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }

    #[test]
    fn runtime_source_bindings_retain_referenced_static_atomic_storage() {
        let source = concat!(
            "trusted module app;\n",
            "internal static STATE: core.atomic.Atomic<u32> = core.atomic.initialize<u32>(0);\n",
            "@abi(c)\n",
            "trusted internal func initialize(pos worker_capacity: usize, pos timer_capacity: usize) -> u32\n",
            "{\n",
            "    let _: usize = timer_capacity;\n",
            "    if worker_capacity == 0\n",
            "    {\n",
            "        let _: u32 = core.atomic.load<u32, 1>(&STATE);\n",
            "    }\n",
            "    return core.atomic.load<u32, 1>(&STATE);\n",
            "}\n",
        );

        let role = RuntimeAbiRole::RuntimeInitialization;

        let binding =
            bray_runtime_interface::RuntimeRoleSourceBinding::try_new(role, "app.initialize")
                .unwrap_or_else(|| panic!("runtime source binding must validate"));

        let (backend, plan) =
            runtime_native_plan_with_source_roles(&[source], ProductKind::Library, [binding]);

        let symbol = role
            .native_symbol()
            .unwrap_or_else(|| panic!("runtime role must have a native symbol"));

        assert!(
            plan.preservation_roots()
                .any(|root| root.as_str() == symbol)
        );

        assert!(plan.mappings().iter().any(|mappings| {
            mappings
                .static_storages()
                .iter()
                .any(bray_codegen::CodegenStaticStorageMapping::defines_storage)
        }));

        assert!(
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }

    fn codegen_compilation_for_sources_target_with_source_roles(
        sources: &[&str],
        product_kind: ProductKind,
        target: SelectedTarget,
        native_link_inputs: &[NativeLinkRequirement],
        platform_services: impl IntoIterator<Item = PlatformServiceBinding>,
        runtime_roles: impl IntoIterator<Item = bray_runtime_interface::RuntimeRoleSourceBinding>,
        worker_budget: WorkerBudget,
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

        let runtime_dependency = crate::test_support::runtime_standard_library_dependency(&target);

        let request = CompilationRequest::with_options(
            crate::test_support::package_identity(),
            sources
                .iter()
                .enumerate()
                .map(|(identity, source)| {
                    crate::test_support::source_input(
                        source,
                        u32::try_from(identity)
                            .unwrap_or_else(|_| panic!("test source identity must fit u32")),
                    )
                })
                .collect(),
            CompilationOptions::new(worker_budget, product_kind, target)
                .with_native_link_inputs(native_link_inputs.iter().cloned()),
        )
        .with_dependency_interfaces([runtime_dependency])
        .with_platform_services(platform_services)
        .with_runtime_roles(runtime_roles);

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
        runtime_artifact_with_roles_and_platform_services(compilation, archive, roles, [])
    }

    fn runtime_artifact_with_roles_and_platform_services(
        compilation: &crate::Compilation,
        archive: &std::path::Path,
        roles: impl IntoIterator<Item = RuntimeAbiRole>,
        platform_services: impl IntoIterator<Item = PlatformServiceRole>,
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

        let roles: Vec<_> = roles
            .into_iter()
            .filter(|role| role.native_symbol().is_some())
            .collect();

        let bindings = roles.iter().copied().map(|role| {
            let symbol = BinarySymbolName::try_new(
                role.native_symbol()
                    .expect("selected native role has a symbol"),
            )
            .unwrap_or_else(|| panic!("test runtime role symbol must be valid"));

            RuntimeRoleBinding::new(role, symbol, RuntimeRoleImplementation::BrayRuntime)
        });

        let version = RuntimeAbiVersion::CURRENT;

        let capabilities = [
            RuntimeCapability::CooperativeExecution,
            RuntimeCapability::LocalLanes,
            RuntimeCapability::MainThreadLane,
        ];

        let contract = RuntimeContract::try_new(
            identity,
            artifact,
            version,
            ProtectedFrameAbiVersions::uniform(version),
            target.identity().clone(),
            target.panic_abi().clone(),
            capabilities,
            bindings,
        )
        .unwrap_or_else(|error| panic!("test runtime contract must validate: {error:?}"));

        let digest = RuntimeArtifactDigest::new(
            bray_base::sha256_file(archive)
                .unwrap_or_else(|error| panic!("test runtime archive must hash: {error}")),
        );

        let product_component = RuntimeArtifactId::try_new("runtime.product")
            .unwrap_or_else(|| panic!("test component identity must be valid"));

        let components = [
            RuntimeArtifactComponentMetadata::try_new(
                product_component.clone(),
                RuntimeArtifactPurpose::Product,
                roles
                    .iter()
                    .copied()
                    .filter(|role| role.available_to_product()),
                capabilities,
                "libbray_runtime_product.a",
                digest,
            )
            .unwrap_or_else(|error| panic!("test component must validate: {error:?}")),
            RuntimeArtifactComponentMetadata::try_new(
                RuntimeArtifactId::try_new("runtime.test")
                    .unwrap_or_else(|| panic!("test component identity must be valid")),
                RuntimeArtifactPurpose::TestRunner,
                roles.iter().copied(),
                capabilities,
                "libbray_runtime_test.a",
                digest,
            )
            .unwrap_or_else(|error| panic!("test component must validate: {error:?}"))
            .with_platform_services(platform_services),
        ];

        let metadata = RuntimeArtifactMetadata::try_new(contract, components)
            .unwrap_or_else(|error| panic!("test runtime metadata must validate: {error:?}"));

        let directory = archive.parent().unwrap_or_else(|| std::path::Path::new(""));
        let product_archive = directory.join("libbray_runtime_product.a");
        let test_archive = directory.join("libbray_runtime_test.a");

        fs::copy(archive, &product_archive)
            .unwrap_or_else(|error| panic!("test product runtime archive must copy: {error}"));

        fs::copy(archive, &test_archive)
            .unwrap_or_else(|error| panic!("test runner runtime archive must copy: {error}"));

        RuntimeArtifact::try_new(
            metadata,
            [
                (product_component, product_archive),
                (
                    RuntimeArtifactId::try_new("runtime.test")
                        .unwrap_or_else(|| panic!("test component identity must be valid")),
                    test_archive,
                ),
            ],
        )
        .unwrap_or_else(|error| panic!("test runtime artifact must validate: {error:?}"))
    }

    fn generated_artifacts(
        backend: &bray_codegen_llvm::LlvmCodeGenerator,
        plan: &super::NativeProductPlan,
    ) -> Vec<Vec<u8>> {
        generated_artifacts_of_kind(backend, plan, BackendArtifactKind::RelocatableObject)
    }

    fn native_partition_recipe(
        plan: &super::NativeProductPlan,
    ) -> Vec<(
        Vec<bray_codegen::CodegenInstanceKey>,
        bray_codegen::CodegenWork,
        Option<bray_codegen::CodegenOversizedUnit>,
    )> {
        plan.units()
            .iter()
            .map(|unit| {
                let key = unit.key();

                (
                    key.instances().to_vec(),
                    key.estimated_work(),
                    key.oversized(),
                )
            })
            .collect()
    }

    fn generated_artifacts_of_kind(
        backend: &bray_codegen_llvm::LlvmCodeGenerator,
        plan: &super::NativeProductPlan,
        kind: BackendArtifactKind,
    ) -> Vec<Vec<u8>> {
        plan.units()
            .iter()
            .zip(plan.mappings())
            .map(|(unit, mappings)| {
                let artifact = BackendArtifactId::new(unit.key().clone(), kind, 0);

                let artifacts = BackendArtifactRequest::try_new(
                    unit.key().clone(),
                    [BackendArtifactRequestEntry::new(
                        artifact,
                        BackendArtifactRequirement::Required,
                    )],
                    plan.backend().policy().debug_output(),
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
                    plan.target(),
                    mappings,
                    plan.options(),
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
        fn capabilities(&self) -> &LinkerDriverCapabilities {
            static CAPABILITIES: std::sync::OnceLock<LinkerDriverCapabilities> =
                std::sync::OnceLock::new();

            CAPABILITIES.get_or_init(|| {
                let identity = LinkerDriverIdentity::try_new(
                    LinkerDriverKind::EmbeddedLld,
                    "test-lld",
                    "1",
                    "22",
                )
                .unwrap_or_else(|| panic!("test linker identity must be valid"));

                LinkerDriverCapabilities::try_for_lld(identity)
                    .unwrap_or_else(|error| panic!("test capabilities must be valid: {error:?}"))
            })
        }

        fn link(
            &self,
            plan: &LinkPlan,
            _cancellation: &dyn bray_base::Cancellation,
        ) -> LinkOutcome {
            LinkOutcome::failed(
                plan,
                LinkFailure::Invocation,
                bray_diagnostics::DiagnosticBag::new(),
            )
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
            .product_semantics()
            .unwrap_or_else(|error| panic!("test product plan must resolve: {error:?}"));

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

    const GENERIC_CONSUMER_SOURCE: &str = concat!(
        "module application;\n",
        "\n",
        "using example.dependency.templates.identity;\n",
        "\n",
        "func main()\n",
        "{\n",
        "    let value: i32 = example.dependency.templates.identity<i32>(1);\n",
        "}\n",
    );

    const ASYNC_GENERIC_CONSUMER_SOURCE: &str = concat!(
        "module application;\n",
        "\n",
        "using example.dependency.templates.identity;\n",
        "\n",
        "async func main()\n",
        "{\n",
        "    let value: i32 = await example.dependency.templates.identity<i32>(1);\n",
        "}\n",
    );

    const TRAIT_DEFAULT_CONSUMER_SOURCE: &str = concat!(
        "module application;\n",
        "\n",
        "using example.dependency.templates.read;\n",
        "using internal example.dependency.templates.I32Counter;\n",
        "\n",
        "func main()\n",
        "{\n",
        "    let value: i32 = 3;\n",
        "    let count: i32 = example.dependency.templates.read<i32>(&value);\n",
        "}\n",
    );

    #[derive(Clone, Copy)]
    struct GenericDependencyFixture {
        source: &'static str,
        runtime_frames: Option<usize>,
        executable_templates: usize,
        platform_service: Option<(PlatformServiceRole, &'static str)>,
    }

    const GENERIC_DEPENDENCY: GenericDependencyFixture = GenericDependencyFixture {
        source: concat!(
            "module templates;\n",
            "\n",
            "func helper<T>(pos value: T) -> T\n",
            "{\n",
            "    return value;\n",
            "}\n",
            "\n",
            "public func identity<T>(pos value: T) -> T\n",
            "{\n",
            "    let invoke = lambda(pos item: T) -> T\n",
            "    {\n",
            "        return helper<T>(item);\n",
            "    };\n",
            "\n",
            "    return invoke(value);\n",
            "}\n",
        ),
        runtime_frames: None,
        executable_templates: 3,
        platform_service: None,
    };

    const ASYNC_GENERIC_DEPENDENCY: GenericDependencyFixture = GenericDependencyFixture {
        source: concat!(
            "module templates;\n",
            "\n",
            "func helper<T>(pos value: T) -> T\n",
            "{\n",
            "    return value;\n",
            "}\n",
            "\n",
            "public async func identity<T>(pos value: T) -> T\n",
            "{\n",
            "    let invoke = async lambda(pos item: T) -> T\n",
            "    {\n",
            "        return helper<T>(item);\n",
            "    };\n",
            "\n",
            "    return await invoke(value);\n",
            "}\n",
        ),
        runtime_frames: Some(2),
        executable_templates: 3,
        platform_service: None,
    };

    const TRAIT_DEFAULT_DEPENDENCY: GenericDependencyFixture = GenericDependencyFixture {
        source: concat!(
            "module templates;\n",
            "\n",
            "public trait Counter\n",
            "{\n",
            "    func count() -> i32;\n",
            "    func doubled() -> i32\n",
            "    {\n",
            "        return self.count() + self.count();\n",
            "    }\n",
            "    func quadrupled() -> i32\n",
            "    {\n",
            "        return self.doubled() + self.through_lambda();\n",
            "    }\n",
            "    func through_lambda() -> i32\n",
            "    {\n",
            "        let invoke = lambda(pos value: &Self) -> i32\n",
            "        {\n",
            "            return value.count();\n",
            "        };\n",
            "\n",
            "        return invoke(&self);\n",
            "    }\n",
            "}\n",
            "\n",
            "public impl I32Counter = i32(Counter)\n",
            "{\n",
            "    func count() -> i32\n",
            "    {\n",
            "        return self;\n",
            "    }\n",
            "}\n",
            "\n",
            "public func read<T>(pos value: &T) -> i32\n",
            "    with(T: Counter)\n",
            "{\n",
            "    return value.quadrupled();\n",
            "}\n",
        ),
        runtime_frames: None,
        executable_templates: 6,
        platform_service: None,
    };

    fn realize_codegen_mappings(
        compilation: &crate::Compilation,
        target: &bray_codegen::CodegenTarget,
        reachability: &super::super::super::specialization::ConcreteCodegenReachability,
        cancellation: &CancellationToken,
    ) {
        let roots = reachability
            .graph()
            .roots()
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();

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
                        cancellation,
                    )
                    .map(|compatibility| (instance.key().clone(), compatibility))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()
            .unwrap_or_else(|error| panic!("consumer partition plan must resolve: {error:?}"));

        let units = partition_codegen_units(
            CodegenPartitionPolicy::NATIVE_BALANCED,
            reachability.graph(),
            |instance| compatibility.get(instance.key()).cloned(),
        )
        .unwrap_or_else(|error| panic!("consumer units must partition: {error:?}"));

        for unit in units.iter() {
            compilation
                .codegen_mappings_for_product(
                    &test_product_identity(),
                    unit,
                    None,
                    &BTreeSet::new(),
                    target,
                    &roots,
                    reachability,
                    false,
                    cancellation,
                )
                .unwrap_or_else(|error| panic!("consumer mappings must realize: {error:?}"));
        }
    }

    fn generic_consumer(dependency: DependencyInterfaceInput) -> crate::Compilation {
        generic_consumer_for_target(dependency, SelectedTarget::baseline())
    }

    fn async_generic_consumer(dependency: DependencyInterfaceInput) -> crate::Compilation {
        generic_consumer_for_target_with_source(
            dependency,
            SelectedTarget::baseline(),
            ASYNC_GENERIC_CONSUMER_SOURCE,
        )
    }

    fn generic_consumer_for_target(
        dependency: DependencyInterfaceInput,
        target: SelectedTarget,
    ) -> crate::Compilation {
        generic_consumer_for_target_with_source(dependency, target, GENERIC_CONSUMER_SOURCE)
    }

    fn generic_consumer_for_target_with_source(
        dependency: DependencyInterfaceInput,
        target: SelectedTarget,
        source: &str,
    ) -> crate::Compilation {
        let request = CompilationRequest::with_options(
            crate::test_support::package_identity(),
            vec![crate::test_support::source_input(source, 0)],
            CompilationOptions::new(WorkerBudget::serial(), ProductKind::Executable, target),
        )
        .with_dependency_interfaces([dependency]);

        crate::Compilation::load(request)
            .unwrap_or_else(|error| panic!("consumer compilation must load: {error:?}"))
    }

    fn generic_dependency(include_implementation: bool) -> DependencyInterfaceInput {
        generic_dependency_with_templates(include_implementation, false)
    }

    fn generic_async_dependency() -> DependencyInterfaceInput {
        generic_dependency_from_fixture(true, false, ASYNC_GENERIC_DEPENDENCY)
    }

    fn trait_default_dependency() -> DependencyInterfaceInput {
        generic_dependency_from_fixture(true, false, TRAIT_DEFAULT_DEPENDENCY)
    }

    fn generic_dependency_with_templates(
        include_implementation: bool,
        malformed_templates: bool,
    ) -> DependencyInterfaceInput {
        generic_dependency_from_fixture(
            include_implementation,
            malformed_templates,
            GENERIC_DEPENDENCY,
        )
    }

    fn generic_dependency_from_fixture(
        include_implementation: bool,
        malformed_templates: bool,
        fixture: GenericDependencyFixture,
    ) -> DependencyInterfaceInput {
        let package = PackageIdentity::try_new("example.dependency")
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
            vec![crate::test_support::source_input(fixture.source, 0)],
            CompilationOptions::new(
                WorkerBudget::serial(),
                ProductKind::Library,
                SelectedTarget::baseline(),
            ),
        )
        .with_package_interface_export(export)
        .with_platform_services(fixture.platform_service.map(|(role, declaration)| {
            PlatformServiceBinding::try_new(role, declaration)
                .unwrap_or_else(|| panic!("fixture platform binding must validate"))
        }));

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

        match fixture.runtime_frames {
            Some(expected) => {
                let [runtime] = bundle.semantics().runtime_requirements() else {
                    panic!("generic callable family must publish one runtime requirement");
                };

                assert_eq!(runtime.frames().len(), expected);
                assert!(runtime.requirements().capabilities().is_empty());
            }
            None => assert!(bundle.semantics().runtime_requirements().is_empty()),
        }

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
                    InterfaceExecutableTemplate::new(
                        template.owner(),
                        template.identity(),
                        template.family_size(),
                        [0_u8],
                    )
                    .unwrap_or_else(|| panic!("malformed test payload must remain nonempty"))
                } else {
                    template.clone()
                }
            })
            .collect::<Vec<_>>();

        let implementation = PackageImplementationArtifact::try_new(
            &validated,
            bundle.surface(),
            bundle.semantics(),
            bundle.implementation_configuration().clone(),
            [],
            templates,
            [],
            [],
            InterfaceValidationLimits::default(),
        )
        .unwrap_or_else(|error| panic!("dependency implementation must encode: {error:?}"));

        assert_eq!(
            bundle.executable_templates().len(),
            fixture.executable_templates
        );

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
    ) -> bray_symbols::ImportedSemanticAddress {
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
            .filter_map(|function| skeleton.imported_semantic_address(function.id().into()))
            .next()
            .unwrap_or_else(|| panic!("imported generic function must have a template address"))
    }

    fn imported_nested_template_address(
        compilation: &crate::Compilation,
    ) -> crate::fact::ImportedExecutableTemplateAddress {
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
            .filter_map(|function| skeleton.imported_semantic_address(function.id().into()))
            .find_map(|symbol| {
                let root = compilation
                    .imported_executable_template_with_cancellation(
                        crate::fact::ImportedExecutableTemplateAddress::root(symbol),
                        &compilation.state.cancellation,
                    )
                    .ok()?;

                root.value()
                    .as_ref()?
                    .operations()
                    .iter()
                    .find_map(|operation| {
                        let MirOperationKind::AnonymousCallable(
                            bray_ir::MirAnonymousCallableReference::Imported(key),
                        ) = operation.kind()
                        else {
                            return None;
                        };

                        Some(crate::fact::ImportedExecutableTemplateAddress::new(
                            symbol,
                            key.template(),
                        ))
                    })
            })
            .unwrap_or_else(|| panic!("imported generic callable must reference a nested template"))
    }
}
