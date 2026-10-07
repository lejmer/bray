use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{AnyBoundNodeId, BoundExpressionId, CheckedExpressionSemantics, StoragePlan};
use bray_diagnostics::{Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticExpressionCategory, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind, DiagnosticNote, DiagnosticNoteKind, SeverityKind};

use super::flow::{ExecutionFlowDomain, analyze_execution_flow};
use super::state::ExecutionState;
use crate::{CheckerOutcome, CheckerRequestContext, CheckerUnitView, ExecutionCondition};

pub(crate) fn check_trusted_contracts<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    expressions: &CheckedExpressionSemantics,
    storage: &StoragePlan,
    graph: &super::super::model::ControlFlowGraph,
    cleanup: Option<&bray_bound_tree::CheckedAsync>,
) -> CheckerOutcome<BTreeSet<BoundExpressionId>, C::UpstreamError> {
    let Some(contracts) = request.trusted_contracts() else {
        return CheckerOutcome::complete(BTreeSet::new(), DiagnosticBag::new());
    };

    if contracts.requirements.is_empty() && contracts.guarantees.is_empty() && contracts.calls.is_empty() {
        return CheckerOutcome::complete(BTreeSet::new(), DiagnosticBag::new());
    }

    let literals = crate::execution_guarantees::condition_literals(expressions, request.semantic_values());
    let completion = BTreeMap::new();

    let (flow, mut diagnostics) = match analyze_execution_flow(graph, request, expressions, &contracts.requirements,
        &literals, storage, &completion, cleanup) {
        CheckerOutcome::Complete(flow) => flow.into_parts(),
        CheckerOutcome::Cancelled => return CheckerOutcome::Cancelled,
        CheckerOutcome::InfrastructureFailure(error) => return CheckerOutcome::InfrastructureFailure(error),
        CheckerOutcome::UpstreamFailure(error) => return CheckerOutcome::UpstreamFailure(error),
    };

    let mut proven = BTreeSet::new();
    let mut failed = BTreeSet::new();
    let mut transfers = BTreeSet::new();

    for block in graph.blocks() {
        let Some(Some(entry)) = flow.states.state(block.id()) else {
            continue;
        };

        // Replay against the converged entry state to publish only live caller evidence.
        let mut state = entry.clone();

        for operation in block.operations().iter().filter_map(|id| graph.operation(*id)) {
            for plan in storage.access_plans().iter().filter(|plan| cleanup.is_some() && plan.node() == operation.kind().node()
                && matches!(plan.purpose(), bray_bound_tree::StorageAccessPurpose::Copy
                    | bray_bound_tree::StorageAccessPurpose::ValueTransfer | bray_bound_tree::StorageAccessPurpose::Move)) {
                let value = flow.domain.storage_value(&state, *plan);

                if plan.purpose() == bray_bound_tree::StorageAccessPurpose::ValueTransfer
                    && storage.root_identity(plan.access()).and_then(|identity| storage.identity(identity))
                        .is_some_and(|identity| matches!(identity, bray_bound_tree::StorageIdentity::Temporary(_)
                            | bray_bound_tree::StorageIdentity::Result(_) | bray_bound_tree::StorageIdentity::Allocation(_))) {
                    // A fresh result or its destructured components move into their first destination.
                    continue;
                }

                let mut is_witness = state.is_witness(&value);
                let mut partial_witness = matches!(&value, ExecutionCondition::Field(_, base) if state.is_witness(base));

                for (condition, subject) in &contracts.required_witnesses {
                    let condition = condition.substitute(&|place| place.value_in(&state.current)
                        .unwrap_or_else(|| ExecutionCondition::Input(place.clone())), &ExecutionCondition::Unknown,
                        &mut { ExecutionCondition::WORK_LIMIT });

                    if condition.prove_trusted(&state.trusted_assumptions, &state.assumptions) != Some(true) { continue; }

                    let subject = subject.value_in(&state.current)
                        .unwrap_or_else(|| ExecutionCondition::Input(subject.clone()));

                    is_witness |= subject == value || matches!((&subject, &value),
                        (ExecutionCondition::Input(subject), ExecutionCondition::Input(copied)) if copied.contains(subject));

                    partial_witness |= subject != value && matches!((&subject, &value),
                        (ExecutionCondition::Input(subject), ExecutionCondition::Input(part)) if subject.contains(part));
                }

                if !is_witness && !partial_witness { continue; }

                let ty = storage.access(plan.access()).expect("checked access plan references existing storage").reached_type();

                if matches!(request.semantic_values().type_data(ty).as_ref(), bray_symbols::TypeData::Borrow { .. }) { continue; }

                let copies = plan.purpose() != bray_bound_tree::StorageAccessPurpose::Move
                    && flow.copied_types.contains(&ty);

                if (is_witness && copies) || (partial_witness && !copies) { transfers.insert(plan.expression()); }
            }

            flow.domain.operation(&mut state, operation.kind());

            let AnyBoundNodeId::Expression(expression) = operation.kind().node() else {
                continue;
            };

            if let Some(bray_bound_tree::BoundExpression::Structured(bound)) = request.view().expression(expression)
                && bound.kind() == bray_bound_tree::BoundStructuredExpressionKind::RepeatedArray
                && let [value, count] = bound.operands()
                && state.is_witness(&flow.domain.value(&state, *value)) {
                let count = flow.domain.value(&state, *count);

                let singleton = matches!(count, ExecutionCondition::Literal(count)
                    if matches!(count.kind(), bray_symbols::ConstantValueKind::Integer(count) if count.to_u64().is_some_and(|count| count <= 1)));

                if !singleton { transfers.insert(expression); }
            }

            let Some(contract) = contracts.calls.get(&expression) else {
                continue;
            };

            if let Some(evidence) = state.entries.get(&expression) {
                if flow.domain.trusted_requirements_proven(evidence, &contract.requirements) {
                    proven.insert(expression);
                } else {
                    failed.insert(expression);
                }
            }
        }
    }

    if cleanup.is_none() {
        // Raw storage checking needs preliminary predicate evidence before selecting cleanup.
        // Publish diagnostics only from the final analysis with actual lifecycle effects.
        return CheckerOutcome::without_diagnostics(proven);
    }

    for (condition, span) in &contracts.guarantees {
        let proven = graph.exits().iter().filter(|exit| matches!(exit.kind(),
            super::super::model::AnalysisExitKind::Return | super::super::model::AnalysisExitKind::NormalFallthrough
            | super::super::model::AnalysisExitKind::ResultErrorPropagation)).all(|exit| {
            let Some(state) = flow.output(exit.block()) else {
                return true;
            };

            let condition = condition.substitute(&|place| place.value_in(&state.current)
                .unwrap_or_else(|| ExecutionCondition::Input(place.clone())), &state.result,
                &mut { ExecutionCondition::WORK_LIMIT });

            condition.prove_trusted(&state.trusted_assumptions, &state.assumptions) == Some(true)
        });

        if !proven {
            diagnostics.add(Diagnostic::new(bray_diagnostics::DiagnosticId::new(span.start().bytes()),
                DiagnosticKind::CheckingTrustedObligationNotProven, SeverityKind::Error).with_primary_span(*span)
                .with_label(DiagnosticLabel::primary(DiagnosticLabelKind::TrustedObligationFailure, *span))
                .with_arg(DiagnosticArg::expression_category(DiagnosticExpressionCategory::Call))
                .with_note(DiagnosticNote::new(DiagnosticNoteKind::TrustedObligationEvidenceRequired)));
        }
    }

    for expression in transfers {
        diagnostics.add(trusted_diagnostic(request, expressions, expression.into(),
            DiagnosticKind::CheckingTrustedWitnessTransferNotProven));
    }

    for expression in failed {
        proven.remove(&expression);

        diagnostics.add(trusted_diagnostic(request, expressions, expression.into(),
            DiagnosticKind::CheckingTrustedObligationNotProven));
    }

    CheckerOutcome::complete(proven, diagnostics)
}

pub(crate) fn check_trusted_cleanup<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    expressions: &CheckedExpressionSemantics,
    storage: &StoragePlan,
    cleanup: &bray_bound_tree::CheckedAsync,
    graph: &super::super::model::ControlFlowGraph,
) -> CheckerOutcome<(), C::UpstreamError> {
    use bray_symbols::{TypeAssociatedLifecycleSlot, TypeData};

    let mut diagnostics = DiagnosticBag::new();

    // Predicate clauses describe entry and completion; they do not execute owner cleanup.
    if request.unit().key().kind() == bray_bound_tree::BoundUnitKind::ContractClause {
        return CheckerOutcome::complete((), diagnostics);
    }

    let accesses = cleanup.scope_exits().iter().flat_map(|plan| plan.lifecycle_resolution().iter().copied())
        .chain(cleanup.replacements().iter().map(|plan| plan.access())).collect::<BTreeSet<_>>();

    let mut contracts = BTreeMap::new();

    macro_rules! query {
        ($query:expr) => { match $query {
            Ok(value) => value,
            Err(crate::CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
            Err(crate::CheckerQueryError::Infrastructure(error)) => return CheckerOutcome::InfrastructureFailure(error),
            Err(crate::CheckerQueryError::Upstream(error)) => return CheckerOutcome::UpstreamFailure(error),
        }};
    }

    let mut parts = BTreeMap::new();

    for access in accesses {
        let reached = storage.access(access).expect("checked cleanup retains its storage access");

        let selected = cleanup.storage_requirements().iter().find(|requirement|
            Some(requirement.identity()) == storage.root_identity(access)).and_then(|requirement| requirement.parts());

        let expanded = query!(crate::asynchronous::full_cleanup_parts(request, reached.reached_type(), reached.source(), selected));

        diagnostics.add_range(expanded.diagnostics().iter().cloned());
        parts.insert(access, expanded.into_parts().0);
    }

    let types = parts.values().flat_map(|parts| parts.iter().map(|(ty, _)| *ty)).collect::<BTreeSet<_>>();

    for ty in types {
        if !matches!(request.semantic_values().type_data(ty).as_ref(), TypeData::Named { .. }) { continue; }

        for slot in [TypeAssociatedLifecycleSlot::Finalizer, TypeAssociatedLifecycleSlot::Destructor] {
            let selected = query!(request.context().lifecycle_callable(ty, slot));

            diagnostics.add_range(selected.diagnostics().iter().cloned());

            let Some((_, signature)) = selected.value() else { continue; };

            let data = request.semantic_values().type_data(signature.callable_type());

            let TypeData::Callable(callable) = data.as_ref() else { panic!("selected lifecycle must have a callable type"); };

            let references = signature.receiver().map(|receiver| bray_bound_tree::BoundReferenceTarget::Surface(receiver.parameter().into()))
                .into_iter().chain(signature.parameters().iter().map(|parameter|
                    bray_bound_tree::BoundReferenceTarget::Surface(parameter.parameter().into()))).collect::<Vec<_>>();

            let behavior = callable.phase_behaviors().invocation();
            let completion = callable.phase_behaviors().deferred_execution().unwrap_or(behavior);

            let mut contract = crate::TrustedCallContract {
                // Failed finalizer attempts can still be followed by destruction.
                completes: callable.execution() == bray_symbols::CallableExecution::Synchronous
                    && behavior.execution_properties().contains(&bray_symbols::ExecutionProperty::Total),
                ..Default::default()
            };

            for (predicates, completion) in [(behavior.predicate_requirements(), false),
                (completion.predicate_guarantees(), true)] {
                for predicate in predicates {
                    let condition = match predicate.condition().map(|term|
                        crate::execution_condition_from_term(request.semantic_values(), term, &references)).transpose() {
                        Ok(condition) => condition.unwrap_or(ExecutionCondition::Unknown),
                        Err(error) => return CheckerOutcome::InfrastructureFailure(
                            crate::CheckerInfrastructureError::SemanticValueStore(error)),
                    };

                    let destination = match (completion, predicate.is_trusted()) {
                        (false, true) => &mut contract.requirements,
                        (false, false) => &mut contract.preconditions,
                        (true, true) => &mut contract.guarantees,
                        (true, false) => &mut contract.postconditions,
                    };

                    destination.push(condition);
                }
            }

            // Even a lifecycle with no predicates can invalidate the next lifecycle's evidence.
            contracts.insert((ty, slot), contract);
        }
    }

    if contracts.is_empty() { return CheckerOutcome::complete((), diagnostics); }

    let literals = crate::execution_guarantees::condition_literals(expressions, request.semantic_values());
    let completion = BTreeMap::new();
    let requirements = request.trusted_contracts().map_or(&[][..], |contracts| contracts.requirements.as_slice());

    let flow = match analyze_execution_flow(graph, request, expressions, requirements, &literals, storage, &completion, Some(cleanup)) {
        CheckerOutcome::Complete(flow) => {
            let (flow, additional) = flow.into_parts();

            diagnostics.add_range(additional);

            flow
        },
        CheckerOutcome::Cancelled => return CheckerOutcome::Cancelled,
        CheckerOutcome::InfrastructureFailure(error) => return CheckerOutcome::InfrastructureFailure(error),
        CheckerOutcome::UpstreamFailure(error) => return CheckerOutcome::UpstreamFailure(error),
    };

    let mut failed = BTreeSet::new();

    for block in graph.blocks() {
        let Some(Some(entry)) = flow.states.state(block.id()) else { continue; };

        let mut state = entry.clone();

        for operation in block.operations().iter().filter_map(|id| graph.operation(*id)) {
            let targets = match operation.kind() {
                super::super::model::AnalysisOperationKind::ScopeExit { block, exit, phase: super::super::model::AnalysisScopeExitPhase::LifecycleResolution } =>
                    cleanup.scope_exits().iter().filter(|plan| plan.scope() == block && plan.exit() == exit)
                        .flat_map(|plan| plan.lifecycle_resolution().iter().copied()).collect::<Vec<_>>(),
                super::super::model::AnalysisOperationKind::Bound(AnyBoundNodeId::Expression(expression)) =>
                    cleanup.replacements().iter().filter(|plan| plan.expression() == expression).map(|plan| plan.access()).collect(),
                _ => Vec::new(),
            };

            for access in targets {
                for (ty, part) in parts.get(&access).expect("cleanup access has its selected expansion") {
                    let place = part.as_ref().and_then(|part| crate::ExecutionPlace::storage(storage, access).and_then(|mut place| {
                        for projection in part.projections() {
                            let bray_bound_tree::StorageCleanupProjectionKind::Component(projection) = projection.projection() else { return None; };

                            place = place.project(&[projection])?;
                        }

                        Some(place)
                    }));

                for slot in [TypeAssociatedLifecycleSlot::Finalizer, TypeAssociatedLifecycleSlot::Destructor] {
                    let Some(contract) = contracts.get(&(*ty, slot)) else { continue; };

                    let evidence = query!(flow.cleanup_place_entry(&state, place.clone(), *ty, slot, false));

                    let established = evidence.as_ref().is_some_and(|(_, _, evidence)|
                        flow.domain.trusted_requirements_proven(evidence, &contract.requirements) && evidence.proves(&contract.preconditions));

                    if !established && !contract.requirements.is_empty() {
                        failed.insert(operation.kind().node());
                    }

                    state.invalidate_cleanup();

                    if established && contract.completes && let Some((_, _, evidence)) = evidence {
                        flow.domain.complete_trusted_cleanup(&mut state, place.clone(), contract, &evidence);
                    }
                }

                state.invalidate_cleanup();
                }
            }

            flow.domain.operation(&mut state, operation.kind());
        }
    }

    for node in failed {
        diagnostics.add(trusted_diagnostic(request, expressions, node,
            DiagnosticKind::CheckingTrustedObligationNotProven));
    }

    CheckerOutcome::complete((), diagnostics)
}

fn trusted_diagnostic<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    expressions: &CheckedExpressionSemantics,
    node: AnyBoundNodeId,
    kind: DiagnosticKind,
) -> Diagnostic {
    let origin = crate::diagnostic::bound_node_origin(request, node)
        .expect("trusted requirement failure retains its source origin");

    let anchor = origin.source_anchor().syntax();
    let span = bray_source::SourceSpan::new(anchor.source_id(), anchor.full_range());

    let category = if let AnyBoundNodeId::Expression(expression) = node {
        crate::diagnostic::expression_category(request.unit().tree().expression(expression)
            .expect("trusted call expression belongs to the checked unit"))
    } else { DiagnosticExpressionCategory::ControlFlow };

    let note = if kind == DiagnosticKind::CheckingTrustedWitnessTransferNotProven {
        DiagnosticNoteKind::TrustedWitnessTransferRequired
    } else { DiagnosticNoteKind::TrustedObligationEvidenceRequired };

    let mut diagnostic = Diagnostic::new(bray_diagnostics::DiagnosticId::new(span.start().bytes()),
        kind, SeverityKind::Error).with_primary_span(span)
        .with_arg(DiagnosticArg::expression_category(category))
        .with_label(DiagnosticLabel::primary(DiagnosticLabelKind::TrustedObligationFailure, span))
        .with_note(DiagnosticNote::new(note));

    if let AnyBoundNodeId::Expression(expression) = node
        && let Some(bray_bound_tree::SemanticSelection::Call(call)) = expressions.selections().expression(expression)
        && let bray_bound_tree::BoundCallableTarget::Declaration(callable) = call.target()
        && let Some(name) = request.context().symbols().member_name(callable.definition().symbol()) {
        diagnostic = diagnostic.with_label(DiagnosticLabel::secondary(DiagnosticLabelKind::TrustedCallable, span)
            .with_arg(DiagnosticArg::declaration_name(name.as_str())));
    }

    diagnostic
}

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    fn trusted_requirements_proven(&self, entry: &crate::ExecutionCallEvidence, requirements: &[ExecutionCondition]) -> bool {
        if entry.proves_trusted(requirements) { return true; }

        let equalities = ExecutionCondition::equalities(&entry.assumptions);

        let ordinary = entry.assumptions.iter().map(|(condition, value)|
            (condition.with_equalities(&equalities), *value)).collect::<BTreeSet<_>>();

        requirements.iter().all(|requirement| {
            if entry.proves_trusted(std::slice::from_ref(requirement)) { return true; }

            let condition = requirement.substitute(&|place|
                place.value_in(&entry.arguments).unwrap_or(ExecutionCondition::Unknown), &ExecutionCondition::Unknown,
                &mut { ExecutionCondition::WORK_LIMIT }).with_equalities(&equalities);

            let ExecutionCondition::Predicate(predicate, substitution, arguments) = condition else { return false; };

            let [pointer, count] = arguments.as_ref() else { return false; };

            if !self.spatial_predicates.contains(&predicate) { return false; }

            entry.trusted_assumptions.iter().take(ExecutionCondition::WORK_LIMIT).any(|(known, value)| {
                if !value { return false; }

                let known = known.with_equalities(&equalities);

                let ExecutionCondition::Predicate(known_predicate, known_substitution, known_arguments) = known else { return false; };

                let [known_pointer, known_count] = known_arguments.as_ref() else { return false; };

                predicate == known_predicate && substitution == known_substitution && pointer == known_pointer
                    && range_count_follows(count, known_count, &ordinary)
            })
        })
    }

    pub(super) fn complete_trusted_cleanup(
        &self,
        state: &mut ExecutionState,
        place: Option<crate::ExecutionPlace>,
        contract: &crate::TrustedCallContract,
        entry: &crate::ExecutionCallEvidence,
    ) {
        let Some(place) = place else {
            return;
        };

        // Cleanup keeps the receiver's owner, but its fields describe a new initialized state.
        for value in entry.arguments.values() {
            state.assumptions.retain(|(condition, _)| !condition.observes(value));
        }

        let root = ExecutionCondition::Input(place.clone());

        state.assign(place, root.clone());

        let arguments = entry.arguments.keys().map(|input| {
            let mut value = root.clone();

            for field in input.fields.iter() {
                value = ExecutionCondition::field(*field, value);
            }

            (input.clone(), value)
        }).collect::<BTreeMap<_, _>>();

        for (conditions, trusted) in [(&contract.guarantees, true), (&contract.postconditions, false)] {
            for condition in conditions {
                let condition = condition.substitute(&|input|
                    input.value_in(&arguments).unwrap_or(ExecutionCondition::Unknown), &ExecutionCondition::Unknown,
                    &mut { ExecutionCondition::WORK_LIMIT });

                if trusted { condition.assume(true, &mut state.trusted_assumptions); }
                else { condition.assume(true, &mut state.assumptions); }
            }
        }
    }

    pub(super) fn expire_trusted_scope(&self, state: &mut ExecutionState, scope: bray_bound_tree::BoundBlockId) {
        let Some(owners) = &self.owners else {
            return;
        };

        let expired = self.storage.bindings().iter().filter_map(|(target, binding)| {
            let identity = match binding {
                bray_bound_tree::StorageBinding::Identity(identity) => *identity,
                bray_bound_tree::StorageBinding::Access(access) => self.storage.root_identity(*access)?,
            };

            if owners.identity_scope(self.storage, identity) != Some(scope) {
                return None;
            }

            crate::execution_guarantees::storage_binding_reference(*target).map(crate::ExecutionPlace::from)
        }).collect::<Vec<_>>();

        let values = expired.iter().filter_map(|place| place.value_in(&state.current)).collect::<Vec<_>>();

        for place in &expired {
            state.invalidate_trusted_place(place);
        }

        state.current.retain(|place, _| !expired.iter().any(|expired| expired.contains(place)));

        let mut pending = values;

        while let Some(value) = pending.pop() {
            if state.result.contains_value(&value) || state.current.values().chain(state.pending_results.values())
                .any(|live| live.contains_value(&value)) {
                continue;
            }

            if let ExecutionCondition::Constructed(_, components) = value {
                pending.extend(components.iter().map(|(_, value)| value.clone()));

                continue;
            }

            state.invalidate_trusted(&value);
        }
    }

    pub(super) fn complete_trusted_await(
        &self,
        state: &mut ExecutionState,
        edge: &super::super::model::AnalysisEdge,
    ) {
        use bray_bound_tree::BoundExpression;
        use super::super::model::{AnalysisEdgeKind, AnalysisOperationKind, AnalysisSuspensionKind};

        if edge.kind() != AnalysisEdgeKind::Resume { return; }

        let suspended = self.graph.block(edge.source()).expect("resume edge has a suspended source block");

        let suspended_from = suspended.predecessors().iter().filter_map(|id| self.graph.edge(*id))
            .find(|incoming| incoming.kind() == AnalysisEdgeKind::Suspension);

        let mut completions = Vec::new();

        if let Some(block) = suspended_from.and_then(|edge| self.graph.block(edge.source())) {
            for operation in block.operations().iter().filter_map(|id| self.graph.operation(*id)) {
                if let AnalysisOperationKind::Suspension { expression, kind: AnalysisSuspensionKind::Await } = operation.kind()
                    && let Some(BoundExpression::Await(awaited)) = self.request.view().expression(expression)
                    && let ExecutionCondition::Expression(source) = self.value(state, awaited.operand()) {
                    if let Some(entry) = state.entries.get_mut(&source) {
                        // Only evidence still valid when execution begins can justify this completion.
                        entry.pending_execution = false;
                    }

                    completions.push((source, expression));
                }
            }
        }

        // A resumed body can have executed opaque code and changed owner epochs.
        state.invalidate_cleanup();

        for (source, expression) in completions {
            self.complete_trusted_call(state, source, expression);
        }
    }

    pub(super) fn complete_trusted_call(&self, state: &mut ExecutionState, expression: BoundExpressionId, result_expression: BoundExpressionId) {
        let Some(contract) = self.request.trusted_contracts().and_then(|contracts| contracts.calls.get(&expression)) else {
            return;
        };

        let Some(entry) = state.entries.get(&expression).filter(|entry| (entry.trusted_boundary || self.trusted_requirements_proven(entry, &contract.requirements))
            && entry.proves(&contract.preconditions)).cloned() else {
            return;
        };

        if !contract.completes && expression == result_expression {
            return;
        }

        let result = ExecutionCondition::Expression(result_expression);
        let mut arguments = entry.arguments;
        let mut carriers = Vec::new();
        let mut owners = BTreeSet::new();

        if !contract.guarantees.is_empty()
            && let Some(bray_bound_tree::SemanticSelection::Call(call)) = self.semantics.selections().expression(expression) {
            for argument in call.arguments() {
                let bray_bound_tree::SelectedArgument::Explicit { expression: operand, ordinal, parameter, conversion, .. } = argument else { continue; };

                if !matches!(self.request.semantic_values().type_data(conversion.target_type()).as_ref(), bray_symbols::TypeData::Borrow { .. }) { continue; }

                let Some(place) = crate::execution_guarantees::expression_place(self.request.unit(), self.semantics, *operand) else {
                    // Indexed storage has no scalar field path. Its enclosing owner still bounds
                    // the lifetime of a returned address or capability.
                    for access in self.storage.access_plans().iter().filter(|plan| plan.expression() == *operand) {
                        if let Some(owner) = crate::ExecutionPlace::storage_owner(self.storage, access.access()) {
                            owners.insert(owner);
                        }
                    }

                    continue;
                };

                let reference = parameter.map(|parameter| bray_bound_tree::BoundReferenceTarget::Surface(parameter.into()))
                    .or_else(|| place.reference()).expect("borrowed call input retains its binding identity");

                owners.insert(place.clone());

                let preserves = trusted_call_preserves_inputs(call);

                let value = if preserves {
                    self.value(state, *operand)
                } else { ExecutionCondition::PostState(expression, reference) };

                if !preserves { state.assign(place, value.clone()); }
                carriers.push(value.clone());
                if let Some(parameter) = parameter { arguments.insert(bray_bound_tree::BoundReferenceTarget::Surface((*parameter).into()).into(), value.clone()); }

                let ordinal = ordinal.checked_add(u32::from(call.receiver().is_some())).expect("selected argument order must fit");

                arguments.insert(crate::ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(ordinal)), value);
            }

            if let Some(receiver) = call.receiver()
                && matches!(receiver.mode(), bray_symbols::ReceiverMode::Shared | bray_symbols::ReceiverMode::Mutable)
                && let Some(place) = crate::execution_guarantees::expression_place(self.request.unit(), self.semantics, receiver.expression()) {
                let reference = bray_bound_tree::BoundReferenceTarget::Surface(receiver.parameter().into());

                owners.insert(place.clone());

                let preserves = trusted_call_preserves_inputs(call);

                let value = if preserves {
                    self.value(state, receiver.expression())
                } else { ExecutionCondition::PostState(expression, reference) };

                if !preserves { state.assign(place, value.clone()); }
                carriers.push(value.clone());
                arguments.insert(reference.into(), value.clone());
                arguments.insert(crate::ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0)), value);
            }
        }

        for (guarantee, subject) in &contract.witness_subjects {
            let guarantee = guarantee.substitute(&|place|
                place.value_in(&arguments).unwrap_or(ExecutionCondition::Unknown), &result,
                &mut { ExecutionCondition::WORK_LIMIT });

            // A guarantee discharged solely by its ordinary guard carries no trusted authority.
            if guarantee.prove(&state.assumptions, &mut { ExecutionCondition::WORK_LIMIT }) == Some(true) { continue; }

            let condition = subject.substitute(&|place|
                place.value_in(&arguments).unwrap_or(ExecutionCondition::Unknown), &result,
                &mut { ExecutionCondition::WORK_LIMIT });

            if condition == ExecutionCondition::Unknown { continue; }

            if contract.result_is_witness && condition.observes(&result) {
                state.witness_carriers.insert(result.clone());
            }

            for carrier in &carriers {
                if *carrier != ExecutionCondition::Unknown && condition.observes(carrier) {
                    state.witness_carriers.insert(carrier.clone());
                }
            }

        }

        for guarantee in &contract.guarantees {
            guarantee.substitute(&|place| place.value_in(&arguments).unwrap_or(ExecutionCondition::Unknown),
                &result, &mut { ExecutionCondition::WORK_LIMIT }).assume(true, &mut state.trusted_assumptions);
        }

        if !contract.guarantees.is_empty() && !owners.is_empty() {
            let dependencies = state.witness_dependencies.entry(result.clone()).or_default();

            // The ordinary dependency checker validates the reached storage and borrow capability.
            // Keep those same input owners attached to the trusted conditions on the returned view.
            dependencies.extend(owners);
        }

        for postcondition in &contract.postconditions {
            let condition = postcondition.substitute(&|place|
                place.value_in(&arguments).unwrap_or(ExecutionCondition::Unknown), &result,
                &mut { ExecutionCondition::WORK_LIMIT });

            condition.assume(true, &mut state.assumptions);
        }

        state.expressions.insert(result_expression, result);
    }
}

fn range_count_follows(count: &ExecutionCondition, extent: &ExecutionCondition,
    assumptions: &BTreeSet<(ExecutionCondition, bool)>) -> bool {
    use bray_bound_tree::BoundOperator;

    if ExecutionCondition::operation(BoundOperator::LessEqual, vec![count.clone(), extent.clone()])
        .prove(assumptions, &mut { ExecutionCondition::WORK_LIMIT }) == Some(true) { return true; }

    let ExecutionCondition::Literal(value) = count else { return false; };

    let bray_symbols::ConstantValueKind::Integer(integer) = value.kind() else { return false; };

    if integer.to_u64() != Some(1) { return false; }

    if let ExecutionCondition::Operation(BoundOperator::Subtract, operands) = extent
        && let [end, start] = operands.as_ref() {
        return ExecutionCondition::operation(BoundOperator::Less, vec![start.clone(), end.clone()])
            .prove(assumptions, &mut { ExecutionCondition::WORK_LIMIT }) == Some(true)
            || assumptions.contains(&(ExecutionCondition::operation(BoundOperator::GreaterEqual,
                vec![start.clone(), end.clone()]), false));
    }

    assumptions.iter().any(|(condition, holds)| {
        if !holds { return false; }

        let ExecutionCondition::Operation(operator, operands) = condition else { return false; };

        let [left, right] = operands.as_ref() else { return false; };

        let lower = match operator {
            BoundOperator::Less if right == extent => left,
            BoundOperator::Greater if left == extent => right,
            _ => return false,
        };

        matches!(lower, ExecutionCondition::Literal(value)
            if matches!(value.kind(), bray_symbols::ConstantValueKind::Integer(integer) if integer.is_zero()))
    })
}

fn trusted_call_preserves_inputs(call: &bray_bound_tree::SelectedCall) -> bool {
    call.phase_behaviors().invocation().execution_properties().contains(&bray_symbols::ExecutionProperty::Pure)
        || matches!(call.implementation_hook(), Some(
            bray_compiler_known::ImplementationHook::AddressOf
            | bray_compiler_known::ImplementationHook::AddressOfMut
            | bray_compiler_known::ImplementationHook::RawBufferCapacity
            | bray_compiler_known::ImplementationHook::RawBufferInitializedCount
            | bray_compiler_known::ImplementationHook::RawBufferPointer
            | bray_compiler_known::ImplementationHook::RawBufferSparePointer))
}
