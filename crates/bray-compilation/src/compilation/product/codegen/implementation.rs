use std::collections::BTreeSet;
use std::sync::Arc;

use bray_base::shared_slice;
use bray_codegen::{
    AssemblySyntaxKind, CodegenInstanceKey, DebugInformationMode, DebugInformationOutputMode,
    LinkableArtifactKind,
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
        linker: Option<(&Linker, bray_linker::LinkedProductKind)>,
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
        linker: Option<(&Linker, bray_linker::LinkedProductKind)>,
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
            linker.map(|(_, kind)| kind),
            runtime.as_ref().map(|runtime| {
                runtime
                    .metadata()
                    .native_indexes()
                    .iter()
                    .map(|index| {
                        crate::fact::RuntimeNativeIndexQueryIdentity::new(
                            runtime.contract().artifact().clone(),
                            index.purpose(),
                            index.digest(),
                            runtime.directory().join(index.file_name()),
                        )
                    })
                    .collect::<Vec<_>>()
                    .into()
            }),
            Arc::from(required_capabilities.clone()),
            Arc::from(
                linker
                    .into_iter()
                    .flat_map(|(linker, _)| linker.driver_identities())
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
                CompilationFactKey::NativeProduct(Arc::new(key)),
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
        linker: Option<(&Linker, bray_linker::LinkedProductKind)>,
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
                root.instance()
                    .callable_instance()
                    .is_some_and(|callable| entry_definitions.contains(&callable.definition()))
            })
            .map(|root| root.instance().clone())
            .collect::<Vec<_>>();

        // Native product plans retain the exact immutable catalog selected for this host.
        let test_catalog = test_discovery
            .as_ref()
            .map(|discovery| discovery.value().catalog().clone());

        let final_image = semantic.value().kind() != ProductKind::Library
            || linker
                .is_some_and(|(_, kind)| kind == bray_linker::LinkedProductKind::SharedLibrary);

        let (host, runtime, units, mappings, host_statics, native_statics, native_main_thread) =
            self.prepare_native_codegen(
                &product,
                semantic.value().kind(),
                final_image,
                source_roots,
                &entry_roots,
                runtime.as_ref(),
                required_capabilities,
                &target,
                options,
                configuration.uses_thin_lto(),
                cancellation,
            )?;

        let product_host = self.profile_native_product_operation(
            crate::profile::ProfileOperation::NativePlanFinalization,
            || {
                self.codegen_product_host_mapping(
                    &product,
                    &mappings,
                    &host_statics,
                    &native_statics,
                    &target,
                )
            },
        )?;

        let product_host = product_host.map(|host| host.with_final_image(final_image));

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
                .with_bitcode_semantics(
                    if configuration.uses_thin_lto()
                        || configuration == crate::BuildConfiguration::ObjectRelease
                    {
                        bray_codegen::BackendBitcodeSemantics::ThinLto
                    } else {
                        bray_codegen::BackendBitcodeSemantics::Plain
                    },
                );

        let policy = BackendEmissionPolicy::new(
            options.debug_information(),
            debug_output,
            Some(linkable_artifact),
            serialization,
        );

        let backend = self.profile_native_product_operation(
            crate::profile::ProfileOperation::NativePlanFinalization,
            || {
                Ok(EmissionBackend::new(
                    codegen.selected().clone(),
                    codegen.selected_capabilities().clone(),
                    units.iter().map(|unit| unit.key().clone()),
                    policy,
                ))
            },
        )?;

        let link = self.profile_native_product_operation(
            crate::profile::ProfileOperation::NativePlanFinalization,
            || {
                linker
                    .map(|(_, kind)| {
                        self.product_link_inputs(
                            kind,
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

        let (link, selected_native) = match link {
            Some((link, payloads)) => (Some(link), payloads),
            None => (None, Vec::new()),
        };

        // The plan owns static instance identities independently of its mapping tables.
        let static_instances = mappings
            .iter()
            .flat_map(bray_codegen::CodegenMappings::static_storages)
            .map(|mapping| mapping.instance().clone())
            .collect::<BTreeSet<_>>();

        let native_statics = product_host
            .iter()
            .flat_map(|host| host.statics())
            .map(|entry| {
                let local = host_statics.iter().find(|local| {
                    super::super::realization::generated_identity("static_host", local.key())
                        == entry.identity().bytes()
                });

                let native = native_statics
                    .iter()
                    .find(|native| native.identity() == entry.identity().bytes());

                let requires_main_thread = local
                    .is_some_and(|local| local.requires_main_thread_cleanup())
                    || native.is_some_and(|native| native.requires_main_thread());

                let requires_host = local.is_some_and(|local| local.requires_host())
                    || native.is_some_and(|native| native.requires_host());

                let order_key = local
                    .map(|local| local.order_key())
                    .or_else(|| native.map(|native| native.order_key()))
                    .expect("retained static must have a structural cleanup key");

                bray_native_artifact::NativeStatic::new(
                    bray_base::NonEmptySharedStr::try_new(entry.host_symbol().as_str())
                        .expect("host symbol must be nonempty"),
                    entry.identity().bytes(),
                    Arc::from(order_key),
                    entry.duration(),
                    entry.dependencies().iter().map(|identity| identity.bytes()),
                    requires_host,
                    requires_main_thread,
                )
            })
            .collect::<Vec<_>>();

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
            native_statics: shared_slice(native_statics),
            native_main_thread,
            selected_native: shared_slice(selected_native),
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

    pub(super) fn profile_runtime_selection(&self, runtime: Option<&RuntimeArtifactSelection>) {
        let Some(profile) = self.state.fact_runtime.profile() else {
            return;
        };

        let Some(runtime) = runtime else {
            return;
        };

        profile.record_metric(
            crate::profile::ProfileMetricKind::RuntimeNativeUnits,
            u64::try_from(runtime.native_units().len()).unwrap_or(u64::MAX),
        );

        let mut bytes = 0_u64;

        for unit in runtime.native_units() {
            let unit_bytes = unit.path().metadata().map_or(0, |metadata| metadata.len());

            bytes = bytes.saturating_add(unit_bytes);

            profile.add_runtime_artifact(bray_profile::CompilationProfileRuntimeArtifact {
                identity: unit.path().file_name().map_or_else(
                    || unit.path().display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                ),
                bytes: unit_bytes,
            });
        }

        profile.record_metric(crate::profile::ProfileMetricKind::RuntimeNativeBytes, bytes);
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
        CodegenLinkage, CodegenOptions, CodegenPartitionPolicy, CodegenRequest,
        CodegenResultMapping, CodegenSpecialization, CodegenStatus, DebugInformationMode,
        LinkableArtifactKind, LinkableArtifactRequirement, OptimizationLevel,
        partition_codegen_units,
    };
    use bray_compiler_known::RepresentationRole;
    use bray_ir::{
        MirBinaryOperator, MirCallTarget, MirHelperReference, MirHostOperation, MirOperand,
        MirOperationKind, MirProjectionKind, MirTerminatorKind, MirUnitKey, MirUnitKind,
    };
    use bray_linker::{
        LinkFailure, LinkInputSource, LinkModel, LinkOutcome, LinkPlan, Linker, LinkerDriver,
        LinkerDriverCapabilities, LinkerDriverIdentity, LinkerDriverKind,
    };
    use bray_native_artifact::{
        NativeArtifactIndex, NativeContentDigest, NativeDefinition, NativeDefinitionSelection,
        NativeUnit, NativeUnitKind, NativeUnitSummary,
    };
    use bray_package_interface::{
        CURRENT_TEMPLATE_SCHEMA_REVISION, ImplementationExternalSymbolIdentity,
        ImportedSemanticRecord, InterfaceExecutableTemplate, InterfaceLanguageRevision,
        InterfaceNativeBinding, InterfaceProductIdentity, InterfaceProductKind,
        InterfaceSemanticRecordKind, InterfaceValidationLimits, InterfaceValidationPolicy,
        PackageImplementationArtifact, PackageImplementationSpecializationKey,
        PackageInterfaceIdentity, ValidatedPackageInterface, encode_package_interface,
    };
    use bray_runtime_interface::{
        BinarySymbolName, ExecutableEntryResult, ExecutableHostContractBuildError,
        PlatformServiceBinding, PlatformServiceRole, ProtectedFrameAbiVersions,
        ProtectedFrameOperation, RootExecution, RuntimeAbiRole, RuntimeAbiVersion, RuntimeArtifact,
        RuntimeArtifactComponentMetadata, RuntimeArtifactId, RuntimeArtifactMetadata,
        RuntimeArtifactPurpose, RuntimeCapability, RuntimeCompatibilityError, RuntimeContract,
        RuntimeIdentity, RuntimeRoleBinding, RuntimeRoleImplementation,
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
        NativeSymbolContract, PackageIdentity, ProductIdentity, ProductKind, StaticStorageDuration,
        SymbolOrigin, TraitApplicationData, TypeData,
    };
    use bray_target::{NativeTarget, TargetAddressSpaces, TargetProfile, TargetProperties};
    use bray_testing::TemporaryFile;

    use super::super::super::specialization::{
        ConcreteCodegenInstance, ConcreteCodegenReachability,
    };
    use super::super::{ConcreteCodegenRoot, NativeDemandReason};
    use super::NativeProductPlanningError;
    use crate::compilation::CodegenPreparationError;
    use crate::{
        CancellationToken, CompilationOptions, CompilationProfileConfiguration,
        CompilationProfileMode, CompilationRequest, DependencyInterfaceInput,
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
        "        inputs = (value, if value == 0 { yield 1; } else { yield value; }, 7),\n",
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
    fn source_authority_selects_native_provider_units_and_flags() {
        let directory = tempfile::tempdir().unwrap();
        let selected = SelectedTarget::baseline();
        let target = selected.profile().identity().clone();
        let native_target = NativeTarget::for_identity(&target).unwrap();
        let runtime_abi = selected.runtime_abi();

        let prefix = format!(
            "targets/{}/{}.{}",
            target.as_str(),
            runtime_abi.major(),
            runtime_abi.minor()
        );

        let artifacts = [(
            StandardLibraryArtifactKind::PackageInterface,
            "std.brayi",
            b"interface".as_slice(),
        )]
        .into_iter()
        .map(|(kind, name, bytes)| {
            let artifact =
                StandardLibraryArtifact::try_for_bytes(kind, format!("{prefix}/{name}"), bytes)
                    .unwrap();

            let path = artifact.beneath(directory.path());

            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();

            artifact
        })
        .collect::<Vec<_>>();

        let mut payloads = BTreeMap::new();
        let mut unit_digests = BTreeMap::new();

        let units = [
            ("fallback", None, None),
            (
                "streams",
                Some(PlatformServiceRole::StandardOutputWrite),
                Some("c"),
            ),
            (
                "filesystem",
                Some(PlatformServiceRole::FileRead),
                Some("filesystem"),
            ),
            (
                "process",
                Some(PlatformServiceRole::ChildSpawn),
                Some("process"),
            ),
        ]
        .into_iter()
        .map(|(name, role, library)| {
            let bytes = name.as_bytes();
            let digest = NativeContentDigest::new(bray_base::sha256_reader(bytes).unwrap());

            let kind = if role.is_some() {
                NativeUnitKind::Object
            } else {
                NativeUnitKind::OpaqueArchive
            };

            let summary = role.map_or(NativeUnitSummary::opaque([]), |role| {
                NativeUnitSummary::Exact {
                    definitions: Arc::from([NativeDefinition::new(
                        NativeSymbolContract::required_name(
                            NonEmptySharedStr::try_new(role.native_symbol()).unwrap(),
                        ),
                        NativeDefinitionSelection::Ordinary,
                    )]),
                    references: Arc::from([]),
                    roots: Arc::from([]),
                }
            });

            let links = library.into_iter().map(|name| {
                NativeLinkRequirement::new(
                    NonEmptySharedStr::try_new(name).unwrap(),
                    NativeLinkKind::System,
                )
            });

            payloads.insert(digest, bytes.to_vec());

            unit_digests.insert(name, digest);

            NativeUnit::new(digest, kind, summary, links)
        })
        .collect::<Vec<_>>();

        let index = NativeArtifactIndex::try_new(
            native_target,
            NativeContentDigest::new([7; 32]),
            units,
            [],
        )
        .unwrap();

        let native_index = index.encode().unwrap();
        let bitcode_bytes = b"optional bitcode";

        let bitcode_digest =
            NativeContentDigest::new(bray_base::sha256_reader(bitcode_bytes.as_slice()).unwrap());

        let bitcode_index = NativeArtifactIndex::try_new(
            native_target,
            NativeContentDigest::new([8; 32]),
            [NativeUnit::new(
                bitcode_digest,
                NativeUnitKind::Bitcode,
                NativeUnitSummary::opaque([]),
                [],
            )],
            [],
        )
        .unwrap()
        .encode()
        .unwrap();

        let package = PackageIdentity::try_new("example.nativefixture").unwrap();

        let identity = PackageInterfaceIdentity::try_new(
            package.clone(),
            crate::test_support::package_version(),
            InterfaceProductIdentity::try_new("library").unwrap(),
            InterfaceProductKind::Library,
            "public",
        )
        .unwrap();

        let export =
            PackageInterfaceExportRequest::new(identity, InterfaceLanguageRevision::new(0));

        let producer = crate::Compilation::load(
            CompilationRequest::with_options(
                package,
                vec![crate::test_support::source_input(
                    "module fixture;\n\npublic func fixture()\n{\n}\n",
                    0,
                )],
                CompilationOptions::new(
                    WorkerBudget::serial(),
                    ProductKind::Library,
                    selected.clone(),
                ),
            )
            .with_package_interface_export(export),
        )
        .unwrap();

        assert!(
            producer.check_diagnostics().is_empty(),
            "{:#?}",
            producer.check_diagnostics()
        );

        let bundle = producer
            .package_interface_export_bundle()
            .unwrap()
            .as_ref()
            .unwrap();

        let interface = encode_package_interface(bundle).unwrap();

        let implementation = PackageImplementationArtifact::try_from_export_bundle(
            &interface,
            bundle,
            InterfaceValidationLimits::default(),
        )
        .unwrap();

        let units = payloads
            .into_iter()
            .map(|(digest, bytes)| (digest.bytes(), Arc::from(bytes)))
            .collect::<Vec<_>>();

        let implementation = implementation
            .try_with_native_variants(&[(NativeUnitKind::Object, &native_index)], &units)
            .unwrap();

        let native_implementation = implementation
            .try_native_only_artifact(
                &[(NativeUnitKind::Bitcode, &bitcode_index)],
                &[(bitcode_digest.bytes(), Arc::from(bitcode_bytes.as_slice()))],
            )
            .unwrap();

        let implementation_artifact = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::PackageImplementation,
            format!("{prefix}/std.brayimpl"),
            &implementation.shared_bytes().unwrap(),
        )
        .unwrap();

        fs::write(
            implementation_artifact.beneath(directory.path()),
            &implementation.shared_bytes().unwrap(),
        )
        .unwrap();

        let native_artifact = StandardLibraryArtifact::try_for_bytes(
            StandardLibraryArtifactKind::NativeImplementation,
            format!("{prefix}/std-native.brayimpl"),
            &native_implementation.shared_bytes().unwrap(),
        )
        .unwrap();

        fs::write(
            native_artifact.beneath(directory.path()),
            &native_implementation.shared_bytes().unwrap(),
        )
        .unwrap();

        let manifest = StandardLibraryBundleManifest::try_new([target_artifacts_for_test(
            target.clone(),
            runtime_abi,
            artifacts
                .into_iter()
                .chain([implementation_artifact.clone(), native_artifact.clone()])
                .collect(),
        )
        .unwrap()])
        .unwrap();

        fs::write(
            directory.path().join("manifest.json"),
            encode_standard_library_manifest(&manifest).unwrap(),
        )
        .unwrap();

        let root = StandardLibraryRoot::try_new(directory.path()).unwrap();

        let native_input = bray_package_interface::PackageArtifactInput::file(
            native_artifact.beneath(root.path()),
            Some(native_artifact.digest().bytes()),
        );

        let published = native_input
            .load_implementation()
            .unwrap()
            .native_variant(NativeUnitKind::Bitcode)
            .unwrap()
            .unwrap();

        assert_eq!(published.producer(), NativeContentDigest::new([8; 32]));

        let request = CompilationRequest::with_options(
            PackageIdentity::try_new("std.tests.api").unwrap(),
            vec![crate::test_support::source_input(
                "module application;\n",
                0,
            )],
            CompilationOptions::new(WorkerBudget::serial(), ProductKind::Executable, selected),
        )
        .with_native_implementations([
            bray_package_interface::PackageArtifactInput::file(
                implementation_artifact.beneath(root.path()),
                None,
            ),
            bray_package_interface::PackageArtifactInput::file(
                native_artifact.beneath(root.path()),
                None,
            ),
        ])
        .with_standard_library_source_authority();

        let compilation = crate::Compilation::load(request).unwrap();

        assert!(compilation.dependency_interfaces().is_empty());

        let select = |role: PlatformServiceRole| {
            compilation
                .native_library_link_inputs(
                    ProductKind::Executable,
                    &BTreeSet::from([role.native_symbol()]),
                    &BTreeSet::new(),
                )
                .unwrap()
        };

        let (streams_links, streams) = select(PlatformServiceRole::StandardOutputWrite);

        assert_eq!(streams.len(), 2);

        assert!(
            streams
                .iter()
                .any(|unit| unit.digest == unit_digests["fallback"]
                    && unit.bytes.as_ref() == b"fallback")
        );

        assert!(streams.iter().any(
            |unit| unit.digest == unit_digests["streams"] && unit.bytes.as_ref() == b"streams"
        ));

        assert!(
            streams_links
                .iter()
                .any(|input| input.source() == &LinkInputSource::try_native_library("c").unwrap())
        );

        assert!(
            streams
                .iter()
                .all(|unit| unit.digest != unit_digests["process"])
        );

        let (_, filesystem) = select(PlatformServiceRole::FileRead);

        assert!(
            filesystem
                .iter()
                .any(|unit| unit.digest == unit_digests["filesystem"])
        );

        assert!(
            filesystem
                .iter()
                .all(|unit| unit.digest != unit_digests["streams"])
        );

        let overridden = compilation
            .native_library_link_inputs(
                ProductKind::Test,
                &BTreeSet::from([PlatformServiceRole::StandardOutputWrite.native_symbol()]),
                &BTreeSet::from([PlatformServiceRole::StandardOutputWrite]),
            )
            .unwrap();

        assert!(overridden.0.is_empty());
        assert_eq!(overridden.1.len(), 1);
        assert_eq!(overridden.1[0].digest, unit_digests["fallback"]);

        assert!(
            compilation
                .native_library_link_inputs(
                    ProductKind::Library,
                    &BTreeSet::new(),
                    &BTreeSet::new(),
                )
                .unwrap()
                .1
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
                Some((&test_linker(), bray_linker::LinkedProductKind::Executable)),
            )
            .unwrap_or_else(|error| panic!("native plan must resolve: {error:?}"));

        let release = compilation
            .native_product_plan(
                product.clone(),
                crate::BuildConfiguration::Release,
                Some(available_runtime.clone()),
                [],
                Some((&test_linker(), bray_linker::LinkedProductKind::Executable)),
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
                Some((&test_linker(), bray_linker::LinkedProductKind::Executable)),
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
    fn native_demand_inventory_is_stable_for_imports_exports_and_static_lifecycle() {
        let source = concat!(
            "module application;\n",
            "using example.dependency.templates.identity;\n",
            "internal struct Resource {}\n",
            "impl Resource\n",
            "{\n",
            "    finalize() {}\n",
            "}\n",
            "internal static RESOURCE: Resource = Resource {};\n",
            "public func exported() -> i32\n",
            "{\n",
            "    return example.dependency.templates.identity<i32>(1);\n",
            "}\n",
        );

        let serial =
            profiled_dependency_library(source, WorkerBudget::serial(), PROFILE_DEPENDENCY);

        let parallel = profiled_dependency_library(
            source,
            WorkerBudget::new(4).unwrap_or_else(|error| {
                panic!("parallel test worker budget must validate: {error:?}")
            }),
            PROFILE_DEPENDENCY,
        );

        let reordered = profiled_dependency_library(
            source,
            WorkerBudget::serial(),
            REORDERED_PROFILE_DEPENDENCY,
        );

        let serial = native_profile(&serial);
        let parallel = native_profile(&parallel);
        let reordered = native_profile(&reordered);

        assert_eq!(serial, parallel);
        assert_eq!(serial, reordered);
        assert_eq!(serial.instances.len(), 8);

        assert!(serial.instances.iter().all(|instance| {
            !instance.inclusion_path.is_empty()
                && instance.pre_optimization_blocks == instance.post_optimization_blocks
                && instance.pre_optimization_operations == instance.post_optimization_operations
        }));

        use bray_profile::CompilationProfileNativeDemandKind as Kind;

        for expected in [Kind::LibraryExport, Kind::StaticLifecycle, Kind::DirectCall] {
            assert!(
                serial.demands.iter().any(|demand| demand.kind == expected),
                "missing {expected:?} from {:?}",
                serial
                    .demands
                    .iter()
                    .map(|demand| demand.kind)
                    .collect::<Vec<_>>()
            );
        }

        assert!(serial.units.iter().any(|unit| {
            unit.packages
                .iter()
                .any(|package| package == "example.dependency")
        }));

        let hosted = profiled_product(
            concat!(
                "module application;\n",
                "internal struct HostedResource {}\n",
                "impl HostedResource\n",
                "{\n",
                "    finalize() {}\n",
                "}\n",
                "internal static HOSTED_RESOURCE: HostedResource = HostedResource {};\n",
                "func main() {}\n",
            ),
            ProductKind::Executable,
            WorkerBudget::serial(),
            [],
        );

        let hosted = native_profile(&hosted);

        assert!(
            hosted
                .demands
                .iter()
                .any(|demand| demand.kind == Kind::ExecutableEntry)
        );

        assert!(
            hosted
                .demands
                .iter()
                .any(|demand| demand.kind == Kind::StaticLifecycle)
        );

        let host_root = hosted
            .demands
            .iter()
            .find(|demand| demand.kind == Kind::HostedRoot)
            .expect("hosted product must retain a generated root")
            .target;

        assert!(hosted.runtime_demands.iter().any(|demand| {
            demand.predecessor == Some(host_root)
                && demand.role == RuntimeAbiRole::ProductHostControl.as_str()
                && !demand.provider.is_empty()
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
    fn inlined_callee_parameter_remains_valid_across_blocks() {
        assert_source_emits_valid_native_units(
            concat!(
                "module app;\n",
                "public func choose(pos value: i32) -> bool\n",
                "{\n",
                "    if value == 1 { return true; }\n",
                "    return false;\n",
                "}\n",
                "public func entry(pos value: i32) -> bool\n",
                "{\n",
                "    return choose(value);\n",
                "}\n",
            ),
            crate::BuildConfiguration::Release,
        );
    }

    #[test]
    fn borrowed_union_patterns_emit_valid_native_units() {
        assert_source_emits_valid_native_units(
            concat!(
                "module app;\n",
                "public union Choice { First; Second; }\n",
                "public func select(pos value: &Choice) -> bool\n",
                "{\n",
                "    match value\n",
                "    {\n",
                "        case .First { return true; }\n",
                "        case .Second { return false; }\n",
                "    }\n",
                "}\n",
            ),
            crate::BuildConfiguration::Development,
        );
    }

    #[test]
    fn indexed_static_accesses_emit_valid_native_units() {
        assert_source_emits_valid_native_units(
            concat!(
                "module app;\n",
                "internal static VALUES: [core.atomic.Atomic<usize>; 1] = [\n",
                "    core.atomic.initialize<usize>(7)\n",
                "];\n",
                "public trusted func read() -> usize\n",
                "{\n",
                "    return core.atomic.load<usize, 0>(&VALUES[0]);\n",
                "}\n",
            ),
            crate::BuildConfiguration::Development,
        );
    }

    #[test]
    fn aggregate_static_borrows_emit_valid_native_units_in_either_declaration_order() {
        let types = r#"
            module app;
            struct Counter { value: core.atomic.Atomic<usize>; }
            struct Guard { count: usize; counter: &Counter; }
        "#;

        let counter =
            "static COUNTER: Counter = Counter { value = core.atomic.initialize<usize>(7) };";

        let guards = r#"
            static FIRST: Guard = Guard { count = 1, counter = &COUNTER };
            static SECOND: Guard = Guard { count = 2, counter = &COUNTER };
        "#;

        let root = r#"
            public func read() -> usize
            {
                return FIRST.count + SECOND.count + core.atomic.load<usize, 0>(&COUNTER.value);
            }
        "#;

        for declarations in [
            format!("{guards}\n{counter}"),
            format!("{counter}\n{guards}"),
        ] {
            let source = format!("{types}\n{declarations}\n{root}");

            assert_source_emits_valid_native_units(&source, crate::BuildConfiguration::Development);
        }
    }

    #[test]
    fn imported_aggregate_static_borrows_emit_valid_native_units() {
        let dependency = generic_dependency_from_fixture(
            true,
            false,
            GenericDependencyFixture {
                source: r#"
                    module templates;
                    public struct Counter { public value: core.atomic.Atomic<usize>; }
                    public static COUNTER: Counter = Counter
                    {
                        value = core.atomic.initialize<usize>(7)
                    };
                "#,
                runtime_frames: None,
                executable_templates: 1,
                platform_service: None,
            },
        );

        let source = r#"
            module app;
            using example.dependency.templates;
            struct Guard
            {
                count: usize;
                counter: &example.dependency.templates.Counter;
            }
            static FIRST: Guard = Guard
            {
                count = 1,
                counter = &example.dependency.templates.COUNTER
            };
            static SECOND: Guard = Guard
            {
                count = 2,
                counter = &example.dependency.templates.COUNTER
            };
            public func read() -> usize
            {
                return FIRST.count + SECOND.count;
            }
        "#;

        let backend = Arc::new(bray_codegen_llvm::LlvmCodeGenerator::try_new().unwrap());

        let registry =
            CodeGeneratorRegistry::try_new([Arc::clone(&backend) as Arc<dyn CodeGenerator>])
                .unwrap();

        let codegen = CodegenConfiguration::try_new(registry, backend.identity().clone()).unwrap();
        let target = SelectedTarget::baseline();

        let request = CompilationRequest::with_options(
            crate::test_support::package_identity(),
            vec![crate::test_support::source_input(source, 0)],
            CompilationOptions::new(WorkerBudget::serial(), ProductKind::Library, target.clone()),
        )
        .with_dependency_interfaces([
            dependency,
            crate::test_support::runtime_standard_library_dependency(&target),
        ]);

        let compilation = crate::Compilation::load_with_codegen(request, codegen).unwrap();

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

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

    #[test]
    fn aggregate_static_relocation_preserves_native_export_names() {
        let source = r#"
            module app;
            @layout(c)
            struct Counter { value: usize; }
            struct Guard { count: usize; counter: &Counter; }
            static FIRST: Guard = Guard { count = 1, counter = &COUNTER };
            @symbol(name = "counter")
            static COUNTER: Counter = Counter { value = 7 };
            @symbol(name = "counter.unresolved")
            static OTHER: usize = 9;
            public func read() -> usize
            {
                return FIRST.count + OTHER;
            }
        "#;

        let (backend, compilation) = codegen_compilation_for_product(source, ProductKind::Library);

        let plan = compilation
            .native_product_plan(
                test_product_identity(),
                crate::BuildConfiguration::Development,
                None,
                [],
                None,
            )
            .unwrap();

        let ir = generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendIr);

        assert!(
            ir.iter().any(|artifact| {
                String::from_utf8_lossy(artifact).contains("@counter.unresolved =")
            }),
            "the explicit native export must retain its exact symbol"
        );
    }

    #[test]
    fn never_calls_in_typed_return_paths_emit_valid_native_units() {
        assert_source_emits_valid_native_units(
            concat!(
                "trusted module app;\n",
                "trusted func terminate() -> never uses(intrinsic)\n",
                "{\n",
                "    trusted core.target.abort();\n",
                "}\n",
                "public trusted func select(pos terminate_now: bool) -> u64\n",
                "{\n",
                "    if terminate_now\n",
                "    {\n",
                "        return trusted terminate();\n",
                "    }\n",
                "\n",
                "    return 7;\n",
                "}\n",
            ),
            crate::BuildConfiguration::Development,
        );
    }

    #[test]
    fn imported_execution_guarantees_emit_native_units() {
        let dependency = generic_dependency_from_fixture(
            true,
            false,
            GenericDependencyFixture {
                source: "module templates; func helper<T>() -> bool executes(pure, total) when(true) { ensures(result) } { return true; } public func certified<T>() -> bool executes(pure, total) when(true) { ensures(result) } { return helper<T>(); }",
                runtime_frames: None,
                executable_templates: 2,
                platform_service: None,
            },
        );

        let backend = Arc::new(bray_codegen_llvm::LlvmCodeGenerator::try_new().unwrap());

        let registry =
            CodeGeneratorRegistry::try_new([Arc::clone(&backend) as Arc<dyn CodeGenerator>])
                .unwrap();

        let codegen = CodegenConfiguration::try_new(registry, backend.identity().clone()).unwrap();
        let target = SelectedTarget::baseline();

        let request = CompilationRequest::with_options(
            crate::test_support::package_identity(),
            vec![crate::test_support::source_input("module app; using example.dependency.templates.certified; public func root() -> bool executes(pure, total) when(true) { ensures(result) } { return example.dependency.templates.certified<bool>(); }", 0)],
            CompilationOptions::new(WorkerBudget::serial(), ProductKind::Library, target.clone()),
        ).with_dependency_interfaces([dependency, crate::test_support::runtime_standard_library_dependency(&target)]);

        let compilation = crate::Compilation::load_with_codegen(request, codegen).unwrap();

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:?}",
            compilation.check_diagnostics()
        );

        let plan = compilation
            .native_product_plan(
                test_product_identity(),
                crate::BuildConfiguration::Development,
                None,
                [],
                None,
            )
            .unwrap();

        let artifacts = generated_artifacts(&backend, &plan);

        assert!(!artifacts.is_empty());
        assert!(artifacts.iter().all(|artifact| !artifact.is_empty()));
    }

    #[test]
    fn weakened_callable_execution_contract_emits_native_units() {
        assert_source_emits_valid_native_units(
            r#"
                module app;
                callable Strong = func() -> bool executes(pure, total);
                callable Plain = func() -> bool;

                func supplied() -> bool executes(pure, total)
                {
                    return true;
                }

                public func root() -> bool
                {
                    let provided: Strong = supplied;
                    let weakened = provided as Plain;
                    return weakened();
                }
            "#,
            crate::BuildConfiguration::Development,
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
    fn concrete_literal_array_length_reaches_native_codegen() {
        let source = r#"
            module app;
            struct Guard
            {
                id: i32;

                destruct() {}
            }
            func make_guard(pos id: i32) -> Guard
            {
                return Guard { id = id };
            }
            func main()
            {
                let guards = box([
                    [make_guard(1), make_guard(2)],
                    [make_guard(3), make_guard(4)],
                ]);
            }
        "#;

        let (backend, plan) = runtime_native_plan(source);

        let artifacts = generated_artifacts(&backend, &plan);

        assert!(!artifacts.is_empty());

        assert!(artifacts.iter().all(|artifact| !artifact.is_empty()));
    }

    #[test]
    fn compile_only_emission_preserves_specialized_cleanup_mir() {
        let source = r#"
            module app;
            struct Guard {}
            impl Guard
            {
                finalize() {}
            }
            func dispose<T>(pos value: T) -> i32
            {
                return 7;
            }
            func main() -> i32
            {
                return dispose<[Guard; 2]>([Guard {}, Guard {}]);
            }
        "#;

        let (_, compilation) = codegen_compilation_for_product(source, ProductKind::Executable);

        let archive = TemporaryFile::write("libbray_runtime.a", b"!<arch>\n");
        let runtime = runtime_artifact(&compilation, archive.path());
        let linker = test_linker();

        let plan = compilation
            .native_product_plan(
                test_product_identity(),
                crate::BuildConfiguration::Development,
                Some(runtime),
                [],
                Some((&linker, bray_linker::LinkedProductKind::Executable)),
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

        let linking = plan.link().expect("native plan must retain link inputs");

        let composed = crate::ProductEmissionInputs::new(&outputs)
            .with_native_codegen(&plan)
            .with_linking(&linker, linking);

        let error = compilation
            .emit_product(request.clone(), composed)
            .expect_err("compile-only requests must reject explicitly supplied linking");

        assert!(matches!(
            error.kind(),
            crate::ProductEmissionErrorKind::UnexpectedLinker
        ));

        let outcome = compilation
            .emit_product(
                request,
                crate::ProductEmissionInputs::new(&outputs).with_native_codegen(&plan),
            )
            .expect("compile-only emission must consume specialized cleanup MIR");

        assert!(matches!(
            outcome.status(),
            bray_emitter::EmissionStatus::Complete
        ));

        assert_eq!(outcome.artifacts().artifacts().len(), plan.units().len());

        assert!(
            outcome
                .artifacts()
                .artifacts()
                .iter()
                .all(|artifact| artifact.id().kind() == bray_emitter::ArtifactKind::BackendIr)
        );
    }

    #[test]
    fn fresh_native_plans_ignore_unrelated_semantic_interning() {
        let storage_source = r#"
            trusted module app;

            static GENERIC_VALUE<const N: i32>: i32 = N;

            func first() -> i32
            {
                return GENERIC_VALUE<1>;
            }

            func second() -> i32
            {
                return GENERIC_VALUE<2>;
            }

            @link(name = "native")
            @symbol(name = "native_value")
            extern trusted static NATIVE: i32;

            trusted func repeated_native_pointer() -> RawPointer<i32>
            {
                return NATIVE;
            }

            trusted func native_pointer() -> RawPointer<i32>
            {
                return NATIVE;
            }
        "#;

        for source in [CONCRETE_GENERIC_SOURCE, storage_source] {
            let native_links = if source == CONCRETE_GENERIC_SOURCE {
                Vec::new()
            } else {
                vec![NativeLinkRequirement::new(
                    NonEmptySharedStr::try_new("native").unwrap(),
                    NativeLinkKind::Dynamic,
                )]
            };

            let (first_backend, first_compilation) = codegen_compilation_for_sources_target(
                &[source],
                ProductKind::Library,
                SelectedTarget::baseline(),
                &native_links,
            );

            let (second_backend, second_compilation) = codegen_compilation_for_sources_target(
                &[source],
                ProductKind::Library,
                SelectedTarget::baseline(),
                &native_links,
            );

            second_compilation
                .semantic_value_store()
                .expect("second semantic store must exist")
                .intern_type(TypeData::tuple([]))
                .expect("unrelated type must intern");

            let plan = |compilation: &crate::Compilation| {
                compilation
                    .native_product_plan(
                        test_product_identity(),
                        crate::BuildConfiguration::Development,
                        None,
                        [],
                        None,
                    )
                    .unwrap_or_else(|error| panic!("native plan must prepare: {error:?}"))
            };

            let first = plan(&first_compilation);
            let second = plan(&second_compilation);

            let identities = |plan: &super::NativeProductPlan| {
                plan.units()
                    .iter()
                    .map(|unit| unit.key().content_identity())
                    .collect::<Vec<_>>()
            };

            assert_eq!(
                native_partition_recipe(&first),
                native_partition_recipe(&second)
            );

            assert_eq!(identities(&first), identities(&second));

            if source != CONCRETE_GENERIC_SOURCE {
                let storage_identities = first.units().iter()
                    .flat_map(|unit| unit.instances().iter().map(|instance| {
                        unit.compatibility(instance.key()).unwrap()
                            .native_storage_dependencies_identity()
                    }))
                    .filter(|identity| *identity != [0; 32])
                    .collect::<BTreeSet<_>>();

                assert_eq!(
                    storage_identities.len(),
                    3,
                    "owned specializations and imported storage must have distinct stable identities",
                );
            }

            assert_eq!(
                generated_artifacts(&first_backend, &first),
                generated_artifacts(&second_backend, &second),
            );

            let profile = first_compilation
                .selected_target()
                .target()
                .profile()
                .clone();

            let name = bray_target::TargetOutputName::for_native(
                profile.machine().object_format(),
                bray_target::TargetOutputKind::RelocatableObject,
            );

            let outputs = bray_target::TargetOutputDescription::try_new(profile, [name])
                .expect("native output names must validate");

            let publish = |compilation: &crate::Compilation, native: &super::NativeProductPlan| {
                let output = tempfile::tempdir().expect("managed output directory must exist");

                let request = bray_emitter::EmissionRequest::try_new(
                    test_product_identity(),
                    ProductKind::Library,
                    None,
                    compilation
                        .selected_target()
                        .target()
                        .profile()
                        .identity()
                        .clone(),
                    bray_emitter::RequestedArtifactDestination::FilesystemDirectory(
                        output.path().to_path_buf().into(),
                    ),
                    [bray_emitter::RequestedArtifact::new(
                        bray_emitter::ArtifactKind::RelocatableObject,
                        bray_emitter::ArtifactRequirement::Required,
                    )],
                    bray_emitter::ReplacementPolicy::ReplaceExisting,
                )
                .expect("native emission request must validate");

                let inputs = crate::ProductEmissionInputs::new(&outputs).with_native_codegen(native);

                let result = compilation
                    .emit_product(request, inputs)
                    .unwrap_or_else(|error| panic!("native emission must complete: {error:?}"));

                let generation = result
                    .generation()
                    .expect("managed generation must publish");

                let artifact = &result.artifacts().artifacts()[0];

                let path = generation
                    .artifact_path(artifact.id())
                    .expect("artifact path must resolve");

                std::fs::read(
                    path.parent()
                        .expect("artifact must have a generation directory")
                        .join("manifest.json"),
                )
                .expect("generation manifest must be readable")
            };

            assert_eq!(
                publish(&first_compilation, &first),
                publish(&second_compilation, &second)
            );

            if source == CONCRETE_GENERIC_SOURCE {
                let changed_source = CONCRETE_GENERIC_SOURCE.replace("return count;", "return 12345;");

                let (changed_backend, changed_compilation) =
                    codegen_compilation_for_product(&changed_source, ProductKind::Library);

                let changed = plan(&changed_compilation);

                assert_eq!(
                    native_partition_recipe(&first),
                    native_partition_recipe(&changed)
                );

                assert_ne!(identities(&first), identities(&changed));

                assert_ne!(
                    generated_artifacts(&first_backend, &first),
                    generated_artifacts(&changed_backend, &changed),
                );
            }
        }
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
                Some((&test_linker(), bray_linker::LinkedProductKind::Executable)),
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
            Some((&test_linker(), bray_linker::LinkedProductKind::Executable)),
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
    fn scalar_and_nullable_branches_prune_native_demand_only_when_enabled() {
        let (_, compilation) = codegen_compilation(concat!(
            "module app;\n",
            "\n",
            "func unused()\n",
            "{\n",
            "}\n",
            "\n",
            "func main()\n",
            "{\n",
            "    let enabled: bool = false;\n",
            "    if enabled\n",
            "    {\n",
            "        unused();\n",
            "    }\n",
            "    let absent: i32? = none;\n",
            "    if absent.is_present()\n",
            "    {\n",
            "        unused();\n",
            "    }\n",
            "}\n",
        ));

        let cancellation = CancellationToken::new();

        let target = compilation
            .selected_target()
            .target()
            .codegen_target()
            .expect("test target must validate");

        let semantic = compilation
            .product_semantics()
            .expect("test product must resolve");

        let roots = compilation
            .product_root_instances(semantic.value(), None, &target, &cancellation)
            .expect("test roots must resolve");

        let none = compilation
            .codegen_reachability(
                roots.clone(),
                None,
                &target,
                CodegenOptions::default(),
                false,
                &cancellation,
            )
            .expect("unoptimized reachability must close");

        let basic = compilation
            .codegen_reachability(
                roots.clone(),
                None,
                &target,
                crate::BuildConfiguration::Development.codegen_options(),
                false,
                &cancellation,
            )
            .expect("development reachability must close");

        let full = compilation
            .codegen_reachability(
                roots,
                None,
                &target,
                crate::BuildConfiguration::Release.codegen_options(),
                false,
                &cancellation,
            )
            .expect("release reachability must close");

        assert_eq!(none.graph().instances().len(), 2);
        assert_eq!(basic.graph().instances().len(), 1);
        assert_eq!(full.graph().instances().len(), 1);
        assert!(basic.graph().instances()[0].mir().is_valid());

        let partition_work = |reachability: &ConcreteCodegenReachability| {
            let roots = reachability.graph().roots().iter().cloned().collect();

            let units = partition_codegen_units(
                CodegenPartitionPolicy::NATIVE_BALANCED,
                reachability.graph(),
                |instance| {
                    Some(
                        compilation
                            .codegen_partition_compatibility(
                                instance,
                                reachability.instance(instance.key()).expect("retained instance must be concrete"),
                                &test_product_identity(),
                                &roots,
                                &cancellation,
                            )
                            .expect("optimized partition metadata must resolve"),
                    )
                },
                |mir| {
                    crate::compilation::product::mir_content_identity(&compilation, mir)
                        .expect("optimized partition content must resolve")
                },
            )
            .expect("optimized semantic demand must partition");

            units
                .iter()
                .map(|unit| unit.estimated_work().units())
                .sum::<u64>()
        };

        assert!(partition_work(&none) > partition_work(&basic));
        assert_eq!(partition_work(&basic), partition_work(&full));

        assert_eq!(
            basic.graph().instances()[0].mir(),
            full.graph().instances()[0].mir()
        );
    }

    #[test]
    fn numeric_constant_branch_removes_its_native_dependency() {
        let (_, compilation) = codegen_compilation(concat!(
            "module app;\n",
            "func unused() {}\n",
            "func main()\n",
            "{\n",
            "    let seed: i32 = 6;\n",
            "    let value: i32 = seed * 7;\n",
            "    if value != 42\n",
            "    {\n",
            "        unused();\n",
            "    }\n",
            "}\n",
        ));

        assert!(compilation.check_diagnostics().is_empty());

        let cancellation = CancellationToken::new();

        let target = compilation
            .selected_target()
            .target()
            .codegen_target()
            .unwrap();

        let semantic = compilation.product_semantics().unwrap();

        let roots = compilation
            .product_root_instances(semantic.value(), None, &target, &cancellation)
            .unwrap();

        let none = compilation
            .codegen_reachability(
                roots.clone(),
                None,
                &target,
                CodegenOptions::default(),
                false,
                &cancellation,
            )
            .unwrap();

        let basic = compilation
            .codegen_reachability(
                roots.clone(),
                None,
                &target,
                crate::BuildConfiguration::Development.codegen_options(),
                false,
                &cancellation,
            )
            .unwrap();

        let full = compilation
            .codegen_reachability(
                roots,
                None,
                &target,
                crate::BuildConfiguration::Release.codegen_options(),
                false,
                &cancellation,
            )
            .unwrap();

        assert_eq!(none.graph().instances().len(), 2);
        assert_eq!(basic.graph().instances().len(), 1);
        assert_eq!(full.graph().instances().len(), 1);

        assert_eq!(
            basic.graph().instances()[0].mir(),
            full.graph().instances()[0].mir()
        );

        assert!(basic.graph().instances()[0].mir().is_valid());
    }

    #[test]
    fn repeated_scalar_expression_uses_one_dominating_result() {
        let (_, compilation) = codegen_compilation(concat!(
            "module app;\n",
            "func main() -> i32\n",
            "{\n",
            "    return (6 * 7) + (6 * 7);\n",
            "}\n",
        ));

        assert!(compilation.check_diagnostics().is_empty());

        let cancellation = CancellationToken::new();

        let target = compilation
            .selected_target()
            .target()
            .codegen_target()
            .unwrap();

        let semantic = compilation.product_semantics().unwrap();

        let roots = compilation
            .product_root_instances(semantic.value(), None, &target, &cancellation)
            .unwrap();

        let none = compilation
            .codegen_reachability(
                roots.clone(),
                None,
                &target,
                CodegenOptions::default(),
                false,
                &cancellation,
            )
            .unwrap();

        let basic = compilation
            .codegen_reachability(
                roots,
                None,
                &target,
                crate::BuildConfiguration::Development.codegen_options(),
                false,
                &cancellation,
            )
            .unwrap();

        let multiply_count = |graph: &bray_codegen::CodegenReachability| {
            graph
                .instances()
                .iter()
                .flat_map(|instance| instance.mir().operations())
                .filter(|operation| {
                    matches!(
                        operation.kind(),
                        MirOperationKind::Binary {
                            operator: MirBinaryOperator::Multiply,
                            ..
                        }
                    )
                })
                .count()
        };

        assert_eq!(multiply_count(none.graph()), 2);
        assert_eq!(multiply_count(basic.graph()), 1);

        assert!(
            basic
                .graph()
                .instances()
                .iter()
                .all(|instance| instance.mir().is_valid())
        );
    }

    #[test]
    fn repeated_scalar_expressions_in_one_large_block_share_one_result() {
        let mut source = String::from("module app;\nfunc main() -> i32 {\n");

        for index in 0..256 {
            source.push_str(&format!("let value{index}: i32 = 6 * 7;\n"));
        }

        source.push_str("return value255;\n}\n");

        let (_, compilation) = codegen_compilation(&source);

        assert!(compilation.check_diagnostics().is_empty());

        let cancellation = CancellationToken::new();

        let target = compilation
            .selected_target()
            .target()
            .codegen_target()
            .unwrap();

        let semantic = compilation.product_semantics().unwrap();

        let roots = compilation
            .product_root_instances(semantic.value(), None, &target, &cancellation)
            .unwrap();

        let start = std::time::Instant::now();

        let basic = compilation
            .codegen_reachability(
                roots,
                None,
                &target,
                crate::BuildConfiguration::Development.codegen_options(),
                false,
                &cancellation,
            )
            .unwrap();

        eprintln!("256 repeated scalar expressions: {:?}", start.elapsed());

        let multiply_count = basic
            .graph()
            .instances()
            .iter()
            .flat_map(|instance| instance.mir().operations())
            .filter(|operation| {
                matches!(
                    operation.kind(),
                    MirOperationKind::Binary {
                        operator: MirBinaryOperator::Multiply,
                        ..
                    }
                )
            })
            .count();

        assert_eq!(multiply_count, 1);

        assert!(
            basic
                .graph()
                .instances()
                .iter()
                .all(|instance| instance.mir().is_valid())
        );
    }

    #[test]
    fn runtime_division_by_zero_remains_in_optimized_mir() {
        let (_, compilation) = codegen_compilation(concat!(
            "module app;\n",
            "func main() -> i32\n",
            "{\n",
            "    let numerator: i32 = 6;\n",
            "    let denominator: i32 = 0;\n",
            "    return numerator / denominator;\n",
            "}\n",
        ));

        assert!(compilation.check_diagnostics().is_empty());

        let cancellation = CancellationToken::new();

        let target = compilation
            .selected_target()
            .target()
            .codegen_target()
            .unwrap();

        let semantic = compilation.product_semantics().unwrap();

        let roots = compilation
            .product_root_instances(semantic.value(), None, &target, &cancellation)
            .unwrap();

        for options in [
            CodegenOptions::default(),
            crate::BuildConfiguration::Development.codegen_options(),
        ] {
            let reachability = compilation
                .codegen_reachability(roots.clone(), None, &target, options, false, &cancellation)
                .unwrap();

            assert!(
                reachability
                    .graph()
                    .instances()
                    .iter()
                    .flat_map(|instance| instance.mir().operations())
                    .any(|operation| matches!(
                        operation.kind(),
                        MirOperationKind::Binary {
                            operator: MirBinaryOperator::Divide,
                            ..
                        }
                    ))
            );
        }
    }

    #[test]
    fn unknown_join_and_loop_values_keep_reachable_calls() {
        let (_, compilation) = codegen_compilation(concat!(
            "module app;\n",
            "func used() {}\n",
            "func gate(flag: bool)\n",
            "{\n",
            "    let mut selected: bool = false;\n",
            "    if flag\n",
            "    {\n",
            "        selected = true;\n",
            "    }\n",
            "    while flag\n",
            "    {\n",
            "        selected = true;\n",
            "    }\n",
            "    if selected\n",
            "    {\n",
            "        used();\n",
            "    }\n",
            "}\n",
            "func main() { gate(flag = false); }\n",
        ));

        assert!(compilation.check_diagnostics().is_empty());

        let cancellation = CancellationToken::new();

        let target = compilation
            .selected_target()
            .target()
            .codegen_target()
            .unwrap();

        let semantic = compilation.product_semantics().unwrap();

        let roots = compilation
            .product_root_instances(semantic.value(), None, &target, &cancellation)
            .unwrap();

        let basic = compilation
            .codegen_reachability(
                roots,
                None,
                &target,
                crate::BuildConfiguration::Development.codegen_options(),
                false,
                &cancellation,
            )
            .unwrap();

        assert_eq!(basic.graph().instances().len(), 3);

        assert!(
            basic
                .graph()
                .instances()
                .iter()
                .all(|instance| instance.mir().is_valid())
        );
    }

    #[test]
    fn generic_scalar_branches_follow_each_concrete_substitution() {
        let source = concat!(
            "module app;\n",
            "func unused() {}\n",
            "func gate<const enabled: bool>()\n",
            "{\n",
            "    if enabled\n",
            "    {\n",
            "        unused();\n",
            "    }\n",
            "}\n",
            "func main()\n",
            "{\n",
            "    gate<false>();\n",
            "    gate<true>();\n",
            "}\n",
        );

        let (_, compilation) = codegen_compilation(source);

        let cancellation = CancellationToken::new();

        let target = compilation
            .selected_target()
            .target()
            .codegen_target()
            .unwrap();

        let semantic = compilation.product_semantics().unwrap();

        let roots = compilation
            .product_root_instances(semantic.value(), None, &target, &cancellation)
            .unwrap();

        let none = compilation
            .codegen_reachability(
                roots.clone(),
                None,
                &target,
                CodegenOptions::default(),
                false,
                &cancellation,
            )
            .unwrap();

        let basic = compilation
            .codegen_reachability(
                roots,
                None,
                &target,
                crate::BuildConfiguration::Development.codegen_options(),
                false,
                &cancellation,
            )
            .unwrap();

        assert_eq!(none.graph().instances().len(), 4);
        assert_eq!(basic.graph().instances().len(), 4);

        assert!(
            basic
                .graph()
                .instances()
                .iter()
                .all(|instance| instance.mir().is_valid())
        );

        realize_codegen_mappings(&compilation, &target, &none, &cancellation);

        assert_boolean_specialization_dependencies(&compilation, &basic);
    }

    #[test]
    fn imported_generic_constant_prunes_its_transitive_native_dependency() {
        let dependency = generic_dependency_from_fixture(
            true,
            false,
            GenericDependencyFixture {
                source: concat!(
                    "module templates;\n",
                    "func called() {}\n",
                    "public func gate<const enabled: bool>()\n",
                    "{\n",
                    "    if enabled\n",
                    "    {\n",
                    "        called();\n",
                    "    }\n",
                    "}\n",
                ),
                runtime_frames: None,
                executable_templates: 2,
                platform_service: None,
            },
        );

        let compilation = generic_consumer_for_target_with_source(
            dependency,
            SelectedTarget::baseline(),
            "module app; using example.dependency.templates.gate; func main() { example.dependency.templates.gate<false>(); example.dependency.templates.gate<true>(); }",
        );

        assert!(compilation.check_diagnostics().is_empty());

        let cancellation = CancellationToken::new();

        let target = compilation
            .selected_target()
            .target()
            .codegen_target()
            .unwrap();

        let semantic = compilation.product_semantics().unwrap();

        let roots = compilation
            .product_root_instances(semantic.value(), None, &target, &cancellation)
            .unwrap();

        let none = compilation
            .codegen_reachability(
                roots.clone(),
                None,
                &target,
                CodegenOptions::default(),
                false,
                &cancellation,
            )
            .unwrap();

        let basic = compilation
            .codegen_reachability(
                roots,
                None,
                &target,
                crate::BuildConfiguration::Development.codegen_options(),
                false,
                &cancellation,
            )
            .unwrap();

        assert_eq!(none.graph().instances().len(), 4);
        assert_eq!(basic.graph().instances().len(), 4);

        assert!(
            basic
                .graph()
                .instances()
                .iter()
                .all(|instance| instance.mir().is_valid())
        );

        realize_codegen_mappings(&compilation, &target, &none, &cancellation);

        assert_boolean_specialization_dependencies(&compilation, &basic);
    }

    #[test]
    fn imported_numeric_generic_constant_prunes_only_its_false_branch() {
        let dependency = generic_dependency_from_fixture(
            true,
            false,
            GenericDependencyFixture {
                source: concat!(
                    "module templates;\n",
                    "func called() {}\n",
                    "public func gate<const seed: i32>()\n",
                    "{\n",
                    "    if seed * 7 != 42\n",
                    "    {\n",
                    "        called();\n",
                    "    }\n",
                    "}\n",
                ),
                runtime_frames: None,
                executable_templates: 2,
                platform_service: None,
            },
        );

        let compilation = generic_consumer_for_target_with_source(
            dependency,
            SelectedTarget::baseline(),
            "module app; using example.dependency.templates.gate; func main() { example.dependency.templates.gate<6>(); example.dependency.templates.gate<7>(); }",
        );

        assert!(compilation.check_diagnostics().is_empty());

        let cancellation = CancellationToken::new();

        let target = compilation
            .selected_target()
            .target()
            .codegen_target()
            .unwrap();

        let semantic = compilation.product_semantics().unwrap();

        let roots = compilation
            .product_root_instances(semantic.value(), None, &target, &cancellation)
            .unwrap();

        let none = compilation
            .codegen_reachability(
                roots.clone(),
                None,
                &target,
                CodegenOptions::default(),
                false,
                &cancellation,
            )
            .unwrap();

        let basic = compilation
            .codegen_reachability(
                roots,
                None,
                &target,
                crate::BuildConfiguration::Development.codegen_options(),
                false,
                &cancellation,
            )
            .unwrap();

        assert_eq!(none.graph().instances().len(), 4);
        assert_eq!(basic.graph().instances().len(), 4);

        let generic = basic
            .graph()
            .instances()
            .iter()
            .filter(|instance| {
                matches!(
                    instance.key().specialization(),
                    CodegenSpecialization::Generic(_)
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(generic.len(), 2);
        assert_ne!(generic[0].mir(), generic[1].mir());

        assert_eq!(
            generic
                .iter()
                .map(|instance| instance.dependencies().len())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([0, 1]),
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
            .codegen_reachability(
                roots.clone(),
                None,
                &target,
                CodegenOptions::default(),
                false,
                &cancellation,
            )
            .unwrap_or_else(|error| panic!("generic reachability must close: {error:?}"));

        let reversed_reachability = compilation
            .codegen_reachability(
                roots.clone().into_iter().rev(),
                None,
                &target,
                CodegenOptions::default(),
                false,
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
                        reachability.instance(instance.key()).expect("retained instance must be concrete"),
                        &test_product_identity(),
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
            |mir| {
                crate::compilation::product::mir_content_identity(&compilation, mir)
                    .unwrap_or_else(|error| panic!("test MIR identity must resolve: {error:?}"))
            },
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

                let data = values.type_data(*ty);

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
            root.instance()
                .callable_instance()
                .is_some_and(|callable| callable.definition() == fulfillment)
        }));
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
            .codegen_reachability(
                roots,
                None,
                &target,
                CodegenOptions::default(),
                false,
                &cancellation,
            )
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
            .codegen_reachability(
                roots,
                None,
                &target,
                CodegenOptions::default(),
                false,
                &cancellation,
            )
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
    fn exact_native_binding_replaces_imported_mir_and_missing_binding_falls_back() {
        let fixture = GenericDependencyFixture {
            source: "module templates;
public func hot(pos value: i32) -> i32 { return value + 1; }
",
            runtime_frames: None,
            executable_templates: 1,
            platform_service: None,
        };

        let selected = native_fixture_reachability(dependency_from_fixture_with_native(
            true,
            false,
            fixture,
            Some((CodegenOptions::default(), false, false, None)),
        ))
        .expect("exact native dependency must plan");

        let imported = selected
            .graph()
            .external_instances()
            .iter()
            .filter(|key| matches!(key.template(), MirUnitKey::ImportedExecutable(_)))
            .collect::<Vec<_>>();

        let [imported] = imported.as_slice() else {
            panic!("one imported definition must become an external native demand");
        };

        assert_eq!(
            selected
                .selected_native(imported)
                .expect("native unit must be selected")
                .symbol
                .as_str(),
            "bray_test_precompiled",
        );

        assert!(
            selected
                .graph()
                .instances()
                .iter()
                .all(|instance| !matches!(
                    instance.key().template(),
                    MirUnitKey::ImportedExecutable(_)
                ))
        );

        for dependency in [
            generic_dependency_from_fixture(true, false, fixture),
            dependency_from_fixture_with_native(
                true,
                false,
                fixture,
                Some((
                    CodegenOptions::default().with_optimization(OptimizationLevel::Full),
                    false,
                    false,
                    None,
                )),
            ),
        ] {
            let fallback = native_fixture_reachability(dependency)
                .expect("missing native specialization must use imported MIR");

            assert!(fallback.graph().instances().iter().any(|instance| matches!(
                instance.key().template(),
                MirUnitKey::ImportedExecutable(_)
            )));

            assert_eq!(fallback.selected_native_units().count(), 0);
        }
    }

    #[test]
    fn imported_native_binding_retains_references_but_excludes_disconnected_package_units() {
        let fixture = GenericDependencyFixture {
            source: "module templates;
public func hot(pos value: i32) -> i32 { return value + 1; }
",
            runtime_frames: None,
            executable_templates: 1,
            platform_service: None,
        };

        let dependency = dependency_from_fixture_with_native(
            true,
            false,
            fixture,
            Some((CodegenOptions::default(), false, true, None)),
        );

        let reachability =
            native_fixture_reachability(dependency.clone()).expect("native binding must be reused");

        let payloads = native_fixture_payloads(dependency, ProductKind::Executable)
            .expect("references must close during product selection");

        let selected = reachability.selected_native_units().collect::<Vec<_>>();

        let [selected] = selected.as_slice() else {
            panic!("one imported definition must select native units");
        };

        assert_eq!(payloads.len(), 2);
        assert_eq!(selected.symbol.as_str(), "bray_test_precompiled");

        assert!(
            payloads
                .iter()
                .all(|unit| unit.bytes.as_ref() != b"disconnected object")
        );

        assert!(
            payloads
                .iter()
                .any(|unit| unit.native_links.iter().any(|link| link.source()
                    == &LinkInputSource::try_native_library("native_support")
                        .expect("test native library name")))
        );

        assert!(
            reachability
                .graph()
                .instances()
                .iter()
                .all(|instance| !matches!(
                    instance.key().template(),
                    MirUnitKey::ImportedExecutable(_)
                ))
        );
    }

    #[test]
    fn opaque_package_code_uses_source_template_to_preserve_support_demand() {
        let fixture = GenericDependencyFixture {
            source: "module templates;
public func hot(pos value: i32) -> i32 { return value + 1; }
",
            runtime_frames: None,
            executable_templates: 1,
            platform_service: None,
        };

        let reachability = native_fixture_reachability(dependency_from_fixture_with_native(
            true,
            false,
            fixture,
            Some((
                CodegenOptions::default(),
                false,
                false,
                Some(NativeUnitKind::Bitcode),
            )),
        ))
        .expect("opaque package code must use its source template");

        assert_eq!(reachability.selected_native_units().count(), 0);

        assert!(
            reachability
                .graph()
                .instances()
                .iter()
                .any(|instance| matches!(
                    instance.key().template(),
                    MirUnitKey::ImportedExecutable(_)
                ))
        );
    }

    #[test]
    fn static_library_reuses_imported_binding_without_flattening_dependency_archive() {
        let fixture = GenericDependencyFixture {
            source: "module templates;
public func hot(pos value: i32) -> i32 { return value + 1; }
",
            runtime_frames: None,
            executable_templates: 1,
            platform_service: None,
        };

        let dependency = dependency_from_fixture_with_native(
            true,
            false,
            fixture,
            Some((
                CodegenOptions::default(),
                false,
                false,
                Some(NativeUnitKind::OpaqueArchive),
            )),
        );

        let executable = native_fixture_reachability(dependency.clone())
            .expect("executable must select its opaque archive input");

        assert_eq!(executable.selected_native_units().count(), 1);

        assert!(
            native_fixture_payloads(dependency.clone(), ProductKind::Executable)
                .expect("executable closure")
                .iter()
                .any(|unit| unit.kind == NativeUnitKind::OpaqueArchive)
        );

        assert!(
            native_fixture_payloads(dependency.clone(), ProductKind::Library)
                .expect("library keeps its dependencies separate")
                .is_empty()
        );

        let reachability =
            native_fixture_reachability_for_product(dependency, ProductKind::Library)
                .expect("static library must reuse the imported native binding");

        assert_eq!(reachability.selected_native_units().count(), 1);

        assert!(
            reachability
                .graph()
                .instances()
                .iter()
                .all(|instance| !matches!(
                    instance.key().template(),
                    MirUnitKey::ImportedExecutable(_)
                ))
        );
    }

    #[test]
    fn direct_native_imports_close_runtime_and_lifecycle_before_host_mapping() {
        let fixture = GenericDependencyFixture {
            source: r#"
                module templates;

                public func hot(pos value: i32) -> i32 {
                    return value + 1;
                }
            "#,
            runtime_frames: None,
            executable_templates: 1,
            platform_service: None,
        };

        let dependency = dependency_from_fixture_with_native(
            true,
            false,
            fixture,
            Some((CodegenOptions::default(), false, false, None)),
        );

        let artifact = dependency
            .shared_implementation_artifact()
            .unwrap()
            .unwrap();

        let original = artifact.native_artifact().unwrap().unwrap();
        let unit = &original.units()[0];
        let role = RuntimeAbiRole::PanicReportSuppression;

        let static_entry = bray_native_artifact::NativeStatic::new(
            NonEmptySharedStr::try_new("bray_test_static_host").unwrap(),
            [77; 32],
            Arc::from(b"native-static".as_slice()),
            StaticStorageDuration::Product,
            [],
            true,
            true,
        );

        let NativeUnitSummary::Exact { definitions, .. } = unit.summary() else {
            panic!("native fixture must have an exact index");
        };

        let definitions = definitions
            .iter()
            .cloned()
            .chain([NativeDefinition::new(
                NativeSymbolContract::required_name(
                    NonEmptySharedStr::try_new(static_entry.symbol()).unwrap(),
                ),
                NativeDefinitionSelection::Ordinary,
            )])
            .collect::<Vec<_>>();

        let replacement = NativeUnit::new(
            unit.digest(),
            unit.kind(),
            NativeUnitSummary::Exact {
                definitions: definitions.into(),
                references: Arc::from([NativeSymbolContract::required_name(
                    NonEmptySharedStr::try_new(role.native_symbol().unwrap()).unwrap(),
                )]),
                roots: Arc::from([]),
            },
            [],
        )
        .with_statics([static_entry.clone()]);

        let index =
            NativeArtifactIndex::try_new(original.target(), original.producer(), [replacement], [])
                .unwrap();

        let encoded = index.encode().unwrap();

        let artifact = artifact
            .try_native_only_artifact(
                &[(NativeUnitKind::Object, &encoded)],
                &[(
                    unit.digest().bytes(),
                    artifact
                        .native_unit_bytes(unit.digest().bytes())
                        .unwrap()
                        .unwrap(),
                )],
            )
            .unwrap();

        let dependency =
            dependency.with_implementation_artifact("native.brayimpl", Arc::new(artifact));

        for (source, retained) in [
            (
                r#"
                trusted module application;

                @link(name = "native")
                @symbol(name = "bray_test_precompiled")
                @abi(c)
                extern trusted func native_value() -> i32 uses(foreign_call);

                trusted func main() -> i32 uses(foreign_call) {
                    return trusted native_value();
                }
            "#,
                true,
            ),
            (
                r#"
                trusted module application;

                @link(name = "native")
                @symbol(name = "bray_test_precompiled")
                extern trusted static NATIVE_VALUE: i32;

                trusted func main() {
                    let pointer: RawPointer<i32> = NATIVE_VALUE;
                }
            "#,
                true,
            ),
            (
                r#"
                trusted module application;

                @link(name = "native")
                @symbol(name = "bray_test_precompiled", presence = optional)
                extern trusted static NATIVE_VALUE: i32;

                trusted func main() {
                    let pointer: RawPointer<i32> = NATIVE_VALUE;
                }
            "#,
                false,
            ),
        ] {
            let selected = SelectedTarget::baseline();

            let compilation = crate::Compilation::load(
                CompilationRequest::with_options(
                    crate::test_support::package_identity(),
                    vec![crate::test_support::source_input(source, 0)],
                    CompilationOptions::new(
                        WorkerBudget::serial(),
                        ProductKind::Executable,
                        selected.clone(),
                    )
                    .with_native_link_inputs([NativeLinkRequirement::new(
                        NonEmptySharedStr::try_new("native").unwrap(),
                        NativeLinkKind::Dynamic,
                    )]),
                )
                .with_dependency_interfaces([
                    dependency.clone(),
                    crate::test_support::runtime_standard_library_dependency(&selected),
                ]),
            )
            .unwrap();

            assert!(
                compilation.check_diagnostics().is_empty(),
                "{:#?}",
                compilation.check_diagnostics()
            );

            let cancellation = CancellationToken::new();
            let target = compilation.requested_target().codegen_target().unwrap();
            let semantic = compilation.product_semantics().unwrap();

            let roots = compilation
                .product_root_instances(semantic.value(), None, &target, &cancellation)
                .unwrap();

            let reachability = compilation
                .codegen_reachability(
                    roots,
                    None,
                    &target,
                    CodegenOptions::default(),
                    false,
                    &cancellation,
                )
                .unwrap();

            assert!(reachability.selected_native_units().next().is_none());

            let (demands, statics) = compilation
                .mapped_product_runtime_demands(
                    &test_product_identity(),
                    &reachability,
                    CodegenOptions::default(),
                    None,
                    false,
                    &target,
                    &cancellation,
                )
                .unwrap();

            assert_eq!(
                statics.statics(),
                if retained {
                    vec![static_entry.clone()]
                } else {
                    vec![]
                }
            );

            assert_eq!(
                demands.iter().any(|demand| demand.role() == Some(role)),
                retained
            );
        }
    }

    #[test]
    fn runtime_native_references_select_ordinary_library_dependencies() {
        let fixture = GenericDependencyFixture {
            source: r#"
                module templates;

                public func hot(pos value: i32) -> i32 {
                    return value + 1;
                }
            "#,
            runtime_frames: None,
            executable_templates: 1,
            platform_service: None,
        };

        let dependency = dependency_from_fixture_with_native(
            true,
            false,
            fixture,
            Some((CodegenOptions::default(), false, true, None)),
        );

        let compilation = native_fixture_compilation(dependency, ProductKind::Executable, None);

        let target = compilation
            .selected_target()
            .target()
            .codegen_target()
            .unwrap();

        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("runtime.lib");

        fs::write(&archive, b"test runtime archive").unwrap();

        let original = runtime_artifact(&compilation, &archive);
        let original_index = original.native_indexes()[0].index();

        let original_unit = original_index
            .units()
            .iter()
            .find(|unit| unit.kind() == NativeUnitKind::Object)
            .unwrap();

        let NativeUnitSummary::Exact { definitions, .. } = original_unit.summary() else {
            panic!("runtime fixture must have exact role definitions");
        };

        let requirements = bray_runtime_interface::RuntimeRequirements::new(
            None,
            compilation.selected_target().target().runtime_abi(),
            None,
            target.identity().clone(),
            target.panic_abi().clone(),
            [RuntimeAbiRole::PanicPropagation],
            [],
            [],
        );

        for required in ["bray_test_precompiled", "missing_library_symbol"] {
            let symbol =
                NativeSymbolContract::required_name(NonEmptySharedStr::try_new(required).unwrap());

            let unit = NativeUnit::new(
                original_unit.digest(),
                NativeUnitKind::Object,
                NativeUnitSummary::Exact {
                    definitions: Arc::clone(definitions),
                    references: Arc::from([symbol.clone()]),
                    roots: Arc::from([]),
                },
                [],
            );

            let index = NativeArtifactIndex::try_new(
                original_index.target(),
                original_index.producer(),
                [unit],
                [],
            )
            .unwrap();

            let bytes = index.encode().unwrap();

            let digest =
                NativeContentDigest::new(bray_base::sha256_reader(bytes.as_slice()).unwrap());

            let imported = NativeArtifactIndex::import(
                &bytes,
                digest,
                index.target(),
                index.producer(),
                &directory.path().join("native"),
            )
            .unwrap();

            let metadata = RuntimeArtifactMetadata::try_new(
                original.contract().clone(),
                original.metadata().components().iter().cloned(),
                RuntimeArtifactPurpose::ALL.map(|purpose| {
                    bray_runtime_interface::RuntimeNativeIndexMetadata::try_new(
                        purpose,
                        format!("{}.json", purpose.as_str()),
                        digest,
                    )
                    .unwrap()
                }),
            )
            .unwrap();

            let runtime = RuntimeArtifact::try_new(
                metadata,
                directory.path().to_owned(),
                [imported.clone(), imported],
            )
            .unwrap();

            let runtime = runtime
                .plan(RuntimeArtifactPurpose::Product, &requirements)
                .unwrap();

            let result = compilation.product_link_inputs(
                bray_linker::LinkedProductKind::Executable,
                None,
                Some(runtime),
                &[],
                None,
                &target,
                crate::BuildConfiguration::Development,
            );

            if required == "missing_library_symbol" {
                assert!(
                    matches!(result, Err(NativeProductPlanningError::NativeResolution(
                    bray_native_artifact::NativeResolutionError::Unresolved(actual)
                )) if actual == symbol)
                );
            } else {
                let (_, payloads) =
                    result.expect("runtime references must close through ordinary libraries");

                assert_eq!(payloads.len(), 2);

                assert!(
                    payloads
                        .iter()
                        .any(|unit| unit.bytes.as_ref() == b"test object bytes")
                );

                assert!(
                    payloads
                        .iter()
                        .any(|unit| unit.bytes.as_ref() == b"test dependency object")
                );

                assert!(
                    payloads
                        .iter()
                        .all(|unit| unit.bytes.as_ref() != b"disconnected object")
                );
            }
        }
    }

    #[test]
    fn native_companions_reject_conflicting_publications_and_semantic_interfaces() {
        let fixture = GenericDependencyFixture {
            source: r#"
                module templates;

                public func hot(pos value: i32) -> i32 {
                    return value + 1;
                }
            "#,
            runtime_frames: None,
            executable_templates: 1,
            platform_service: None,
        };

        let dependency = dependency_from_fixture_with_native(
            true,
            false,
            fixture,
            Some((CodegenOptions::default(), false, false, None)),
        );

        let artifact = dependency
            .shared_implementation_artifact()
            .unwrap()
            .unwrap();

        let duplicate = bray_package_interface::PackageArtifactInput::memory(
            "copy.brayimpl",
            artifact.shared_bytes().unwrap(),
        );

        let payloads = native_fixture_payloads(
            dependency.clone().with_native_implementations([duplicate]),
            ProductKind::Executable,
        )
        .expect("identical companion must select one representation");

        assert_eq!(payloads.len(), 1);

        let conflicting = dependency_from_fixture_with_native(
            true,
            false,
            fixture,
            Some((CodegenOptions::default(), false, true, None)),
        );

        let artifact = conflicting
            .shared_implementation_artifact()
            .unwrap()
            .unwrap();

        let companion = bray_package_interface::PackageArtifactInput::memory(
            "conflict.brayimpl",
            artifact.shared_bytes().unwrap(),
        );

        let error = native_fixture_payloads(
            dependency.clone().with_native_implementations([companion]),
            ProductKind::Executable,
        )
        .expect_err("different publications must not silently choose the first input");

        assert!(
            matches!(error, NativeProductPlanningError::ConflictingNativeArtifacts { first, second }
            if first == std::path::Path::new("dependency.brayimpl")
                && second == std::path::Path::new("conflict.brayimpl"))
        );

        let different_interface = dependency_from_fixture_with_native(
            true,
            false,
            GenericDependencyFixture {
                source: r#"
                    module templates;

                    public struct Extra {}

                    public func hot(pos value: i32) -> i32 {
                        return value + 1;
                    }
                "#,
                ..fixture
            },
            Some((CodegenOptions::default(), false, false, None)),
        );

        let artifact = different_interface
            .shared_implementation_artifact()
            .unwrap()
            .unwrap();

        let companion = bray_package_interface::PackageArtifactInput::memory(
            "wrong-interface.brayimpl",
            artifact.shared_bytes().unwrap(),
        );

        let error = native_fixture_payloads(
            dependency.with_native_implementations([companion]),
            ProductKind::Executable,
        )
        .expect_err("companion must belong to the selected semantic interface");

        let NativeProductPlanningError::Codegen(CodegenPreparationError::Diagnostics(diagnostics)) =
            error
        else {
            panic!("interface mismatch must retain validation diagnostics: {error:?}");
        };

        assert!(diagnostics.iter().any(|diagnostic|
            diagnostic.kind() == bray_diagnostics::DiagnosticKind::InterfaceHashMismatch
                && diagnostic.args().iter().any(|arg| matches!(arg.value(),
                    bray_diagnostics::DiagnosticArgValue::InterfaceValidationFailure(
                        bray_diagnostics::DiagnosticInterfaceValidationFailure::ContentHashMismatch { .. }
                    )
                ))
        ));
    }

    #[test]
    fn selected_corrupt_native_payload_reports_dependency_validation_failure() {
        let fixture = GenericDependencyFixture {
            source: "module templates;
public func hot(pos value: i32) -> i32 { return value + 1; }
",
            runtime_frames: None,
            executable_templates: 1,
            platform_service: None,
        };

        let dependency = dependency_from_fixture_with_native(
            true,
            false,
            fixture,
            Some((CodegenOptions::default(), true, false, None)),
        );

        let error = native_fixture_payloads(dependency, ProductKind::Executable)
            .err()
            .expect("corrupt selected unit must fail planning");

        let NativeProductPlanningError::Codegen(CodegenPreparationError::Diagnostics(diagnostics)) =
            error
        else {
            panic!("corrupt native unit must retain structured validation diagnostics: {error:?}");
        };

        assert!(diagnostics.has_errors());

        assert!(diagnostics.iter().any(|diagnostic| diagnostic.kind()
            == bray_diagnostics::DiagnosticKind::InterfaceValidationFailed));
    }

    #[test]
    fn selected_native_binding_with_wrong_producer_reports_exact_import_failure() {
        let fixture = GenericDependencyFixture {
            source: "module templates;
public func hot(pos value: i32) -> i32 { return value + 1; }
",
            runtime_frames: None,
            executable_templates: 1,
            platform_service: None,
        };

        let backend = Arc::new(
            bray_codegen_llvm::LlvmCodeGenerator::try_new().expect("LLVM backend must initialize"),
        );

        let registry =
            CodeGeneratorRegistry::try_new([Arc::clone(&backend) as Arc<dyn CodeGenerator>])
                .expect("LLVM backend must register");

        let codegen = CodegenConfiguration::try_new(registry, backend.identity().clone())
            .expect("LLVM backend must select");

        let dependency = dependency_from_fixture_with_native(
            true,
            false,
            fixture,
            Some((CodegenOptions::default(), false, false, None)),
        );

        let error = native_fixture_reachability_with_codegen(dependency, Some(codegen))
            .err()
            .expect("mismatched selected producer must fail planning");

        let NativeProductPlanningError::Codegen(CodegenPreparationError::Diagnostics(diagnostics)) =
            error
        else {
            panic!("wrong native producer must retain structured diagnostics: {error:?}");
        };

        assert!(
            diagnostics
                .iter()
                .flat_map(|diagnostic| diagnostic.args())
                .any(|arg| {
                    matches!(
                arg.value(),
                bray_diagnostics::DiagnosticArgValue::InterfaceValidationFailure(
                    bray_diagnostics::DiagnosticInterfaceValidationFailure::NativeArtifact {
                        cause: bray_diagnostics::DiagnosticNativeArtifactCause::WrongProducer,
                        ..
                    }
                )
            )
                })
        );
    }

    fn native_fixture_reachability(
        dependency: DependencyInterfaceInput,
    ) -> Result<ConcreteCodegenReachability, NativeProductPlanningError> {
        native_fixture_reachability_with_codegen(dependency, None)
    }

    fn native_fixture_reachability_with_codegen(
        dependency: DependencyInterfaceInput,
        codegen: Option<CodegenConfiguration>,
    ) -> Result<ConcreteCodegenReachability, NativeProductPlanningError> {
        native_fixture_reachability_for_product_with_codegen(
            dependency,
            ProductKind::Executable,
            codegen,
        )
    }

    fn native_fixture_reachability_for_product(
        dependency: DependencyInterfaceInput,
        product_kind: ProductKind,
    ) -> Result<ConcreteCodegenReachability, NativeProductPlanningError> {
        native_fixture_reachability_for_product_with_codegen(dependency, product_kind, None)
    }

    fn native_fixture_reachability_for_product_with_codegen(
        dependency: DependencyInterfaceInput,
        product_kind: ProductKind,
        codegen: Option<CodegenConfiguration>,
    ) -> Result<ConcreteCodegenReachability, NativeProductPlanningError> {
        let compilation = native_fixture_compilation(dependency, product_kind, codegen);
        let cancellation = CancellationToken::new();

        let target = compilation
            .selected_target()
            .target()
            .codegen_target()
            .expect("test target must support codegen");

        let semantic = compilation.product_semantics()?;

        let roots =
            compilation.product_root_instances(semantic.value(), None, &target, &cancellation)?;

        compilation.codegen_reachability(
            roots,
            None,
            &target,
            CodegenOptions::default(),
            false,
            &cancellation,
        )
    }

    fn native_fixture_compilation(
        dependency: DependencyInterfaceInput,
        product_kind: ProductKind,
        codegen: Option<CodegenConfiguration>,
    ) -> crate::Compilation {
        let source = match product_kind {
            ProductKind::Library => {
                "module application;
using example.dependency.templates.hot;
public func forwarded(pos value: i32) -> i32 {
    return example.dependency.templates.hot(value);
}
"
            }
            ProductKind::Executable => {
                "module application;
using example.dependency.templates.hot;
func main() { let value: i32 = example.dependency.templates.hot(1); }
"
            }
            ProductKind::Test => {
                unreachable!("native fixture only exercises libraries and executables")
            }
        };

        let request = CompilationRequest::with_options(
            crate::test_support::package_identity(),
            vec![crate::test_support::source_input(source, 0)],
            CompilationOptions::new(
                WorkerBudget::serial(),
                product_kind,
                SelectedTarget::baseline(),
            ),
        )
        .with_dependency_interfaces([dependency]);

        let compilation = match codegen {
            Some(codegen) => crate::Compilation::load_with_codegen(request, codegen),
            None => crate::Compilation::load(request),
        }
        .expect("consumer compilation must load");

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        compilation
    }

    fn native_fixture_payloads(
        dependency: DependencyInterfaceInput,
        kind: ProductKind,
    ) -> Result<Vec<super::super::reuse::SelectedNativePayload>, NativeProductPlanningError> {
        native_fixture_compilation(dependency, kind, None)
            .native_library_link_inputs(
                kind,
                &BTreeSet::from(["bray_test_precompiled"]),
                &BTreeSet::new(),
            )
            .map(|(_, payloads)| payloads)
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
            .codegen_reachability(
                roots,
                None,
                &target,
                CodegenOptions::default(),
                false,
                &cancellation,
            )
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

        let roots = reachability.graph().roots().iter().cloned().collect();
        let first_product = test_product_identity();

        let other_product = ProductIdentity::try_new(first_product.package().clone(), "other")
            .expect("alternate test product identity must validate");

        for instance in imported {
            let first = compilation
                .codegen_partition_compatibility(instance, reachability.instance(instance.key()).expect("retained instance must be concrete"), &first_product, &roots, &cancellation)
                .expect("imported compatibility must resolve");

            let other = compilation
                .codegen_partition_compatibility(instance, reachability.instance(instance.key()).expect("retained instance must be concrete"), &other_product, &roots, &cancellation)
                .expect("imported compatibility must resolve");

            assert_eq!(first, other);
        }

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
            .codegen_reachability(
                roots,
                None,
                &target,
                CodegenOptions::default(),
                false,
                &cancellation,
            )
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
            .codegen_reachability(
                roots,
                None,
                &target,
                CodegenOptions::default(),
                false,
                &cancellation,
            )
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
            .codegen_reachability(
                roots,
                None,
                &target,
                CodegenOptions::default(),
                false,
                &compilation.state.cancellation,
            )
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

        let error = match compilation.codegen_reachability(
            roots,
            None,
            &target,
            CodegenOptions::default(),
            false,
            &cancellation,
        ) {
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

        let root_key = root.key().clone();

        let reachability = compilation
            .codegen_reachability(
                [
                    ConcreteCodegenRoot::new(root.clone(), NativeDemandReason::ExecutableEntry),
                    ConcreteCodegenRoot::new(root, NativeDemandReason::NativeExport),
                ],
                None,
                &target,
                CodegenOptions::default(),
                false,
                &cancellation,
            )
            .unwrap_or_else(|error| panic!("generic reachability must close: {error:?}"));

        let [instance] = reachability.graph().instances() else {
            panic!("test root must be the only reachable instance");
        };

        assert_eq!(
            reachability
                .demands()
                .iter()
                .filter(|demand| demand.predecessor().is_none()
                    && demand.instance_target() == Some(&root_key))
                .map(crate::compilation::NativeDemand::reason)
                .collect::<Vec<_>>(),
            [
                NativeDemandReason::ExecutableEntry,
                NativeDemandReason::NativeExport,
            ]
        );

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
    fn nested_cleanup_native_fixture_emits() {
        let (backend, plan) = runtime_native_plan_for_product(
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../xtask/fixtures/composition/cleanup/main.bray"
            )),
            ProductKind::Test,
        );

        assert!(!generated_artifacts(&backend, &plan).is_empty());
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

                    if matches!(operation.kind(), MirOperationKind::Call(call) if call.may_propagate_panic())
                    {
                        assert!(matches!(
                            block.terminator().kind(),
                            MirTerminatorKind::CheckCallOutcome { .. }
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
            crate::BuildConfiguration::Development,
        )
    }

    fn test_linked_product(kind: ProductKind) -> bray_linker::LinkedProductKind {
        match kind {
            ProductKind::Library => bray_linker::LinkedProductKind::StaticLibrary,
            ProductKind::Executable | ProductKind::Test => {
                bray_linker::LinkedProductKind::Executable
            }
        }
    }

    fn runtime_native_plan_for_sources_target_with_platform_overrides(
        sources: &[&str],
        product_kind: ProductKind,
        target: SelectedTarget,
        native_link_inputs: &[NativeLinkRequirement],
        platform_services: impl IntoIterator<Item = PlatformServiceBinding>,
        runtime_platform_services: impl IntoIterator<Item = PlatformServiceRole>,
        configuration: crate::BuildConfiguration,
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
                configuration,
                Some(runtime),
                [
                    RuntimeCapability::CooperativeExecution,
                    RuntimeCapability::MainThreadLane,
                ],
                Some((&test_linker(), test_linked_product(product_kind))),
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
                Some((&test_linker(), test_linked_product(product_kind))),
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
                Some((&test_linker(), bray_linker::LinkedProductKind::Executable)),
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
                Some((&test_linker(), bray_linker::LinkedProductKind::Executable)),
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
    fn trivial_generic_statics_use_direct_native_storage() {
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

        assert!(plan.product_host().is_none());

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
            "public func main() -> i32\n",
            "{\n",
            "    let first = core.memory.address_of<i32>(&ANSWER);\n",
            "    let second = answer_address();\n",
            "    return ANSWER;\n",
            "}\n",
        );

        let (backend, plan) = runtime_native_plan_for_target(
            source,
            ProductKind::Library,
            SelectedTarget::for_native(NativeTarget::X86_64WindowsMsvc),
        );

        assert_eq!(
            plan.mappings()
                .iter()
                .flat_map(bray_codegen::CodegenMappings::static_storages)
                .map(|mapping| mapping.instance().clone())
                .collect::<BTreeSet<_>>()
                .len(),
            1
        );

        let units = generated_artifacts(&backend, &plan);

        let definitions = units
            .iter()
            .flat_map(|artifact| {
                match bray_native_artifact::scan_object_unit_summary(artifact).unwrap() {
                    bray_native_artifact::NativeUnitSummary::Exact { definitions, .. } => {
                        definitions.to_vec()
                    }
                    summary => {
                        panic!("ordinary static units must have exact summaries: {summary:?}")
                    }
                }
            })
            .collect::<Vec<_>>();

        let storage = plan
            .mappings()
            .iter()
            .flat_map(bray_codegen::CodegenMappings::static_storages)
            .next()
            .unwrap();

        for name in [
            storage.symbol().as_str().to_owned(),
            storage.accessor_name(),
            storage.host_name(),
        ] {
            assert_eq!(
                definitions
                    .iter()
                    .filter(
                        |definition| definition.symbol().identity().name() == Some(name.as_str())
                    )
                    .count(),
                1,
                "static definition {name} must have one owning unit"
            );
        }

        assert!(units.iter().all(|artifact| !artifact.is_empty()));
    }

    #[test]
    fn static_cleanup_orders_declaration_names_independently_of_source_order() {
        let (_, plan) = runtime_native_plan_for_target(
            concat!(
                "module app;\n",
                "@thread_local static ZETA: u64 = 1;\n",
                "@thread_local static ALPHA: u64 = 2;\n",
                "func main() -> i32 { let _: u64 = ZETA + ALPHA; return 0; }\n"
            ),
            ProductKind::Executable,
            SelectedTarget::for_native(NativeTarget::X86_64WindowsMsvc),
        );

        let host = plan.product_host().expect("thread statics need a host");

        let names = host
            .statics()
            .iter()
            .map(|entry| {
                let mapping = plan
                    .mappings()
                    .iter()
                    .flat_map(bray_codegen::CodegenMappings::static_storages)
                    .find(|mapping| mapping.host_name() == entry.host_symbol().as_str())
                    .unwrap();

                if mapping
                    .instance()
                    .order_key()
                    .windows(5)
                    .any(|text| text == b"ALPHA")
                {
                    "ALPHA"
                } else {
                    assert!(
                        mapping
                            .instance()
                            .order_key()
                            .windows(4)
                            .any(|text| text == b"ZETA")
                    );

                    "ZETA"
                }
            })
            .collect::<Vec<_>>();

        assert_eq!(names, ["ALPHA", "ZETA"]);
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

        let host = plan
            .product_host()
            .expect("thread static instances need a host");

        let values = host
            .statics()
            .iter()
            .map(|entry| {
                let storage = mappings
                    .iter()
                    .find(|mapping| mapping.host_name() == entry.host_symbol().as_str())
                    .unwrap();

                let value = plan
                    .mappings()
                    .iter()
                    .flat_map(bray_codegen::CodegenMappings::constants)
                    .find(|constant| constant.value() == storage.initial_value())
                    .unwrap();

                let bray_symbols::ConstantValueKind::Integer(integer) = value.data().kind() else {
                    panic!("generic integer static must retain its integer initializer");
                };

                integer.magnitude().to_vec()
            })
            .collect::<Vec<_>>();

        assert_eq!(values, [vec![7], vec![11]]);
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
                    value: MirOperand::Move(place),
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
    fn synchronous_finalizer_can_propagate_run_result_cancellation() {
        let source = r#"
            module app;
            struct Guard {
                finalize() {
                    let outcome: RunResult<unit> = Cancelled;
                    try outcome;
                }
                destruct() {}
            }
            async func main() {
                let mut value = Guard {};
                value = Guard {};
            }
        "#;

        for configuration in [
            crate::BuildConfiguration::Development,
            crate::BuildConfiguration::Release,
        ] {
            let (backend, plan) = runtime_native_plan_for_sources_target_with_platform_overrides(
                &[source],
                ProductKind::Executable,
                SelectedTarget::baseline(),
                &[],
                [],
                [],
                configuration,
            );

            assert!(!generated_artifacts(&backend, &plan).is_empty());
        }
    }

    #[test]
    fn library_runtime_defaults_coalesce_with_consumer_generated_bodies() {
        let (backend, plan) = runtime_native_plan_for_product(
            "module app; public func answer(pos value: i32 = 42) -> i32 { return value; }",
            ProductKind::Library,
        );

        let defaults = plan
            .mappings()
            .iter()
            .flat_map(bray_codegen::CodegenMappings::symbols)
            .filter_map(|symbol| match symbol.key() {
                bray_codegen::CodegenSymbolKey::Instance(instance)
                    if matches!(instance.template(), MirUnitKey::Bound(unit)
                        if unit.kind() == bray_bound_tree::BoundUnitKind::RuntimeDefault) =>
                {
                    Some((symbol, instance))
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(defaults.len(), 1);

        let (symbol, instance) = defaults[0];

        assert_eq!(symbol.linkage(), CodegenLinkage::LinkOnce);

        let compatibility = plan
            .units()
            .iter()
            .find_map(|unit| unit.key().compatibility(instance))
            .expect("default body must retain its partition compatibility");

        assert_eq!(compatibility.linkage(), CodegenLinkage::LinkOnce);

        let backend_ir =
            generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendIr);

        assert!(backend_ir.iter().any(|bytes| {
            String::from_utf8_lossy(bytes).lines().any(|line| {
                line.starts_with("define weak_odr ") && line.contains(symbol.name().as_str())
            })
        }));
    }

    #[test]
    fn library_preserves_main_thread_cleanup_without_selecting_a_runtime() {
        let source = concat!(
            "module app;\n",
            "public struct Resource { mut state: i32; }\n",
            "impl Resource {\n",
            "    async finalize() requires(main_thread_execution()) { self.state = 2; }\n",
            "    destruct() {}\n",
            "}\n",
            "public static RESOURCE: Resource = Resource { state = 1 };\n",
        );

        let (_, compilation) = codegen_compilation_for_product(source, ProductKind::Library);

        let plan = compilation
            .native_product_plan(
                test_product_identity(),
                crate::BuildConfiguration::Development,
                None,
                [],
                Some((
                    &test_linker(),
                    bray_linker::LinkedProductKind::StaticLibrary,
                )),
            )
            .unwrap_or_else(|error| {
                panic!("library must preserve cleanup obligations for its consumer: {error:?}")
            });

        assert!(
            plan.native_statics()
                .iter()
                .any(bray_native_artifact::NativeStatic::requires_main_thread)
        );

        let (_, executable) = runtime_native_plan(&format!(
            "{source}\nfunc main() -> i32 {{ return RESOURCE.state; }}\n"
        ));

        assert!(
            executable
                .executable_host()
                .unwrap()
                .requirements()
                .requires_role(RuntimeAbiRole::RuntimeInitialization)
        );

        let host = executable
            .units()
            .iter()
            .flat_map(|unit| unit.instances())
            .find(|instance| {
                matches!(
                    instance.mir().kind(),
                    bray_ir::MirUnitKind::ExecutableHost(_)
                )
            })
            .expect("executable must lower its host");

        assert!(matches!(
            host.mir()
                .operations()
                .first()
                .map(bray_ir::MirOperation::kind),
            Some(bray_ir::MirOperationKind::Host(
                bray_ir::MirHostOperation::InitializeRuntime { .. }
            ))
        ));

        assert!(
            executable
                .executable_host()
                .unwrap()
                .requirements()
                .capabilities()
                .contains(&RuntimeCapability::MainThreadLane)
        );
    }

    #[test]
    fn shared_libraries_own_hosts_while_archives_contribute_storage() {
        let source = r#"
            module app;

            public static VALUE: i32 = 42;
        "#;

        let (_, compilation) = codegen_compilation_for_product(source, ProductKind::Library);

        let archive = TemporaryFile::write("libbray_runtime.a", b"!<arch>\n");
        let runtime = runtime_artifact(&compilation, archive.path());
        let linker = test_linker();

        let plans = [
            bray_linker::LinkedProductKind::StaticLibrary,
            bray_linker::LinkedProductKind::SharedLibrary,
        ]
        .map(|kind| {
            compilation
                .native_product_plan(
                    test_product_identity(),
                    crate::BuildConfiguration::Development,
                    Some(runtime.clone()),
                    [],
                    Some((&linker, kind)),
                )
                .expect("library output must prepare with its host ownership")
        });

        assert!(!Arc::ptr_eq(&plans[0], &plans[1]));
        assert!(!plans[0].product_host().unwrap().is_final_image());
        assert!(plans[1].product_host().unwrap().is_final_image());

        assert!(
            plans[0]
                .preservation_roots()
                .all(|symbol| symbol.as_str() != bray_codegen::LINKED_PRODUCT_HOST_SYMBOL)
        );

        assert!(
            plans[1]
                .preservation_roots()
                .any(|symbol| symbol.as_str() == bray_codegen::LINKED_PRODUCT_HOST_SYMBOL)
        );
    }

    #[test]
    fn shared_library_runtime_roles_do_not_require_static_storage() {
        let source = r#"
            module app;

            public func fail()
            {
                panic("library failure");
            }
        "#;

        let (_, compilation) = codegen_compilation_for_product(source, ProductKind::Library);

        let archive = TemporaryFile::write("libbray_runtime.a", b"!<arch>\n");
        let runtime = runtime_artifact(&compilation, archive.path());
        let linker = test_linker();

        assert!(matches!(compilation.native_product_plan(
            test_product_identity(), crate::BuildConfiguration::Development,
            None, [], Some((&linker, bray_linker::LinkedProductKind::SharedLibrary)),
        ), Err(error) if matches!(error.as_ref(), NativeProductPlanningError::MissingRuntime)));

        let plan = compilation
            .native_product_plan(
                test_product_identity(),
                crate::BuildConfiguration::Development,
                Some(runtime),
                [],
                Some((&linker, bray_linker::LinkedProductKind::SharedLibrary)),
            )
            .expect("shared library runtime roles must select their implementation");

        assert!(plan.product_host().is_none());
        assert!(plan.link().is_some());
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
                Some((
                    &test_linker(),
                    bray_linker::LinkedProductKind::StaticLibrary,
                )),
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

        assert!(
            !plan.native_statics()[0].requires_host(),
            "plain exported storage needs no executable host"
        );

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

        let dependency = generic_dependency_from_fixture(
            true,
            false,
            GenericDependencyFixture {
                source,
                runtime_frames: None,
                executable_templates: 3,
                platform_service: None,
            },
        );

        let consumer = generic_consumer_for_target_with_source(
            dependency,
            SelectedTarget::baseline(),
            r#"
            module consumer;
            using example.dependency.app;
            func main() { example.dependency.app.main(); }
        "#,
        );

        assert!(
            consumer.check_diagnostics().is_empty(),
            "{:#?}",
            consumer.check_diagnostics()
        );

        let semantic = consumer
            .product_semantics()
            .expect("template consumer must select its product");

        let roots = consumer
            .product_root_instances(semantic.value(), None, &target, &cancellation)
            .expect("template consumer must preserve the library lifecycle contract");

        assert_eq!(
            roots.len(),
            1,
            "publication roots must not materialize unused private library statics"
        );
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
        assert!(mapping.outgoing_capacity() >= 2);

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
    fn static_admission_counts_repeated_constant_owners() {
        let (_, plan) = runtime_native_plan(
            r#"
            module app;
            struct Resource { value: i32; destruct() {} }
            static ONE: Resource = Resource { value = 1 };
            static TWO: [Resource; 2] = [Resource { value = 1 }, Resource { value = 1 }];
            func main() {}
        "#,
        );

        let capacities = plan
            .mappings()
            .iter()
            .flat_map(bray_codegen::CodegenMappings::static_storages)
            .map(bray_codegen::CodegenStaticStorageMapping::outgoing_capacity)
            .filter(|capacity| *capacity != 0)
            .collect::<BTreeSet<_>>();

        let capacities = capacities.into_iter().collect::<Vec<_>>();

        assert_eq!(capacities.len(), 2);
        assert_eq!(capacities[1], capacities[0] * 2);
    }

    #[test]
    fn static_host_orders_dependencies_reached_only_by_finalization() {
        for finalizer in ["", "impl Provider { finalize() {} }\n"] {
            let source = concat!(
                "module app;\n",
                "struct Provider { mut state: i32; }\n",
                "static PROVIDER: Provider = Provider { state = 1 };\n",
                "static UNRELATED: i32 = 7;\n",
                "struct Consumer {}\n",
                "impl Consumer\n",
                "{\n",
                "    finalize() { if PROVIDER.state == 1 {} }\n",
                "}\n",
                "static CONSUMER: Consumer = Consumer {};\n",
                "func main() {}\n",
            );

            let (_, plan) = runtime_native_plan(&format!("{source}\n{finalizer}"));

            let host = plan
                .product_host()
                .unwrap_or_else(|| panic!("lifecycle-bearing statics must retain a product host"));

            let owner = plan
                .mappings()
                .iter()
                .find(|mapping| {
                    mapping
                        .static_storages()
                        .iter()
                        .any(bray_codegen::CodegenStaticStorageMapping::defines_storage)
                })
                .map(bray_codegen::CodegenMappings::unit)
                .unwrap_or_else(|| panic!("lifecycle-bearing statics must define storage"));

            assert_eq!(host.owner(), owner);

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
            assert_eq!(host.statics().len(), 2);
        }
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

        assert!(
            !plan
                .units()
                .iter()
                .flat_map(bray_codegen::CodegenUnit::mir_units)
                .any(|mir| matches!(
                    mir.source(),
                    bray_ir::MirSourceOrigin::GeneratedLifecycle(MirHelperReference::Finalize(_))
                ))
        );

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

        let definitions = super::super::link::product_native_definitions(plan.mappings());

        let exports = super::super::plan::product_native_exports(plan.mappings())
            .map(|name| name.as_str())
            .collect::<BTreeSet<_>>();

        assert!(exports.contains("weak_export"));
        assert!(exports.contains("unused_export"));
        assert!(!exports.contains(callback_body.as_str()));

        assert_eq!(
            definitions.get("weak_export"),
            Some(&NativeSymbolBinding::Weak)
        );

        assert_eq!(
            definitions.get(callback_body.as_str()),
            Some(&NativeSymbolBinding::Strong)
        );

        assert!(plan.mappings().iter().any(|mappings| {
            mappings.static_storages().iter().any(|mapping| {
                mapping.symbol().as_str() == "unused_export"
                    && mapping.native_binding() == Some(NativeSymbolBinding::Strong)
                    && mapping.defines_storage()
            })
        }));

        assert_eq!(
            definitions.get("unused_export"),
            Some(&NativeSymbolBinding::Strong)
        );

        assert_eq!(
            definitions.get("bray.static.host.unused_export"),
            Some(&NativeSymbolBinding::Strong)
        );

        assert!(plan.mappings().iter().any(|mappings| {
            mappings.types().iter().any(|mapping| {
                mapping
                    .layout()
                    .is_some_and(|layout| layout.size() == 40 && layout.alignment().get() == 8)
            })
        }));

        let backend_ir =
            generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendIr);

        for artifact in &backend_ir {
            let ir = String::from_utf8_lossy(artifact);

            for redundant_retention in [
                "dllexport",
                "@llvm.used",
                "@llvm.compiler.used",
                "section \".bray",
            ] {
                assert!(!ir.contains(redundant_retention));
            }
        }

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
    fn timed_synchronous_entries_repeat_inside_one_runtime_root() {
        let source = concat!(
            "trusted module app;\n",
            "@link(name = \"native\")\n",
            "@symbol(name = \"native_status\")\n",
            "@abi(c)\n",
            "extern trusted func native_status() -> i32 uses(foreign_call);\n",
            "trusted func main() -> i32 uses(foreign_call)\n",
            "{\n",
            "    return trusted native_status();\n",
            "}\n",
        );

        let native_link = NativeLinkRequirement::new(
            NonEmptySharedStr::try_new("native")
                .unwrap_or_else(|| panic!("native link name must be valid")),
            NativeLinkKind::Dynamic,
        );

        let (backend, plan) = runtime_native_plan_for_sources_target_with_platform_overrides(
            &[source],
            ProductKind::Executable,
            SelectedTarget::baseline(),
            &[native_link],
            [],
            [],
            crate::BuildConfiguration::TimedRelease {
                inner_iterations: std::num::NonZeroU64::new(3)
                    .unwrap_or_else(|| panic!("timed test iteration count must be nonzero")),
            },
        );

        let backend_ir =
            generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendIr)
                .into_iter()
                .map(|artifact| String::from_utf8_lossy(&artifact).into_owned())
                .collect::<String>();

        let callback = backend_ir
            .split("define private void @bray_host_synchronous_root_callback_0")
            .nth(1)
            .and_then(|tail| tail.split("\n}").next())
            .unwrap_or_else(|| panic!("timed executable must define its synchronous callback"));

        assert!(callback.contains(bray_runtime_abi::PERFORMANCE_INTERVAL_BEGIN_SYMBOL));
        assert!(callback.contains(bray_runtime_abi::PERFORMANCE_INTERVAL_END_SYMBOL));
        assert!(callback.contains("performance.iteration"));
        assert!(callback.contains("root.performance.succeeded"));

        let root_execution_symbol = RuntimeAbiRole::SynchronousRootExecution
            .native_symbol()
            .unwrap_or_else(|| panic!("synchronous root execution must have a native symbol"));

        assert!(!callback.contains(root_execution_symbol));

        let root_executions = backend_ir
            .lines()
            .filter(|line| line.contains(" call ") && line.contains(root_execution_symbol))
            .count();

        assert_eq!(root_executions, 1);
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

        let provided = super::super::link::product_native_definitions(plan.mappings());

        assert!(
            provided.get(symbol) == Some(&bray_symbols::NativeSymbolBinding::Weak),
            "a weak platform fallback must not suppress a strong provider"
        );

        let runtime_reference = plan
            .mappings()
            .iter()
            .flat_map(bray_codegen::CodegenMappings::symbols)
            .find(|mapping| matches!(mapping.key(), bray_codegen::CodegenSymbolKey::Runtime(_)))
            .expect("host plan must reference a runtime role");

        assert!(
            !provided.contains_key(runtime_reference.name().as_str()),
            "a runtime reference must not count as a product definition"
        );

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
            crate::BuildConfiguration::Development,
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
    fn runtime_source_imports_share_generated_runtime_calls() {
        let source = r#"
            trusted module app;
            @abi(c)
            extern trusted internal func reserve(pos count: usize, pos outcome: RawPointer<u8>) uses(foreign_call);
            struct Guard { destruct() {} }
            public trusted func invoke(pos outcome: RawPointer<u8>) uses(foreign_call) {
                let guard = Guard {};
                trusted reserve(1, outcome);
            }
        "#;

        let role = RuntimeAbiRole::OutgoingAdmission;

        let binding =
            bray_runtime_interface::RuntimeRoleSourceBinding::try_new(role, "app.reserve")
                .unwrap_or_else(|| panic!("runtime source binding must validate"));

        let (backend, plan) =
            runtime_native_plan_with_source_roles(&[source], ProductKind::Library, [binding]);

        assert!(plan.mappings().iter().any(|mappings| mappings.symbols().iter().any(|symbol| matches!(symbol.key(), bray_codegen::CodegenSymbolKey::Runtime(reference) if reference.role() == role))));

        assert!(plan.mappings().iter().all(|mappings| {
            mappings
                .symbols()
                .iter()
                .filter(|symbol| symbol.name().as_str() == role.native_symbol().unwrap())
                .count()
                <= 1
        }));

        assert!(
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
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

    const FALLBACK_LIBRARY_SOURCE: &str = r#"
            trusted module app;
            @layout(c)
            internal struct PlatformStatus {
                category: u32;
                reserved: u32;
                native_code: i64;
            }
            @abi(c)
            trusted internal func flush() -> PlatformStatus {
                return { category = 1, reserved = 0, native_code = 0 };
            }
            @symbol(name = "live")
            static LIVE: i32 = 3;
        "#;

    #[test]
    fn ordinary_library_fallback_publication_is_exact() {
        let source = FALLBACK_LIBRARY_SOURCE;

        let role = PlatformServiceRole::StandardOutputFlush;
        let binding = PlatformServiceBinding::try_new(role, "app.flush").unwrap();

        let (backend, plan) = runtime_native_plan_for_sources_target_with_platform_services(
            &[source],
            ProductKind::Library,
            SelectedTarget::for_native(NativeTarget::X86_64WindowsMsvc),
            &[],
            [binding],
        );

        let ir = generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendIr);
        let objects = generated_artifacts(&backend, &plan);

        for (ir, object) in ir.iter().zip(objects) {
            let text = String::from_utf8_lossy(ir);

            if !text.contains("/alternatename:") {
                continue;
            }

            let summary = bray_native_artifact::scan_object_unit_summary(&object).unwrap();

            let bray_native_artifact::NativeUnitSummary::Exact {
                definitions, roots, ..
            } = summary
            else {
                panic!("ordinary fallback object must publish exact selection");
            };

            assert!(roots.is_empty());

            assert!(
                definitions
                    .iter()
                    .any(|definition| definition.symbol().identity().name()
                        == Some(role.native_symbol())
                        && definition.selection()
                            == &bray_native_artifact::NativeDefinitionSelection::Fallback)
            );
        }

        assert!(
            ir.iter()
                .any(|ir| String::from_utf8_lossy(ir).contains("/alternatename:"))
        );

        let (backend, plan) = runtime_native_plan_for_sources_target_with_platform_services(
            &[source],
            ProductKind::Library,
            SelectedTarget::for_native(NativeTarget::X86_64LinuxGnu),
            &[],
            [PlatformServiceBinding::try_new(role, "app.flush").unwrap()],
        );

        let bitcode =
            generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendBitcode);

        let mut ordinary = false;

        for bytes in bitcode {
            let summary = bray_codegen_llvm::inspect_bitcode_unit_summary(
                &bytes,
                NativeTarget::X86_64LinuxGnu,
            )
            .unwrap();

            match summary {
                bray_native_artifact::NativeUnitSummary::Exact { definitions, .. } => {
                    ordinary |= definitions
                        .iter()
                        .any(|definition| definition.symbol().identity().name() == Some("live"));
                }
                bray_native_artifact::NativeUnitSummary::Opaque { provided, .. } => {
                    assert!(
                        !provided.iter().any(|symbol| symbol.name() == Some("live")),
                        "an opaque fallback must not hide the ordinary export"
                    );
                }
            }
        }

        assert!(
            ordinary,
            "the ordinary export must remain independently exact"
        );
    }

    #[test]
    fn macho_string_and_static_helpers_keep_unrelated_exports_exact() {
        let target = NativeTarget::X86_64MacOs;

        for user in [
            r#"
                internal static VALUE: i32 = 42;

                func user() -> i32
                {
                    return VALUE;
                }
            "#,
            r#"
                func user() -> &string
                {
                    return &"shared literal";
                }
            "#,
        ] {
            let mut source = format!("module app;\n{user}");

            for index in 0..8 {
                source.push_str(&format!(
                    r#"
                        func ordinary_{index}(pos value: i32) -> i32
                        {{
                            return value + {index};
                        }}
                    "#
                ));
            }

            let (backend, plan) = runtime_native_plan_for_sources_target(
                &[&source],
                ProductKind::Library,
                SelectedTarget::for_native(target),
                &[],
            );

            let mut ordinary = BTreeSet::new();
            let mut user_symbol = None;

            for (unit, mappings) in plan.units().iter().zip(plan.mappings()) {
                for instance in unit.instances() {
                    if !matches!(instance.mir().kind(), MirUnitKind::Synchronous)
                        || !unit
                            .compatibility(instance.key())
                            .is_some_and(|class| class.linkage() == CodegenLinkage::Export)
                    {
                        continue;
                    }

                    let symbol = mappings
                        .symbols()
                        .iter()
                        .find(|symbol| {
                            matches!(symbol.key(), bray_codegen::CodegenSymbolKey::Instance(key)
                            if key == instance.key())
                        })
                        .expect("a generated export must have a symbol mapping");

                    let name = target
                        .object_symbol_name(symbol.name().as_str())
                        .into_owned();

                    if instance.mir().storages().iter().any(|storage| {
                        matches!(storage.kind(), bray_ir::MirStorageKind::Parameter(_))
                    }) {
                        ordinary.insert(name);
                    } else {
                        assert!(unit.compatibility(instance.key()).expect("helper user metadata must resolve").native_selection_boundary());
                        user_symbol = Some(name);
                    }
                }
            }

            assert_eq!(ordinary.len(), 8);

            assert!(user_symbol.is_some(), "the helper user must be exported");

            let mut exact = BTreeSet::new();
            let mut opaque_helpers = false;

            for bytes in
                generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendBitcode)
            {
                match bray_codegen_llvm::inspect_bitcode_unit_summary(&bytes, target).unwrap() {
                    NativeUnitSummary::Exact { definitions, .. } => {
                        exact.extend(definitions.iter().filter_map(|definition| {
                            definition.symbol().identity().name().map(str::to_owned)
                        }));
                    }
                    NativeUnitSummary::Opaque { provided, .. } => {
                        opaque_helpers |= !provided.is_empty();

                        assert!(
                            provided.iter().all(|symbol| {
                                symbol.name().is_none_or(|name| !ordinary.contains(name))
                            }),
                            "MachO weak helpers must not hide unrelated strong exports"
                        );
                    }
                }
            }

            assert!(
                opaque_helpers,
                "MachO weak helpers must retain their opaque summaries"
            );

            assert!(ordinary.is_subset(&exact));
        }
    }

    #[test]
    fn library_publication_preserves_named_native_dependencies() {
        for callable in [false, true] {
        let mut source = String::from("trusted module app;\n");

        source.push_str(if callable { r#"
            @link(name = "native")
            @symbol(name = "native_a")
            @abi(c)
            extern trusted func NATIVE_A() -> i32 uses(foreign_call);
            @link(name = "native")
            @symbol(name = "native_b")
            @abi(c)
            extern trusted func NATIVE_B() -> i32 uses(foreign_call);
        "# } else { r#"
            @link(name = "native")
            @symbol(name = "native_a")
            extern trusted static NATIVE_A: i32;
            @link(name = "native")
            @symbol(name = "native_b")
            extern trusted static NATIVE_B: i32;
        "# });

        for (storage, suffix) in [("NATIVE_A", "a"), ("NATIVE_B", "b")] {
            for index in 0..4 {
                source.push_str(&if callable {
                    format!(r#"
                        trusted func get_{suffix}_{index}() -> i32 uses(foreign_call)
                        {{
                            return trusted {storage}();
                        }}
                    "#)
                } else {
                    format!(r#"
                        trusted func get_{suffix}_{index}() -> RawPointer<i32>
                        {{
                            return {storage};
                        }}
                    "#)
                });
            }
        }

        let target = NativeTarget::X86_64LinuxGnu;

        let (backend, plan) = runtime_native_plan_for_sources_target(
            &[&source],
            ProductKind::Library,
            SelectedTarget::for_native(target),
            &[NativeLinkRequirement::new(
                NonEmptySharedStr::try_new("native").unwrap(),
                NativeLinkKind::Dynamic,
            )],
        );

        assert!(plan.units().iter().any(|unit| unit.instances().len() > 1));

        let mut selected = BTreeSet::new();

        for bytes in generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendBitcode) {
            let summary = bray_codegen_llvm::inspect_bitcode_unit_summary(&bytes, target).unwrap();

            let references = summary.references().iter().filter_map(|symbol| symbol.identity().name())
                .filter(|name| matches!(*name, "native_a" | "native_b")).collect::<BTreeSet<_>>();

            assert!(references.len() <= 1, "unrelated named native providers must not share a publication unit");
            selected.extend(references.into_iter().map(str::to_owned));
        }

        assert_eq!(selected, BTreeSet::from(["native_a".to_owned(), "native_b".to_owned()]));
        }
    }

    #[test]
    fn optional_native_storage_keeps_unrelated_library_exports_exact() {
        let source = r#"
            trusted module app;
            @link(name = "native")
            @symbol(name = "optional_native_value", presence = optional)
            extern trusted static NATIVE_VALUE: i32;

            trusted func optional_value() -> RawPointer<i32> {
                return NATIVE_VALUE;
            }

            func ordinary_value(pos value: i32) -> i32 {
                return value * 3 + 7;
            }
        "#;

        let (backend, plan) = runtime_native_plan_for_sources_target(
            &[source],
            ProductKind::Library,
            SelectedTarget::for_native(NativeTarget::X86_64LinuxGnu),
            &[NativeLinkRequirement::new(
                NonEmptySharedStr::try_new("native").unwrap(),
                NativeLinkKind::Dynamic,
            )],
        );

        let symbol_for_storage = |native_storage| {
            plan.units()
                .iter()
                .zip(plan.mappings())
                .find_map(|(unit, mappings)| {
                    let instance = unit.instances().iter().find(|instance| {
                        matches!(instance.mir().kind(), MirUnitKind::Synchronous)
                            && unit
                                .compatibility(instance.key())
                                .is_some_and(|class| class.linkage() == CodegenLinkage::Export)
                            && instance.mir().storages().iter().any(|storage| {
                                matches!(storage.kind(), bray_ir::MirStorageKind::NativeStatic(_))
                            }) == native_storage
                    })?;

                    mappings.symbols().iter().find_map(|symbol| {
                        matches!(symbol.key(), bray_codegen::CodegenSymbolKey::Instance(key)
                            if key == instance.key())
                        .then(|| symbol.name())
                    })
                })
                .expect("the library must publish both strong Bray function exports")
        };

        let ordinary_symbol = symbol_for_storage(false);
        let optional_symbol = symbol_for_storage(true);

        let bitcode =
            generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendBitcode);

        let mut ordinary = false;
        let mut optional = false;

        for bytes in bitcode {
            let summary = bray_codegen_llvm::inspect_bitcode_unit_summary(
                &bytes,
                NativeTarget::X86_64LinuxGnu,
            )
            .unwrap();

            match summary {
                bray_native_artifact::NativeUnitSummary::Exact { definitions, .. } => {
                    ordinary |= definitions.iter().any(|definition| {
                        definition.symbol().identity().name() == Some(ordinary_symbol.as_str())
                    });
                }
                bray_native_artifact::NativeUnitSummary::Opaque { provided, .. } => {
                    optional |= provided
                        .iter()
                        .any(|symbol| symbol.name() == Some(optional_symbol.as_str()));

                    assert!(
                        !provided
                            .iter()
                            .any(|symbol| symbol.name() == Some(ordinary_symbol.as_str())),
                        "an optional native reference must not hide the ordinary export"
                    );
                }
            }
        }

        assert!(
            ordinary && optional,
            "both definitions must survive with their native selection semantics"
        );
    }

    #[test]
    fn mixed_ordinary_library_fallbacks_yield_to_strong_objects_and_lazy_archives() {
        let source = FALLBACK_LIBRARY_SOURCE;

        let role = PlatformServiceRole::StandardOutputFlush;

        let (backend, plan) = runtime_native_plan_for_sources_target_with_platform_services(
            &[source],
            ProductKind::Library,
            SelectedTarget::for_native(NativeTarget::X86_64WindowsMsvc),
            &[],
            [PlatformServiceBinding::try_new(role, "app.flush").unwrap()],
        );

        let directory = tempfile::tempdir().unwrap();

        let prefix =
            std::path::Path::new(bray_codegen_llvm::COMPILED_LLVM_PREFIX.unwrap()).join("bin");

        let run = |tool: &str, arguments: Vec<std::ffi::OsString>| {
            let output = std::process::Command::new(
                prefix.join(format!("{tool}{}", std::env::consts::EXE_SUFFIX)),
            )
            .args(arguments)
            .current_dir(directory.path())
            .output()
            .unwrap();

            assert!(
                output.status.success(),
                "{tool}: {}",
                String::from_utf8_lossy(&output.stderr)
            );

            output
        };

        let ir = generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendIr);
        let mut sources = Vec::new();

        for (ordinal, bytes) in ir.iter().enumerate() {
            let text = String::from_utf8_lossy(bytes);

            if text.contains("/alternatename:") || text.contains("@live =") {
                let name = format!("source-{ordinal}.ll");

                fs::write(directory.path().join(&name), bytes).unwrap();
                sources.push(name.into());
            }
        }

        assert!(
            ir.iter()
                .any(|bytes| String::from_utf8_lossy(bytes).contains("/alternatename:")),
            "the fallback definition must be emitted",
        );

        assert!(
            ir.iter()
                .any(|bytes| String::from_utf8_lossy(bytes).contains("@live =")),
            "the ordinary storage definition must be emitted",
        );

        sources.extend(["-o".into(), "mixed.bc".into()]);
        run("llvm-link", sources);

        run(
            "opt",
            vec![
                "-passes=default<O2>".into(),
                "mixed.bc".into(),
                "-o".into(),
                "optimized.bc".into(),
            ],
        );

        run(
            "llc",
            vec![
                "-filetype=obj".into(),
                "optimized.bc".into(),
                "-o".into(),
                "mixed.obj".into(),
            ],
        );

        let first_ir = String::from_utf8_lossy(&ir[0]);

        let preamble = first_ir
            .lines()
            .filter(|line| line.starts_with("target "))
            .collect::<Vec<_>>()
            .join("\n");

        let symbol = role.native_symbol();

        let main = format!(
            r#"
{preamble}
declare void @{symbol}(ptr)
@live = external global i32
define i32 @main() {{
    %out = alloca [16 x i8], align 8
    call void @{symbol}(ptr %out)
    %category = load i32, ptr %out
    %live = load i32, ptr @live
    %result = add i32 %category, %live
    ret i32 %result
}}
"#
        );

        let strong = format!(
            r#"
{preamble}
define void @{symbol}(ptr %out) {{
    store i32 2, ptr %out
    ret void
}}
"#
        );

        fs::write(directory.path().join("main.ll"), main).unwrap();
        fs::write(directory.path().join("strong.ll"), strong).unwrap();

        fs::write(
            directory.path().join("unused.ll"),
            format!("{preamble}@unused = global [4096 x i8] zeroinitializer"),
        )
        .unwrap();

        for name in ["main", "strong", "unused"] {
            run(
                "llc",
                vec![
                    "-filetype=obj".into(),
                    format!("{name}.ll").into(),
                    "-o".into(),
                    format!("{name}.obj").into(),
                ],
            );
        }

        run(
            "llvm-ar",
            vec![
                "rc".into(),
                "strong.lib".into(),
                "strong.obj".into(),
                "unused.obj".into(),
            ],
        );

        for (kind, payload) in [
            (bray_native_artifact::NativeUnitKind::Object, "mixed.obj"),
            (
                bray_native_artifact::NativeUnitKind::Bitcode,
                "optimized.bc",
            ),
        ] {
            let bytes = fs::read(directory.path().join(payload)).unwrap();

            let summary = if kind == bray_native_artifact::NativeUnitKind::Object {
                bray_native_artifact::scan_object_unit_summary(&bytes).unwrap()
            } else {
                bray_codegen_llvm::inspect_bitcode_unit_summary(
                    &bytes,
                    NativeTarget::X86_64WindowsMsvc,
                )
                .unwrap()
            };

            let bray_native_artifact::NativeUnitSummary::Exact {
                definitions, roots, ..
            } = &summary
            else {
                panic!("mixed {kind:?} must retain exact provider selection: {summary:?}");
            };

            assert!(roots.is_empty());

            assert!(
                definitions
                    .iter()
                    .any(
                        |definition| definition.symbol().identity().name() == Some("live")
                            && definition.selection()
                                == &bray_native_artifact::NativeDefinitionSelection::Ordinary
                    )
            );

            assert!(
                definitions
                    .iter()
                    .any(|definition| definition.symbol().identity().name()
                        == Some(role.native_symbol())
                        && definition.selection()
                            == &bray_native_artifact::NativeDefinitionSelection::Fallback)
            );

            for provider in [None, Some("strong.obj"), Some("strong.lib")] {
                for reverse in [false, true] {
                    let mut inputs = vec![std::ffi::OsString::from(payload)];

                    if let Some(provider) = provider {
                        if reverse {
                            inputs.insert(0, provider.into());
                        } else {
                            inputs.push(provider.into());
                        }
                    }

                    let mut arguments = vec![
                        "/entry:main".into(),
                        "/subsystem:console".into(),
                        "/nodefaultlib".into(),
                        "/out:result.exe".into(),
                        "/map:result.map".into(),
                        "main.obj".into(),
                    ];

                    arguments.extend(inputs);
                    run("lld-link", arguments);

                    #[cfg(windows)]
                    {
                        let result =
                            std::process::Command::new(directory.path().join("result.exe"))
                                .status()
                                .unwrap();

                        assert_eq!(result.code(), Some(if provider.is_some() { 5 } else { 4 }));
                    }

                    let map = fs::read_to_string(directory.path().join("result.map")).unwrap();

                    if provider.is_some() {
                        assert!(
                            map.lines().any(|line| line.contains(symbol)
                                && !line.contains("__bray_fallback.")
                                && line.contains("strong.obj")),
                            "the public platform symbol must resolve to the strong provider: {map}",
                        );
                    } else {
                        assert!(
                            map.contains(&format!("__bray_fallback.{symbol}")),
                            "the default provider must remain available: {map}"
                        );
                    }

                    assert!(
                        !map.contains("unused"),
                        "unreferenced foreign members must remain lazy"
                    );

                    eprintln!(
                        "{kind:?} provider={provider:?} reverse={reverse} payload={} linked={} bytes",
                        bytes.len(),
                        fs::metadata(directory.path().join("result.exe"))
                            .unwrap()
                            .len()
                    );
                }
            }
        }
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

        let version = RuntimeAbiVersion::new(1, 0);

        let capabilities = [
            RuntimeCapability::CooperativeExecution,
            RuntimeCapability::LocalLanes,
            RuntimeCapability::MainThreadLane,
            RuntimeCapability::PerformanceObservation,
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
            )
            .unwrap_or_else(|error| panic!("test component must validate: {error:?}")),
            RuntimeArtifactComponentMetadata::try_new(
                RuntimeArtifactId::try_new("runtime.test")
                    .unwrap_or_else(|| panic!("test component identity must be valid")),
                RuntimeArtifactPurpose::TestRunner,
                roles.iter().copied(),
                capabilities,
            )
            .unwrap_or_else(|error| panic!("test component must validate: {error:?}"))
            .with_platform_services(platform_services),
        ];

        let directory = archive.parent().unwrap_or_else(|| std::path::Path::new(""));

        let indexes = RuntimeArtifactPurpose::ALL.map(|purpose| {
            let symbols = contract
                .role_bindings()
                .iter()
                .filter(|binding| {
                    purpose == RuntimeArtifactPurpose::TestRunner
                        || binding.role().available_to_product()
                })
                .map(|binding| binding.symbol_name().as_str());

            bray_testing::test_runtime_native_index(
                directory,
                bray_target::NativeTarget::for_identity(target.identity())
                    .expect("test target must be native"),
                purpose,
                symbols,
                archive,
            )
        });

        let metadata = RuntimeArtifactMetadata::try_new(
            contract,
            components,
            indexes.iter().map(|(reference, _)| reference.clone()),
        )
        .unwrap_or_else(|error| panic!("test runtime metadata must validate: {error:?}"));

        RuntimeArtifact::try_new(
            metadata,
            directory.to_path_buf(),
            indexes.map(|(_, index)| index),
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

                let artifacts = BackendArtifactRequest::new(
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
                );

                let cancellation = CancellationToken::new();

                let request = CodegenRequest::new(
                    unit,
                    backend.identity(),
                    backend.capabilities().revision(),
                    ProductKind::Executable,
                    plan.target(),
                    mappings,
                    plan.options(),
                    &artifacts,
                    &cancellation,
                );

                let outcome = backend.generate(request);

                assert!(
                    matches!(outcome.status(), CodegenStatus::Complete(_)),
                    "{:?}: {:?}",
                    unit.key(),
                    outcome.status()
                );

                let contribution = outcome
                    .artifacts()
                    .and_then(|artifacts| artifacts.first())
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
            .codegen_reachability(
                roots,
                None,
                &target,
                CodegenOptions::default(),
                false,
                &cancellation,
            )
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

    const PROFILE_DEPENDENCY: GenericDependencyFixture = GenericDependencyFixture {
        source: concat!(
            "module templates;\n",
            "func helper<T>(pos value: T) -> T { return value; }\n",
            "public func identity<T>(pos value: T) -> T\n",
            "{\n",
            "    let invoke = lambda(pos item: T) -> T { return helper<T>(item); };\n",
            "    return invoke(value);\n",
            "}\n",
            "public func disconnected(pos value: i32) -> i32 { return value; }\n",
        ),
        runtime_frames: None,
        executable_templates: 4,
        platform_service: None,
    };

    const REORDERED_PROFILE_DEPENDENCY: GenericDependencyFixture = GenericDependencyFixture {
        source: concat!(
            "module templates;\n",
            "public func disconnected(pos value: i32) -> i32 { return value; }\n",
            "public func identity<T>(pos value: T) -> T\n",
            "{\n",
            "    let invoke = lambda(pos item: T) -> T { return helper<T>(item); };\n",
            "    return invoke(value);\n",
            "}\n",
            "func helper<T>(pos value: T) -> T { return value; }\n",
        ),
        runtime_frames: None,
        executable_templates: 4,
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

    fn assert_boolean_specialization_dependencies(
        compilation: &crate::Compilation,
        reachability: &ConcreteCodegenReachability,
    ) {
        let generic = reachability
            .graph()
            .instances()
            .iter()
            .filter(|instance| {
                matches!(
                    instance.key().specialization(),
                    CodegenSpecialization::Generic(_)
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(generic.len(), 2);
        assert_eq!(generic[0].key().template(), generic[1].key().template());
        assert_ne!(generic[0].mir(), generic[1].mir());

        let values = compilation.semantic_value_store().unwrap();
        let mut seen = BTreeSet::new();

        for instance in generic {
            let realization = reachability
                .instance(instance.key())
                .expect("reachable specialization must have a concrete realization");

            let substitution = values.generic_substitution_data(
                realization
                    .substitution()
                    .expect("generic callable must carry a substitution"),
            );

            let [binding] = substitution.bindings() else {
                panic!("test gate must have exactly one constant argument");
            };

            let GenericArgument::Constant(term) = binding.argument() else {
                panic!("test gate argument must be a constant");
            };

            let term_data = values.constant_term_data(term);

            let ConstantTermData::Value(value) = term_data.as_ref() else {
                panic!("concrete test argument must have a value");
            };

            let value_data = values.constant_value_data(*value);

            let ConstantValueKind::Boolean(enabled) = value_data.kind() else {
                panic!("test gate argument must be Boolean");
            };

            assert!(seen.insert(*enabled));
            assert_eq!(instance.dependencies().len(), usize::from(*enabled));
        }
    }

    fn realize_codegen_mappings(
        compilation: &crate::Compilation,
        target: &bray_codegen::CodegenTarget,
        reachability: &ConcreteCodegenReachability,
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
                        reachability.instance(instance.key()).expect("retained instance must be concrete"),
                        &test_product_identity(),
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
            |mir| {
                crate::compilation::product::mir_content_identity(&compilation, mir)
                    .unwrap_or_else(|error| panic!("test MIR identity must resolve: {error:?}"))
            },
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

    fn profiled_dependency_library(
        source: &str,
        worker_budget: WorkerBudget,
        dependency: GenericDependencyFixture,
    ) -> crate::Compilation {
        profiled_product(
            source,
            ProductKind::Library,
            worker_budget,
            [generic_dependency_from_fixture(true, false, dependency)],
        )
    }

    fn profiled_product<const N: usize>(
        source: &str,
        kind: ProductKind,
        worker_budget: WorkerBudget,
        dependencies: [DependencyInterfaceInput; N],
    ) -> crate::Compilation {
        let backend = Arc::new(
            bray_codegen_llvm::LlvmCodeGenerator::try_new()
                .unwrap_or_else(|error| panic!("LLVM backend must initialize: {error:?}")),
        );

        let registry =
            CodeGeneratorRegistry::try_new([Arc::clone(&backend) as Arc<dyn CodeGenerator>])
                .unwrap_or_else(|error| panic!("LLVM backend must register: {error:?}"));

        let codegen = CodegenConfiguration::try_new(registry, backend.identity().clone())
            .unwrap_or_else(|error| panic!("LLVM backend must select: {error:?}"));

        let target = SelectedTarget::baseline();
        let runtime = crate::test_support::runtime_standard_library_dependency(&target);

        let request = CompilationRequest::with_options(
            crate::test_support::package_identity(),
            vec![crate::test_support::source_input(source, 0)],
            CompilationOptions::new(worker_budget, kind, target),
        )
        .with_dependency_interfaces(std::iter::once(runtime).chain(dependencies))
        .with_profile(CompilationProfileConfiguration::new(
            CompilationProfileMode::Summary,
        ));

        let compilation = crate::Compilation::load_with_codegen(request, codegen)
            .unwrap_or_else(|error| panic!("profiled library must load: {error:?}"));

        assert!(
            compilation.check_diagnostics().is_empty(),
            "{:#?}",
            compilation.check_diagnostics()
        );

        let archive = TemporaryFile::write("libbray_runtime.a", b"!<arch>\n");

        let runtime = match kind {
            ProductKind::Executable | ProductKind::Test => {
                Some(runtime_artifact(&compilation, archive.path()))
            }
            ProductKind::Library => None,
        };

        let required_capabilities = runtime.as_ref().map_or_else(Vec::new, |_| {
            vec![
                RuntimeCapability::CooperativeExecution,
                RuntimeCapability::MainThreadLane,
            ]
        });

        compilation
            .native_product_plan(
                test_product_identity(),
                crate::BuildConfiguration::Development,
                runtime,
                required_capabilities,
                Some((&test_linker(), test_linked_product(kind))),
            )
            .unwrap_or_else(|error| panic!("profiled native plan must resolve: {error:?}"));

        compilation
    }

    fn native_profile(
        compilation: &crate::Compilation,
    ) -> bray_profile::CompilationProfileNativeCodegen {
        let report = compilation
            .profile_report()
            .unwrap_or_else(|| panic!("native compilation profile must exist"));

        report
            .validate()
            .unwrap_or_else(|error| panic!("native compilation profile must validate: {error:?}"));

        report
            .native_codegen
            .unwrap_or_else(|| panic!("native compilation profile must retain its inventory"))
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
        dependency_from_fixture_with_native(
            include_implementation,
            malformed_templates,
            fixture,
            None,
        )
    }

    fn dependency_from_fixture_with_native(
        include_implementation: bool,
        malformed_templates: bool,
        fixture: GenericDependencyFixture,
        native: Option<(CodegenOptions, bool, bool, Option<NativeUnitKind>)>,
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

        let implementation = if let Some((producer_options, corrupt, linked, opaque)) = native {
            let [template] = bundle.executable_templates() else {
                panic!("native fixture must publish exactly one executable template");
            };

            let owner = template.owner();

            let owner_symbol = bundle
                .surface()
                .symbols()
                .symbol(owner)
                .expect("native fixture owner must be exported");

            let symbol = NonEmptySharedStr::try_new("bray_test_precompiled")
                .expect("test native symbol must be valid");

            let bytes: Arc<[u8]> = Arc::from(b"test object bytes".as_slice());

            let digest = NativeContentDigest::new(
                bray_base::sha256_reader(bytes.as_ref()).expect("in-memory hash cannot fail"),
            );

            let unit = NativeUnit::new(
                digest,
                NativeUnitKind::Object,
                NativeUnitSummary::Exact {
                    definitions: Arc::from([NativeDefinition::new(
                        NativeSymbolContract::required_name(symbol.clone()),
                        NativeDefinitionSelection::Ordinary,
                    )]),
                    references: if linked {
                        Arc::from([NativeSymbolContract::required_name(
                            NonEmptySharedStr::try_new("bray_test_dependency")
                                .expect("test dependency symbol must validate"),
                        )])
                    } else {
                        Arc::from([])
                    },
                    roots: Arc::from([]),
                },
                [],
            );

            let target = NativeTarget::for_identity(bundle.implementation_configuration().target())
                .expect("test target must have a native artifact kind");

            let mut units = vec![unit];

            let mut payloads = vec![(
                digest.bytes(),
                if corrupt {
                    Arc::from(b"corrupt object".as_slice())
                } else {
                    bytes
                },
            )];

            if linked {
                let dependency_bytes: Arc<[u8]> = Arc::from(b"test dependency object".as_slice());

                let dependency_digest = NativeContentDigest::new(
                    bray_base::sha256_reader(dependency_bytes.as_ref())
                        .expect("in-memory dependency hash cannot fail"),
                );

                units.push(NativeUnit::new(
                    dependency_digest,
                    NativeUnitKind::Object,
                    NativeUnitSummary::Exact {
                        definitions: Arc::from([NativeDefinition::new(
                            NativeSymbolContract::required_name(
                                NonEmptySharedStr::try_new("bray_test_dependency")
                                    .expect("test dependency symbol must validate"),
                            ),
                            NativeDefinitionSelection::Ordinary,
                        )]),
                        references: Arc::from([]),
                        roots: Arc::from([]),
                    },
                    [NativeLinkRequirement::new(
                        NonEmptySharedStr::try_new("native_support")
                            .expect("test native library name must validate"),
                        NativeLinkKind::System,
                    )],
                ));

                payloads.push((dependency_digest.bytes(), dependency_bytes));

                let unused_bytes: Arc<[u8]> = Arc::from(b"disconnected object".as_slice());

                let unused_digest = NativeContentDigest::new(
                    bray_base::sha256_reader(unused_bytes.as_ref())
                        .expect("in-memory unused hash cannot fail"),
                );

                units.push(NativeUnit::new(
                    unused_digest,
                    NativeUnitKind::Object,
                    NativeUnitSummary::Exact {
                        definitions: Arc::from([NativeDefinition::new(
                            NativeSymbolContract::required_name(
                                NonEmptySharedStr::try_new("bray_test_disconnected")
                                    .expect("test unused symbol must validate"),
                            ),
                            NativeDefinitionSelection::Ordinary,
                        )]),
                        references: Arc::from([]),
                        roots: Arc::from([]),
                    },
                    [],
                ));

                payloads.push((unused_digest.bytes(), unused_bytes));
            }

            if let Some(kind) = opaque {
                let opaque_bytes: Arc<[u8]> = Arc::from(b"opaque package bitcode".as_slice());

                let opaque_digest = NativeContentDigest::new(
                    bray_base::sha256_reader(opaque_bytes.as_ref())
                        .expect("in-memory opaque hash cannot fail"),
                );

                units.push(NativeUnit::new(
                    opaque_digest,
                    kind,
                    NativeUnitSummary::opaque([]),
                    [],
                ));

                payloads.push((opaque_digest.bytes(), opaque_bytes));
            }

            let index =
                NativeArtifactIndex::try_new(target, NativeContentDigest::new([1; 32]), units, [])
                    .expect("test native index must validate");

            let index_bytes = index.encode().expect("test native index must encode");

            let binding = InterfaceNativeBinding::new(
                owner,
                PackageImplementationSpecializationKey::new(
                    ImplementationExternalSymbolIdentity::new(owner_symbol.key()),
                    [],
                    [],
                    bundle.implementation_configuration().clone(),
                    CURRENT_TEMPLATE_SCHEMA_REVISION,
                    bundle.surface().dependencies().iter().cloned(),
                ),
                producer_options,
                digest.bytes(),
                symbol,
            );

            PackageImplementationArtifact::try_from_export_bundle_with_native(
                &interface,
                bundle,
                &index_bytes,
                &payloads,
                &[binding],
                InterfaceValidationLimits::default(),
            )
            .expect("native dependency implementation must encode")
        } else {
            PackageImplementationArtifact::try_new(
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
            .expect("dependency implementation must encode")
        };

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
