use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_codegen::{
    CodegenInstanceKey, CodegenMappings, CodegenOptions, CodegenPartitionPolicy, CodegenTarget,
    CodegenUnit, DebugInformationMode, partition_codegen_units,
};
use bray_runtime_interface::{
    ExecutableHostContract, RuntimeArtifact, RuntimeArtifactPlan, RuntimeCapability,
};
use bray_symbols::{ProductIdentity, ProductKind};

use super::super::super::Compilation;
use super::super::realization::ProductStaticHostEntry;
use super::super::specialization::{ConcreteCodegenInstance, ConcreteCodegenReachability};
use super::error::{NativeProductPlanningError, native_batch_error};
use super::host::{native_host_runtime_roles, requires_main_thread_cleanup};
use super::implementation::{GENERATED_HOST_UNIT, bound_template};
use super::{ConcreteCodegenRoot, NativeDemand, NativeDemandReason};
use crate::fact::{BatchWork, CancellationToken};

type NativeCodegenPreparation = (
    Option<ExecutableHostContract>,
    Option<RuntimeArtifactPlan>,
    Arc<[CodegenUnit]>,
    Vec<CodegenMappings>,
    Vec<ProductStaticHostEntry>,
    Vec<bray_native_artifact::NativeStatic>,
    BTreeSet<CodegenInstanceKey>,
);

impl Compilation {
    #[expect(
        clippy::too_many_arguments,
        reason = "host closure combines existing semantic and native phase contracts"
    )]
    pub(super) fn close_executable_host(
        &self,
        product: &ProductIdentity,
        kind: ProductKind,
        final_image: bool,
        entry_roots: &[ConcreteCodegenInstance],
        source_reachability: &mut Option<ConcreteCodegenReachability>,
        runtime: Option<&RuntimeArtifact>,
        required_capabilities: impl IntoIterator<Item = RuntimeCapability>,
        target: &CodegenTarget,
        options: bray_codegen::CodegenOptions,
        allow_bitcode: bool,
        cancellation: &CancellationToken,
    ) -> Result<
        (
            Option<ExecutableHostContract>,
            Option<RuntimeArtifactPlan>,
            Vec<super::super::realization::ProductStaticHostEntry>,
            Vec<bray_native_artifact::NativeStatic>,
            super::super::realization::NativeCallableEffects,
        ),
        NativeProductPlanningError,
    > {
        let required_capabilities = required_capabilities.into_iter().collect::<Vec<_>>();
        let mut host_statics = Vec::new();
        let mut native_statics = Vec::new();

        loop {
            let host = self.profile_native_product_operation(
                crate::profile::ProfileOperation::NativeHostPreparation,
                || {
                    self.executable_host(
                        product,
                        kind,
                        entry_roots,
                        source_reachability.as_ref(),
                        &host_statics,
                        &native_statics,
                        runtime,
                        required_capabilities.iter().copied(),
                        target,
                        cancellation,
                    )
                },
            )?;

            let library_requirements;

            let requirements = match host.as_ref() {
                Some(host) => Some(host.requirements()),
                None if final_image => {
                    let roles = native_host_runtime_roles(
                        source_reachability.as_ref(),
                        &host_statics,
                        &native_statics,
                    );

                    let mut capabilities = required_capabilities
                        .iter()
                        .copied()
                        .collect::<BTreeSet<_>>();

                    if requires_main_thread_cleanup(&host_statics, &native_statics) {
                        capabilities.insert(RuntimeCapability::MainThreadLane);
                    }

                    library_requirements =
                        self.product_runtime_requirements(runtime, target, &roles, capabilities)?;

                    Some(&library_requirements)
                }
                None => None,
            };

            let runtime_plan = if let Some(requirements) =
                requirements.filter(|requirements| requirements.requires_implementation())
            {
                Some(
                    runtime
                        .expect("runtime requirements must select a runtime")
                        .plan(
                            super::implementation::runtime_artifact_purpose(kind),
                            requirements,
                        )
                        .map_err(NativeProductPlanningError::InvalidRuntimeSelection)?,
                )
            } else {
                None
            };

            let Some(reachability) = source_reachability.as_ref() else {
                break Ok((
                    host,
                    runtime_plan,
                    host_statics,
                    native_statics,
                    BTreeMap::new(),
                ));
            };

            let (demands, selected) = self.mapped_product_runtime_demands(
                product,
                reachability,
                options,
                runtime_plan.as_ref(),
                allow_bitcode,
                target,
                cancellation,
            )?;

            let callable_effects =
                self.native_callable_effects(reachability, &selected, cancellation)?;

            let entries = self.profile_native_product_operation(
                crate::profile::ProfileOperation::NativeHostPreparation,
                || {
                    self.product_static_host_entries(
                        kind,
                        reachability,
                        &callable_effects,
                        target,
                        cancellation,
                    )
                    .map_err(NativeProductPlanningError::from)
                },
            )?;

            let selected_statics =
                super::host::native_static_host_entries(kind, selected.statics(), &entries);

            let stable = selected_statics == native_statics
                && entries == host_statics
                && demands
                    .iter()
                    .all(|demand| reachability.demands().binary_search(demand).is_ok());

            native_statics = selected_statics;
            host_statics = entries;

            *source_reachability = Some(
                source_reachability
                    .take()
                    .expect("native host preparation must retain source reachability")
                    .with_demands(demands),
            );

            if stable || (kind == ProductKind::Library && !final_image) {
                break Ok((
                    host,
                    runtime_plan,
                    host_statics,
                    native_statics,
                    callable_effects,
                ));
            }
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "native product preparation keeps each selected contract explicit"
    )]
    pub(super) fn prepare_native_codegen(
        &self,
        product: &ProductIdentity,
        kind: ProductKind,
        final_image: bool,
        source_roots: Vec<ConcreteCodegenRoot>,
        entry_roots: &[ConcreteCodegenInstance],
        runtime: Option<&RuntimeArtifact>,
        required_capabilities: impl IntoIterator<Item = RuntimeCapability>,
        target: &CodegenTarget,
        options: CodegenOptions,
        allow_bitcode: bool,
        cancellation: &CancellationToken,
    ) -> Result<NativeCodegenPreparation, NativeProductPlanningError> {
        if source_roots.is_empty() && kind != ProductKind::Test {
            return Ok((
                None,
                None,
                Arc::from([]),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                BTreeSet::new(),
            ));
        }

        let mut source_reachability = if source_roots.is_empty() {
            None
        } else {
            // Reachability owns its Arc-backed roots while preparation retains them for host MIR.
            let reachability = self.profile_native_product_operation(
                crate::profile::ProfileOperation::NativeReachability,
                || {
                    self.codegen_reachability(
                        source_roots.clone(),
                        None,
                        target,
                        options,
                        allow_bitcode,
                        cancellation,
                    )
                },
            )?;

            Some(reachability)
        };

        let (host, runtime_plan, host_statics, native_statics, callable_effects) = self
            .close_executable_host(
                product,
                kind,
                final_image,
                entry_roots,
                &mut source_reachability,
                runtime,
                required_capabilities,
                target,
                options,
                allow_bitcode,
                cancellation,
            )?;

        let platform_overrides =
            super::link::runtime_platform_services(runtime_plan.as_ref()).collect();

        let reachability = match host.as_ref() {
            Some(host) => {
                let host_mir = self.profile_native_product_operation(
                    crate::profile::ProfileOperation::NativeHostPreparation,
                    || {
                        self.lower_native_host(
                            host,
                            &source_roots,
                            entry_roots,
                            &host_statics,
                            target,
                        )
                    },
                )?;

                let host =
                    ConcreteCodegenInstance::generated(CodegenInstanceKey::non_generic(&host_mir));

                self.profile_native_product_operation(
                    crate::profile::ProfileOperation::NativeReachability,
                    || match source_reachability {
                        Some(source) => self.extend_codegen_reachability(
                            source,
                            [ConcreteCodegenRoot::new(
                                host,
                                NativeDemandReason::HostedRoot,
                            )],
                            (host_mir, source_roots),
                            target,
                            options,
                            allow_bitcode,
                            cancellation,
                        ),
                        None => self.codegen_reachability(
                            [ConcreteCodegenRoot::new(
                                host,
                                NativeDemandReason::HostedRoot,
                            )],
                            Some((host_mir, source_roots)),
                            target,
                            options,
                            allow_bitcode,
                            cancellation,
                        ),
                    },
                )?
            }
            None => source_reachability.ok_or(NativeProductPlanningError::MissingProductRoot)?,
        };

        let reachability = match host.as_ref() {
            Some(host) => {
                let host_root = reachability
                    .graph()
                    .roots()
                    .first()
                    .expect("generated executable host must be the reachability root")
                    .clone();

                let host_demands = host
                    .requirements()
                    .roles()
                    .iter()
                    .copied()
                    .map(|role| NativeDemand::runtime_role(host_root.clone(), role));

                reachability.with_demands(host_demands)
            }
            None => reachability,
        };

        let roots: BTreeSet<_> = reachability.graph().roots().iter().cloned().collect();

        let units = self.profile_native_product_operation(
            crate::profile::ProfileOperation::NativePartitioning,
            || self.partition_native_codegen(product, kind, &reachability, &roots, cancellation),
        )?;

        let mappings = self.profile_native_product_operation(
            crate::profile::ProfileOperation::NativeMapping,
            || {
                self.map_native_codegen_units(
                    product,
                    &units,
                    host.as_ref(),
                    &platform_overrides,
                    target,
                    &roots,
                    &reachability,
                    options.debug_information() != DebugInformationMode::None,
                    cancellation,
                )
            },
        )?;

        if let Some(profile) = self.state.fact_runtime.profile() {
            profile.set_native_codegen_plan(
                reachability.graph(),
                reachability.demands(),
                host.as_ref(),
                &units,
                &mappings,
            );
        }

        let native_main_thread = if kind == ProductKind::Library {
            callable_effects
                .into_iter()
                .filter_map(|(key, effects)| effects.requires_main_thread.then_some(key))
                .collect()
        } else {
            BTreeSet::new()
        };

        Ok((
            host,
            runtime_plan,
            units,
            mappings,
            host_statics,
            native_statics,
            native_main_thread,
        ))
    }

    fn lower_native_host(
        &self,
        host: &ExecutableHostContract,
        source_roots: &[ConcreteCodegenRoot],
        entry_roots: &[ConcreteCodegenInstance],
        host_statics: &[ProductStaticHostEntry],
        target: &CodegenTarget,
    ) -> Result<bray_ir::MirUnit, NativeProductPlanningError> {
        // Generated host MIR owns its target and host contracts after this query returns.
        let host_target = source_roots.first().map_or_else(
            || {
                bray_ir::MirTargetContract::new(
                    target.profile().clone(),
                    self.selected_target().target().runtime_abi(),
                )
            },
            |root| root.key().target().clone(),
        );

        bray_lowering::lower_executable_host(
            bray_lowering::ExecutableHostLoweringInput::new(
                GENERATED_HOST_UNIT,
                entry_roots
                    .iter()
                    .map(|root| bound_template(root.key()))
                    .collect::<Result<Vec<_>, _>>()?,
                host.clone(),
                host_target,
            )
            .with_statics(
                host_statics
                    .iter()
                    .filter(|entry| {
                        entry.key().duration() == bray_symbols::StaticStorageDuration::Product
                    })
                    .map(ProductStaticHostEntry::lowering_entry),
            ),
        )
        .map_err(NativeProductPlanningError::MirCapacity)
    }

    fn partition_native_codegen(
        &self,
        product: &ProductIdentity,
        kind: ProductKind,
        reachability: &ConcreteCodegenReachability,
        roots: &BTreeSet<CodegenInstanceKey>,
        cancellation: &CancellationToken,
    ) -> Result<Arc<[CodegenUnit]>, NativeProductPlanningError> {
        let completed = self
            .state
            .fact_runtime
            .complete_batch(
                reachability
                    .graph()
                    .instances()
                    .iter()
                    .map(|instance| instance.key().clone()),
                cancellation,
                |key| {
                    self.profile_native_product_operation(
                        crate::profile::ProfileOperation::NativePartitioning,
                        || {
                            let instance = reachability.graph().instance(key)
                                .expect("partition batch keys must come from retained reachability instances");

                            let compatibility = self
                                .codegen_partition_compatibility(
                                    instance,
                                    product,
                                    roots,
                                    cancellation,
                                )
                                .map_err(NativeProductPlanningError::from)?;

                            Ok(BatchWork::leaf(compatibility))
                        },
                    )
                },
            )
            .map_err(native_batch_error)?;

        let compatibility = completed.into_iter().collect::<BTreeMap<_, _>>();

        // The partitioner receives owned compatibility identities independently of the table.
        let policy = if kind == ProductKind::Library {
            CodegenPartitionPolicy::NATIVE_LIBRARY_PUBLICATION
        } else {
            CodegenPartitionPolicy::NATIVE_BALANCED
        };

        let mut identities = BTreeMap::new();

        for instance in reachability.graph().instances() {
            let mir = instance.mir();

            if !identities.contains_key(&mir.unit()) {
                identities.insert(mir.unit(), super::mir_content_identity(self, mir)?);
            }
        }

        partition_codegen_units(
            policy,
            reachability.graph(),
            |instance| compatibility.get(instance.key()).cloned(),
            |mir| {
                *identities
                    .get(&mir.unit())
                    .expect("partition MIR identity must be prepared")
            },
        )
        .map_err(NativeProductPlanningError::InvalidCodegenPartition)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "unit mapping requires the complete selected native product context"
    )]
    fn map_native_codegen_units(
        &self,
        product: &ProductIdentity,
        units: &[CodegenUnit],
        host: Option<&ExecutableHostContract>,
        platform_overrides: &BTreeSet<bray_runtime_interface::PlatformServiceRole>,
        target: &CodegenTarget,
        roots: &BTreeSet<CodegenInstanceKey>,
        reachability: &ConcreteCodegenReachability,
        debug_information: bool,
        cancellation: &CancellationToken,
    ) -> Result<Vec<CodegenMappings>, NativeProductPlanningError> {
        let units_by_key = units
            .iter()
            .map(|unit| (unit.key().clone(), unit))
            .collect::<BTreeMap<_, _>>();

        let completed =
            self.state
                .fact_runtime
                .complete_batch(units_by_key.keys().cloned(), cancellation, |key| {
                    self.profile_native_product_operation(
                        crate::profile::ProfileOperation::NativeMapping,
                        || {
                            let unit = units_by_key.get(key).copied().expect(
                                "mapping batch keys must come from the retained codegen units",
                            );

                            let mappings = self
                                .codegen_mappings_for_product(
                                    product,
                                    unit,
                                    host,
                                    platform_overrides,
                                    target,
                                    roots,
                                    reachability,
                                    debug_information,
                                    cancellation,
                                )
                                .map_err(NativeProductPlanningError::from)?;

                            Ok(BatchWork::leaf(mappings))
                        },
                    )
                })
                .map_err(native_batch_error)?;

        let mut completed = completed.into_iter().collect::<BTreeMap<_, _>>();
        let mut mappings = Vec::with_capacity(units.len());

        for unit in units {
            mappings.push(
                completed
                    .remove(unit.key())
                    .expect("completed mapping batch must publish every retained codegen unit"),
            );
        }

        assert!(
            completed.is_empty(),
            "mapping batch must publish only retained codegen units"
        );

        Ok(mappings)
    }
}
