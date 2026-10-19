use std::collections::BTreeSet;

use crate::{CodegenSymbolKey, CodegenSymbolMapping, CodegenUnit, FOREIGN_CALLBACK_RUNTIME_ROLES};
use bray_ir::{
    MirAsyncOperation, MirFrameInitializer, MirGeneratorOperation, MirHostOperation,
    MirOperationKind, MirRuntimeReference, MirSourceAnchor, MirTerminatorKind,
};
use bray_runtime_interface::RuntimeAbiRole;

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
    let boundary_panic = boundary_panic_propagation_is_demanded(mir);

    let asynchronous_root = mir.operations().iter().any(|operation| {
        matches!(
            operation.kind(),
            MirOperationKind::Host(MirHostOperation::ExecuteRoot {
                execution: bray_runtime_interface::RootExecution::Asynchronous { .. },
                ..
            })
        )
    });

    let owns_collection = mir
        .operations()
        .iter()
        .any(|operation| operation.kind().owns_cleanup_collection());

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
        .chain(
            mir.operations()
                .iter()
                .filter_map(|operation| match operation.kind() {
                    MirOperationKind::Async(MirAsyncOperation::CreateFrame {
                        initializer, ..
                    }) => frame_creation_runtime_role(initializer),
                    _ => None,
                })
                .map(|role| Some(MirRuntimeReference::new(role, runtime_abi))),
        )
        .chain(boundary_panic.then_some(Some(MirRuntimeReference::new(
            bray_runtime_interface::RuntimeAbiRole::PanicPropagation,
            runtime_abi,
        ))))
        .chain(
            [
                RuntimeAbiRole::OutgoingActivation,
                RuntimeAbiRole::ActivationPublication,
                RuntimeAbiRole::CleanupStorageRelease,
            ]
            .into_iter()
            .filter(|_| mir.frame_descriptor().is_some())
            .map(|role| Some(MirRuntimeReference::new(role, runtime_abi))),
        )
        .chain(
            [
                RuntimeAbiRole::OutgoingAdmission,
                RuntimeAbiRole::OutgoingDischarge,
            ]
            .into_iter()
            .filter(|_| asynchronous_root)
            .map(|role| Some(MirRuntimeReference::new(role, runtime_abi))),
        )
        .chain(
            [
                RuntimeAbiRole::CleanupShieldEnter,
                RuntimeAbiRole::CleanupShieldLeave,
            ]
            .into_iter()
            .filter(|_| owns_collection)
            .map(|role| Some(MirRuntimeReference::new(role, runtime_abi))),
        )
        .flatten()
        .collect()
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

/// Returns runtime roles required by an operation's frame initializer mapping.
pub fn frame_creation_runtime_role(initializer: &MirFrameInitializer) -> Option<RuntimeAbiRole> {
    match initializer {
        MirFrameInitializer::Callable(_) => None,
        MirFrameInitializer::TaskObservation { .. } => {
            Some(RuntimeAbiRole::TaskObservationCreation)
        }
    }
}

/// Returns runtime roles required by a mapped callable symbol.
pub fn mapped_symbol_runtime_roles(
    native_entry: Option<bray_runtime_interface::NativeEntryKind>,
    panic_report_context: bool,
) -> impl Iterator<Item = RuntimeAbiRole> {
    FOREIGN_CALLBACK_RUNTIME_ROLES
        .into_iter()
        .filter(move |role| match native_entry {
            Some(bray_runtime_interface::NativeEntryKind::ForeignCallback) => true,
            Some(
                bray_runtime_interface::NativeEntryKind::ThreadStorageExit
                | bray_runtime_interface::NativeEntryKind::RuntimeStatus,
            ) => *role == RuntimeAbiRole::PanicReporting,
            Some(bray_runtime_interface::NativeEntryKind::EntryStatus) => {
                *role == RuntimeAbiRole::PanicReporting
            }
            None => false,
        })
        .chain(
            std::iter::once(RuntimeAbiRole::PanicReportSuppression).filter(move |_| {
                native_entry == Some(bray_runtime_interface::NativeEntryKind::EntryStatus)
            }),
        )
        .chain(
            std::iter::once(RuntimeAbiRole::PanicPropagation).filter(move |_| panic_report_context),
        )
}

/// Returns runtime symbols demanded by the unit and its mapped callable symbols.
pub fn mapped_runtime_references(
    unit: &CodegenUnit,
    symbols: &[CodegenSymbolMapping],
) -> BTreeSet<MirRuntimeReference> {
    let mut references = demanded_runtime_references(unit);

    let native_entries = symbols.iter().filter_map(|symbol| {
        matches!(
            symbol.key(),
            CodegenSymbolKey::Instance(instance)
                if unit.instances().iter().any(|member| member.key() == instance)
        )
        .then(|| {
            symbol
                .native_entry()
                .map(crate::CodegenNativeEntryMapping::kind)
        })
        .flatten()
    });

    let panic_report_context = symbols
        .iter()
        .any(|symbol| symbol.signature().has_panic_report_context());

    references.extend(
        native_entries
            .map(Some)
            .chain([None])
            .flat_map(|entry| mapped_symbol_runtime_roles(entry, panic_report_context))
            .map(|role| MirRuntimeReference::new(role, unit.target().runtime_abi())),
    );

    references
}

fn operation_runtime_references(
    operation: &MirOperationKind,
    abi: bray_runtime_interface::RuntimeAbiVersion,
) -> [Option<MirRuntimeReference>; 3] {
    match operation {
        MirOperationKind::AdmitOutgoing { runtime, .. }
        | MirOperationKind::DischargeOutgoing { runtime, .. } => [Some(*runtime), None, None],
        MirOperationKind::Async(
            MirAsyncOperation::BroadcastInactiveFrame { runtime, .. }
            | MirAsyncOperation::DestroyInactiveFrame { runtime, .. },
        ) => [
            Some(*runtime),
            Some(MirRuntimeReference::new(
                bray_runtime_interface::RuntimeAbiRole::ActivationCallback,
                abi,
            )),
            Some(MirRuntimeReference::new(
                bray_runtime_interface::RuntimeAbiRole::ActivationActionReadiness,
                abi,
            )),
        ],
        MirOperationKind::Memory(operation) => {
            let mut roles = operation.runtime_roles();

            std::array::from_fn(|_| roles.next().map(|role| MirRuntimeReference::new(role, abi)))
        }
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
        MirOperationKind::Async(MirAsyncOperation::TransferCleanupIncident { runtime, .. }) => [
            Some(*runtime),
            Some(MirRuntimeReference::new(
                RuntimeAbiRole::CleanupStorageClaim,
                abi,
            )),
            Some(MirRuntimeReference::new(
                RuntimeAbiRole::CleanupStorageRelease,
                abi,
            )),
        ],
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
            | MirAsyncOperation::ResolveTask { runtime, .. },
        )
        | MirOperationKind::Host(
            MirHostOperation::InitializeRuntime { runtime }
            | MirHostOperation::SelectTestEntry { runtime, .. }
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
        | MirOperationKind::Text(_)
        | MirOperationKind::PanicReport(_)
        | MirOperationKind::Quiesce { .. }
        | MirOperationKind::Destroy { .. }
        | MirOperationKind::Cleanup { .. }
        | MirOperationKind::Host(
            MirHostOperation::MaterializeStatic { .. }
            | MirHostOperation::BeginProductEntry
            | MirHostOperation::BeginStaticCleanup,
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
            | MirAsyncOperation::ObserveFrameCancellation
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
        | MirTerminatorKind::PropagateCancellation { runtime, .. } => [Some(*runtime), None, None],
        MirTerminatorKind::Goto(_)
        | MirTerminatorKind::Branch { .. }
        | MirTerminatorKind::PatternBranch { .. }
        | MirTerminatorKind::Iterate { .. }
        | MirTerminatorKind::RangeIterate { .. }
        | MirTerminatorKind::Switch { .. }
        | MirTerminatorKind::InlineAssembly(_)
        | MirTerminatorKind::Return { .. }
        | MirTerminatorKind::Unreachable
        | MirTerminatorKind::ForwardRunResult { .. }
        | MirTerminatorKind::CheckCallOutcome { .. }
        | MirTerminatorKind::TakeCallReport { .. }
        | MirTerminatorKind::BeginCleanup(_)
        | MirTerminatorKind::ContinueCleanup(_)
        | MirTerminatorKind::Panic { .. }
        | MirTerminatorKind::CancelCurrentRun { .. } => [None, None, None],
    }
}

#[cfg(test)]
mod tests {
    use bray_runtime_interface::{BinarySymbolName, RuntimeAbiRole};
    use std::collections::BTreeSet;

    use crate::test_support::codegen_request;
    use crate::{
        CodegenLinkage, CodegenNativeEntryMapping, CodegenSymbolMapping, mapped_runtime_references,
    };

    #[test]
    fn indirect_frame_creation_uses_its_callable_value() {
        let call = bray_ir::MirCall::imported(
            bray_ir::MirCallTarget::Indirect {
                callee: bray_ir::MirOperand::Value(bray_ir::MirValueId::from_slot(
                    bray_ir::MirUnitId::new(0),
                    0,
                )),
                abi: bray_symbols::CallableAbi::Bray,
            },
            bray_bound_tree::BoundCallResult::Immediate(bray_testing::test_mir_type()),
            [],
            None,
            [],
            None,
            None,
            [],
        );

        assert_eq!(
            super::frame_creation_runtime_role(&bray_ir::MirFrameInitializer::Callable(call)),
            None,
        );
    }

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

        let references = mapped_runtime_references(request.unit(), &[direct.clone()]);

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

        let references = mapped_runtime_references(request.unit(), &[callback]);

        for role in crate::FOREIGN_CALLBACK_RUNTIME_ROLES {
            assert!(references.iter().any(|reference| reference.role() == role));
        }
    }

    #[test]
    fn terminal_thread_callbacks_report_panics_without_recursive_attachment() {
        for kind in [
            bray_runtime_interface::NativeEntryKind::ThreadStorageExit,
            bray_runtime_interface::NativeEntryKind::RuntimeStatus,
        ] {
            let roles: BTreeSet<_> =
                super::mapped_symbol_runtime_roles(Some(kind), false).collect();

            assert_eq!(roles, BTreeSet::from([RuntimeAbiRole::PanicReporting]));
        }
    }

    #[test]
    fn entry_cleanup_preserves_body_reports_without_recursive_entry() {
        let roles: BTreeSet<_> = super::mapped_symbol_runtime_roles(
            Some(bray_runtime_interface::NativeEntryKind::EntryStatus),
            false,
        )
        .collect();

        assert_eq!(
            roles,
            BTreeSet::from([
                RuntimeAbiRole::PanicReporting,
                RuntimeAbiRole::PanicReportSuppression,
            ])
        );
    }

    #[test]
    fn hidden_outcome_context_demands_the_terminal_report_consumer() {
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

        let references = mapped_runtime_references(request.unit(), &[direct]);

        assert!(
            references
                .iter()
                .any(|reference| reference.role() == RuntimeAbiRole::PanicPropagation)
        );
    }
}
