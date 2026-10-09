use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{AnyBoundNodeId, BoundExpressionId, CheckedExpressionSemantics, StoragePlan};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticExpressionCategory, DiagnosticKind,
    DiagnosticLabel, DiagnosticLabelKind, DiagnosticNote, DiagnosticNoteKind, SeverityKind,
};

use super::super::flow::{ExecutionFlow, analyze_execution_flow};
use super::super::state::ExecutionState;
use crate::{CheckerOutcome, CheckerRequestContext, CheckerUnitView, ExecutionCondition};

pub(crate) fn collect_trusted_memory_evidence<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    expressions: &CheckedExpressionSemantics,
    storage: &StoragePlan,
    graph: &super::super::super::model::ControlFlowGraph,
) -> CheckerOutcome<BTreeMap<BoundExpressionId, bool>, C::UpstreamError> {
    let Some(contracts) = request.trusted_contracts() else {
        return CheckerOutcome::complete(BTreeMap::new(), DiagnosticBag::new());
    };

    if contracts.requirements.is_empty()
        && contracts.guarantees.is_empty()
        && contracts.calls.is_empty()
    {
        return CheckerOutcome::complete(BTreeMap::new(), DiagnosticBag::new());
    }

    let literals =
        crate::execution_guarantees::condition_literals(expressions, request.semantic_values());

    let completion = BTreeMap::new();

    let flow = match analyze_execution_flow(
        graph,
        request,
        expressions,
        &contracts.requirements,
        &literals,
        storage,
        &completion,
        None,
    ) {
        CheckerOutcome::Complete(flow) => flow.into_parts().0,
        CheckerOutcome::Cancelled => return CheckerOutcome::Cancelled,
        CheckerOutcome::InfrastructureFailure(error) => {
            return CheckerOutcome::InfrastructureFailure(error);
        }
        CheckerOutcome::UpstreamFailure(error) => return CheckerOutcome::UpstreamFailure(error),
    };

    check_trusted_contracts_in_flow(request, expressions, storage, &flow, false)
}

fn check_trusted_contracts_in_flow<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    expressions: &CheckedExpressionSemantics,
    storage: &StoragePlan,
    flow: &ExecutionFlow<'_, '_, C>,
    final_check: bool,
) -> CheckerOutcome<BTreeMap<BoundExpressionId, bool>, C::UpstreamError> {
    let Some(contracts) = request.trusted_contracts() else {
        return CheckerOutcome::without_diagnostics(BTreeMap::new());
    };

    if contracts.requirements.is_empty()
        && contracts.guarantees.is_empty()
        && contracts.calls.is_empty()
    {
        return CheckerOutcome::without_diagnostics(BTreeMap::new());
    }

    let graph = flow.domain.graph;
    let mut diagnostics = DiagnosticBag::new();
    let mut proven = BTreeMap::new();
    let mut failed = BTreeSet::new();
    let mut transfers = BTreeSet::new();

    for block in graph.blocks() {
        let Some(Some(entry)) = flow.states.state(block.id()) else {
            continue;
        };

        // Replay against the converged entry state to publish only live caller evidence.
        let mut state = entry.clone();

        for operation in block
            .operations()
            .iter()
            .filter_map(|id| graph.operation(*id))
        {
            if final_check {
                check_witness_transfers(
                    request,
                    storage,
                    flow,
                    &state,
                    operation.kind().occurrence(),
                    &contracts.required_witnesses,
                    &mut transfers,
                );
            }

            flow.domain.operation(&mut state, operation.kind());

            let AnyBoundNodeId::Expression(expression) = operation.kind().node() else {
                continue;
            };

            if let Some(bray_bound_tree::BoundExpression::Structured(bound)) =
                request.view().expression(expression)
                && bound.kind() == bray_bound_tree::BoundStructuredExpressionKind::RepeatedArray
                && let [value, count] = bound.operands()
                && state.is_witness(&flow.domain.value(&state, *value))
            {
                let count = flow.domain.value(&state, *count);

                let singleton = matches!(count, ExecutionCondition::Literal(count)
                    if matches!(count.kind(), bray_symbols::ConstantValueKind::Integer(count) if count.to_u64().is_some_and(|count| count <= 1)));

                if !singleton {
                    transfers.insert(expression);
                }
            }

            let invocation = match operation.kind() {
                super::super::super::model::AnalysisOperationKind::Call { invocation, .. } => {
                    invocation
                }
                _ => expression.into(),
            };

            let Some(contract) = contracts.calls.get(&invocation) else {
                continue;
            };

            if let Some(evidence) = state.entries.get(&invocation) {
                if flow
                    .domain
                    .trusted_requirements_proven(evidence, &contract.requirements)
                {
                    let empty_copy = flow.domain.empty_memory_copy(&state, invocation);

                    // A copy is empty only when every reachable entry proves that extent.
                    proven
                        .entry(expression)
                        .and_modify(|empty| *empty &= empty_copy)
                        .or_insert(empty_copy);
                } else {
                    failed.insert(expression);
                }
            }
        }
    }

    if !final_check {
        // Raw storage checking needs preliminary predicate evidence before selecting cleanup.
        // Publish diagnostics only from the final analysis with actual lifecycle effects.
        return CheckerOutcome::without_diagnostics(proven);
    }

    let boundary = super::super::super::fixed_point::FixedPointDomain::boundary(&flow.domain);

    let guarantees = boundary
        .as_ref()
        .map(|boundary| {
            contracts
                .guarantees
                .iter()
                .map(|(condition, span)| {
                    let condition = condition.capture_entry(
                        &|place| {
                            place
                                .value_in(&boundary.current)
                                .unwrap_or_else(|| ExecutionCondition::Input(place.clone()))
                        },
                        &boundary.trusted_assumptions,
                        &boundary.assumptions,
                        &mut { ExecutionCondition::WORK_LIMIT },
                    );

                    (condition, *span)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    for span in super::super::postcondition::unproven_postconditions(flow, &guarantees) {
        diagnostics.add(
            Diagnostic::new(
                bray_diagnostics::DiagnosticId::new(span.start().bytes()),
                DiagnosticKind::CheckingTrustedObligationNotProven,
                SeverityKind::Error,
            )
            .with_primary_span(span)
            .with_label(DiagnosticLabel::primary(
                DiagnosticLabelKind::TrustedObligationFailure,
                span,
            ))
            .with_arg(DiagnosticArg::expression_category(
                DiagnosticExpressionCategory::Call,
            ))
            .with_note(DiagnosticNote::new(
                DiagnosticNoteKind::TrustedObligationEvidenceRequired,
            )),
        );
    }

    for expression in transfers {
        diagnostics.add(trusted_diagnostic(
            request,
            expressions,
            expression.into(),
            DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
        ));
    }

    for expression in failed {
        proven.remove(&expression);

        diagnostics.add(trusted_diagnostic(
            request,
            expressions,
            expression.into(),
            DiagnosticKind::CheckingTrustedObligationNotProven,
        ));
    }

    CheckerOutcome::complete(proven, diagnostics)
}

fn check_witness_transfers<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    flow: &super::super::flow::ExecutionFlow<'_, '_, C>,
    state: &ExecutionState,
    occurrence: bray_bound_tree::BoundExecutionSite,
    requirements: &[(ExecutionCondition, crate::ExecutionPlace)],
    transfers: &mut BTreeSet<BoundExpressionId>,
) {
    for plan in storage.occurrence_plans(occurrence).filter(|plan| {
        matches!(
            plan.purpose(),
            bray_bound_tree::StorageAccessPurpose::Copy
                | bray_bound_tree::StorageAccessPurpose::ValueTransfer
                | bray_bound_tree::StorageAccessPurpose::Move
        )
    }) {
        let value = flow
            .domain
            .storage_value(&state, plan.expression(), plan.access());

        if plan.purpose() == bray_bound_tree::StorageAccessPurpose::ValueTransfer
            && storage
                .root_identity(plan.access())
                .and_then(|identity| storage.identity(identity))
                .is_some_and(|identity| {
                    matches!(
                        identity,
                        bray_bound_tree::StorageIdentity::Temporary(_)
                            | bray_bound_tree::StorageIdentity::Result(_)
                            | bray_bound_tree::StorageIdentity::Allocation(_)
                    )
                })
        {
            // A fresh result or its destructured components move into their first destination.
            continue;
        }

        let mut is_witness = state.is_witness(&value);

        let mut partial_witness =
            matches!(&value, ExecutionCondition::Projection(_, base) if state.is_witness(base));

        for (condition, subject) in requirements {
            let condition = condition.substitute(
                &|place| {
                    place
                        .value_in(&state.current)
                        .unwrap_or_else(|| ExecutionCondition::Input(place.clone()))
                },
                &ExecutionCondition::Unknown,
                &mut { ExecutionCondition::WORK_LIMIT },
            );

            if condition
                .prove_trusted(&state.trusted_assumptions, &state.assumptions)
                .is_none()
            {
                continue;
            }

            let subject = subject
                .value_in(&state.current)
                .unwrap_or_else(|| ExecutionCondition::Input(subject.clone()));

            is_witness |= subject == value
                || matches!((&subject, &value),
                        (ExecutionCondition::Input(subject), ExecutionCondition::Input(copied)) if copied.contains(subject));

            partial_witness |= subject != value
                && matches!((&subject, &value),
                        (ExecutionCondition::Input(subject), ExecutionCondition::Input(part)) if subject.contains(part));
        }

        if !is_witness && !partial_witness {
            continue;
        }

        let ty = storage
            .access(plan.access())
            .expect("checked access plan references existing storage")
            .reached_type();

        if matches!(
            request.semantic_values().type_data(ty).as_ref(),
            bray_symbols::TypeData::Borrow { .. }
        ) {
            continue;
        }

        let copies = plan.purpose() != bray_bound_tree::StorageAccessPurpose::Move
            && flow.domain.copied_types.contains(&ty);

        if (is_witness && copies) || (partial_witness && !copies) {
            transfers.insert(plan.expression());
        }
    }
}

pub(crate) fn check_trusted_completion<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    expressions: &CheckedExpressionSemantics,
    storage: &StoragePlan,
    cleanup: &bray_bound_tree::CheckedAsync,
    graph: &super::super::super::model::ControlFlowGraph,
) -> CheckerOutcome<(), C::UpstreamError> {
    use bray_symbols::{TypeAssociatedLifecycleSlot, TypeData};

    let mut diagnostics = DiagnosticBag::new();

    // Predicate clauses describe entry and completion; they do not execute owner cleanup.
    let accesses = if request.unit().key().kind() == bray_bound_tree::BoundUnitKind::ContractClause
    {
        BTreeSet::new()
    } else {
        cleanup
            .scope_exits()
            .iter()
            .flat_map(|plan| plan.lifecycle_resolution().iter().copied())
            .chain(cleanup.replacements().iter().map(|plan| plan.access()))
            .collect::<BTreeSet<_>>()
    };

    let mut contracts = BTreeMap::new();

    macro_rules! query {
        ($query:expr) => {
            match $query {
                Ok(value) => value,
                Err(crate::CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
                Err(crate::CheckerQueryError::Infrastructure(error)) => {
                    return CheckerOutcome::InfrastructureFailure(error)
                }
                Err(crate::CheckerQueryError::Upstream(error)) => {
                    return CheckerOutcome::UpstreamFailure(error)
                }
            }
        };
    }

    let mut parts = BTreeMap::new();

    for access in accesses {
        let reached = storage
            .access(access)
            .expect("checked cleanup retains its storage access");

        let selected = storage
            .root_identity(access)
            .and_then(|identity| cleanup.storage_requirement(identity))
            .and_then(|requirement| requirement.parts());

        let expanded = query!(crate::asynchronous::full_cleanup_parts(
            request,
            reached.reached_type(),
            reached.source(),
            selected
        ));

        diagnostics.add_range(expanded.diagnostics().iter().cloned());
        parts.insert(access, expanded.into_parts().0);
    }

    let types = parts
        .values()
        .flat_map(|parts| parts.iter().map(|(ty, _)| *ty))
        .collect::<BTreeSet<_>>();

    for ty in types {
        if !matches!(
            request.semantic_values().type_data(ty).as_ref(),
            TypeData::Named { .. }
        ) {
            continue;
        }

        for slot in [
            TypeAssociatedLifecycleSlot::Finalizer,
            TypeAssociatedLifecycleSlot::Destructor,
        ] {
            let selected = query!(request.context().lifecycle_callable(ty, slot));

            diagnostics.add_range(selected.diagnostics().iter().cloned());

            let Some((_, signature)) = selected.value() else {
                continue;
            };

            let contract = match cleanup_contract(request.semantic_values(), signature) {
                Ok(contract) => contract,
                Err(error) => {
                    return CheckerOutcome::InfrastructureFailure(
                        crate::CheckerInfrastructureError::SemanticValueStore(error),
                    );
                }
            };

            // Even a lifecycle with no predicates can invalidate the next lifecycle's evidence.
            contracts.insert((ty, slot), contract);
        }
    }

    if contracts.is_empty()
        && request.trusted_contracts().is_none_or(|contracts| {
            contracts.requirements.is_empty()
                && contracts.guarantees.is_empty()
                && contracts.calls.is_empty()
        })
    {
        return CheckerOutcome::complete((), diagnostics);
    }

    let literals =
        crate::execution_guarantees::condition_literals(expressions, request.semantic_values());

    let completion = BTreeMap::new();

    let requirements = request
        .trusted_contracts()
        .map_or(&[][..], |contracts| contracts.requirements.as_slice());

    let flow = match analyze_execution_flow(
        graph,
        request,
        expressions,
        requirements,
        &literals,
        storage,
        &completion,
        Some(cleanup),
    ) {
        CheckerOutcome::Complete(flow) => {
            let (flow, additional) = flow.into_parts();

            diagnostics.add_range(additional);

            flow
        }
        CheckerOutcome::Cancelled => return CheckerOutcome::Cancelled,
        CheckerOutcome::InfrastructureFailure(error) => {
            return CheckerOutcome::InfrastructureFailure(error);
        }
        CheckerOutcome::UpstreamFailure(error) => return CheckerOutcome::UpstreamFailure(error),
    };

    if !contracts.is_empty() {
        let mut failed = BTreeSet::new();

        for block in graph.blocks() {
            let Some(Some(entry)) = flow.states.state(block.id()) else {
                continue;
            };

            let mut state = entry.clone();

            for operation in block
                .operations()
                .iter()
                .filter_map(|id| graph.operation(*id))
            {
                let targets = cleanup_targets(cleanup, operation.kind());

                for access in targets {
                    let preserves_inputs = flow.domain.cleanup_preserves_inputs(&state, access);

                    for (ty, part) in parts
                        .get(&access)
                        .expect("cleanup access has its selected expansion")
                    {
                        let place = part.as_ref().and_then(|part| {
                            crate::ExecutionPlace::cleanup_part(storage, access, part)
                        });

                        for slot in [
                            TypeAssociatedLifecycleSlot::Finalizer,
                            TypeAssociatedLifecycleSlot::Destructor,
                        ] {
                            let Some(contract) = contracts.get(&(*ty, slot)) else {
                                continue;
                            };

                            let evidence = query!(flow.cleanup_place_entry(
                                &state,
                                place.clone(),
                                *ty,
                                slot,
                                false
                            ));

                            let established = evidence.as_ref().is_some_and(|(_, _, evidence)| {
                                flow.domain
                                    .trusted_requirements_proven(evidence, &contract.requirements)
                                    && evidence.proves(&contract.preconditions)
                            });

                            if !established && !contract.requirements.is_empty() {
                                failed.insert(operation.kind().node());
                            }

                            if !preserves_inputs {
                                state.invalidate_cleanup(None);
                            }

                            if established
                                && contract.completes
                                && let Some((_, _, evidence)) = evidence
                            {
                                flow.domain.complete_trusted_cleanup(
                                    &mut state,
                                    place.clone(),
                                    contract,
                                    &evidence,
                                );
                            }
                        }

                        if !preserves_inputs {
                            state.invalidate_cleanup(None);
                        }
                    }
                }

                flow.domain.operation(&mut state, operation.kind());
            }
        }

        for node in failed {
            diagnostics.add(trusted_diagnostic(
                request,
                expressions,
                node,
                DiagnosticKind::CheckingTrustedObligationNotProven,
            ));
        }
    }

    match check_trusted_contracts_in_flow(request, expressions, storage, &flow, true) {
        CheckerOutcome::Complete(result) => diagnostics.add_range(result.into_parts().1),
        CheckerOutcome::Cancelled => return CheckerOutcome::Cancelled,
        CheckerOutcome::InfrastructureFailure(error) => {
            return CheckerOutcome::InfrastructureFailure(error);
        }
        CheckerOutcome::UpstreamFailure(error) => return CheckerOutcome::UpstreamFailure(error),
    }

    CheckerOutcome::complete((), diagnostics)
}

fn cleanup_targets(
    cleanup: &bray_bound_tree::CheckedAsync,
    operation: super::super::super::model::AnalysisOperationKind,
) -> Vec<bray_bound_tree::StorageAccessId> {
    match operation {
        super::super::super::model::AnalysisOperationKind::ScopeExit {
            block,
            exit,
            phase: super::super::super::model::AnalysisScopeExitPhase::LifecycleResolution,
        } => cleanup
            .scope_exit_plan(block, exit)
            .into_iter()
            .flat_map(|plan| plan.lifecycle_resolution().iter().copied())
            .collect::<Vec<_>>(),
        super::super::super::model::AnalysisOperationKind::Bound(AnyBoundNodeId::Expression(
            expression,
        )) => cleanup
            .replacement(expression)
            .into_iter()
            .map(|plan| plan.access())
            .collect(),
        _ => Vec::new(),
    }
}

fn cleanup_contract(
    values: &bray_symbols::SemanticValueStore,
    signature: &bray_symbols::CallableSignature,
) -> Result<crate::TrustedCallContract, bray_symbols::SemanticValueStoreError> {
    use bray_symbols::TypeData;

    let data = values.type_data(signature.callable_type());

    let TypeData::Callable(callable) = data.as_ref() else {
        panic!("selected lifecycle must have a callable type");
    };

    let references = signature
        .receiver()
        .map(|receiver| bray_bound_tree::BoundReferenceTarget::Surface(receiver.parameter().into()))
        .into_iter()
        .chain(signature.parameters().iter().map(|parameter| {
            bray_bound_tree::BoundReferenceTarget::Surface(parameter.parameter().into())
        }))
        .collect::<Vec<_>>();

    let behavior = callable.phase_behaviors().invocation();

    let completion = callable
        .phase_behaviors()
        .deferred_execution()
        .unwrap_or(behavior);

    let mut contract = crate::TrustedCallContract {
        // Failed finalizer attempts can still be followed by destruction.
        completes: callable.execution() == bray_symbols::CallableExecution::Synchronous
            && behavior
                .execution_properties()
                .contains(&bray_symbols::ExecutionProperty::Total),
        ..Default::default()
    };

    for (predicates, completion) in [
        (behavior.predicate_requirements(), false),
        (completion.predicate_guarantees(), true),
    ] {
        for predicate in predicates {
            let condition = match predicate
                .condition()
                .map(|term| crate::execution_condition_from_term(values, term, &references))
                .transpose()
            {
                Ok(condition) => condition.unwrap_or(ExecutionCondition::Unknown),
                Err(error) => return Err(error),
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

    Ok(contract)
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
        crate::diagnostic::expression_category(
            request
                .unit()
                .tree()
                .expression(expression)
                .expect("trusted call expression belongs to the checked unit"),
        )
    } else {
        DiagnosticExpressionCategory::ControlFlow
    };

    let note = if kind == DiagnosticKind::CheckingTrustedWitnessTransferNotProven {
        DiagnosticNoteKind::TrustedWitnessTransferRequired
    } else {
        DiagnosticNoteKind::TrustedObligationEvidenceRequired
    };

    let mut diagnostic = Diagnostic::new(
        bray_diagnostics::DiagnosticId::new(span.start().bytes()),
        kind,
        SeverityKind::Error,
    )
    .with_primary_span(span)
    .with_arg(DiagnosticArg::expression_category(category))
    .with_label(DiagnosticLabel::primary(
        DiagnosticLabelKind::TrustedObligationFailure,
        span,
    ))
    .with_note(DiagnosticNote::new(note));

    if let AnyBoundNodeId::Expression(expression) = node
        && let Some(bray_bound_tree::SemanticSelection::Call(call)) =
            expressions.selections().expression(expression)
        && let bray_bound_tree::BoundCallableTarget::Declaration(callable) = call.target()
        && let Some(name) = request
            .context()
            .symbols()
            .member_name(callable.definition().symbol())
    {
        diagnostic = diagnostic.with_label(
            DiagnosticLabel::secondary(DiagnosticLabelKind::TrustedCallable, span)
                .with_arg(DiagnosticArg::declaration_name(name.as_str())),
        );
    }

    diagnostic
}
