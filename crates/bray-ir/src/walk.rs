use crate::{
    MirAsyncOperation, MirBlock, MirBlockId, MirCall, MirCallTarget, MirFrameInitializer,
    MirGeneratorOperation, MirOperand, MirOperation, MirOperationId, MirOperationKind,
    MirPanicCause, MirPlace, MirProjectionKind, MirStorage, MirStorageId, MirTaskTerminalState,
    MirTerminator, MirTerminatorKind, MirUnit, MirValue, MirValueId,
};

/// Controls deterministic traversal of immutable MIR.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MirVisitControl {
    /// Visit the current element's owned children and later siblings.
    Continue,
    /// Skip the current element's owned children and continue with later siblings.
    Skip,
    /// Stop the walk immediately.
    Stop,
}

/// Callback interface for deterministic serial MIR traversal.
pub trait MirVisitor {
    /// Visits the MIR unit before any contained table.
    fn visit_unit(&mut self, _unit: &MirUnit) -> MirVisitControl {
        MirVisitControl::Continue
    }

    /// Visits one storage allocation.
    fn visit_storage(&mut self, _id: MirStorageId, _storage: &MirStorage) -> MirVisitControl {
        MirVisitControl::Continue
    }

    /// Visits one value.
    fn visit_value(&mut self, _id: MirValueId, _value: &MirValue) -> MirVisitControl {
        MirVisitControl::Continue
    }

    /// Visits one block before its operations and terminator.
    fn visit_block(&mut self, _id: MirBlockId, _block: &MirBlock) -> MirVisitControl {
        MirVisitControl::Continue
    }

    /// Visits one operation in block execution order.
    fn visit_operation(
        &mut self,
        _id: MirOperationId,
        _operation: &MirOperation,
    ) -> MirVisitControl {
        MirVisitControl::Continue
    }

    /// Visits one block terminator after its operations.
    fn visit_terminator(
        &mut self,
        _block: MirBlockId,
        _terminator: &MirTerminator,
    ) -> MirVisitControl {
        MirVisitControl::Continue
    }
}

/// Walks one immutable MIR unit in deterministic table and block order.
pub fn walk_mir_unit<V: MirVisitor + ?Sized>(unit: &MirUnit, visitor: &mut V) {
    match visitor.visit_unit(unit) {
        MirVisitControl::Continue => {}
        MirVisitControl::Skip | MirVisitControl::Stop => return,
    }

    for (index, storage) in unit.storages().iter().enumerate() {
        let Some(slot) = u32::try_from(index).ok() else {
            return;
        };

        let id = MirStorageId::from_slot(unit.unit(), slot);

        if visitor.visit_storage(id, storage) == MirVisitControl::Stop {
            return;
        }
    }

    for (index, value) in unit.values().iter().enumerate() {
        let Some(slot) = u32::try_from(index).ok() else {
            return;
        };

        let id = MirValueId::from_slot(unit.unit(), slot);

        if visitor.visit_value(id, value) == MirVisitControl::Stop {
            return;
        }
    }

    for (index, block) in unit.blocks().iter().enumerate() {
        let Some(slot) = u32::try_from(index).ok() else {
            return;
        };

        let block_id = MirBlockId::from_slot(unit.unit(), slot);

        match visitor.visit_block(block_id, block) {
            MirVisitControl::Continue => {}
            MirVisitControl::Skip => continue,
            MirVisitControl::Stop => return,
        }

        for operation_id in block.operations() {
            let Some(operation) = unit.operation(*operation_id) else {
                return;
            };

            if visitor.visit_operation(*operation_id, operation) == MirVisitControl::Stop {
                return;
            }
        }

        if visitor.visit_terminator(block_id, block.terminator()) == MirVisitControl::Stop {
            return;
        }
    }
}

impl MirOperationKind {
    /// Visits evaluated operands, including storage selectors, in execution order.
    ///
    /// Addressed storage is not a value operand. Operations such as frame moves also have
    /// explicit storage effects that callers must handle independently.
    pub fn for_each_operand(&self, mut visit: impl FnMut(&MirOperand)) {
        match self {
            Self::Store {
                destination, value, ..
            } => {
                visit_place_operands(destination, &mut visit);
                visit_operand(value, &mut visit);
            }
            Self::Borrow { place, .. }
            | Self::Finalize(place)
            | Self::Destroy(place)
            | Self::Cleanup { place, .. } => visit_place_operands(place, &mut visit),
            Self::Unary { operand, .. }
            | Self::Convert { operand, .. }
            | Self::NumericConversion { operand, .. } => visit_operand(operand, &mut visit),
            Self::Binary { left, right, .. } => {
                visit_operand(left, &mut visit);
                visit_operand(right, &mut visit);
            }
            Self::Aggregate(aggregate) => {
                for operand in aggregate.operands() {
                    visit_operand(operand, &mut visit);
                }
            }
            Self::Construct(construction) => {
                for input in construction.inputs() {
                    visit_operand(input.value(), &mut visit);
                }
            }
            Self::NullableQuery(query) => visit_operand(query.operand(), &mut visit),
            Self::PatternProjection { subject, .. } => visit_operand(subject, &mut visit),
            Self::Generator(operation) => match operation {
                MirGeneratorOperation::Push { destination, value } => {
                    visit_place_operands(destination, &mut visit);
                    visit_operand(value, &mut visit);
                }
                MirGeneratorOperation::Begin { destination, .. }
                | MirGeneratorOperation::Finish { destination }
                | MirGeneratorOperation::CleanupBroadcast { destination, .. }
                | MirGeneratorOperation::Destroy { destination, .. } => {
                    visit_place_operands(destination, &mut visit);
                }
            },
            Self::Call(call) => visit_call_operands(call, &mut visit),
            Self::Memory(operation) => {
                for operand in operation.operands() {
                    visit_operand(operand, &mut visit);
                }
            }
            Self::Text(operation) => {
                for operand in operation.operands() {
                    visit_operand(operand, &mut visit);
                }
            }
            Self::PanicReport(cause) => match cause {
                MirPanicCause::Message(operand) | MirPanicCause::ExplicitTestFailure(operand) => {
                    visit_operand(operand, &mut visit);
                }
                MirPanicCause::Assertion(message) => {
                    if let Some(message) = message {
                        visit_operand(message, &mut visit);
                    }
                }
            },
            Self::Async(operation) => visit_async_operands(operation, &mut visit),
            Self::Host(operation) => match operation {
                crate::MirHostOperation::MaterializeStatic { place } => {
                    visit_place_operands(place, &mut visit);
                }
                crate::MirHostOperation::SelectTestEntry { .. }
                | crate::MirHostOperation::ExecuteRoot { .. }
                | crate::MirHostOperation::ObserveRootTerminal { .. }
                | crate::MirHostOperation::ResolveRootTerminal { .. }
                | crate::MirHostOperation::BeginStaticCleanup
                | crate::MirHostOperation::ReportCleanupIncidents { .. }
                | crate::MirHostOperation::StructuredShutdown { .. } => {}
            },
            Self::AnonymousCallable(_)
            | Self::DeclaredCallable(_)
            | Self::AdmitOutgoing { .. }
            | Self::DischargeOutgoing { .. } => {}
        }
    }
}

fn visit_call_operands(call: &MirCall, visit: &mut impl FnMut(&MirOperand)) {
    if let MirCallTarget::Indirect { callee, .. } = call.target() {
        visit_operand(callee, visit);
    }

    for argument in call.arguments() {
        visit_operand(argument.value(), visit);
    }
}

fn visit_async_operands(operation: &MirAsyncOperation, visit: &mut impl FnMut(&MirOperand)) {
    match operation {
        MirAsyncOperation::CreateFrame { initializer, .. } => match initializer {
            MirFrameInitializer::Callable(call) => visit_call_operands(call, visit),
            MirFrameInitializer::TaskObservation { task, .. } => visit_operand(task, visit),
        },
        MirAsyncOperation::MoveInactiveFrame {
            source,
            destination,
            ..
        } => {
            visit_place_operands(source, visit);
            visit_place_operands(destination, visit);
        }
        MirAsyncOperation::ComposeAwaitedFrame { frame, .. } => visit_operand(frame, visit),
        MirAsyncOperation::StartTask { value, .. } => visit_operand(value, visit),
        MirAsyncOperation::RequestTaskCancellation { task, .. }
        | MirAsyncOperation::ResolveTask { task, .. }
        | MirAsyncOperation::DestroyTerminalTask { task } => visit_operand(task, visit),
        MirAsyncOperation::PublishTerminalState { state, .. } => match state {
            MirTaskTerminalState::Completed(value) | MirTaskTerminalState::Panicked(value) => {
                visit_operand(value, visit);
            }
            MirTaskTerminalState::Cancelled => {}
        },
        MirAsyncOperation::TransferCleanupIncident { incident, .. } => {
            visit_operand(incident, visit)
        }
        MirAsyncOperation::ResumeFrame { .. }
        | MirAsyncOperation::CommitAwaitedCompletion { .. }
        | MirAsyncOperation::ObserveCurrentRunCancellation { .. }
        | MirAsyncOperation::ExecuteCleanupBroadcast { .. }
        | MirAsyncOperation::ExecuteLifecycleResolution { .. } => {}
    }
}

impl MirOperand {
    /// Visits nested selectors before this operand, in evaluation order.
    pub fn for_each_operand(&self, mut visit: impl FnMut(&MirOperand)) {
        visit_operand(self, &mut visit);
    }
}

impl MirTerminatorKind {
    /// Visits inputs evaluated before successor selection, excluding edge arguments.
    pub fn for_each_input(&self, mut visit: impl FnMut(&MirOperand)) {
        match self {
            Self::Branch { condition, .. } => visit_operand(condition, &mut visit),
            Self::PatternBranch { subject, .. } => visit_operand(subject, &mut visit),
            Self::Switch { discriminant, .. } => visit_operand(discriminant, &mut visit),
            Self::InlineAssembly(assembly) => visit_operand(assembly.inputs(), &mut visit),
            Self::Return(value) | Self::Suspend { payload: value, .. } => {
                if let Some(value) = value {
                    visit_operand(value, &mut visit);
                }
            }
            Self::ForwardRunResult { result, .. } => visit_operand(result, &mut visit),
            Self::Panic { report, .. } | Self::PropagatePanic { report, .. } => {
                visit_operand(report, &mut visit)
            }
            Self::Iterate { cursor, .. } | Self::RangeIterate { cursor, .. } => {
                visit_place_operands(cursor, &mut visit)
            }
            Self::Goto(_)
            | Self::Unreachable
            | Self::CheckCallOutcome { .. }
            | Self::BeginCleanup(_)
            | Self::ContinueCleanup(_)
            | Self::PropagateCancellation { .. }
            | Self::CancelCurrentRun { .. } => {}
        }
    }
}

fn visit_operand(operand: &MirOperand, visit: &mut impl FnMut(&MirOperand)) {
    if let MirOperand::Copy(place) | MirOperand::Move(place) = operand {
        visit_place_operands(place, visit);
    }

    visit(operand);
}

fn visit_place_operands(place: &MirPlace, visit: &mut impl FnMut(&MirOperand)) {
    for projection in place.projections() {
        match projection.kind() {
            MirProjectionKind::Index(operand) => visit_operand(operand, visit),
            MirProjectionKind::Slice { start, end } => {
                for operand in start.iter().chain(end) {
                    visit_operand(operand, visit);
                }
            }
            MirProjectionKind::Dereference
            | MirProjectionKind::Field(_)
            | MirProjectionKind::TupleField(_)
            | MirProjectionKind::ElementFromStart(_)
            | MirProjectionKind::ElementFromEnd(_)
            | MirProjectionKind::Variant(_)
            | MirProjectionKind::ActiveUnionPayloadField { .. }
            | MirProjectionKind::ActiveUnionPayloadElement { .. }
            | MirProjectionKind::NullableValue
            | MirProjectionKind::OwnedStorage => {}
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn terminator_inputs_do_not_visit_unselected_edge_arguments() {
        use crate::{
            MirBlockId, MirEdge, MirOperand, MirPlace, MirStorageId, MirTerminatorKind, MirUnitId,
        };

        let unit = MirUnitId::new(9);
        let ty = crate::test_support::test_type();
        let condition = MirOperand::Copy(MirPlace::new(MirStorageId::from_slot(unit, 0), [], ty));
        let moved = MirOperand::Move(MirPlace::new(MirStorageId::from_slot(unit, 1), [], ty));

        let terminator = MirTerminatorKind::Branch {
            condition: condition.clone(),
            then_edge: MirEdge::new(MirBlockId::from_slot(unit, 1), [moved]),
            else_edge: MirEdge::new(MirBlockId::from_slot(unit, 2), []),
        };

        let mut inputs = Vec::new();
        terminator.for_each_input(|input| inputs.push(input.clone()));
        assert_eq!(inputs, [condition]);
    }

    #[test]
    fn operand_walk_includes_nested_selectors_before_the_consuming_input() {
        use crate::{
            MirOperand, MirOperationKind, MirPlace, MirProjection, MirProjectionKind, MirStorageId,
            MirStoreKind, MirUnitId,
        };

        let ty = crate::test_support::test_type();
        let unit = MirUnitId::new(9);
        let selector = MirOperand::Move(MirPlace::new(MirStorageId::from_slot(unit, 0), [], ty));

        let destination = MirPlace::new(
            MirStorageId::from_slot(unit, 1),
            [MirProjection::new(
                MirProjectionKind::Index(selector.clone()),
                ty,
                ty,
            )],
            ty,
        );

        let value = MirOperand::Move(MirPlace::new(MirStorageId::from_slot(unit, 2), [], ty));

        let operation = MirOperationKind::Store {
            kind: MirStoreKind::Initialize,
            destination,
            value: value.clone(),
        };

        let mut visited = Vec::new();
        operation.for_each_operand(|operand| visited.push(operand.clone()));

        assert_eq!(visited, [selector, value]);
    }

    #[test]
    fn operand_walk_retains_terminal_state_ownership_transfers() {
        use crate::{
            MirAsyncOperation, MirOperand, MirOperationKind, MirPlace, MirRuntimeReference,
            MirStorageId, MirTaskTerminalState, MirUnitId,
        };

        use bray_runtime_interface::{RuntimeAbiRole, RuntimeAbiVersion};

        let ty = crate::test_support::test_type();

        let value = MirOperand::Move(MirPlace::new(
            MirStorageId::from_slot(MirUnitId::new(10), 0),
            [],
            ty,
        ));

        let operation = MirOperationKind::Async(MirAsyncOperation::PublishTerminalState {
            state: MirTaskTerminalState::Completed(value.clone()),
            runtime: MirRuntimeReference::new(
                RuntimeAbiRole::TerminalPublication,
                RuntimeAbiVersion::new(1, 0),
            ),
        });

        let mut visited = Vec::new();
        operation.for_each_operand(|operand| visited.push(operand.clone()));

        assert_eq!(visited, [value]);
    }

    use bray_testing::test_bound_unit;

    use super::{MirVisitControl, MirVisitor, walk_mir_unit};
    use crate::{
        MirBlock, MirBlockId, MirBlockKind, MirSourceAnchor, MirTerminatorKind, MirUnitBuilder,
        MirUnitKind,
    };

    #[test]
    fn walkers_visit_blocks_and_terminators_in_construction_order() {
        let unit = mir_unit();
        let mut visitor = BlockVisitor::default();

        walk_mir_unit(&unit, &mut visitor);

        assert_eq!(visitor.blocks, vec![unit.entry()]);
        assert_eq!(visitor.terminators, vec![unit.entry()]);
    }

    #[derive(Default)]
    struct BlockVisitor {
        blocks: Vec<MirBlockId>,
        terminators: Vec<MirBlockId>,
    }

    impl MirVisitor for BlockVisitor {
        fn visit_block(&mut self, id: MirBlockId, _block: &MirBlock) -> MirVisitControl {
            self.blocks.push(id);

            MirVisitControl::Continue
        }

        fn visit_terminator(
            &mut self,
            block: MirBlockId,
            _terminator: &crate::MirTerminator,
        ) -> MirVisitControl {
            self.terminators.push(block);

            MirVisitControl::Continue
        }
    }

    fn mir_unit() -> crate::MirUnit {
        let bound = test_bound_unit(4);
        let source = MirSourceAnchor::from(bound.key().source());
        let target = crate::test_support::test_target();

        let mut builder =
            MirUnitBuilder::for_bound(bound.identity(), MirUnitKind::Synchronous, target);

        let Ok(entry) = builder.push_block(source.clone(), MirBlockKind::Ordinary) else {
            panic!("test MIR block must be valid");
        };

        builder.set_terminator(entry, source, MirTerminatorKind::Return(None));

        builder.finish(entry)
    }
}
