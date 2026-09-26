use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_codegen::{
    CodegenInstanceKey, CodegenMappings, CodegenOptions, CodegenPartitionPolicy, CodegenTarget,
    CodegenUnit, DebugInformationMode, partition_codegen_units,
};
use bray_runtime_interface::{ExecutableHostContract, RuntimeArtifact, RuntimeCapability};
use bray_symbols::{ProductIdentity, ProductKind};

use super::super::super::Compilation;
use super::super::error::{ProductDataKind, ProductQueryContext, ProductQueryFailure};
use super::super::realization::ProductStaticHostEntry;
use super::super::specialization::{ConcreteCodegenInstance, ConcreteCodegenReachability};
use super::error::{NativeProductPlanningError, native_batch_error};
use super::implementation::{GENERATED_HOST_UNIT, bound_template, runtime_artifact_purpose};
use super::{ConcreteCodegenRoot, NativeDemandReason};
use crate::fact::{BatchWork, CancellationToken, FactQueryError};

type NativeCodegenPreparation = (
    Option<ExecutableHostContract>,
    Arc<[CodegenUnit]>,
    Vec<CodegenMappings>,
    Vec<ProductStaticHostEntry>,
);

impl Compilation {
    #[expect(
        clippy::too_many_arguments,
        reason = "native product preparation keeps each selected contract explicit"
    )]
    pub(super) fn prepare_native_codegen(
        &self,
        product: &ProductIdentity,
        kind: ProductKind,
        source_roots: Vec<ConcreteCodegenRoot>,
        entry_roots: &[ConcreteCodegenInstance],
        runtime: Option<&RuntimeArtifact>,
        required_capabilities: impl IntoIterator<Item = RuntimeCapability>,
        target: &CodegenTarget,
        options: CodegenOptions,
        cancellation: &CancellationToken,
    ) -> Result<NativeCodegenPreparation, NativeProductPlanningError> {
        if source_roots.is_empty() && kind != ProductKind::Test {
            return Ok((None, Arc::from([]), Vec::new(), Vec::new()));
        }

        let source_reachability = if source_roots.is_empty() {
            None
        } else {
            // Reachability owns its Arc-backed roots while preparation retains them for host MIR.
            let reachability = self.profile_native_product_operation(
                crate::profile::ProfileOperation::NativeReachability,
                || self.codegen_reachability(source_roots.clone(), None, target, options, cancellation),
            )?;

            Some(reachability)
        };

        let host_statics = match source_reachability.as_ref() {
            Some(reachability) => self.profile_native_product_operation(
                crate::profile::ProfileOperation::NativeHostPreparation,
                || {
                    self.product_static_host_entries(kind, reachability, target, cancellation)
                        .map_err(NativeProductPlanningError::from)
                },
            )?,
            None => Vec::new(),
        };

        let foreign_callback_roles = if kind == ProductKind::Library {
            BTreeSet::new()
        } else if let Some(reachability) = source_reachability.as_ref() {
            self.foreign_callback_runtime_roles(reachability, cancellation)?
        } else {
            BTreeSet::new()
        };

        let host = self.profile_native_product_operation(
            crate::profile::ProfileOperation::NativeHostPreparation,
            || {
                self.executable_host(
                    product,
                    kind,
                    entry_roots,
                    source_reachability
                        .as_ref()
                        .map(ConcreteCodegenReachability::graph),
                    &host_statics,
                    runtime,
                    foreign_callback_roles,
                    required_capabilities,
                    target,
                    cancellation,
                )
            },
        )?;

        let platform_overrides = match (runtime, host.as_ref()) {
            (Some(runtime), Some(host)) if host.requirements().requires_implementation() => {
                let selection = runtime
                    .select(runtime_artifact_purpose(kind), host.requirements())
                    .map_err(NativeProductPlanningError::InvalidRuntimeSelection)?;

                super::link::runtime_platform_services(Some(&selection))
            }
            (Some(_) | None, Some(_) | None) => BTreeSet::new(),
        };

        let reachability = match host.as_ref() {
            Some(host) => {
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

                let host_mir = self.profile_native_product_operation(
                    crate::profile::ProfileOperation::NativeHostPreparation,
                    || {
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
                                        entry.key().duration()
                                            == bray_symbols::StaticStorageDuration::Product
                                    })
                                    .map(ProductStaticHostEntry::lowering_entry),
                            ),
                        )
                        .map_err(NativeProductPlanningError::MirCapacity)
                    },
                )?;

                let host =
                    ConcreteCodegenInstance::generated(CodegenInstanceKey::non_generic(&host_mir));

                self.profile_native_product_operation(
                    crate::profile::ProfileOperation::NativeReachability,
                    || {
                        self.codegen_reachability(
                            [ConcreteCodegenRoot::new(
                                host,
                                NativeDemandReason::HostedRoot,
                            )],
                            Some((host_mir, source_roots)),
                            target,
                            options,
                            cancellation,
                        )
                    },
                )?
            }
            None => source_reachability.ok_or(NativeProductPlanningError::MissingProductRoot)?,
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
                &units,
                &mappings,
            );

            let runtime_roles = host
                .iter()
                .flat_map(|host| host.requirements().roles())
                .map(|role| role.as_str());

            let native_callback_entries = mappings
                .iter()
                .flat_map(CodegenMappings::symbols)
                .filter_map(bray_codegen::CodegenSymbolMapping::native_entry)
                .map(bray_codegen::CodegenNativeEntryMapping::name)
                .map(bray_runtime_interface::BinarySymbolName::as_str);

            profile.add_native_product_contract(runtime_roles, native_callback_entries);
        }

        Ok((host, units, mappings, host_statics))
    }

    fn foreign_callback_runtime_roles(
        &self,
        reachability: &ConcreteCodegenReachability,
        cancellation: &CancellationToken,
    ) -> Result<BTreeSet<bray_runtime_interface::RuntimeAbiRole>, NativeProductPlanningError> {
        for instance in reachability.graph().instances() {
            let Some(boundary) =
                self.codegen_native_boundary(instance.key(), &BTreeSet::new(), cancellation)?
            else {
                continue;
            };

            if boundary.is_callback() {
                return Ok(BTreeSet::from(bray_codegen::FOREIGN_CALLBACK_RUNTIME_ROLES));
            }
        }

        Ok(BTreeSet::new())
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
                            let instance = reachability.graph().instance(key).ok_or_else(|| {
                                FactQueryError::from(ProductQueryFailure::missing(
                                    ProductQueryContext::Instance(key.clone()),
                                    ProductDataKind::PartitionInstance,
                                ))
                            })?;

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
            |mir| *identities.get(&mir.unit()).expect("partition MIR identity must be prepared"),
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

        let completed = self
            .state
            .fact_runtime
            .complete_batch(units_by_key.keys().cloned(), cancellation, |key| {
                self.profile_native_product_operation(
                    crate::profile::ProfileOperation::NativeMapping,
                    || {
                        let unit = units_by_key.get(key).copied().ok_or_else(|| {
                            FactQueryError::from(ProductQueryFailure::missing(
                                ProductQueryContext::CodegenUnit(key.clone()),
                                ProductDataKind::PartitionInstance,
                            ))
                        })?;

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
            mappings.push(completed.remove(unit.key()).ok_or_else(|| {
                FactQueryError::from(ProductQueryFailure::missing(
                    ProductQueryContext::CodegenUnit(unit.key().clone()),
                    ProductDataKind::CodegenUnitMapping,
                ))
            })?);
        }

        if let Some(key) = completed.keys().next() {
            return Err(FactQueryError::from(ProductQueryFailure::Conflict {
                context: ProductQueryContext::CodegenUnit(key.clone()),
                data: ProductDataKind::CodegenUnitMapping,
            })
            .into());
        }

        Ok(mappings)
    }
}
