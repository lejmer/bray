use std::sync::Arc;

use crate::{
    MirAsyncOperation, MirBlockId, MirCleanupEdge, MirEdge, MirFrameInitializer,
    MirGeneratorOperation, MirHostOperation, MirOperationKind, MirPanicCause, MirStorageId,
    MirTaskTerminalState, MirTerminatorKind, MirValueId,
};

pub(crate) trait MirLocalIdMapping {
    fn block(&self, old: MirBlockId) -> MirBlockId;
    fn storage(&self, old: MirStorageId) -> MirStorageId;
    fn value(&self, old: MirValueId) -> MirValueId;
}

pub(super) fn remap_operation(
    operation: &mut MirOperationKind,
    mappings: &impl MirLocalIdMapping,
) {
    match operation {
        MirOperationKind::Store {
            destination, value, ..
        } => {
            destination.remap_local_ids(mappings);
            value.remap_local_ids(mappings);
        }
        MirOperationKind::Borrow { place, .. }
        | MirOperationKind::Finalize(place)
        | MirOperationKind::Destroy(place)
        | MirOperationKind::Cleanup { place, .. } => place.remap_local_ids(mappings),
        MirOperationKind::Unary { operand, .. }
        | MirOperationKind::Convert { operand, .. }
        | MirOperationKind::NumericConversion { operand, .. } => {
            operand.remap_local_ids(mappings);
        }
        MirOperationKind::Binary { left, right, .. } => {
            left.remap_local_ids(mappings);
            right.remap_local_ids(mappings);
        }
        MirOperationKind::Aggregate(aggregate) => {
            for operand in Arc::make_mut(&mut aggregate.operands) {
                operand.remap_local_ids(mappings);
            }
        }
        MirOperationKind::Construct(construction) => {
            for input in Arc::make_mut(&mut construction.inputs) {
                input.value.remap_local_ids(mappings);
            }
        }
        MirOperationKind::NullableQuery(query) => query.remap_local_ids(mappings),
        MirOperationKind::PatternProjection { subject, .. } => {
            subject.remap_local_ids(mappings);
        }
        MirOperationKind::Generator(operation) => remap_generator_operation(operation, mappings),
        MirOperationKind::Call(call) => call.remap_local_ids(mappings),
        MirOperationKind::Memory(operation) => {
            for operand in Arc::make_mut(&mut operation.operands) {
                operand.remap_local_ids(mappings);
            }
        }
        MirOperationKind::Text(operation) => {
            for operand in Arc::make_mut(&mut operation.operands) {
                operand.remap_local_ids(mappings);
            }
        }
        MirOperationKind::PanicReport(cause) => remap_panic_cause(cause, mappings),
        MirOperationKind::Async(operation) => remap_async_operation(operation, mappings),
        MirOperationKind::Host(MirHostOperation::MaterializeStatic { place }) => {
            place.remap_local_ids(mappings);
        }
        MirOperationKind::Host(
            MirHostOperation::SelectTestEntry { .. }
            | MirHostOperation::ExecuteRoot { .. }
            | MirHostOperation::ObserveRootTerminal { .. }
            | MirHostOperation::ResolveRootTerminal { .. }
            | MirHostOperation::BeginStaticCleanup
            | MirHostOperation::ReportCleanupIncidents { .. }
            | MirHostOperation::StructuredShutdown { .. },
        )
        | MirOperationKind::AnonymousCallable(_)
        | MirOperationKind::DeclaredCallable(_)
        | MirOperationKind::AdmitOutgoing { .. }
        | MirOperationKind::DischargeOutgoing { .. } => {}
    }
}

pub(super) fn remap_terminator(
    terminator: &mut MirTerminatorKind,
    mappings: &impl MirLocalIdMapping,
) {
    match terminator {
        MirTerminatorKind::Goto(edge) => remap_edge(edge, mappings),
        MirTerminatorKind::Branch {
            condition,
            then_edge,
            else_edge,
        } => {
            condition.remap_local_ids(mappings);
            remap_edge(then_edge, mappings);
            remap_edge(else_edge, mappings);
        }
        MirTerminatorKind::PatternBranch {
            subject,
            matched,
            unmatched,
            ..
        } => {
            subject.remap_local_ids(mappings);
            remap_edge(matched, mappings);
            remap_edge(unmatched, mappings);
        }
        MirTerminatorKind::Iterate {
            cursor,
            item,
            exhausted,
            ..
        }
        | MirTerminatorKind::RangeIterate {
            cursor,
            item,
            exhausted,
            ..
        } => {
            cursor.remap_local_ids(mappings);
            *item = mappings.block(*item);
            remap_edge(exhausted, mappings);
        }
        MirTerminatorKind::Switch {
            discriminant,
            cases,
            otherwise,
        } => {
            discriminant.remap_local_ids(mappings);

            for case in Arc::make_mut(cases) {
                remap_edge(&mut case.edge, mappings);
            }

            remap_edge(otherwise, mappings);
        }
        MirTerminatorKind::InlineAssembly(assembly) => {
            assembly.inputs.remap_local_ids(mappings);
            assembly.normal = mappings.block(assembly.normal);

            for alternate in Arc::make_mut(&mut assembly.alternates) {
                *alternate = mappings.block(*alternate);
            }
        }
        MirTerminatorKind::Return(value) => {
            if let Some(value) = value {
                value.remap_local_ids(mappings);
            }
        }
        MirTerminatorKind::Suspend {
            payload,
            resume,
            cancellation,
            ..
        } => {
            if let Some(payload) = payload {
                payload.remap_local_ids(mappings);
            }

            remap_edge(resume, mappings);
            remap_cleanup_edge(cancellation, mappings);
        }
        MirTerminatorKind::ForwardRunResult { result, edges } => {
            result.remap_local_ids(mappings);
            remap_edge(&mut edges.completed, mappings);
            remap_cleanup_edge(&mut edges.panicked, mappings);
            remap_cleanup_edge(&mut edges.cancelled, mappings);
        }
        MirTerminatorKind::CheckCallOutcome {
            completed,
            panicked,
            cancelled,
        } => {
            remap_edge(completed, mappings);
            panicked.target = mappings.block(panicked.target);
            panicked.report.remap_local_ids(mappings);
            remap_edge(cancelled, mappings);
        }
        MirTerminatorKind::BeginCleanup(cleanup)
        | MirTerminatorKind::ContinueCleanup(cleanup)
        | MirTerminatorKind::CancelCurrentRun { cleanup } => {
            remap_cleanup_edge(cleanup, mappings);
        }
        MirTerminatorKind::Panic { report, cleanup } => {
            report.remap_local_ids(mappings);
            remap_cleanup_edge(cleanup, mappings);
        }
        MirTerminatorKind::PropagatePanic { report, .. } => {
            report.remap_local_ids(mappings);
        }
        MirTerminatorKind::Unreachable | MirTerminatorKind::PropagateCancellation { .. } => {}
    }
}

fn remap_generator_operation(
    operation: &mut MirGeneratorOperation,
    mappings: &impl MirLocalIdMapping,
) {
    match operation {
        MirGeneratorOperation::Push { destination, value } => {
            destination.remap_local_ids(mappings);
            value.remap_local_ids(mappings);
        }
        MirGeneratorOperation::Begin { destination, .. }
        | MirGeneratorOperation::Finish { destination }
        | MirGeneratorOperation::CleanupBroadcast { destination, .. }
        | MirGeneratorOperation::Destroy { destination, .. } => {
            destination.remap_local_ids(mappings);
        }
    }
}

fn remap_panic_cause(cause: &mut MirPanicCause, mappings: &impl MirLocalIdMapping) {
    match cause {
        MirPanicCause::Message(operand) | MirPanicCause::ExplicitTestFailure(operand) => {
            operand.remap_local_ids(mappings);
        }
        MirPanicCause::Assertion(message) => {
            if let Some(message) = message {
                message.remap_local_ids(mappings);
            }
        }
    }
}

fn remap_async_operation(operation: &mut MirAsyncOperation, mappings: &impl MirLocalIdMapping) {
    match operation {
        MirAsyncOperation::CreateFrame { initializer, .. } => match initializer {
            MirFrameInitializer::Callable(call) => call.remap_local_ids(mappings),
            MirFrameInitializer::TaskObservation { task, .. } => task.remap_local_ids(mappings),
        },
        MirAsyncOperation::MoveInactiveFrame {
            source,
            destination,
            ..
        } => {
            source.remap_local_ids(mappings);
            destination.remap_local_ids(mappings);
        }
        MirAsyncOperation::ResumeFrame { storage, .. } => *storage = mappings.storage(*storage),
        MirAsyncOperation::ComposeAwaitedFrame { frame, .. } => {
            frame.remap_local_ids(mappings);
        }
        MirAsyncOperation::StartTask { value, .. } => value.remap_local_ids(mappings),
        MirAsyncOperation::RequestTaskCancellation { task, .. }
        | MirAsyncOperation::ResolveTask { task, .. }
        | MirAsyncOperation::DestroyTerminalTask { task } => task.remap_local_ids(mappings),
        MirAsyncOperation::PublishTerminalState { state, .. } => match state {
            MirTaskTerminalState::Completed(value) | MirTaskTerminalState::Panicked(value) => {
                value.remap_local_ids(mappings);
            }
            MirTaskTerminalState::Cancelled => {}
        },
        MirAsyncOperation::TransferCleanupIncident { incident, .. } => {
            incident.remap_local_ids(mappings);
        }
        MirAsyncOperation::CommitAwaitedCompletion { .. }
        | MirAsyncOperation::ObserveCurrentRunCancellation { .. }
        | MirAsyncOperation::ExecuteCleanupBroadcast { .. }
        | MirAsyncOperation::ExecuteLifecycleResolution { .. } => {}
    }
}

fn remap_edge(edge: &mut MirEdge, mappings: &impl MirLocalIdMapping) {
    edge.target = mappings.block(edge.target);

    for argument in Arc::make_mut(&mut edge.arguments) {
        argument.remap_local_ids(mappings);
    }
}

fn remap_cleanup_edge(cleanup: &mut MirCleanupEdge, mappings: &impl MirLocalIdMapping) {
    remap_edge(&mut cleanup.edge, mappings);
}
