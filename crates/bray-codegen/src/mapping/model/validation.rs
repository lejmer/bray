use std::collections::BTreeSet;

use bray_ir::{
    MirAsyncOperation, MirGeneratorOperation, MirHostOperation, MirOperationKind,
    MirRuntimeReference, MirSourceAnchor, MirTerminatorKind,
};
use bray_symbols::TypeId;

use crate::{
    CodegenConstantMapping, CodegenInstanceTypeMapping, CodegenOperationMapping,
    CodegenParameterMapping, CodegenSymbolKey, CodegenSymbolMapping, CodegenTypeKind,
    CodegenTypeMapping, CodegenUnit, FOREIGN_CALLBACK_RUNTIME_ROLES,
};

use super::core::CodegenMappingsBuildError;

/// Returns semantic types directly demanded by one code generation unit.
pub fn demanded_types(unit: &CodegenUnit) -> BTreeSet<TypeId> {
    unit.instances()
        .iter()
        .flat_map(|instance| instance.mir().referenced_types())
        .collect()
}

/// Returns source anchors directly demanded by one code generation unit.
pub fn demanded_debug_sources(unit: &CodegenUnit) -> BTreeSet<MirSourceAnchor> {
    unit.mir_units()
        .flat_map(|mir| {
            mir.blocks()
                .iter()
                .map(bray_ir::MirBlock::source)
                .chain(mir.storages().iter().map(bray_ir::MirStorage::source))
                .chain(mir.values().iter().map(bray_ir::MirValue::source))
                .chain(mir.operations().iter().map(bray_ir::MirOperation::source))
                .chain(mir.blocks().iter().map(|block| block.terminator().source()))
        })
        .cloned()
        .collect()
}

/// Returns private runtime symbols directly demanded by one code generation unit.
pub fn demanded_runtime_references(unit: &CodegenUnit) -> BTreeSet<MirRuntimeReference> {
    unit.mir_units()
        .flat_map(demanded_runtime_references_for_mir)
        .collect()
}

/// Returns private runtime symbols directly demanded by one MIR definition.
pub fn demanded_runtime_references_for_mir(
    mir: &bray_ir::MirUnit,
) -> BTreeSet<MirRuntimeReference> {
    let runtime_abi = mir.target().runtime_abi();

    mir.operations()
        .iter()
        .flat_map(|operation| operation_runtime_references(operation.kind()))
        .chain(
            mir.blocks()
                .iter()
                .flat_map(|block| terminator_runtime_references(block.terminator().kind())),
        )
        .chain(
            mir.operations()
                .iter()
                .flat_map(|operation| operation.kind().helper_references())
                .filter_map(|helper| helper.runtime_role())
                .map(|role| MirRuntimeReference::new(role, runtime_abi))
                .map(Some),
        )
        .chain(boundary_panic_propagation_is_demanded(mir).then_some(Some(
            MirRuntimeReference::new(
                bray_runtime_interface::RuntimeAbiRole::PanicPropagation,
                runtime_abi,
            ),
        )))
        .chain(checked_call_cancellation_propagation_is_demanded(mir).then_some(Some(
            MirRuntimeReference::new(
                bray_runtime_interface::RuntimeAbiRole::CurrentRunCancellationPropagation,
                runtime_abi,
            ),
        )))
        .flatten()
        .collect()
}

fn checked_call_cancellation_propagation_is_demanded(mir: &bray_ir::MirUnit) -> bool {
    mir.blocks().iter().any(|block| {
        matches!(
            block.terminator().kind(),
            MirTerminatorKind::CheckCallPanic { .. }
        )
    })
}

fn boundary_panic_propagation_is_demanded(mir: &bray_ir::MirUnit) -> bool {
    mir.blocks().iter().any(|block| {
        let checked_call = matches!(
            block.terminator().kind(),
            MirTerminatorKind::CheckCallPanic { .. }
        )
        .then(|| block.operations().last().copied())
        .flatten();

        block.operations().iter().any(|operation| {
            Some(*operation) != checked_call
                && mir.operation(*operation).is_some_and(|operation| {
                    matches!(
                        operation.kind(),
                        MirOperationKind::Call(call) if call.may_propagate_panic()
                    )
                })
        })
    })
}

/// Returns direct MIR roles plus runtime helpers selected by operation mappings.
pub fn mapped_runtime_references(
    unit: &CodegenUnit,
    operations: &[CodegenOperationMapping],
    symbols: &[CodegenSymbolMapping],
) -> BTreeSet<MirRuntimeReference> {
    let mut references = demanded_runtime_references(unit);

    references.extend(operations.iter().flat_map(|operation| {
        operation.helpers().iter().filter_map(|helper| {
            let Some(CodegenSymbolKey::Runtime(reference)) = helper.symbol() else {
                return None;
            };

            Some(*reference)
        })
    }));

    if symbols.iter().any(|symbol| {
        symbol.native_entry().is_some()
            && matches!(
                symbol.key(),
                CodegenSymbolKey::Instance(instance)
                    if unit.instances().iter().any(|member| member.key() == instance)
            )
    }) {
        references.extend(
            FOREIGN_CALLBACK_RUNTIME_ROLES
                .map(|role| MirRuntimeReference::new(role, unit.target().runtime_abi())),
        );
    }

    if symbols
        .iter()
        .any(|symbol| symbol.signature().has_panic_report_context())
    {
        references.insert(MirRuntimeReference::new(
            bray_runtime_interface::RuntimeAbiRole::PanicPropagation,
            unit.target().runtime_abi(),
        ));
    }

    references
}

pub(super) fn validate_type_structure(
    types: &[CodegenTypeMapping],
) -> Result<(), CodegenMappingsBuildError> {
    if types.windows(2).any(|pair| pair[0].ty() == pair[1].ty()) {
        return Err(CodegenMappingsBuildError::DuplicateType);
    }

    if types.iter().any(|mapping| {
        matches!(
            (mapping.layout(), mapping.kind()),
            (
                Some(_),
                CodegenTypeKind::Opaque
                    | CodegenTypeKind::UnsizedSlice { .. }
                    | CodegenTypeKind::UnsizedTraitView
            ) | (
                None,
                CodegenTypeKind::Unit
                    | CodegenTypeKind::Boolean
                    | CodegenTypeKind::SignedInteger(_)
                    | CodegenTypeKind::UnsignedInteger(_)
                    | CodegenTypeKind::Float(_)
                    | CodegenTypeKind::Pointer { .. }
                    | CodegenTypeKind::Aggregate(_)
                    | CodegenTypeKind::Array { .. }
                    | CodegenTypeKind::Union { .. }
                    | CodegenTypeKind::Callable(_)
            )
        )
    }) {
        return Err(CodegenMappingsBuildError::InvalidTypeLayout);
    }

    if types.iter().any(|mapping| {
        if mapping.backend_type() == mapping.ty() {
            return false;
        }

        types
            .binary_search_by_key(&mapping.backend_type(), CodegenTypeMapping::ty)
            .ok()
            .and_then(|index| types.get(index))
            .is_none_or(|backend| {
                backend.backend_type() != backend.ty()
                    || mapping.layout() != backend.layout()
                    || mapping.kind() != backend.kind()
            })
    }) {
        return Err(CodegenMappingsBuildError::InvalidTypeLayout);
    }

    Ok(())
}

pub(super) fn validate_instance_type_structure(
    unit: &CodegenUnit,
    types: &[CodegenTypeMapping],
    instance_types: &[CodegenInstanceTypeMapping],
) -> Result<(), CodegenMappingsBuildError> {
    if instance_types.windows(2).any(|pair| {
        pair[0].instance() == pair[1].instance() && pair[0].template() == pair[1].template()
    }) {
        return Err(CodegenMappingsBuildError::DuplicateInstanceType);
    }

    if instance_types.iter().any(|mapping| {
        !unit
            .instances()
            .iter()
            .any(|instance| instance.key() == mapping.instance())
            || types
                .binary_search_by_key(&mapping.concrete(), CodegenTypeMapping::ty)
                .is_err()
    }) {
        return Err(CodegenMappingsBuildError::InvalidInstanceType);
    }

    Ok(())
}

pub(super) fn validate_type_coverage(
    unit: &CodegenUnit,
    types: &[CodegenTypeMapping],
    instance_types: &[CodegenInstanceTypeMapping],
    symbols: &[CodegenSymbolMapping],
    constants: &[CodegenConstantMapping],
) -> Result<(), CodegenMappingsBuildError> {
    let mut expected_types = BTreeSet::new();

    expected_types.extend(constants.iter().map(|mapping| mapping.data().ty()));

    expected_types.extend(symbols.iter().flat_map(|symbol| {
        let signature = symbol.signature();

        signature
            .parameters()
            .iter()
            .flat_map(CodegenParameterMapping::demanded_types)
            .chain(signature.result().demanded_types())
            .flatten()
    }));

    if expected_types.iter().any(|ty| {
        types
            .binary_search_by_key(ty, CodegenTypeMapping::ty)
            .is_err()
    }) {
        return Err(CodegenMappingsBuildError::TypeCoverageMismatch);
    }

    if unit.instances().iter().any(|instance| {
        instance.mir().referenced_types().iter().any(|ty| {
            let key = (instance.key(), *ty);

            let concrete = instance_types
                .binary_search_by(|mapping| (mapping.instance(), mapping.template()).cmp(&key))
                .ok()
                .and_then(|index| instance_types.get(index))
                .map(CodegenInstanceTypeMapping::concrete)
                .unwrap_or(*ty);

            types
                .binary_search_by_key(&concrete, CodegenTypeMapping::ty)
                .is_err()
        })
    }) {
        return Err(CodegenMappingsBuildError::TypeCoverageMismatch);
    }

    let is_unsized = |ty: TypeId| {
        types
            .binary_search_by_key(&ty, CodegenTypeMapping::ty)
            .ok()
            .is_some_and(|index| types[index].layout().is_none())
    };

    if symbols
        .iter()
        .any(|symbol| signature_passes_unsized_by_value(symbol.signature(), &is_unsized))
        || types.iter().any(|mapping| {
            let CodegenTypeKind::Callable(signature) = mapping.kind() else {
                return false;
            };

            signature_passes_unsized_by_value(signature, &is_unsized)
        })
    {
        return Err(CodegenMappingsBuildError::InvalidAbiTypeLayout);
    }

    Ok(())
}

fn signature_passes_unsized_by_value(
    signature: &crate::CodegenCallableSignature,
    is_unsized: &impl Fn(TypeId) -> bool,
) -> bool {
    signature
        .parameters()
        .iter()
        .any(|parameter| match parameter {
            CodegenParameterMapping::Ignore => false,
            CodegenParameterMapping::Direct { ty, .. } => is_unsized(*ty),
            CodegenParameterMapping::Indirect {
                pointer, pointee, ..
            } => is_unsized(*pointer) || is_unsized(*pointee),
        })
        || match signature.result() {
            crate::CodegenResultMapping::Void => false,
            crate::CodegenResultMapping::Direct { ty, .. } => is_unsized(*ty),
            crate::CodegenResultMapping::Indirect {
                pointer, pointee, ..
            } => is_unsized(*pointer) || is_unsized(*pointee),
        }
}

fn operation_runtime_references(operation: &MirOperationKind) -> [Option<MirRuntimeReference>; 3] {
    match operation {
        MirOperationKind::Call(call) => match call.target() {
            bray_ir::MirCallTarget::Runtime(runtime) => [Some(*runtime), None, None],
            bray_ir::MirCallTarget::Direct(_) | bray_ir::MirCallTarget::Indirect { .. } => {
                [None, None, None]
            }
        },
        MirOperationKind::Async(MirAsyncOperation::StartTask {
            allocation, start, ..
        }) => [Some(*allocation), Some(*start), None],
        MirOperationKind::Async(MirAsyncOperation::CreateFrame {
            initializer: bray_ir::MirFrameInitializer::TaskObservation { runtime, .. },
            ..
        }) => [Some(*runtime), None, None],
        MirOperationKind::Host(MirHostOperation::ExecuteRoot { runtime, .. }) => [
            Some(*runtime),
            Some(MirRuntimeReference::new(
                bray_runtime_interface::RuntimeAbiRole::PanicPropagation,
                runtime.abi_version(),
            )),
            None,
        ],
        MirOperationKind::Host(MirHostOperation::ResolveRootTerminal {
            completion,
            panic,
            entry_failure,
            ..
        }) => [Some(*completion), Some(*panic), Some(*entry_failure)],
        MirOperationKind::Async(
            MirAsyncOperation::ResumeFrame { runtime, .. }
            | MirAsyncOperation::RequestTaskCancellation { runtime, .. }
            | MirAsyncOperation::ObserveCurrentRunCancellation { runtime }
            | MirAsyncOperation::ResolveTask { runtime, .. }
            | MirAsyncOperation::PublishTerminalState { runtime, .. }
            | MirAsyncOperation::ExecuteCleanupBroadcast { runtime, .. }
            | MirAsyncOperation::ExecuteLifecycleResolution { runtime, .. }
            | MirAsyncOperation::TransferCleanupIncident { runtime, .. },
        )
        | MirOperationKind::Host(
            MirHostOperation::SelectTestEntry { runtime, .. }
            | MirHostOperation::ObserveRootTerminal { runtime, .. }
            | MirHostOperation::ReportCleanupIncidents { runtime }
            | MirHostOperation::StructuredShutdown { runtime },
        )
        | MirOperationKind::Generator(
            MirGeneratorOperation::CleanupBroadcast { runtime, .. }
            | MirGeneratorOperation::Destroy { runtime, .. },
        ) => [Some(*runtime), None, None],
        MirOperationKind::AnonymousCallable(_)
        | MirOperationKind::DeclaredCallable(_)
        | MirOperationKind::Store { .. }
        | MirOperationKind::Borrow { .. }
        | MirOperationKind::Unary { .. }
        | MirOperationKind::Binary { .. }
        | MirOperationKind::Aggregate(_)
        | MirOperationKind::Construct(_)
        | MirOperationKind::Convert { .. }
        | MirOperationKind::NumericConversion { .. }
        | MirOperationKind::PatternProjection { .. }
        | MirOperationKind::Generator(
            MirGeneratorOperation::Begin { .. }
            | MirGeneratorOperation::Push { .. }
            | MirGeneratorOperation::Finish { .. },
        )
        | MirOperationKind::Memory(_)
        | MirOperationKind::Text(_)
        | MirOperationKind::PanicReport(_)
        | MirOperationKind::Finalize(_)
        | MirOperationKind::Destroy(_)
        | MirOperationKind::Cleanup { .. }
        | MirOperationKind::Host(
            MirHostOperation::MaterializeStatic { .. } | MirHostOperation::BeginStaticCleanup,
        )
        | MirOperationKind::Async(
            MirAsyncOperation::CreateFrame {
                initializer: bray_ir::MirFrameInitializer::Callable(_),
                ..
            }
            | MirAsyncOperation::MoveInactiveFrame { .. }
            | MirAsyncOperation::ComposeAwaitedFrame { .. }
            | MirAsyncOperation::CommitAwaitedCompletion { .. }
            | MirAsyncOperation::DestroyTerminalTask { .. },
        ) => [None, None, None],
    }
}

fn terminator_runtime_references(
    terminator: &MirTerminatorKind,
) -> [Option<MirRuntimeReference>; 3] {
    match terminator {
        MirTerminatorKind::Suspend {
            registration, wake, ..
        } => [Some(*registration), Some(*wake), None],
        MirTerminatorKind::PropagatePanic { runtime, .. }
        | MirTerminatorKind::PropagateCancellation { runtime } => [Some(*runtime), None, None],
        MirTerminatorKind::Goto(_)
        | MirTerminatorKind::Branch { .. }
        | MirTerminatorKind::PatternBranch { .. }
        | MirTerminatorKind::Iterate { .. }
        | MirTerminatorKind::RangeIterate { .. }
        | MirTerminatorKind::Switch { .. }
        | MirTerminatorKind::InlineAssembly(_)
        | MirTerminatorKind::Return(_)
        | MirTerminatorKind::Unreachable
        | MirTerminatorKind::ForwardRunResult { .. }
        | MirTerminatorKind::CheckCallPanic { .. }
        | MirTerminatorKind::BeginCleanup(_)
        | MirTerminatorKind::ContinueCleanup(_)
        | MirTerminatorKind::Panic { .. }
        | MirTerminatorKind::CancelCurrentRun { .. } => [None, None, None],
    }
}

#[cfg(test)]
mod tests {
    use bray_runtime_interface::{BinarySymbolName, RuntimeAbiRole};

    use crate::test_support::codegen_request;
    use crate::{
        CodegenLinkage, CodegenNativeEntryMapping, CodegenSymbolMapping, mapped_runtime_references,
    };

    #[test]
    fn callback_runtime_roles_follow_explicit_native_entry() {
        let fixture = codegen_request();
        let request = fixture.request();

        let symbol = request
            .mappings()
            .symbols()
            .first()
            .unwrap_or_else(|| panic!("fixture must map one symbol"));

        let direct = CodegenSymbolMapping::new(
            symbol.key().clone(),
            symbol.name().clone(),
            CodegenLinkage::Fallback,
            symbol.signature().clone(),
        );

        let references = mapped_runtime_references(request.unit(), &[], &[direct.clone()]);

        assert!(
            !references
                .iter()
                .any(|reference| reference.role() == RuntimeAbiRole::ForeignCallbackExecution)
        );

        let callback = direct.with_native_entry(CodegenNativeEntryMapping::new(
            BinarySymbolName::try_new("native_callback")
                .unwrap_or_else(|| panic!("native callback symbol must validate")),
            CodegenLinkage::Export,
        ));

        let references = mapped_runtime_references(request.unit(), &[], &[callback]);

        for role in crate::FOREIGN_CALLBACK_RUNTIME_ROLES {
            assert!(references.iter().any(|reference| reference.role() == role));
        }
    }

    #[test]
    fn hidden_panic_context_demands_panic_propagation_runtime() {
        let fixture = codegen_request();
        let request = fixture.request();

        let symbol = request
            .mappings()
            .symbols()
            .first()
            .unwrap_or_else(|| panic!("fixture must map one symbol"));

        let direct = CodegenSymbolMapping::new(
            symbol.key().clone(),
            symbol.name().clone(),
            CodegenLinkage::Fallback,
            symbol.signature().clone().with_panic_report_context(),
        );

        let references = mapped_runtime_references(request.unit(), &[], &[direct]);

        assert!(
            references
                .iter()
                .any(|reference| reference.role() == RuntimeAbiRole::PanicPropagation)
        );
    }
}
