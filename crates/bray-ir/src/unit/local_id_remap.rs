use std::sync::Arc;

use bray_symbols::TypeId;

use crate::{
    MirAsyncOperation, MirBlockId, MirCleanupEdge, MirEdge, MirFrameInitializer,
    MirGeneratorOperation, MirHostOperation, MirOperand, MirOperationKind, MirPanicCause,
    MirStorageId, MirTerminatorKind, MirValueId,
};

pub(crate) trait MirLocalIdMapping {
    fn block(&self, old: MirBlockId) -> MirBlockId;
    fn storage(&self, old: MirStorageId) -> MirStorageId;
    fn value(&self, old: MirValueId) -> MirValueId;

    fn storage_place(&self, _old: MirStorageId) -> Option<&crate::MirPlace> {
        None
    }

    fn ty(&self, old: TypeId) -> TypeId {
        old
    }

    fn callable(&self, old: crate::MirCallableReference) -> crate::MirCallableReference {
        old
    }

    fn remap_operand(&self, operand: &mut MirOperand)
    where
        Self: Sized,
    {
        remap_operand_ids(operand, self);
    }
}

pub(crate) fn remap_operand_ids(operand: &mut MirOperand, mappings: &impl MirLocalIdMapping) {
    match operand {
        MirOperand::Value(value) => *value = mappings.value(*value),
        MirOperand::Copy(place) | MirOperand::Move(place) => place.remap_local_ids(mappings),
        MirOperand::Constant { .. }
        | MirOperand::ConstantTerm { .. }
        | MirOperand::Immediate { .. } => {}
    }
}

pub(super) fn remap_operation(operation: &mut MirOperationKind, mappings: &impl MirLocalIdMapping) {
    if let Some(Some(collection)) = operation.cleanup_collection_slot() {
        collection.state.remap_local_ids(mappings);
        collection.report.remap_local_ids(mappings);

        if let Some(retention) = &mut collection.retention {
            Arc::make_mut(retention).remap_local_ids(mappings);
        }
    }

    match operation {
        MirOperationKind::Store {
            destination, value, ..
        } => {
            destination.remap_local_ids(mappings);
            value.remap_local_ids(mappings);
        }
        MirOperationKind::Borrow { place, .. }
        | MirOperationKind::Quiesce { place, .. }
        | MirOperationKind::ReleaseBuffer { place, .. }
        | MirOperationKind::Cleanup { place, .. } => place.remap_local_ids(mappings),
        MirOperationKind::Destroy {
            place, abandoned, ..
        } => {
            place.remap_local_ids(mappings);

            if let Some(abandoned) = abandoned {
                abandoned.remap_local_ids(mappings);
            }
        }
        MirOperationKind::TraitView { operand, .. }
        | MirOperationKind::TraitViewMethod { operand, .. }
        | MirOperationKind::Unary { operand, .. }
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
            MirHostOperation::InitializeRuntime { .. }
            | MirHostOperation::SelectTestEntry { .. }
            | MirHostOperation::ExecuteRoot { .. }
            | MirHostOperation::ObserveRootTerminal { .. }
            | MirHostOperation::ResolveRootTerminal { .. }
            | MirHostOperation::BeginProductEntry
            | MirHostOperation::BeginStaticCleanup
            | MirHostOperation::ReportCleanupIncidents { .. }
            | MirHostOperation::StructuredShutdown { .. },
        )
        | MirOperationKind::AnonymousCallable(_)
        | MirOperationKind::DeclaredCallable(_) => {}
        MirOperationKind::AdmitOutgoing { owner, .. }
        | MirOperationKind::DischargeOutgoing { owner, .. } => {
            if let crate::MirOutgoingOwner::FrameFactory(callee) = owner {
                callee.remap_local_ids(mappings);
            }
        }
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
        MirTerminatorKind::RangeIterate {
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
        MirTerminatorKind::Return { value, report } => {
            for operand in value.iter_mut().chain(report.iter_mut()) {
                operand.remap_local_ids(mappings);
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
        MirTerminatorKind::TakeCallReport { completed, report } => {
            remap_edge(completed, mappings);
            report.remap_local_ids(mappings);
        }
        MirTerminatorKind::CheckCallOutcome {
            completed,
            panicked,
            cancelled,
            completed_report,
        } => {
            remap_edge(completed, mappings);

            if let Some(report) = completed_report {
                report.remap_local_ids(mappings);
            }

            panicked.target = mappings.block(panicked.target);
            panicked.report.remap_local_ids(mappings);
            cancelled.target = mappings.block(cancelled.target);
            cancelled.report.remap_local_ids(mappings);
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
        MirTerminatorKind::PropagatePanic { report, .. }
        | MirTerminatorKind::PropagateCancellation { report, .. } => {
            report.remap_local_ids(mappings);
        }
        MirTerminatorKind::Unreachable => {}
    }
}

fn remap_generator_operation(
    operation: &mut MirGeneratorOperation,
    mappings: &impl MirLocalIdMapping,
) {
    if let MirGeneratorOperation::CleanupBroadcast { collection, .. }
    | MirGeneratorOperation::Destroy { collection, .. } = operation
    {
        collection.state.remap_local_ids(mappings);
        collection.report.remap_local_ids(mappings);
    }

    match operation {
        MirGeneratorOperation::Destroy {
            destination,
            abandoned,
            ..
        } => {
            destination.remap_local_ids(mappings);
            abandoned.remap_local_ids(mappings);
        }
        MirGeneratorOperation::Push { destination, value } => {
            destination.remap_local_ids(mappings);
            value.remap_local_ids(mappings);
        }
        MirGeneratorOperation::Begin { destination, .. }
        | MirGeneratorOperation::Finish { destination }
        | MirGeneratorOperation::CleanupBroadcast { destination, .. } => {
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
        MirAsyncOperation::ComposeAwaitedFrame {
            frame, composition, ..
        } => {
            frame.remap_local_ids(mappings);

            if let crate::MirFrameComposition::Completion(storage) = composition {
                *storage = mappings.storage(*storage);
            }
        }
        MirAsyncOperation::CommitAwaitedCompletion { completion, .. } => {
            *completion = completion.map(|storage| mappings.storage(storage));
        }
        MirAsyncOperation::StartTask { value, .. } => value.remap_local_ids(mappings),
        MirAsyncOperation::DestroyTerminalTask { task } => task.remap_local_ids(mappings),
        MirAsyncOperation::RequestTaskCancellation { task, .. }
        | MirAsyncOperation::ResolveTask { task, .. } => task.remap_local_ids(mappings),
        MirAsyncOperation::PublishTerminalState { state, .. } => {
            for operand in state.operands_mut() {
                operand.remap_local_ids(mappings);
            }
        }
        MirAsyncOperation::TransferCleanupIncident {
            incident, report, ..
        } => {
            incident.remap_local_ids(mappings);
            report.remap_local_ids(mappings);
        }
        MirAsyncOperation::ObserveFrameCancellation
        | MirAsyncOperation::ObserveCurrentRunCancellation { .. } => {}
        MirAsyncOperation::BroadcastInactiveFrame { frame, .. }
        | MirAsyncOperation::ResolveInactiveFrame { frame, .. }
        | MirAsyncOperation::DestroyInactiveFrame { frame, .. } => frame.remap_local_ids(mappings),
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
