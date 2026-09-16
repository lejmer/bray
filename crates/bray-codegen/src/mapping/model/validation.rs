use std::collections::BTreeSet;

use bray_ir::{
    MirAsyncOperation, MirGeneratorOperation, MirHostOperation, MirOperationKind,
    MirRuntimeReference, MirSourceAnchor, MirTerminatorKind,
};
use crate::{
    CodegenOperationMapping, CodegenSymbolKey, CodegenSymbolMapping, CodegenUnit,
    FOREIGN_CALLBACK_RUNTIME_ROLES,
};

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
        .flat_map(|operation| operation_runtime_references(operation.kind(), runtime_abi))
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
        .chain(
            (checked_call_cancellation_propagation_is_demanded(mir)
                || boundary_panic_propagation_is_demanded(mir))
            .then_some(Some(MirRuntimeReference::new(
                bray_runtime_interface::RuntimeAbiRole::CurrentRunCancellationPropagation,
                runtime_abi,
            ))),
        )
        .flatten()
        .collect()
}

fn checked_call_cancellation_propagation_is_demanded(mir: &bray_ir::MirUnit) -> bool {
    mir.blocks().iter().any(|block| {
        matches!(
            block.terminator().kind(),
            MirTerminatorKind::CheckCallOutcome { .. }
        )
    })
}

fn boundary_panic_propagation_is_demanded(mir: &bray_ir::MirUnit) -> bool {
    mir.blocks().iter().any(|block| {
        let checked_call = matches!(
            block.terminator().kind(),
            MirTerminatorKind::CheckCallOutcome { .. }
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
        references.extend(
            [
                bray_runtime_interface::RuntimeAbiRole::PanicPropagation,
                bray_runtime_interface::RuntimeAbiRole::CurrentRunCancellationPropagation,
            ]
            .map(|role| MirRuntimeReference::new(role, unit.target().runtime_abi())),
        );
    }

    references
}

fn operation_runtime_references(
    operation: &MirOperationKind,
    abi: bray_runtime_interface::RuntimeAbiVersion,
) -> [Option<MirRuntimeReference>; 3] {
    match operation {
        MirOperationKind::AdmitOutgoing { runtime, .. }
        | MirOperationKind::DischargeOutgoing { runtime, .. } => [Some(*runtime), None, None],
        MirOperationKind::Call(call) if call.is_cleanup() => [
            Some(MirRuntimeReference::new(
                bray_runtime_interface::RuntimeAbiRole::OutgoingActivation,
                abi,
            )),
            Some(MirRuntimeReference::new(
                bray_runtime_interface::RuntimeAbiRole::OutgoingRetirement,
                abi,
            )),
            None,
        ],
        MirOperationKind::Call(call) => match call.target() {
            bray_ir::MirCallTarget::Runtime(runtime) => [Some(*runtime), None, None],
            bray_ir::MirCallTarget::Direct(_)
            | bray_ir::MirCallTarget::DefaultValue { .. }
            | bray_ir::MirCallTarget::Indirect { .. } => [None, None, None],
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
        | MirOperationKind::NullableQuery(_)
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
            | MirAsyncOperation::PublishTerminalState { .. }
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
        | MirTerminatorKind::CheckCallOutcome { .. }
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
    fn hidden_outcome_context_demands_panic_and_cancellation_runtime() {
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

        assert!(
            references
                .iter()
                .any(|reference| reference.role()
                    == RuntimeAbiRole::CurrentRunCancellationPropagation)
        );
    }
}
