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

use super::super::error::NativeProductPlanningError;
use super::super::plan::NativeProductPlan;
use crate::Compilation;
use crate::fact::{CancellationToken, CompilationFactKey, NativeProductQueryKey};

pub(in super::super) const GENERATED_HOST_UNIT: MirUnitId = MirUnitId::new(u32::MAX);

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

    pub(super) fn native_product_plan_with_cancellation(
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

        let entry_definitions = super::super::roots::product_entry_symbols(
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
                    crate::compilation::product::realization::generated_identity(
                        "static_host",
                        local.key(),
                    ) == entry.identity().bytes()
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
    pub(in super::super) fn profile_native_product_operation<T>(
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

    pub(in super::super) fn profile_runtime_selection(
        &self,
        runtime: Option<&RuntimeArtifactSelection>,
    ) {
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

pub(in super::super) const fn runtime_artifact_purpose(
    kind: ProductKind,
) -> RuntimeArtifactPurpose {
    match kind {
        ProductKind::Test => RuntimeArtifactPurpose::TestRunner,
        ProductKind::Executable | ProductKind::Library => RuntimeArtifactPurpose::Product,
    }
}

pub(in super::super) fn bound_template(
    key: &CodegenInstanceKey,
) -> Result<bray_bound_tree::BoundUnitKey, NativeProductPlanningError> {
    let MirUnitKey::Bound(template) = key.template() else {
        return Err(NativeProductPlanningError::MissingProductRoot);
    };

    Ok(template.clone())
}
