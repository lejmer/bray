use std::collections::BTreeSet;

use bray_bound_tree::CheckedMemoryOperationKind;
use bray_codegen::{CodegenLinkage, CodegenTarget, demanded_runtime_references_for_mir};
use bray_compiler_known::RepresentationRole;
use bray_ir::{MirOperationKind, MirTextOperationKind};
use bray_runtime_interface::{
    BinarySymbolName, ExecutableEntryResult, ExecutableHostContract, ExecutableHostContractBuilder,
    ExecutableHostEntry, RootExecution, RuntimeAbiRole, RuntimeArtifact, RuntimeCapability,
    RuntimeRequirements, RuntimeRoleBinding, RuntimeRoleImplementation,
};
use bray_symbols::{GenericArgument, ProductIdentity, ProductKind};

use super::super::super::Compilation;
use super::super::specialization::ConcreteCodegenInstance;
use super::error::NativeProductPlanningError;
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(super) fn executable_host(
        &self,
        product: &ProductIdentity,
        kind: ProductKind,
        roots: &[ConcreteCodegenInstance],
        reachability: Option<&bray_codegen::CodegenReachability>,
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

        let mut runtime_roles = match reachability {
            Some(reachability) => demanded_product_runtime_roles(reachability),
            None => BTreeSet::new(),
        };

        if kind != ProductKind::Test {
            runtime_roles.remove(&RuntimeAbiRole::TestEntrySelection);
        }

        if has_async_entries {
            runtime_roles.insert(RuntimeAbiRole::RootExecution);
            runtime_roles.extend(RuntimeAbiRole::EXECUTABLE_HOST_CONTROL);
        }

        if kind == ProductKind::Test && !entries.is_empty() {
            runtime_roles.insert(RuntimeAbiRole::TestEntrySelection);
        }

        let synchronous_host_runtime = (kind == ProductKind::Test && !entries.is_empty())
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
                RuntimeAbiRole::RootCompletionResolution,
                RuntimeAbiRole::PanicReporting,
                RuntimeAbiRole::EntryFailureReporting,
                RuntimeAbiRole::StructuredShutdown,
            ]);
        }

        if runtime_contract.is_none() && (has_async_entries || synchronous_host_runtime) {
            return Err(NativeProductPlanningError::MissingRuntime);
        }

        let mut capabilities: BTreeSet<_> = required_capabilities.into_iter().collect();

        if let Some(reachability) = reachability {
            capabilities.extend(demanded_product_runtime_capabilities(reachability));
        }

        if has_async_entries {
            capabilities.insert(RuntimeCapability::CooperativeExecution);
            capabilities.insert(RuntimeCapability::MainThreadLane);
        }

        let requires_runtime = !runtime_roles.is_empty() || !capabilities.is_empty();

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
            .chain(RuntimeAbiRole::EXECUTABLE_HOST_CONTROL)
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

        let data = values
            .type_data(ty)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

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
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                let bray_symbols::TypeData::Named { substitution, .. } = data.as_ref() else {
                    return Err(NativeProductPlanningError::InvalidEntryResult);
                };

                let substitution = values
                    .generic_substitution_data(*substitution)
                    .map_err(|_| FactQueryError::InfrastructureFailure)?;

                let [success, error] = substitution.bindings() else {
                    return Err(NativeProductPlanningError::InvalidEntryResult);
                };

                let GenericArgument::Type(success) = success.argument() else {
                    return Err(NativeProductPlanningError::InvalidEntryResult);
                };

                let success = values
                    .type_data(success)
                    .map_err(|_| FactQueryError::InfrastructureFailure)?;

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

fn demanded_product_runtime_capabilities(
    reachability: &bray_codegen::CodegenReachability,
) -> BTreeSet<RuntimeCapability> {
    reachability
        .instances()
        .iter()
        .flat_map(|instance| demanded_runtime_capabilities(instance.mir()))
        .collect()
}

pub(in crate::compilation) fn demanded_runtime_capabilities(
    mir: &bray_ir::MirUnit,
) -> BTreeSet<RuntimeCapability> {
    mir.operations()
        .iter()
        .filter_map(|operation| runtime_operation_capability(operation.kind()))
        .collect()
}

const fn runtime_operation_capability(operation: &MirOperationKind) -> Option<RuntimeCapability> {
    match operation {
        MirOperationKind::Memory(memory) => match memory.kind() {
            CheckedMemoryOperationKind::RawAllocate
            | CheckedMemoryOperationKind::RawDeallocate
            | CheckedMemoryOperationKind::Allocate
            | CheckedMemoryOperationKind::Deallocate
            | CheckedMemoryOperationKind::RawBufferRelease { .. }
            | CheckedMemoryOperationKind::RawBufferReplace { .. }
            | CheckedMemoryOperationKind::RawBufferRelocate { .. } => {
                Some(RuntimeCapability::MemoryOperations)
            }
            _ => None,
        },
        MirOperationKind::Text(text) => match text.kind() {
            MirTextOperationKind::ScalarCount
            | MirTextOperationKind::Equals
            | MirTextOperationKind::ScalarAt
            | MirTextOperationKind::ScalarSlice
            | MirTextOperationKind::FromUtf8 => Some(RuntimeCapability::StringOperations),
            MirTextOperationKind::CharacterScalarValue
            | MirTextOperationKind::CharacterFromScalarValue
            | MirTextOperationKind::CharacterUtf8Length
            | MirTextOperationKind::CharacterUtf8Byte
            | MirTextOperationKind::CharacterIsAlphabetic
            | MirTextOperationKind::CharacterIsNumeric
            | MirTextOperationKind::CharacterIsWhitespace => {
                Some(RuntimeCapability::CharacterOperations)
            }
            MirTextOperationKind::Release => Some(RuntimeCapability::MemoryOperations),
            MirTextOperationKind::IsEmpty | MirTextOperationKind::Utf8 => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::CheckedMemoryOperationKind;
    use bray_ir::{MirMemoryOperation, MirOperationKind, MirTextOperation, MirTextOperationKind};
    use bray_runtime_interface::RuntimeCapability;

    use super::runtime_operation_capability;

    #[test]
    fn native_builtin_operations_demand_their_owning_runtime_capability() {
        let memory = MirOperationKind::Memory(MirMemoryOperation::new(
            CheckedMemoryOperationKind::RawAllocate,
            [],
            [],
            None,
        ));

        let string = MirOperationKind::Text(MirTextOperation::new(
            MirTextOperationKind::ScalarSlice,
            [],
            [],
            None,
        ));

        let character = MirOperationKind::Text(MirTextOperation::new(
            MirTextOperationKind::CharacterIsAlphabetic,
            [],
            [],
            None,
        ));

        let release = MirOperationKind::Text(MirTextOperation::new(
            MirTextOperationKind::Release,
            [],
            [],
            None,
        ));

        assert_eq!(
            runtime_operation_capability(&memory),
            Some(RuntimeCapability::MemoryOperations)
        );

        assert_eq!(
            runtime_operation_capability(&string),
            Some(RuntimeCapability::StringOperations)
        );

        assert_eq!(
            runtime_operation_capability(&character),
            Some(RuntimeCapability::CharacterOperations)
        );

        assert_eq!(
            runtime_operation_capability(&release),
            Some(RuntimeCapability::MemoryOperations)
        );
    }

    #[test]
    fn inline_text_operations_do_not_select_native_builtins() {
        for kind in [MirTextOperationKind::IsEmpty, MirTextOperationKind::Utf8] {
            let operation = MirOperationKind::Text(MirTextOperation::new(kind, [], [], None));

            assert_eq!(runtime_operation_capability(&operation), None);
        }
    }
}
