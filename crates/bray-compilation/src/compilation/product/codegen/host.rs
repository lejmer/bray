use std::collections::{BTreeMap, BTreeSet};

use bray_codegen::{
    CodegenLinkage, CodegenMappings, CodegenProductHostMapping, CodegenProductHostStatic,
    CodegenTarget,
};
use bray_compiler_known::RepresentationRole;
use bray_runtime_interface::{
    BinarySymbolName, ExecutableEntryResult, ExecutableHostContract, ExecutableHostContractBuilder,
    ExecutableHostEntry, RootExecution, RuntimeAbiRole, RuntimeArtifact, RuntimeCapability,
    RuntimeRequirements, RuntimeRoleBinding, RuntimeRoleImplementation, RuntimeServiceClass,
};
use bray_symbols::{GenericArgument, ProductIdentity, ProductKind};

use super::super::super::Compilation;
use super::super::specialization::{ConcreteCodegenInstance, ConcreteCodegenReachability};
use super::NativeDemand;
use super::error::NativeProductPlanningError;
use crate::fact::CancellationToken;

impl Compilation {
    pub(super) fn codegen_product_host_mapping(
        &self,
        product: &ProductIdentity,
        mappings: &[CodegenMappings],
        entries: &[super::super::realization::ProductStaticHostEntry],
        native_statics: &[bray_native_artifact::NativeStatic],
        mut runtime_dependencies: BTreeMap<bray_runtime_abi::NativeStaticIdentity, Vec<bray_runtime_abi::NativeStaticIdentity>>,
        target: &CodegenTarget,
    ) -> Result<Option<CodegenProductHostMapping>, NativeProductPlanningError> {
        if entries.is_empty() && native_statics.is_empty() {
            return Ok(None);
        }

        let owner = mappings
            .iter()
            .find(|mapping| {
                mapping
                    .static_storages()
                    .iter()
                    .any(bray_codegen::CodegenStaticStorageMapping::defines_storage)
            })
            .or_else(|| mappings.first())
            .map(CodegenMappings::unit)
            .expect("product host entries require an emitted host or static unit");

        let mut realizations = BTreeMap::new();

        for mapping in mappings.iter().flat_map(CodegenMappings::static_storages) {
            if let Some(previous) = realizations.insert(mapping.instance(), mapping)
                && previous.symbol() != mapping.symbol()
            {
                panic!(
                    "static {:?} must have one emitted symbol, found {:?} and {:?}",
                    mapping.instance(),
                    previous.symbol(),
                    mapping.symbol()
                );
            }
        }

        let descriptor_symbol = BinarySymbolName::try_new(bray_codegen::LINKED_PRODUCT_HOST_SYMBOL)
            .expect("linked product descriptor symbol must be valid");

        let control_symbol = super::super::realization::generated_symbol_name(
            target,
            CodegenLinkage::LinkOnce,
            "product_host_control",
            product,
        )?;

        let identity = bray_runtime_abi::NativeProductIdentity::new(
            super::super::realization::generated_identity("product_host", product),
        );

        let mut statics = entries
            .iter()
            .enumerate()
            .map(|(order, entry)| {
                let mapping = realizations
                    .get(entry.key())
                    .expect("retained source static must have its emitted storage mapping");

                let host_symbol = BinarySymbolName::try_new(mapping.host_name())
                    .expect("emitted static host symbol must be a valid binary name");

                let identity = bray_runtime_abi::NativeStaticIdentity::new(
                    super::super::realization::generated_identity("static_host", entry.key()),
                );

                let dependencies = entry.dependencies().iter().map(|dependency| {
                    static_dependency_identity(dependency, native_statics)
                });

                let order = u64::try_from(order)
                    .expect("retained static ordinal must fit the native host ABI");

                CodegenProductHostStatic::new(
                    host_symbol,
                    identity,
                    entry.key().duration(),
                    order,
                    dependencies,
                )
            })
            .collect::<Vec<_>>();

        statics.extend(native_statics.iter().map(|entry| {
            CodegenProductHostStatic::new(
                BinarySymbolName::try_new(entry.symbol())
                    .expect("validated native static symbol must be valid"),
                bray_runtime_abi::NativeStaticIdentity::new(entry.identity()),
                entry.duration(),
                0,
                entry
                    .dependencies()
                    .iter()
                    .copied()
                    .map(bray_runtime_abi::NativeStaticIdentity::new),
            )
        }));

        let key_inputs = entries
            .iter()
            .map(|entry| {
                (
                    bray_runtime_abi::NativeStaticIdentity::new(
                        super::super::realization::generated_identity("static_host", entry.key()),
                    ),
                    entry.order_key(),
                )
            })
            .chain(native_statics.iter().map(|entry| {
                (
                    bray_runtime_abi::NativeStaticIdentity::new(entry.identity()),
                    entry.order_key(),
                )
            }));

        let mut order_keys = BTreeMap::new();

        for (identity, key) in key_inputs {
            if let Some(previous) = order_keys.insert(identity, key) {
                if previous != key {
                    return Err(NativeProductPlanningError::NativeResolution(
                        bray_native_artifact::NativeResolutionError::ConflictingStatic(
                            identity.bytes(),
                        ),
                    ));
                }
            }
        }

        for entry in entries {
            let identity = bray_runtime_abi::NativeStaticIdentity::new(
                super::super::realization::generated_identity("static_host", entry.key()),
            );

            runtime_dependencies.entry(identity).or_default().extend(
                entry.runtime_dependencies().iter().map(|provider| static_dependency_identity(provider, native_statics)),
            );
        }

        let statics = order_product_statics(statics, &order_keys, &runtime_dependencies)
            .map_err(NativeProductPlanningError::NativeResolution)?;

        // The product mapping owns the Arc-backed unit identity after preparation returns.
        Ok(Some(
            CodegenProductHostMapping::try_new(
                owner.clone(),
                identity,
                descriptor_symbol,
                control_symbol,
                statics,
            )
            .expect("ordered static host contributions must form a valid product mapping"),
        ))
    }

    pub(super) fn product_runtime_requirements(
        &self,
        runtime: Option<&RuntimeArtifact>,
        target: &CodegenTarget,
        roles: &BTreeSet<RuntimeAbiRole>,
        mut capabilities: BTreeSet<RuntimeCapability>,
    ) -> Result<RuntimeRequirements, NativeProductPlanningError> {
        if roles
            .iter()
            .any(|role| role.service_class() == Some(RuntimeServiceClass::Execution))
        {
            capabilities.insert(RuntimeCapability::CooperativeExecution);
        }

        let requires_runtime = !roles.is_empty() || !capabilities.is_empty();

        let runtime = runtime
            .filter(|_| requires_runtime)
            .map(RuntimeArtifact::contract);

        if runtime.is_none() && requires_runtime {
            return Err(NativeProductPlanningError::MissingRuntime);
        }

        Ok(RuntimeRequirements::new(
            runtime.map(|runtime| runtime.identity().clone()),
            self.selected_target().target().runtime_abi(),
            runtime.map(|runtime| runtime.frame_abi()),
            target.identity().clone(),
            target.panic_abi().clone(),
            roles.iter().copied(),
            capabilities,
            [],
        ))
    }

    pub(super) fn executable_host(
        &self,
        product: &ProductIdentity,
        kind: ProductKind,
        roots: &[ConcreteCodegenInstance],
        reachability: Option<&ConcreteCodegenReachability>,
        statics: &[super::super::realization::ProductStaticHostEntry],
        native_statics: &[bray_native_artifact::NativeStatic],
        runtime: Option<&RuntimeArtifact>,
        required_capabilities: impl IntoIterator<Item = RuntimeCapability>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<Option<ExecutableHostContract>, NativeProductPlanningError> {
        if kind == ProductKind::Library {
            return Ok(None);
        }

        let mut entries = Vec::with_capacity(roots.len());

        for root_realization in roots {
            let root = reachability
                .and_then(|reachability| reachability.graph().instance(root_realization.key()))
                .ok_or(NativeProductPlanningError::MissingProductRoot)?;

            let entry_result_type = root
                .mir()
                .frame_descriptor()
                .map(bray_ir::MirFrameDescriptor::result_type);

            let result =
                self.executable_entry_result(root_realization, entry_result_type, cancellation)?;

            let entry = match root.protected_frame_identity() {
                Some(frame) => ExecutableHostEntry::asynchronous(
                    frame,
                    super::super::realization::generated_frame_symbol_name(
                        target,
                        frame,
                        bray_runtime_interface::ProtectedFrameOperation::MoveBeforeStart,
                    )?,
                    result,
                ),
                None => ExecutableHostEntry::synchronous(result),
            };

            entries.push(entry);
        }

        if entries.is_empty() && kind != ProductKind::Test {
            return Err(NativeProductPlanningError::MissingProductRoot);
        }

        let has_async_entries = entries
            .iter()
            .any(|entry| matches!(entry.root(), RootExecution::Asynchronous { .. }));

        let runtime_contract = runtime.map(RuntimeArtifact::contract);

        let mut runtime_roles = native_host_runtime_roles(reachability, statics, native_statics);

        let has_exact_thread_statics =
            runtime_roles.contains(&RuntimeAbiRole::ThreadStaticCleanupRegistration);

        if kind != ProductKind::Test {
            runtime_roles.remove(&RuntimeAbiRole::TestEntrySelection);
        }

        if has_async_entries {
            runtime_roles.insert(RuntimeAbiRole::RootExecution);
            runtime_roles.extend(RuntimeAbiRole::host_controls());
        }

        if kind == ProductKind::Test && !entries.is_empty() {
            runtime_roles.insert(RuntimeAbiRole::TestEntrySelection);
        }

        if entries
            .iter()
            .any(|entry| entry.root() == RootExecution::Synchronous)
        {
            runtime_roles.extend([
                RuntimeAbiRole::PanicPropagation,
                RuntimeAbiRole::CurrentRunCancellationPropagation,
            ]);
        }

        let synchronous_host_runtime = (kind == ProductKind::Test && !entries.is_empty())
            || (has_exact_thread_statics
                && entries
                    .iter()
                    .any(|entry| entry.root() == RootExecution::Synchronous))
            || (entries
                .iter()
                .any(|entry| entry.root() == RootExecution::Synchronous)
                && (runtime_roles.contains(&RuntimeAbiRole::PanicPropagation)
                    || entries.iter().any(|entry| {
                        matches!(entry.result(), ExecutableEntryResult::Fallible { .. })
                    })));

        if synchronous_host_runtime {
            runtime_roles.extend([
                RuntimeAbiRole::SynchronousRootExecution,
                RuntimeAbiRole::CleanupIncidentReporting,
                RuntimeAbiRole::PanicReporting,
                RuntimeAbiRole::EntryFailureReporting,
                RuntimeAbiRole::StructuredShutdown,
            ]);
        }

        let mut capabilities: BTreeSet<_> = required_capabilities.into_iter().collect();

        if has_async_entries || requires_main_thread_cleanup(statics, native_statics) {
            capabilities.insert(RuntimeCapability::MainThreadLane);
            runtime_roles.insert(RuntimeAbiRole::RuntimeInitialization);
        }

        let requirements =
            self.product_runtime_requirements(runtime, target, &runtime_roles, capabilities)?;

        let requires_runtime = requirements.requires_implementation();

        let native_entry =
            BinarySymbolName::try_new("main").expect("executable native entry name must be valid");

        let mut entries = entries.into_iter();

        let mut builder = match entries.next() {
            Some(first_entry) => ExecutableHostContractBuilder::new(
                product.clone(),
                native_entry,
                first_entry,
                requirements,
            ),
            None => {
                ExecutableHostContractBuilder::empty(product.clone(), native_entry, requirements)
            }
        };

        for entry in entries {
            builder.push_entry(entry);
        }

        if let Some(runtime) = runtime_contract.filter(|_| requires_runtime) {
            builder.select_runtime(runtime.clone());
        }

        let root_role = if runtime_roles.contains(&RuntimeAbiRole::SynchronousRootExecution) {
            RuntimeAbiRole::SynchronousRootExecution
        } else {
            RuntimeAbiRole::RootExecution
        };

        for role in std::iter::once(root_role)
            .chain(RuntimeAbiRole::host_controls())
            .filter(|role| !runtime_roles.contains(role))
        {
            let symbol_name = super::super::realization::generated_symbol_name(
                target,
                CodegenLinkage::Internal,
                "host-role",
                &(product, role),
            )?;

            builder.push_role_binding(RuntimeRoleBinding::new(
                role,
                symbol_name,
                RuntimeRoleImplementation::CompilerLowering,
            ));
        }

        builder
            .finish()
            .map(Some)
            .map_err(NativeProductPlanningError::InvalidExecutableHost)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "native demand retains the selected product and codegen contracts"
    )]
    fn executable_entry_result(
        &self,
        root: &ConcreteCodegenInstance,
        async_result: Option<bray_symbols::TypeId>,
        cancellation: &CancellationToken,
    ) -> Result<ExecutableEntryResult, NativeProductPlanningError> {
        let ty = match async_result {
            Some(ty) => ty,
            None => match self
                .codegen_instance_signature(root, cancellation)?
                .result()
            {
                bray_codegen::CodegenResultMapping::Void => {
                    return Ok(ExecutableEntryResult::Unit);
                }
                bray_codegen::CodegenResultMapping::Direct { ty, .. } => *ty,
                bray_codegen::CodegenResultMapping::Indirect { pointee, .. } => *pointee,
            },
        };

        let values = self.semantic_value_store()?;

        let data = values.type_data(ty);

        let bray_symbols::TypeData::Named { definition, .. } = data.as_ref() else {
            return Err(NativeProductPlanningError::InvalidEntryResult);
        };

        let role = super::super::super::foreign::compiler_known_representation(self, *definition);

        match role {
            Some(RepresentationRole::Unit) => Ok(ExecutableEntryResult::Unit),
            Some(RepresentationRole::ScalarI32) => Ok(ExecutableEntryResult::I32),
            Some(RepresentationRole::Result) => {
                let representation = self
                    .available_compiler_known_symbols()
                    .result_representation()
                    .expect("selected compiler-known Result must have its representation");

                let bray_symbols::TypeData::Named { substitution, .. } = data.as_ref() else {
                    return Err(NativeProductPlanningError::InvalidEntryResult);
                };

                let substitution = values.generic_substitution_data(*substitution);

                let [success, error] = substitution.bindings() else {
                    return Err(NativeProductPlanningError::InvalidEntryResult);
                };

                let GenericArgument::Type(success) = success.argument() else {
                    return Err(NativeProductPlanningError::InvalidEntryResult);
                };

                let success = values.type_data(success);

                let bray_symbols::TypeData::Named { definition, .. } = success.as_ref() else {
                    return Err(NativeProductPlanningError::InvalidEntryResult);
                };

                if super::super::super::foreign::compiler_known_representation(self, *definition)
                    != Some(RepresentationRole::Unit)
                {
                    return Err(NativeProductPlanningError::InvalidEntryResult);
                }

                let GenericArgument::Type(error) = error.argument() else {
                    return Err(NativeProductPlanningError::InvalidEntryResult);
                };

                Ok(ExecutableEntryResult::Fallible {
                    ty,
                    error,
                    success_variant: representation.success_variant(),
                })
            }
            _ => Err(NativeProductPlanningError::InvalidEntryResult),
        }
    }
}

pub(super) fn native_static_host_entries(
    kind: ProductKind,
    native_statics: &[bray_native_artifact::NativeStatic],
    source_statics: &[super::super::realization::ProductStaticHostEntry],
    runtime_dependencies: &BTreeMap<bray_runtime_abi::NativeStaticIdentity, Vec<bray_runtime_abi::NativeStaticIdentity>>,
) -> Vec<bray_native_artifact::NativeStatic> {
    let contributions = native_statics
        .iter()
        .map(|entry| (entry.identity(), entry))
        .collect::<BTreeMap<_, _>>();

    let retained = bray_base::transitive_dependencies(
        native_statics
            .iter()
            .filter(|entry| kind == ProductKind::Library || entry.requires_host())
            .map(bray_native_artifact::NativeStatic::identity)
            .chain(
                source_statics
                    .iter()
                    .flat_map(|entry| entry.dependencies().iter().chain(entry.runtime_dependencies()))
                    .map(|key| static_dependency_identity(key, native_statics).bytes()),
            ),
        |identity| {
            contributions
                .get(identity)
                .into_iter()
                .flat_map(|entry| entry.dependencies())
                .copied()
                .chain(runtime_dependencies.get(&bray_runtime_abi::NativeStaticIdentity::new(*identity))
                    .into_iter().flatten().map(|identity| identity.bytes()))
        },
    );

    native_statics
        .iter()
        .filter(|entry| retained.contains(&entry.identity()))
        .cloned()
        .collect()
}

fn static_dependency_identity(
    key: &bray_codegen::CodegenStaticInstanceKey,
    native_statics: &[bray_native_artifact::NativeStatic],
) -> bray_runtime_abi::NativeStaticIdentity {
    let identity = native_statics.iter()
        .find(|entry| entry.order_key() == key.order_key() && entry.duration() == key.duration())
        .map_or_else(
            || super::super::realization::generated_identity("static_host", key),
            bray_native_artifact::NativeStatic::identity,
        );

    bray_runtime_abi::NativeStaticIdentity::new(identity)
}

pub(super) fn native_runtime_static_dependencies(
    selected: &bray_native_artifact::NativeUnitSelection,
    runtime: Option<&bray_runtime_interface::RuntimeArtifactPlan>,
    target: &CodegenTarget,
) -> BTreeMap<bray_runtime_abi::NativeStaticIdentity, Vec<bray_runtime_abi::NativeStaticIdentity>> {
    let providers = runtime.into_iter()
        .flat_map(|runtime| runtime.native_index().units())
        .flat_map(bray_native_artifact::NativeUnit::statics)
        .map(bray_native_artifact::NativeStatic::identity)
        .collect::<BTreeSet<_>>();
    let target = bray_target::NativeTarget::for_identity(target.identity())
        .expect("native static hosting requires a native target");

    selected.statics().iter().filter_map(|entry| {
        let symbol = bray_symbols::NativeSymbolContract::required_name(
            bray_base::NonEmptySharedStr::try_new(target.object_symbol_name(entry.symbol()).as_ref())
                .expect("retained native static host symbol must be nonempty"),
        );
        let accesses = selected.static_accesses(&symbol)
            .expect("retained native static must publish its access summary");
        let dependencies = accesses.iter().copied()
            .filter(|provider| *provider != entry.identity() && providers.contains(provider))
            .map(bray_runtime_abi::NativeStaticIdentity::new).collect::<Vec<_>>();

        (!dependencies.is_empty()).then(|| (bray_runtime_abi::NativeStaticIdentity::new(entry.identity()), dependencies))
    }).collect()
}

pub(super) fn native_host_runtime_roles(
    reachability: Option<&ConcreteCodegenReachability>,
    statics: &[super::super::realization::ProductStaticHostEntry],
    native_statics: &[bray_native_artifact::NativeStatic],
) -> BTreeSet<RuntimeAbiRole> {
    let mut roles = reachability
        .into_iter()
        .flat_map(ConcreteCodegenReachability::demands)
        .filter_map(NativeDemand::role)
        .collect::<BTreeSet<_>>();

    if !statics.is_empty() || !native_statics.is_empty() {
        roles.insert(RuntimeAbiRole::ProductHostControl);
    }

    if statics
        .iter()
        .any(|entry| entry.key().duration() == bray_symbols::StaticStorageDuration::ExactThread)
        || native_statics
            .iter()
            .any(|entry| entry.duration() == bray_symbols::StaticStorageDuration::ExactThread)
    {
        roles.extend([
            RuntimeAbiRole::ThreadAttachmentIdentity,
            RuntimeAbiRole::ThreadStaticCleanupRegistration,
        ]);
    }

    roles
}

pub(super) fn requires_main_thread_cleanup(
    statics: &[super::super::realization::ProductStaticHostEntry],
    native_statics: &[bray_native_artifact::NativeStatic],
) -> bool {
    statics
        .iter()
        .any(super::super::realization::ProductStaticHostEntry::requires_main_thread_cleanup)
        || native_statics
            .iter()
            .any(bray_native_artifact::NativeStatic::requires_main_thread)
}

fn order_product_statics(
    entries: Vec<CodegenProductHostStatic>,
    order_keys: &BTreeMap<bray_runtime_abi::NativeStaticIdentity, &[u8]>,
    runtime_dependencies: &BTreeMap<bray_runtime_abi::NativeStaticIdentity, Vec<bray_runtime_abi::NativeStaticIdentity>>,
) -> Result<Vec<CodegenProductHostStatic>, bray_native_artifact::NativeResolutionError> {
    use bray_native_artifact::NativeResolutionError as Error;

    let mut unique = BTreeMap::<_, CodegenProductHostStatic>::new();
    let mut order_identities = BTreeMap::new();

    for entry in entries {
        if let Some(previous) = unique.get(&entry.identity()) {
            if previous.host_symbol() != entry.host_symbol()
                || previous.duration() != entry.duration()
                || previous.dependencies() != entry.dependencies()
            {
                return Err(Error::ConflictingStatic(entry.identity().bytes()));
            }
        }

        if let Some(previous) =
            order_identities.insert(order_keys[&entry.identity()], entry.identity())
        {
            if previous != entry.identity() {
                return Err(Error::AmbiguousStaticOrder {
                    first: previous.bytes(),
                    second: entry.identity().bytes(),
                });
            }
        }

        unique.insert(entry.identity(), entry);
    }

    for entry in unique.values() {
        for provider in entry.dependencies().iter().chain(runtime_dependencies.get(&entry.identity()).into_iter().flatten()) {
            assert!(
                unique.contains_key(provider),
                "resolved static lifecycle provider {provider:?} must have a product host entry"
            );
        }
    }

    let dependencies = unique
        .iter()
        .map(|(identity, entry)| (*identity, entry.dependencies().iter().copied()
            .chain(runtime_dependencies.get(identity).into_iter().flatten().copied())
            .collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>()))
        .collect();

    let ordered = super::super::structural_order::dependency_order(&dependencies, order_keys)
        .map_err(|cycle| {
            Error::StaticLifecycleCycle(
                cycle.into_iter().map(|identity| identity.bytes()).collect(),
            )
        })?;

    Ok(ordered
        .into_iter()
        .enumerate()
        .map(|(order, identity)| {
            let entry = unique
                .remove(&identity)
                .expect("ordered static must have an entry");

            CodegenProductHostStatic::new(
                entry.host_symbol().clone(),
                entry.identity(),
                entry.duration(),
                u64::try_from(order).expect("static count must fit u64"),
                entry.dependencies().iter().copied(),
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::order_product_statics;
    use bray_codegen::CodegenProductHostStatic;
    use bray_native_artifact::NativeResolutionError;
    use bray_runtime_abi::NativeStaticIdentity;
    use bray_runtime_interface::BinarySymbolName;
    use bray_symbols::StaticStorageDuration;
    use std::collections::BTreeMap;

    fn entry(id: u8, dependencies: &[u8]) -> CodegenProductHostStatic {
        CodegenProductHostStatic::new(
            BinarySymbolName::try_new(format!("static_{id}")).unwrap(),
            NativeStaticIdentity::new([id; 32]),
            StaticStorageDuration::Product,
            0,
            dependencies
                .iter()
                .map(|id| NativeStaticIdentity::new([*id; 32])),
        )
    }

    #[test]
    fn native_hosts_keep_lifecycle_providers_without_hosting_unrelated_plain_storage() {
        let entry = |identity: u8, requires_host, dependencies: Vec<[u8; 32]>| {
            bray_native_artifact::NativeStatic::new(
                bray_base::NonEmptySharedStr::try_new(format!("host_{identity}")).unwrap(),
                [identity; 32],
                vec![identity].into(),
                StaticStorageDuration::Product,
                dependencies,
                requires_host,
                false,
            )
        };

        let statics = [
            entry(1, true, vec![[2; 32]]),
            entry(2, false, vec![]),
            entry(3, false, vec![]),
        ];

        assert_eq!(
            super::native_static_host_entries(bray_symbols::ProductKind::Executable, &statics, &[], &BTreeMap::new()),
            statics[..2]
        );

        assert_eq!(
            super::native_static_host_entries(bray_symbols::ProductKind::Test, &statics, &[], &BTreeMap::new()),
            statics[..2]
        );

        assert_eq!(
            super::native_static_host_entries(bray_symbols::ProductKind::Library, &statics, &[], &BTreeMap::new()),
            statics
        );

        assert!(
            super::native_static_host_entries(
                bray_symbols::ProductKind::Executable,
                &statics[1..],
                &[],
                &BTreeMap::new(),
            )
            .is_empty()
        );
    }

    #[test]
    fn imported_cleanup_retains_and_orders_runtime_providers_without_changing_portable_records() {
        let consumer = bray_native_artifact::NativeStatic::new(
            bray_base::NonEmptySharedStr::try_new("consumer").unwrap(),
            [2; 32],
            vec![2].into(),
            StaticStorageDuration::Product,
            [],
            true,
            false,
        );
        let provider = bray_native_artifact::NativeStatic::new(
            bray_base::NonEmptySharedStr::try_new("provider").unwrap(),
            [1; 32],
            vec![1].into(),
            StaticStorageDuration::Product,
            [],
            false,
            false,
        );
        let first = NativeStaticIdentity::new([1; 32]);
        let second = NativeStaticIdentity::new([2; 32]);
        let dependencies = BTreeMap::from([(second, vec![first])]);
        let statics = [provider, consumer];
        let retained = super::native_static_host_entries(
            bray_symbols::ProductKind::Executable,
            &statics,
            &[],
            &dependencies,
        );

        assert_eq!(retained, statics);

        let keys = BTreeMap::from([(first, b"a".as_slice()), (second, b"b".as_slice())]);
        let ordered = order_product_statics(
            vec![entry(1, &[]), entry(2, &[])],
            &keys,
            &dependencies,
        ).unwrap();

        assert_eq!(ordered.iter().map(CodegenProductHostStatic::identity).collect::<Vec<_>>(), [second, first]);
        assert!(ordered.iter().all(|entry| entry.dependencies().is_empty()));
    }

    #[test]
    #[should_panic(expected = "must have a product host entry")]
    fn product_host_requires_the_resolved_static_closure() {
        let first = NativeStaticIdentity::new([1; 32]);
        let keys = BTreeMap::from([(first, b"a".as_slice())]);
        let _ = order_product_statics(vec![entry(1, &[2])], &keys, &BTreeMap::new());
    }

    #[test]
    fn imported_lifecycle_dependencies_are_validated_before_ordering() {
        let first = NativeStaticIdentity::new([1; 32]);
        let second = NativeStaticIdentity::new([2; 32]);
        let keys = BTreeMap::from([(first, b"a".as_slice()), (second, b"b".as_slice())]);

        assert!(matches!(
            order_product_statics(vec![entry(1, &[2]), entry(2, &[1])], &keys, &BTreeMap::new()),
            Err(NativeResolutionError::StaticLifecycleCycle(_))
        ));

        assert!(matches!(
            order_product_statics(vec![entry(1, &[]), entry(1, &[2]), entry(2, &[])], &keys, &BTreeMap::new()),
            Err(NativeResolutionError::ConflictingStatic(_))
        ));

        let same_key = BTreeMap::from([(first, b"a".as_slice()), (second, b"a".as_slice())]);

        assert!(matches!(
            order_product_statics(vec![entry(1, &[]), entry(2, &[])], &same_key, &BTreeMap::new()),
            Err(NativeResolutionError::AmbiguousStaticOrder { .. })
        ));

        let ordered =
            order_product_statics(vec![entry(1, &[]), entry(1, &[]), entry(2, &[1])], &keys, &BTreeMap::new())
                .unwrap();

        assert_eq!(
            ordered
                .iter()
                .map(CodegenProductHostStatic::identity)
                .collect::<Vec<_>>(),
            [second, first]
        );
    }
}
