use std::collections::BTreeSet;

use bray_bound_tree::{CheckedMemoryOperationKind, ConversionTarget, SelectedConversion};
use bray_symbols::TypeId;

use crate::{
    MirAsyncOperation, MirCall, MirCallTarget, MirCleanupEdge, MirEdge, MirFrameInitializer,
    MirGeneratorOperation, MirHostOperation, MirOperand, MirOperationKind, MirPanicCause, MirPlace,
    MirProjectionKind, MirTaskTerminalState, MirTerminatorKind, MirUnit,
};

impl MirUnit {
    /// Returns every semantic type retained directly by this MIR unit.
    pub fn referenced_types(&self) -> BTreeSet<TypeId> {
        let mut types = BTreeSet::new();

        types.extend(self.storages().iter().map(crate::MirStorage::ty));
        types.extend(self.values().iter().map(crate::MirValue::ty));

        if let Some(descriptor) = self.frame_descriptor() {
            types.insert(descriptor.result_type());
        }

        if let crate::MirUnitKind::ExecutableHost(host) = self.kind() {
            for entry in host.entries() {
                if let bray_runtime_interface::ExecutableEntryResult::Fallible {
                    ty, error, ..
                } = entry.result()
                {
                    types.extend([ty, error]);
                }
            }
        }

        for operation in self.operations() {
            collect_operation_types(operation.kind(), &mut types);
        }

        for block in self.blocks() {
            collect_terminator_types(block.terminator().kind(), &mut types);
        }

        types
    }
}

fn collect_operation_types(operation: &MirOperationKind, types: &mut BTreeSet<TypeId>) {
    match operation {
        MirOperationKind::AnonymousCallable(_) | MirOperationKind::DeclaredCallable(_) => {}
        MirOperationKind::Store {
            destination, value, ..
        } => {
            collect_place_types(destination, types);
            collect_operand_types(value, types);
        }
        MirOperationKind::Borrow { place, .. }
        | MirOperationKind::Finalize(place)
        | MirOperationKind::Destroy(place)
        | MirOperationKind::Cleanup { place, .. } => collect_place_types(place, types),
        MirOperationKind::Unary { operand, .. } => collect_operand_types(operand, types),
        MirOperationKind::Binary { left, right, .. } => {
            collect_operand_types(left, types);
            collect_operand_types(right, types);
        }
        MirOperationKind::Aggregate(aggregate) => {
            collect_operands_types(aggregate.operands(), types);
        }
        MirOperationKind::Construct(construction) => {
            for input in construction.inputs() {
                if let crate::MirConstructionInput::Explicit { value, .. } = input {
                    collect_operand_types(value, types);
                }
            }
        }
        MirOperationKind::Convert {
            operand,
            conversion,
        } => {
            collect_operand_types(operand, types);
            collect_conversion_types(conversion, types);
        }
        MirOperationKind::NumericConversion { operand, .. } => {
            collect_operand_types(operand, types);
        }
        MirOperationKind::PatternProjection { subject, .. } => {
            collect_operand_types(subject, types);
        }
        MirOperationKind::Generator(operation) => collect_generator_types(operation, types),
        MirOperationKind::Call(call) => collect_call_types(call, types),
        MirOperationKind::Memory(memory) => {
            collect_operands_types(memory.operands(), types);
            collect_memory_types(memory.kind(), types);
        }
        MirOperationKind::Text(text) => {
            collect_operands_types(text.operands(), types);
            types.extend(text.operand_types());
            types.extend(text.result_type());
        }
        MirOperationKind::PanicReport(cause) => collect_panic_types(cause, types),
        MirOperationKind::Async(operation) => collect_async_types(operation, types),
        MirOperationKind::Host(operation) => collect_host_types(operation, types),
    }
}

fn collect_memory_types(kind: CheckedMemoryOperationKind, types: &mut BTreeSet<TypeId>) {
    match kind {
        CheckedMemoryOperationKind::Address { pointee, .. }
        | CheckedMemoryOperationKind::Null { pointee }
        | CheckedMemoryOperationKind::IsNull { pointee }
        | CheckedMemoryOperationKind::Offset { pointee, .. }
        | CheckedMemoryOperationKind::Read { pointee, .. }
        | CheckedMemoryOperationKind::Write { pointee }
        | CheckedMemoryOperationKind::Copy { pointee, .. }
        | CheckedMemoryOperationKind::VolatileRead { pointee, .. }
        | CheckedMemoryOperationKind::VolatileWrite { pointee, .. }
        | CheckedMemoryOperationKind::ExposeAddress { pointee }
        | CheckedMemoryOperationKind::FromExposedAddress { pointee }
        | CheckedMemoryOperationKind::CompareAddress { pointee, .. } => {
            types.insert(pointee);
        }
        CheckedMemoryOperationKind::Reinterpret { source, target } => {
            types.extend([source, target]);
        }
        CheckedMemoryOperationKind::LayoutQuery { ty, .. } => {
            types.insert(ty);
        }
        CheckedMemoryOperationKind::CallbackState { state } => {
            types.insert(state);
        }
        CheckedMemoryOperationKind::InlineAssembly {
            inputs,
            output,
            labels,
            contract,
        } => {
            types.insert(inputs);

            if let Some(output) = output {
                types.insert(output);
            }

            if let Some(labels) = labels {
                types.insert(labels);
            }

            types.extend(contract.operands().map(bray_bound_tree::InlineAssemblyOperand::ty));
        }
        CheckedMemoryOperationKind::AtomicInitialize { value }
        | CheckedMemoryOperationKind::AtomicLoad { value, .. }
        | CheckedMemoryOperationKind::AtomicStore { value, .. }
        | CheckedMemoryOperationKind::AtomicExchange { value, .. }
        | CheckedMemoryOperationKind::AtomicCompareExchange { value, .. }
        | CheckedMemoryOperationKind::AtomicFetch { value, .. }
        | CheckedMemoryOperationKind::AtomicWait { value, .. }
        | CheckedMemoryOperationKind::AtomicNotify { value, .. } => {
            types.insert(value);
        }
        CheckedMemoryOperationKind::RawBufferSparePointer { element }
        | CheckedMemoryOperationKind::RawBufferRelease { element }
        | CheckedMemoryOperationKind::RawBufferReplace { element }
        | CheckedMemoryOperationKind::RawBufferRelocate { element } => {
            types.insert(element);
        }
        CheckedMemoryOperationKind::RawAllocate
        | CheckedMemoryOperationKind::RawDeallocate
        | CheckedMemoryOperationKind::Allocate
        | CheckedMemoryOperationKind::Deallocate
        | CheckedMemoryOperationKind::RawBufferCapacity
        | CheckedMemoryOperationKind::RawBufferInitializedCount
        | CheckedMemoryOperationKind::RawBufferPointer
        | CheckedMemoryOperationKind::RawBufferInitializedSlice
        | CheckedMemoryOperationKind::RawBufferInitializedSliceMut
        | CheckedMemoryOperationKind::RawBufferSetInitializedCount
        | CheckedMemoryOperationKind::ByteBufferFill
        | CheckedMemoryOperationKind::ByteSliceCopy
        | CheckedMemoryOperationKind::ByteBufferRead
        | CheckedMemoryOperationKind::SliceLength
        | CheckedMemoryOperationKind::Fence { .. }
        | CheckedMemoryOperationKind::CatastrophicAbort
        | CheckedMemoryOperationKind::DebuggerTrap
        | CheckedMemoryOperationKind::UnreachableTermination
        | CheckedMemoryOperationKind::SpinLoopHint
        | CheckedMemoryOperationKind::TargetFeatureEnabled { .. } => {}
    }
}

fn collect_terminator_types(terminator: &MirTerminatorKind, types: &mut BTreeSet<TypeId>) {
    match terminator {
        MirTerminatorKind::Goto(edge) => collect_edge_types(edge, types),
        MirTerminatorKind::Branch {
            condition,
            then_edge,
            else_edge,
        } => {
            collect_operand_types(condition, types);
            collect_edge_types(then_edge, types);
            collect_edge_types(else_edge, types);
        }
        MirTerminatorKind::PatternBranch {
            subject,
            matched,
            unmatched,
            ..
        } => {
            collect_operand_types(subject, types);
            collect_edge_types(matched, types);
            collect_edge_types(unmatched, types);
        }
        MirTerminatorKind::Iterate {
            cursor,
            element_type,
            exhausted,
            ..
        } => {
            collect_place_types(cursor, types);
            types.insert(*element_type);
            collect_edge_types(exhausted, types);
        }
        MirTerminatorKind::Switch {
            discriminant,
            cases,
            otherwise,
        } => {
            collect_operand_types(discriminant, types);

            for case in cases.iter() {
                collect_edge_types(case.edge(), types);
            }

            collect_edge_types(otherwise, types);
        }
        MirTerminatorKind::InlineAssembly(assembly) => {
            collect_operand_types(assembly.inputs(), types);
            types.insert(assembly.inputs_type());
            types.insert(assembly.output_type());

            for operand in assembly.contract().operands() {
                types.insert(operand.ty());
            }
        }
        MirTerminatorKind::Return(value) => {
            if let Some(value) = value {
                collect_operand_types(value, types);
            }
        }
        MirTerminatorKind::Unreachable => {}
        MirTerminatorKind::Suspend {
            resume,
            cancellation,
            ..
        } => {
            collect_edge_types(resume, types);
            collect_cleanup_edge_types(cancellation, types);
        }
        MirTerminatorKind::ForwardRunResult { result, edges } => {
            collect_operand_types(result, types);
            collect_edge_types(edges.completed(), types);
            collect_cleanup_edge_types(edges.panicked(), types);
            collect_cleanup_edge_types(edges.cancelled(), types);
        }
        MirTerminatorKind::BeginCleanup(edge) | MirTerminatorKind::ContinueCleanup(edge) => {
            collect_cleanup_edge_types(edge, types)
        }
        MirTerminatorKind::Panic { report, cleanup } => {
            collect_operand_types(report, types);
            collect_cleanup_edge_types(cleanup, types);
        }
        MirTerminatorKind::PropagatePanic { report, .. } => {
            collect_operand_types(report, types);
        }
        MirTerminatorKind::PropagateCancellation { .. } => {}
        MirTerminatorKind::CancelCurrentRun { cleanup } => {
            collect_cleanup_edge_types(cleanup, types);
        }
    }
}

fn collect_generator_types(operation: &MirGeneratorOperation, types: &mut BTreeSet<TypeId>) {
    match operation {
        MirGeneratorOperation::Begin {
            destination,
            element,
            ..
        }
        | MirGeneratorOperation::CleanupBroadcast {
            destination,
            element,
            ..
        }
        | MirGeneratorOperation::Destroy {
            destination,
            element,
            ..
        } => {
            collect_place_types(destination, types);
            types.insert(*element);
        }
        MirGeneratorOperation::Finish { destination } => collect_place_types(destination, types),
        MirGeneratorOperation::Push { destination, value } => {
            collect_place_types(destination, types);
            collect_operand_types(value, types);
        }
    }
}

fn collect_async_types(operation: &MirAsyncOperation, types: &mut BTreeSet<TypeId>) {
    match operation {
        MirAsyncOperation::CreateFrame { initializer, .. } => match initializer {
            MirFrameInitializer::Callable(call) => collect_call_types(call, types),
            MirFrameInitializer::TaskObservation { task, result, .. } => {
                collect_operand_types(task, types);
                types.insert(result.future_type());
            }
        },
        MirAsyncOperation::MoveInactiveFrame {
            source,
            destination,
            ..
        } => {
            collect_place_types(source, types);
            collect_place_types(destination, types);
        }
        MirAsyncOperation::ResumeFrame { .. }
        | MirAsyncOperation::CommitAwaitedCompletion { .. }
        | MirAsyncOperation::ObserveCurrentRunCancellation { .. }
        | MirAsyncOperation::ExecuteCleanupBroadcast { .. }
        | MirAsyncOperation::ExecuteLifecycleResolution { .. } => {}
        MirAsyncOperation::ComposeAwaitedFrame { frame, .. }
        | MirAsyncOperation::StartTask { value: frame, .. } => {
            collect_operand_types(frame, types);
        }
        MirAsyncOperation::RequestTaskCancellation { task, .. }
        | MirAsyncOperation::ResolveTask { task, .. }
        | MirAsyncOperation::DestroyTerminalTask { task } => {
            collect_operand_types(task, types);
        }
        MirAsyncOperation::PublishTerminalState { state, .. } => match state {
            MirTaskTerminalState::Completed(value) | MirTaskTerminalState::Panicked(value) => {
                collect_operand_types(value, types);
            }
            MirTaskTerminalState::Cancelled => {}
        },
        MirAsyncOperation::TransferCleanupIncident { incident, .. } => {
            collect_operand_types(incident, types);
        }
    }
}

fn collect_host_types(operation: &MirHostOperation, types: &mut BTreeSet<TypeId>) {
    match operation {
        MirHostOperation::ResolveRootTerminal {
            error: Some(error), ..
        } => {
            types.insert(*error);
        }
        MirHostOperation::SelectTestEntry { .. }
        | MirHostOperation::ExecuteRoot { .. }
        | MirHostOperation::ObserveRootTerminal { .. }
        | MirHostOperation::ResolveRootTerminal { error: None, .. }
        | MirHostOperation::ReportCleanupIncidents { .. }
        | MirHostOperation::StructuredShutdown { .. } => {}
    }
}

fn collect_call_types(call: &MirCall, types: &mut BTreeSet<TypeId>) {
    if let MirCallTarget::Indirect { callee, .. } = call.target() {
        collect_operand_types(callee, types);
    }

    for argument in call.arguments() {
        if let Some(value) = argument.value() {
            collect_operand_types(value, types);
        }
    }
}

fn collect_conversion_types(conversion: &SelectedConversion, types: &mut BTreeSet<TypeId>) {
    types.insert(conversion.source_type());
    types.insert(conversion.target_type());

    if let ConversionTarget::Composite(conversions) = conversion.target() {
        for conversion in conversions.iter() {
            collect_conversion_types(conversion, types);
        }
    }
}

fn collect_panic_types(cause: &MirPanicCause, types: &mut BTreeSet<TypeId>) {
    match cause {
        MirPanicCause::Message(message) | MirPanicCause::ExplicitTestFailure(message) => {
            collect_operand_types(message, types);
        }
        MirPanicCause::Assertion(message) => {
            if let Some(message) = message {
                collect_operand_types(message, types);
            }
        }
    }
}

fn collect_edge_types(edge: &MirEdge, types: &mut BTreeSet<TypeId>) {
    collect_operands_types(edge.arguments(), types);
}

fn collect_cleanup_edge_types(edge: &MirCleanupEdge, types: &mut BTreeSet<TypeId>) {
    collect_edge_types(edge.edge(), types);
}

fn collect_place_types(place: &MirPlace, types: &mut BTreeSet<TypeId>) {
    types.insert(place.ty());

    for projection in place.projections() {
        types.insert(projection.source_type());
        types.insert(projection.result_type());

        match projection.kind() {
            MirProjectionKind::Index(index) => collect_operand_types(index, types),
            MirProjectionKind::Slice { start, end } => {
                if let Some(start) = start {
                    collect_operand_types(start, types);
                }

                if let Some(end) = end {
                    collect_operand_types(end, types);
                }
            }
            MirProjectionKind::Dereference
            | MirProjectionKind::Field(_)
            | MirProjectionKind::TupleField(_)
            | MirProjectionKind::ElementFromStart(_)
            | MirProjectionKind::ElementFromEnd(_)
            | MirProjectionKind::Variant(_)
            | MirProjectionKind::ActiveUnionPayloadField { .. }
            | MirProjectionKind::NullableValue
            | MirProjectionKind::OwnedStorage => {}
        }
    }
}

fn collect_operands_types(operands: &[MirOperand], types: &mut BTreeSet<TypeId>) {
    for operand in operands {
        collect_operand_types(operand, types);
    }
}

fn collect_operand_types(operand: &MirOperand, types: &mut BTreeSet<TypeId>) {
    match operand {
        MirOperand::Constant { ty, .. } | MirOperand::Immediate { ty, .. } => {
            types.insert(*ty);
        }
        MirOperand::Copy(place) | MirOperand::Move(place) => collect_place_types(place, types),
        MirOperand::Value(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use bray_testing::test_bound_unit;

    use crate::{
        MirBlockKind, MirEdge, MirImmediateValue, MirOperand, MirSourceAnchor, MirTerminatorKind,
        MirUnitBuilder, MirUnitKind,
    };

    #[test]
    fn referenced_types_include_terminator_only_immediates() {
        let bound = test_bound_unit(21);
        let source = MirSourceAnchor::from(bound.key().source());
        let ty = crate::test_support::test_type();

        let mut builder = MirUnitBuilder::for_bound(
            bound.identity(),
            MirUnitKind::Synchronous,
            crate::test_support::test_target(),
        );

        let Ok(entry) = builder.push_block(source.clone(), MirBlockKind::Ordinary) else {
            panic!("test entry block must be valid");
        };

        let Ok(then_block) = builder.push_block(source.clone(), MirBlockKind::Ordinary) else {
            panic!("test then block must be valid");
        };

        let Ok(else_block) = builder.push_block(source.clone(), MirBlockKind::Ordinary) else {
            panic!("test else block must be valid");
        };

        let condition = MirOperand::Immediate {
            value: MirImmediateValue::Boolean(true),
            ty,
        };

        let branch = MirTerminatorKind::Branch {
            condition,
            then_edge: MirEdge::new(then_block, []),
            else_edge: MirEdge::new(else_block, []),
        };

        if let Err(error) = builder.set_terminator(entry, source.clone(), branch) {
            panic!("test branch must be valid: {error:?}");
        }

        for block in [then_block, else_block] {
            if let Err(error) =
                builder.set_terminator(block, source.clone(), MirTerminatorKind::Return(None))
            {
                panic!("test return must be valid: {error:?}");
            }
        }

        let Ok(unit) = builder.finish(entry) else {
            panic!("test MIR unit must be valid");
        };

        assert_eq!(unit.referenced_types(), [ty].into_iter().collect());
    }
}
