use std::collections::{BTreeMap, BTreeSet};

use bray_codegen::{
    CodegenLinkage, CodegenMappings, CodegenProductHostMapping, CodegenProductHostStatic,
    CodegenTarget, CodegenUnit, demanded_runtime_references_for_mir,
};
use bray_compiler_known::RepresentationRole;
use bray_runtime_interface::{
    BinarySymbolName, ExecutableEntryResult, ExecutableHostContract, ExecutableHostContractBuilder,
    ExecutableHostEntry, RootExecution, RuntimeAbiRole, RuntimeArtifact,
    RuntimeCapability, RuntimeRequirements, RuntimeRoleBinding, RuntimeRoleImplementation,
    RuntimeServiceClass,
};
use bray_symbols::{GenericArgument, ProductIdentity, ProductKind};

use super::super::super::Compilation;
use super::super::error::{ProductDataKind, ProductQueryContext, ProductQueryFailure};
use super::super::specialization::ConcreteCodegenInstance;
use super::error::NativeProductPlanningError;
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(super) fn codegen_product_host_mapping(
        &self,
        product: &ProductIdentity,
        units: &[CodegenUnit],
        mappings: &[CodegenMappings],
        entries: &[super::super::realization::ProductStaticHostEntry],
        target: &CodegenTarget,
    ) -> Result<Option<CodegenProductHostMapping>, NativeProductPlanningError> {
        if entries.is_empty() {
            return Ok(None);
        }

        let owner = units.first().map(CodegenUnit::key).ok_or_else(|| {
            FactQueryError::from(ProductQueryFailure::missing(
                ProductQueryContext::Target(target.clone()),
                ProductDataKind::ProductHostOwnerUnit,
            ))
        })?;

        let mut realizations = BTreeMap::new();

        for mapping in mappings.iter().flat_map(CodegenMappings::static_storages) {
            if let Some(previous) = realizations.insert(mapping.instance(), mapping)
                && previous.symbol() != mapping.symbol()
            {
                return Err(FactQueryError::from(ProductQueryFailure::Conflict {
                    context: ProductQueryContext::CodegenStatic(mapping.instance().clone()),
                    data: ProductDataKind::ProductHostStaticMapping,
                })
                .into());
            }
        }

        let descriptor_symbol = super::super::realization::generated_symbol_name(
            target,
            CodegenLinkage::LinkOnce,
            "product_host",
            product,
        )?;

        let control_symbol = super::super::realization::generated_symbol_name(
            target,
            CodegenLinkage::LinkOnce,
            "product_host_control",
            product,
        )?;

        let identity = bray_runtime_abi::NativeProductIdentity::new(
            super::super::realization::generated_identity("product_host", product),
        );

        let statics = entries
            .iter()
            .enumerate()
            .map(|(order, entry)| {
                let mapping = realizations.get(entry.key()).ok_or_else(|| {
                    FactQueryError::from(ProductQueryFailure::missing(
                        ProductQueryContext::CodegenStatic(entry.key().clone()),
                        ProductDataKind::ProductHostStaticMapping,
                    ))
                })?;

                let host_symbol = BinarySymbolName::try_new(mapping.host_name())
                    .ok_or(NativeProductPlanningError::InvalidSymbolName)?;

                let identity = bray_runtime_abi::NativeStaticIdentity::new(
                    super::super::realization::generated_identity("static_host", entry.key()),
                );

                let dependencies = entry.dependencies().iter().map(|dependency| {
                    bray_runtime_abi::NativeStaticIdentity::new(
                        super::super::realization::generated_identity("static_host", dependency),
                    )
                });

                let order = u64::try_from(order).map_err(|_| {
                    FactQueryError::from(ProductQueryFailure::CountMismatch {
                        context: ProductQueryContext::CodegenStatic(entry.key().clone()),
                        data: ProductDataKind::ProductHostStaticMapping,
                        expected: usize::try_from(u64::MAX).unwrap_or(usize::MAX),
                        actual: order,
                    })
                })?;

                Ok(CodegenProductHostStatic::new(
                    host_symbol,
                    identity,
                    entry.key().duration(),
                    order,
                    dependencies,
                ))
            })
            .collect::<Result<Vec<_>, NativeProductPlanningError>>()?;

        // The product mapping owns the Arc-backed unit identity after preparation returns.
        CodegenProductHostMapping::try_new(
            owner.clone(),
            identity,
            descriptor_symbol,
            control_symbol,
            statics,
        )
        .map(Some)
        .ok_or_else(|| {
            FactQueryError::from(ProductQueryFailure::Conflict {
                context: ProductQueryContext::Target(target.clone()),
                data: ProductDataKind::ProductHostOwnerUnit,
            })
            .into()
        })
    }

    pub(super) fn executable_host(
        &self,
        product: &ProductIdentity,
        kind: ProductKind,
        roots: &[ConcreteCodegenInstance],
        reachability: Option<&bray_codegen::CodegenReachability>,
        statics: &[super::super::realization::ProductStaticHostEntry],
        runtime: Option<&RuntimeArtifact>,
        required_roles: impl IntoIterator<Item = RuntimeAbiRole>,
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
                .and_then(|reachability| reachability.instance(root_realization.key()))
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

        let mut runtime_roles: BTreeSet<_> = required_roles.into_iter().collect();

        if let Some(reachability) = reachability {
            runtime_roles.extend(demanded_product_runtime_roles(reachability));
        }

        if !statics.is_empty() {
            runtime_roles.insert(RuntimeAbiRole::ProductHostControl);
        }

        let has_exact_thread_statics = statics.iter().any(|entry| {
            entry.key().duration() == bray_symbols::StaticStorageDuration::ExactThread
        });

        if has_exact_thread_statics {
            runtime_roles.extend([
                RuntimeAbiRole::ThreadAttachmentIdentity,
                RuntimeAbiRole::ThreadStaticCleanupRegistration,
            ]);
        }

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

        if runtime_roles
            .iter()
            .any(|role| role.service_class() == Some(RuntimeServiceClass::Execution))
        {
            capabilities.insert(RuntimeCapability::CooperativeExecution);
        }

        if has_async_entries {
            capabilities.insert(RuntimeCapability::MainThreadLane);
        }

        let requires_runtime = !runtime_roles.is_empty() || !capabilities.is_empty();

        if runtime_contract.is_none() && requires_runtime {
            return Err(NativeProductPlanningError::MissingRuntime);
        }

        let requirements = RuntimeRequirements::new(
            runtime_contract
                .filter(|_| requires_runtime)
                .map(|runtime| runtime.identity().clone()),
            self.selected_target().target().runtime_abi(),
            runtime_contract
                .filter(|_| requires_runtime)
                .map(|runtime| runtime.frame_abi()),
            target.identity().clone(),
            target.panic_abi().clone(),
            runtime_roles.iter().copied(),
            capabilities,
            [],
        );

        let native_entry = BinarySymbolName::try_new("main")
            .ok_or(NativeProductPlanningError::InvalidSymbolName)?;

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
                    .ok_or_else(|| {
                        FactQueryError::from(ProductQueryFailure::missing(
                            ProductQueryContext::Type(ty),
                            ProductDataKind::ResultRepresentation,
                        ))
                    })?;

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

fn demanded_product_runtime_roles(
    reachability: &bray_codegen::CodegenReachability,
) -> BTreeSet<RuntimeAbiRole> {
    reachability
        .instances()
        .iter()
        .flat_map(|instance| demanded_runtime_references_for_mir(instance.mir()))
        .map(|reference| reference.role())
        .collect()
}
