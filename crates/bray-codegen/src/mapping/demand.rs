use std::collections::{BTreeMap, BTreeSet};

use bray_ir::{
    MirAsyncOperation, MirCall, MirCallTarget, MirCleanupEdge, MirEdge, MirFrameInitializer,
    MirGeneratorOperation, MirHostOperation, MirOperand, MirOperationKind, MirPanicCause,
    MirPatternPredicate, MirPlace, MirProjectionKind, MirTerminatorKind, MirUnit,
};
use bray_symbols::{ConstantTermId, ConstantValueId, ConstantValueKind, TypeId};

/// Constant values and exact use-site types demanded by one code generation unit.
pub struct ConstantDemands {
    values: BTreeSet<ConstantValueId>,
    types: BTreeMap<ConstantValueId, BTreeSet<TypeId>>,
}

impl ConstantDemands {
    /// Returns directly demanded constant values.
    pub fn values(&self) -> &BTreeSet<ConstantValueId> {
        &self.values
    }

    /// Returns the semantic type required at each direct constant use.
    pub fn types(&self) -> &BTreeMap<ConstantValueId, BTreeSet<TypeId>> {
        &self.types
    }
}

/// Returns constant values directly demanded by the retained MIR units.
pub fn demanded_constants<'a>(units: impl IntoIterator<Item = &'a MirUnit>) -> ConstantDemands {
    let mut demands = ConstantDemands {
        values: BTreeSet::new(),
        types: BTreeMap::new(),
    };

    for unit in units {
        for operation in unit.operations() {
            collect_operation_values(operation.kind(), &mut demands);
        }

        for block in unit.blocks() {
            collect_terminator_values(block.terminator().kind(), &mut demands);
        }
    }

    demands
}

/// Returns closed constant terms retained by one MIR unit.
pub fn demanded_constant_terms(unit: &MirUnit) -> BTreeSet<ConstantTermId> {
    let mut terms = BTreeSet::new();

    for operation in unit.operations() {
        operation.kind().for_each_operand(|operand| {
            if let MirOperand::ConstantTerm { term, .. } = operand {
                terms.insert(*term);
            }
        });

        if let MirOperationKind::Generator(MirGeneratorOperation::Begin {
            exact_count: Some(term),
            ..
        }) = operation.kind()
        {
            terms.insert(*term);
        }
    }

    for block in unit.blocks() {
        let mut terminator = block.terminator().kind().clone();

        terminator.for_each_input(|operand| {
            if let MirOperand::ConstantTerm { term, .. } = operand {
                terms.insert(*term);
            }
        });

        if let MirTerminatorKind::CheckCallOutcome {
            panicked,
            cancelled,
            completed_report,
            ..
        } = &terminator
        {
            for report in [panicked.report(), cancelled.report()]
                .into_iter()
                .chain(completed_report)
            {
                report.for_each_operand(|operand| {
                    if let MirOperand::ConstantTerm { term, .. } = operand {
                        terms.insert(*term);
                    }
                });
            }
        }

        let _ = terminator.try_for_each_edge_mut::<()>(|edge| {
            for argument in edge.arguments() {
                argument.for_each_operand(|operand| {
                    if let MirOperand::ConstantTerm { term, .. } = operand {
                        terms.insert(*term);
                    }
                });
            }

            Ok(())
        });

        if let Some(term) = block.terminator().kind().pattern_constant_term() {
            terms.insert(term);
        }
    }

    terms
}

/// Returns constant values directly retained by one aggregate constant.
pub fn child_constants(kind: &ConstantValueKind) -> impl Iterator<Item = ConstantValueId> {
    let values: Vec<_> = match kind {
        ConstantValueKind::Borrowed(_) => {
            panic!("constant observation reached generated-code demands")
        }
        ConstantValueKind::NullablePresent(value) => vec![*value],
        ConstantValueKind::Tuple(values) | ConstantValueKind::Array(values) => values.to_vec(),
        ConstantValueKind::Product(fields) => fields.iter().map(|field| *field.value()).collect(),
        ConstantValueKind::Union { fields, .. } => {
            fields.iter().map(|field| *field.value()).collect()
        }
        ConstantValueKind::Error
        | ConstantValueKind::Boolean(_)
        | ConstantValueKind::Character(_)
        | ConstantValueKind::Integer(_)
        | ConstantValueKind::Real(_)
        | ConstantValueKind::Complex { .. }
        | ConstantValueKind::String(_)
        | ConstantValueKind::StaticAddress(_)
        | ConstantValueKind::Unit
        | ConstantValueKind::NullableAbsent => Vec::new(),
    };

    values.into_iter()
}

fn collect_operation_values(operation: &MirOperationKind, demands: &mut ConstantDemands) {
    if let Some(collection) = operation.cleanup_collection() {
        for place in collection.places() {
            collect_place_values(place, demands);
        }
    }

    match operation {
        MirOperationKind::AnonymousCallable(_) | MirOperationKind::DeclaredCallable(_) => {}
        MirOperationKind::AdmitOutgoing { owner, .. }
        | MirOperationKind::DischargeOutgoing { owner, .. } => {
            if let bray_ir::MirOutgoingOwner::FrameFactory(callee) = owner {
                collect_operand_value(callee, demands);
            }
        }
        MirOperationKind::Store {
            destination, value, ..
        } => {
            collect_place_values(destination, demands);
            collect_operand_value(value, demands);
        }
        MirOperationKind::Borrow { place, .. }
        | MirOperationKind::Quiesce { place, .. }
        | MirOperationKind::Cleanup { place, .. } => collect_place_values(place, demands),
        MirOperationKind::Unary { operand, .. }
        | MirOperationKind::Convert { operand, .. }
        | MirOperationKind::NumericConversion { operand, .. } => {
            collect_operand_value(operand, demands);
        }
        MirOperationKind::NullableQuery(query) => {
            collect_operand_value(query.operand(), demands);
        }
        MirOperationKind::Binary { left, right, .. } => {
            collect_operand_value(left, demands);
            collect_operand_value(right, demands);
        }
        MirOperationKind::Aggregate(aggregate) => {
            collect_operands(aggregate.operands(), demands);
        }
        MirOperationKind::Construct(construction) => {
            for input in construction.inputs() {
                collect_operand_value(input.value(), demands);
            }
        }
        MirOperationKind::PatternProjection { subject, .. } => {
            collect_operand_value(subject, demands);
        }
        MirOperationKind::Generator(operation) => collect_generator_values(operation, demands),
        MirOperationKind::Call(call) => collect_call_values(call, demands),
        MirOperationKind::Memory(memory) => {
            if let Some(abandoned) = memory.abandoned_cleanup() {
                collect_operand_value(abandoned, demands);
            }

            demands.values.extend(memory.kind().contract_constants());
            collect_operands(memory.operands(), demands);
        }
        MirOperationKind::Destroy {
            place, abandoned, ..
        } => {
            collect_place_values(place, demands);

            if let Some(abandoned) = abandoned {
                collect_operand_value(abandoned, demands);
            }
        }
        MirOperationKind::Text(text) => collect_operands(text.operands(), demands),
        MirOperationKind::PanicReport(cause) => collect_panic_values(cause, demands),
        MirOperationKind::Async(operation) => collect_async_values(operation, demands),
        MirOperationKind::Host(operation) => collect_host_values(operation, demands),
    }
}

fn collect_terminator_values(terminator: &MirTerminatorKind, demands: &mut ConstantDemands) {
    match terminator {
        MirTerminatorKind::Goto(edge) => collect_edge_values(edge, demands),
        MirTerminatorKind::Branch {
            condition,
            then_edge,
            else_edge,
        } => {
            collect_operand_value(condition, demands);
            collect_edge_values(then_edge, demands);
            collect_edge_values(else_edge, demands);
        }
        MirTerminatorKind::PatternBranch {
            subject,
            predicate,
            matched,
            unmatched,
        } => {
            collect_operand_value(subject, demands);

            if let MirPatternPredicate::Literal(literal) = predicate {
                demands.values.insert(*literal);
            }

            collect_edge_values(matched, demands);
            collect_edge_values(unmatched, demands);
        }
        MirTerminatorKind::RangeIterate {
            cursor, exhausted, ..
        } => {
            collect_place_values(cursor, demands);
            collect_edge_values(exhausted, demands);
        }
        MirTerminatorKind::Switch {
            discriminant,
            cases,
            otherwise,
        } => {
            collect_operand_value(discriminant, demands);
            collect_edge_values(otherwise, demands);

            for case in cases.iter() {
                demands.values.insert(case.value());
                collect_edge_values(case.edge(), demands);
            }
        }
        MirTerminatorKind::InlineAssembly(assembly) => {
            demands.values.extend(assembly.contract().constant_values());
            collect_operand_value(assembly.inputs(), demands);
        }
        MirTerminatorKind::Return { value, report } => {
            for operand in value.iter().chain(report) {
                collect_operand_value(operand, demands);
            }
        }
        MirTerminatorKind::Unreachable => {}
        MirTerminatorKind::Suspend {
            payload,
            resume,
            cancellation,
            ..
        } => {
            if let Some(payload) = payload {
                collect_operand_value(payload, demands);
            }

            collect_edge_values(resume, demands);
            collect_cleanup_edge_values(cancellation, demands);
        }
        MirTerminatorKind::ForwardRunResult { result, edges } => {
            collect_operand_value(result, demands);
            collect_edge_values(edges.completed(), demands);
            collect_cleanup_edge_values(edges.panicked(), demands);
            collect_cleanup_edge_values(edges.cancelled(), demands);
        }
        MirTerminatorKind::TakeCallReport { completed, report } => {
            collect_edge_values(completed, demands);
            collect_place_values(report, demands);
        }
        MirTerminatorKind::CheckCallOutcome {
            completed,
            panicked,
            cancelled,
            completed_report,
        } => {
            collect_edge_values(completed, demands);

            if let Some(report) = completed_report {
                collect_place_values(report, demands);
            }

            collect_place_values(panicked.report(), demands);
            collect_place_values(cancelled.report(), demands);
        }
        MirTerminatorKind::BeginCleanup(edge) | MirTerminatorKind::ContinueCleanup(edge) => {
            collect_cleanup_edge_values(edge, demands);
        }
        MirTerminatorKind::Panic { report, cleanup } => {
            collect_operand_value(report, demands);
            collect_cleanup_edge_values(cleanup, demands);
        }
        MirTerminatorKind::PropagatePanic { report, .. }
        | MirTerminatorKind::PropagateCancellation { report, .. } => {
            collect_operand_value(report, demands);
        }
        MirTerminatorKind::CancelCurrentRun { cleanup } => {
            collect_cleanup_edge_values(cleanup, demands);
        }
    }
}

fn collect_generator_values(operation: &MirGeneratorOperation, demands: &mut ConstantDemands) {
    if let MirGeneratorOperation::Destroy { abandoned, .. } = operation {
        collect_operand_value(abandoned, demands);
    }

    match operation {
        MirGeneratorOperation::Begin { destination, .. }
        | MirGeneratorOperation::Finish { destination }
        | MirGeneratorOperation::CleanupBroadcast { destination, .. }
        | MirGeneratorOperation::Destroy { destination, .. } => {
            collect_place_values(destination, demands);
        }
        MirGeneratorOperation::Push { destination, value } => {
            collect_place_values(destination, demands);
            collect_operand_value(value, demands);
        }
    }
}

fn collect_async_values(operation: &MirAsyncOperation, demands: &mut ConstantDemands) {
    match operation {
        MirAsyncOperation::CreateFrame { initializer, .. } => match initializer {
            MirFrameInitializer::Callable(call) => collect_call_values(call, demands),
            MirFrameInitializer::TaskObservation { task, .. } => {
                collect_operand_value(task, demands);
            }
        },
        MirAsyncOperation::MoveInactiveFrame {
            source,
            destination,
            ..
        } => {
            collect_place_values(source, demands);
            collect_place_values(destination, demands);
        }
        MirAsyncOperation::ResumeFrame { .. }
        | MirAsyncOperation::CommitAwaitedCompletion { .. }
        | MirAsyncOperation::ObserveFrameCancellation
        | MirAsyncOperation::ObserveCurrentRunCancellation { .. } => {}
        MirAsyncOperation::BroadcastInactiveFrame { frame, .. }
        | MirAsyncOperation::DestroyInactiveFrame { frame, .. } => {
            collect_operand_value(frame, demands);
        }
        MirAsyncOperation::ComposeAwaitedFrame { frame, .. }
        | MirAsyncOperation::StartTask { value: frame, .. } => {
            collect_operand_value(frame, demands);
        }
        MirAsyncOperation::DestroyTerminalTask { task } => {
            collect_operand_value(task, demands);
        }
        MirAsyncOperation::RequestTaskCancellation { task, .. }
        | MirAsyncOperation::ResolveTask { task, .. } => collect_place_values(task, demands),
        MirAsyncOperation::PublishTerminalState { state, .. } => {
            for operand in state.operands() {
                collect_operand_value(operand, demands);
            }
        }
        MirAsyncOperation::TransferCleanupIncident {
            incident, report, ..
        } => {
            collect_place_values(incident, demands);
            collect_place_values(report, demands);
        }
    }
}

fn collect_host_values(operation: &MirHostOperation, _demands: &mut ConstantDemands) {
    match operation {
        MirHostOperation::InitializeRuntime { .. }
        | MirHostOperation::MaterializeStatic { .. }
        | MirHostOperation::SelectTestEntry { .. }
        | MirHostOperation::ExecuteRoot { .. }
        | MirHostOperation::ObserveRootTerminal { .. }
        | MirHostOperation::ResolveRootTerminal { .. }
        | MirHostOperation::BeginProductEntry
        | MirHostOperation::BeginStaticCleanup
        | MirHostOperation::ReportCleanupIncidents { .. }
        | MirHostOperation::StructuredShutdown { .. } => {}
    }
}

fn collect_call_values(call: &MirCall, demands: &mut ConstantDemands) {
    if let MirCallTarget::Indirect { callee, .. } = call.target() {
        collect_operand_value(callee, demands);
    }

    for argument in call.arguments() {
        collect_operand_value(argument.value(), demands);
    }
}

fn collect_panic_values(cause: &MirPanicCause, demands: &mut ConstantDemands) {
    match cause {
        MirPanicCause::Message(message) | MirPanicCause::ExplicitTestFailure(message) => {
            collect_operand_value(message, demands);
        }
        MirPanicCause::Assertion(message) => {
            if let Some(message) = message {
                collect_operand_value(message, demands);
            }
        }
    }
}

fn collect_edge_values(edge: &MirEdge, demands: &mut ConstantDemands) {
    collect_operands(edge.arguments(), demands);
}

fn collect_cleanup_edge_values(edge: &MirCleanupEdge, demands: &mut ConstantDemands) {
    collect_edge_values(edge.edge(), demands);
}

fn collect_place_values(place: &MirPlace, demands: &mut ConstantDemands) {
    for projection in place.projections() {
        match projection.kind() {
            MirProjectionKind::Index(index) => collect_operand_value(index, demands),
            MirProjectionKind::Slice { start, end } => {
                if let Some(start) = start {
                    collect_operand_value(start, demands);
                }

                if let Some(end) = end {
                    collect_operand_value(end, demands);
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

fn collect_operands(operands: &[MirOperand], demands: &mut ConstantDemands) {
    for operand in operands {
        collect_operand_value(operand, demands);
    }
}

fn collect_operand_value(operand: &MirOperand, demands: &mut ConstantDemands) {
    match operand {
        MirOperand::Constant { value, ty } => {
            demands.values.insert(*value);
            demands.types.entry(*value).or_default().insert(*ty);
        }
        MirOperand::Copy(place) | MirOperand::Move(place) => collect_place_values(place, demands),
        MirOperand::Value(_) | MirOperand::ConstantTerm { .. } | MirOperand::Immediate { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use bray_ir::{
        MirBlockId, MirCallFailureEdge, MirEdge, MirOperand, MirPlace, MirProjection,
        MirProjectionKind, MirStorageId, MirTerminatorKind, MirUnitId,
    };
    use bray_symbols::{ConstantValueData, ConstantValueKind, SemanticValueStore, TypeData};

    #[test]
    fn call_failure_destinations_demand_their_projection_constants() {
        let values = SemanticValueStore::try_new().unwrap();
        let ty = values.intern_type(TypeData::tuple([])).unwrap();

        let constants = [false, true].map(|value| {
            values
                .intern_constant_value(ConstantValueData::new(
                    ty,
                    ConstantValueKind::Boolean(value),
                ))
                .unwrap()
        });

        let unit = MirUnitId::new(0);

        let report = |value| {
            MirPlace::new(
                MirStorageId::from_slot(unit, 0),
                [MirProjection::new(
                    MirProjectionKind::Index(MirOperand::Constant { value, ty }),
                    ty,
                    ty,
                )],
                ty,
            )
        };

        let terminator = MirTerminatorKind::CheckCallOutcome {
            completed: MirEdge::new(MirBlockId::from_slot(unit, 1), []),
            panicked: MirCallFailureEdge::new(MirBlockId::from_slot(unit, 2), report(constants[0])),
            cancelled: MirCallFailureEdge::new(
                MirBlockId::from_slot(unit, 3),
                report(constants[1]),
            ),
            completed_report: None,
        };

        let mut demands = super::demanded_constants([]);

        super::collect_terminator_values(&terminator, &mut demands);

        assert_eq!(demands.values(), &constants.into_iter().collect());

        assert_eq!(
            demands.types(),
            &constants
                .into_iter()
                .map(|value| (value, [ty].into_iter().collect()))
                .collect()
        );
    }
}
