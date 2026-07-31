use std::collections::BTreeSet;

use bray_ir::{
    MirAsyncOperation, MirGeneratorOperation, MirHostOperation, MirOperationKind,
    MirRuntimeReference, MirSourceAnchor, MirTerminatorKind,
};
use bray_symbols::TypeId;

use crate::CodegenUnit;
use crate::{CodegenOperationMapping, CodegenSymbolKey};

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
        .flat_map(|mir| {
            mir.operations()
                .iter()
                .flat_map(|operation| operation_runtime_references(operation.kind()))
                .chain(
                    mir.blocks()
                        .iter()
                        .flat_map(|block| terminator_runtime_references(block.terminator().kind())),
                )
        })
        .flatten()
        .collect()
}

/// Returns direct MIR roles plus runtime helpers selected by operation mappings.
pub fn mapped_runtime_references(
    unit: &CodegenUnit,
    operations: &[CodegenOperationMapping],
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

    references
}

fn operation_runtime_references(operation: &MirOperationKind) -> [Option<MirRuntimeReference>; 3] {
    match operation {
        MirOperationKind::Async(MirAsyncOperation::StartTask {
            allocation, start, ..
        }) => [Some(*allocation), Some(*start), None],
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
            MirHostOperation::ExecuteRoot { runtime, .. }
            | MirHostOperation::ObserveRootTerminal { runtime }
            | MirHostOperation::ReportCleanupIncidents { runtime }
            | MirHostOperation::StructuredShutdown { runtime },
        )
        | MirOperationKind::Generator(
            MirGeneratorOperation::CleanupBroadcast { runtime, .. }
            | MirGeneratorOperation::Destroy { runtime, .. },
        ) => [Some(*runtime), None, None],
        MirOperationKind::AnonymousCallable(_)
        | MirOperationKind::Store { .. }
        | MirOperationKind::Borrow { .. }
        | MirOperationKind::Unary { .. }
        | MirOperationKind::Binary { .. }
        | MirOperationKind::Aggregate(_)
        | MirOperationKind::Construct(_)
        | MirOperationKind::Convert { .. }
        | MirOperationKind::PatternProjection { .. }
        | MirOperationKind::Generator(
            MirGeneratorOperation::Begin { .. }
            | MirGeneratorOperation::Push { .. }
            | MirGeneratorOperation::Finish { .. },
        )
        | MirOperationKind::Call(_)
        | MirOperationKind::PanicReport(_)
        | MirOperationKind::Finalize(_)
        | MirOperationKind::Destroy(_)
        | MirOperationKind::Cleanup { .. }
        | MirOperationKind::Async(
            MirAsyncOperation::CreateFrame { .. }
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
        MirTerminatorKind::PropagatePanic { runtime, .. } => [Some(*runtime), None, None],
        MirTerminatorKind::Goto(_)
        | MirTerminatorKind::Branch { .. }
        | MirTerminatorKind::PatternBranch { .. }
        | MirTerminatorKind::Iterate { .. }
        | MirTerminatorKind::Switch { .. }
        | MirTerminatorKind::Return(_)
        | MirTerminatorKind::Unreachable
        | MirTerminatorKind::ForwardRunResult { .. }
        | MirTerminatorKind::BeginCleanup(_)
        | MirTerminatorKind::ContinueCleanup(_)
        | MirTerminatorKind::Panic { .. }
        | MirTerminatorKind::CancelCurrentRun { .. } => [None, None, None],
    }
}
