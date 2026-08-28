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
            .map_err(|error| Arc::new(NativeProductPlanningError::Query(error)))?;

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
            .map_err(|error| Arc::new(NativeProductPlanningError::Query(error)))?;

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

        let requirements = match host {
            Some(host) if host.requirements().requires_implementation() => {
                // Runtime selection owns the Arc-backed executable requirements.
                host.requirements().clone()
            }
            Some(_) | None => {
                let Some(product_host) = product_host else {
                    return Ok(None);
                };

                let mut roles = mappings
                    .iter()
                    .flat_map(bray_codegen::CodegenMappings::symbols)
                    .filter_map(|symbol| match symbol.key() {
                        bray_codegen::CodegenSymbolKey::Runtime(reference) => {
                            Some(reference.role())
                        }
                        bray_codegen::CodegenSymbolKey::Instance(_)
                        | bray_codegen::CodegenSymbolKey::ProtectedFrame { .. } => None,
                    })
                    .collect::<Vec<_>>();

                roles.push(bray_runtime_interface::RuntimeAbiRole::ProductHostControl);

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
        MirCallTarget, MirHelperReference, MirHostOperation, MirOperationKind, MirTerminatorKind,
        MirUnitKey, MirUnitKind,
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

    const TARGET_FENCE_SOURCE: &str = concat!(
        "module app;\n",
        "\n",
        "@copy\n",
        "public union FenceChoice\n",
        "{\n",
        "    Acquire;\n",
        "    Release;\n",
        "    AcquireRelease;\n",
        "    SequentiallyConsistent;\n",
        "}\n",
        "\n",
        "public trusted func fence(order: FenceChoice) uses(intrinsic)\n",
        "{\n",
        "    match order\n",
        "    {\n",
        "        case FenceChoice.Acquire\n",
        "        {\n",
        "            trusted core.target.hardware_fence(MemoryOrder.Acquire);\n",
        "        }\n",
        "        case FenceChoice.Release\n",
        "        {\n",
        "            trusted core.target.hardware_fence(MemoryOrder.Release);\n",
        "        }\n",
        "        case FenceChoice.AcquireRelease\n",
        "        {\n",
        "            trusted core.target.hardware_fence(MemoryOrder.AcquireRelease);\n",
        "        }\n",
        "        case FenceChoice.SequentiallyConsistent\n",
        "        {\n",
        "            trusted core.target.hardware_fence(\n",
        "                MemoryOrder.SequentiallyConsistent,\n",
        "            );\n",
        "        }\n",
        "    }\n",
        "}\n",
    );

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

    const STRUCTURAL_ASSEMBLY_SOURCE: &str = concat!(
        "module app;\n",
        "\n",
        "public trusted func assemble(pos value: i32) -> i32\n",
        "    uses(device_memory, intrinsic, raw_memory, unchecked_alias, unchecked_init)\n",
        "{\n",
        "    let outputs: (i32, i32) = trusted core.target.assembly<\n",
        "        (i32, i32, i32),\n",
        "        (i32, i32),\n",
        "    >(\n",
        "        template = \"\",\n",
        "        constraints = \"+reg,=reg,reg,i\",\n",
        "        clobbers = \"\",\n",
        "        features = \"\",\n",
        "        options = 1,\n",
        "        inputs = (value, value, 7),\n",
        "    );\n",
        "\n",
        "    return outputs.0;\n",
        "}\n",
        "\n",
        "func alternate() -> never\n",
        "{\n",
        "    loop {}\n",
        "}\n",
        "\n",
        "func generic_alternate<T>(pos value: T) -> never\n",
        "{\n",
        "    loop {}\n",
        "}\n",
        "\n",
        "public trusted func assemble_addresses(pos pointer: RawPointer<u8>)\n",
        "    uses(device_memory, intrinsic, raw_memory, unchecked_alias, unchecked_init)\n",
        "{\n",
        "    let ignored: (i32,) = trusted core.target.assembly<\n",
        "        (func() -> never, func(pos value: i32) -> never, RawPointer<u8>),\n",
        "        (i32,),\n",
        "    >(\n",
        "        template = \"\",\n",
        "        constraints = \"=reg,s,s,m\",\n",
        "        clobbers = \"\",\n",
        "        features = \"\",\n",
        "        options = 1,\n",
        "        inputs = (alternate, generic_alternate, pointer),\n",
        "    );\n",
        "}\n",
        "\n",
        "public trusted func branch(pos value: i32) -> i32\n",
        "    uses(device_memory, intrinsic, raw_memory, unchecked_alias, unchecked_init)\n",
        "{\n",
        "    let output: (i32,) = trusted core.target.branching_assembly<\n",
        "        (i32,),\n",
        "        (i32,),\n",
        "        (func() -> never,),\n",
        "    >(\n",
        "        template = \"\",\n",
        "        constraints = \"+reg,label\",\n",
        "        clobbers = \"\",\n",
        "        features = \"\",\n",
        "        options = 1,\n",
        "        inputs = (value,),\n",
        "        labels = (alternate,),\n",
        "    );\n",
        "\n",
        "    return output.0;\n",
        "}\n",
    );

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
            [bray_runtime_interface::native_platform_service_role_symbol(
                PlatformServiceRole::StandardOutputWrite,
            )],
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
                &BTreeSet::from(
                    [bray_runtime_interface::native_platform_service_role_symbol(
                        PlatformServiceRole::StandardOutputWrite,
                    )],
                ),
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
                &BTreeSet::from(
                    [bray_runtime_interface::native_platform_service_role_symbol(
                        PlatformServiceRole::FileRead,
                    )],
                ),
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
                &BTreeSet::from(
                    [bray_runtime_interface::native_platform_service_role_symbol(
                        PlatformServiceRole::StandardOutputWrite,
                    )],
                ),
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

        let plan = compilation
            .native_product_plan(
                product.clone(),
                crate::BuildConfiguration::Development,
                None,
                [],
                Some(&test_linker()),
            )
            .unwrap_or_else(|error| panic!("native plan must resolve: {error:?}"));

        let release = compilation
            .native_product_plan(
                product.clone(),
                crate::BuildConfiguration::Release,
                None,
                [],
                Some(&test_linker()),
            )
            .unwrap_or_else(|error| panic!("release native plan must resolve: {error:?}"));

        let host = plan
            .executable_host()
            .unwrap_or_else(|| panic!("executable must retain its host"));

        assert!(!host.requirements().requires_implementation());
        assert_eq!(host.runtime_artifact(), None);

        let archive = TemporaryFile::write("libbray_runtime.a", b"!<arch>\n");
        let available_runtime = runtime_artifact(&compilation, archive.path());

        compilation
            .native_product_plan(
                product,
                crate::BuildConfiguration::Development,
                Some(available_runtime),
                [],
                Some(&test_linker()),
            )
            .unwrap_or_else(|error| {
                panic!("unused available runtime must not create an empty selection: {error:?}")
            });

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

        let [operation] = alternate.operations() else {
            panic!("alternate trampoline must contain one callback call");
        };

        let operation = mir
            .operation(*operation)
            .unwrap_or_else(|| panic!("alternate callback operation must exist"));

        let MirOperationKind::Call(call) = operation.kind() else {
            panic!("alternate trampoline must call its checked label");
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

        assert!(matches!(
            alternate.terminator().kind(),
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

                assert!(helper_references.contains(&&MirHelperReference::Finalize(error)));

                assert!(helper_references.contains(&&MirHelperReference::Destroy(error)));

                let ir =
                    generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendIr)
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>();

                let ir = String::from_utf8(ir)
                    .unwrap_or_else(|error| panic!("LLVM IR must be UTF-8: {error}"));

                assert!(ir.contains("entry.failure"));
                assert!(ir.contains("bray_runtime_entry_failure_reporting"));
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
        let (backend, plan) = runtime_native_plan(include_str!(
            "../../../../../../xtask/fixtures/native-execution/sync-panic.bray"
        ));

        let host = plan
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
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
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

        for unit in units.iter() {
            let mappings = compilation
                .codegen_mappings_for_product(
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
                        &cancellation,
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
                    unit,
                    None,
                    &BTreeSet::new(),
                    &target,
                    &roots,
                    &reachability,
                    false,
                    &cancellation,
                )
                .unwrap_or_else(|error| panic!("consumer mappings must realize: {error:?}"));
        }
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
            product_instances
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

        let (_, plan) = runtime_native_plan(source);

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
    fn static_mapping_retains_asynchronous_fallible_finalizer() {
        let source = concat!(
            "module app;\n",
            "struct Resource { mut state: i32; }\n",
            "impl Resource\n",
            "{\n",
            "    async finalize() -> Result<unit, i32>\n",
            "    {\n",
            "        self.state = 2;\n",
            "        return await finish();\n",
            "    }\n",
            "    destruct() { self.state = 3; }\n",
            "}\n",
            "async func finish() -> Result<unit, i32> { return Error(42); }\n",
            "static RESOURCE: Resource = Resource { state = 1 };\n",
            "func main() {}\n",
        );

        let (backend, plan) = runtime_native_plan(source);

        let finalization = plan
            .mappings()
            .iter()
            .flat_map(bray_codegen::CodegenMappings::static_storages)
            .find_map(bray_codegen::CodegenStaticStorageMapping::finalization)
            .unwrap_or_else(|| panic!("asynchronous static finalizer must be retained"));

        assert_eq!(
            finalization.execution(),
            bray_symbols::CallableExecution::Asynchronous
        );

        assert!(
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );

        let (windows_backend, windows_plan) = runtime_native_plan_for_target(
            source,
            ProductKind::Executable,
            SelectedTarget::for_native(NativeTarget::X86_64WindowsMsvc),
        );

        assert!(
            generated_artifacts(&windows_backend, &windows_plan)
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
        assert!(requirements.requires_role(RuntimeAbiRole::FrameCompletionMove));

        assert!(
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }

    #[test]
    fn native_exports_and_opaque_storage_survive_reachability_and_codegen() {
        let source = concat!(
            "module app;\n",
            "@layout(c, size = 40, align = 8)\n",
            "struct NativeMutex;\n",
            "@link(name = \"native\")\n",
            "@symbol(name = \"native_mutex\")\n",
            "extern trusted static NATIVE_MUTEX_STORAGE: NativeMutex;\n",
            "@link(name = \"native\")\n",
            "@symbol(name = \"native_pointer\")\n",
            "extern trusted static mut NATIVE_POINTER: RawPointer<u8>;\n",
            "@symbol(name = \"unused_export\")\n",
            "static UNUSED_EXPORT: i32 = 7;\n",
            "@symbol(name = \"weak_export\", binding = weak)\n",
            "@abi(c)\n",
            "func weak_export() -> i32\n",
            "{\n",
            "    return 1;\n",
            "}\n",
            "func main()\n",
            "{\n",
            "    let value: i32 = weak_export();\n",
            "    let pointer: RawPointer<NativeMutex> = NATIVE_MUTEX_STORAGE;\n",
            "    let pointer_storage: RawPointer<RawPointer<u8>> = NATIVE_POINTER;\n",
            "}\n",
        );

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

        let platform_symbol = bray_runtime_interface::native_platform_service_role_symbol(
            PlatformServiceRole::StandardOutputFlush,
        );

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

        let symbol = bray_runtime_interface::native_platform_service_role_symbol(
            PlatformServiceRole::StandardOutputFlush,
        );

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
        let backend = Arc::new(
            bray_codegen_llvm::LlvmCodeGenerator::try_new()
                .unwrap_or_else(|error| panic!("LLVM backend must initialize: {error:?}")),
        );

        let registry =
            CodeGeneratorRegistry::try_new([Arc::clone(&backend) as Arc<dyn CodeGenerator>])
                .unwrap_or_else(|error| panic!("LLVM backend must register: {error:?}"));

        let codegen = CodegenConfiguration::try_new(registry, backend.identity().clone())
            .unwrap_or_else(|error| panic!("LLVM backend must select: {error:?}"));

        let runtime_dependency =
            crate::test_support::runtime_standard_library_dependency(&target);

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
        .with_platform_services(platform_services);

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

        let roles: Vec<_> = roles.into_iter().collect();

        let bindings = roles.iter().copied().map(|role| {
            let symbol = BinarySymbolName::try_new(format!("bray_runtime_{}", role.as_str()))
                .unwrap_or_else(|| panic!("test runtime role symbol must be valid"));

            RuntimeRoleBinding::new(role, symbol, RuntimeRoleImplementation::BrayRuntime)
        });

        let version = RuntimeAbiVersion::new(1, 0);

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
                    .filter(|role| *role != RuntimeAbiRole::TestEntrySelection),
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

    #[derive(Clone, Copy)]
    struct GenericDependencyFixture {
        source: &'static str,
        runtime_frames: Option<usize>,
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
    };

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

        assert_eq!(bundle.executable_templates().len(), 3);

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
