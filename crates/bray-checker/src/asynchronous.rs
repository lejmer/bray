use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AsyncScopeExitPlan, AsyncSuspensionPoint, AsyncTaskOperation, AsyncTaskOperationKind,
    BodyBehaviorCall, BodyBehaviorPhase, BoundBlock, BoundBlockItem, BoundCallResult,
    BoundCallableTarget, BoundDependencyContractId, BoundDependencyGuard,
    BoundDependencyRequirement, BoundDependencySubject, BoundExpression, BoundExpressionId,
    CheckedAsyncFacts, CheckedDependencyContracts, CheckedExpressionTypes,
    CheckedSemanticSelections, LivenessFacts, SemanticSelection, StorageFlowFacts,
};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticKind, SeverityKind};
use bray_symbols::{
    AnyLocalSymbolId, CallableExecution, CallableSignatureFact, TypeData, TypeExpressionTemplate,
};

use crate::analysis::{
    AnalysisOperationKind, AnalysisTaskOperationKind, ControlFlowGraphBuildOutcome,
    build_control_flow_graph,
};
use crate::diagnostic::{diagnostic_id, expression_span};
use crate::{
    CheckerFactError, CheckerInfrastructureError, CheckerOutcome, CheckerRequestContext,
    CheckerSemanticFactProvider, CheckerUnitView,
};

pub(crate) fn check_async_facts<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    selections: &CheckedSemanticSelections,
    liveness: &LivenessFacts,
    dependencies: &CheckedDependencyContracts,
    flow: &StorageFlowFacts,
) -> CheckerOutcome<CheckedAsyncFacts>
where
    C: CheckerRequestContext + CheckerSemanticFactProvider<CallableSignatureFact> + ?Sized,
{
    if request.is_cancelled() {
        return CheckerOutcome::Cancelled;
    }

    if !inputs_match(request, types, selections, liveness, dependencies, flow) {
        return CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidStorageFlowFacts,
        );
    }

    let graph = match build_control_flow_graph(request) {
        ControlFlowGraphBuildOutcome::Complete(graph) => graph,
        ControlFlowGraphBuildOutcome::Cancelled => return CheckerOutcome::Cancelled,
    };

    let execution = match containing_execution(request) {
        Ok(execution) => execution,
        Err(CheckerFactError::Cancelled) => return CheckerOutcome::Cancelled,
        Err(CheckerFactError::Infrastructure(error)) => {
            return CheckerOutcome::InfrastructureFailure(error);
        }
    };

    let local_initializers = local_initializers(request);
    let mut deferred_calls = BTreeMap::new();
    let mut active = BTreeSet::new();
    let mut diagnostics = DiagnosticBag::new();
    let mut suspensions = Vec::new();
    let mut task_operations = BTreeMap::new();
    let mut frame_dependencies = BTreeSet::new();
    let mut is_recovered = liveness.is_recovered()
        || dependencies.is_recovered()
        || flow.is_recovered()
        || types.is_recovered();

    for operation in graph.operations() {
        if request.is_cancelled() {
            return CheckerOutcome::Cancelled;
        }

        match operation.kind() {
            AnalysisOperationKind::DirectAwait(expression) => {
                let Some(BoundExpression::Await(await_expression)) =
                    request.view().expression(expression)
                else {
                    is_recovered = true;

                    continue;
                };

                if execution == Some(CallableExecution::Synchronous) {
                    let span = match expression_span(request, expression) {
                        Ok(span) => span,
                        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
                    };

                    diagnostics.add(
                        Diagnostic::new(
                            diagnostic_id(diagnostics.len()),
                            DiagnosticKind::CheckingAwaitOutsideAsyncCallable,
                            SeverityKind::Error,
                        )
                        .with_primary_span(span),
                    );
                }

                let calls = expression_deferred_calls(
                    request,
                    selections,
                    types,
                    &local_initializers,
                    await_expression.operand(),
                    &mut deferred_calls,
                    &mut active,
                );

                let dependency_contract = dependencies.expression(await_expression.operand());
                let retained = retained_suspension_subjects(
                    liveness,
                    dependencies,
                    expression,
                    dependency_contract,
                );

                frame_dependencies.extend(retained.iter().copied());

                // TODO(BRA-200): Validate deferred requirements against the exact flow-sensitive
                // storage state at this suspension point.

                let suspension_recovered = request
                    .view()
                    .expression(await_expression.operand())
                    .is_none_or(BoundExpression::is_recovered);

                is_recovered |= suspension_recovered;

                suspensions.push(AsyncSuspensionPoint::new(
                    expression,
                    await_expression.operand(),
                    dependency_contract,
                    calls,
                    retained,
                    suspension_recovered,
                ));
            }
            AnalysisOperationKind::TaskOperation { expression, kind } => {
                let operation = AsyncTaskOperation::new(expression, task_operation_kind(kind));

                if task_operations.insert(expression, operation).is_none()
                    && let Err(error) = add_task_context_diagnostic(
                        request,
                        execution,
                        expression,
                        kind,
                        &mut diagnostics,
                    )
                {
                    return CheckerOutcome::InfrastructureFailure(error);
                }
            }
            AnalysisOperationKind::Recovery(_) => is_recovered = true,
            AnalysisOperationKind::Bound(_) | AnalysisOperationKind::ScopeExit { .. } => {}
        }
    }

    for entry in selections.entries() {
        let Some(analysis_kind) = selected_task_operation(request, entry.selection()) else {
            continue;
        };

        let operation =
            AsyncTaskOperation::new(entry.expression(), task_operation_kind(analysis_kind));

        if task_operations
            .insert(entry.expression(), operation)
            .is_none()
            && let Err(error) = add_task_context_diagnostic(
                request,
                execution,
                entry.expression(),
                analysis_kind,
                &mut diagnostics,
            )
        {
            return CheckerOutcome::InfrastructureFailure(error);
        }
    }

    let scope_exits = flow.exits().iter().map(|exit| {
        // TODO(BRA-200): Replace these conservative roots with unresolved task storage and
        // projection paths ordered by checked task and lifecycle dependencies.
        let cancellation = exit.initialized().iter().rev().copied().collect::<Vec<_>>();
        let lifecycle = exit.initialized().iter().rev().copied().collect::<Vec<_>>();

        AsyncScopeExitPlan::new(exit.scope(), cancellation, lifecycle, exit.is_recovered())
    });

    let facts = match CheckedAsyncFacts::try_new(
        request.unit().unit(),
        request.unit().key().kind(),
        frame_dependencies,
        suspensions,
        task_operations.into_values(),
        scope_exits,
        is_recovered,
    ) {
        Ok(facts) => facts,
        Err(_) => {
            return CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::InvalidStorageFlowFacts,
            );
        }
    };

    CheckerOutcome::complete(facts, diagnostics)
}

fn retained_suspension_subjects(
    liveness: &LivenessFacts,
    dependencies: &CheckedDependencyContracts,
    await_expression: BoundExpressionId,
    dependency_contract: Option<BoundDependencyContractId>,
) -> Vec<BoundDependencySubject> {
    let mut retained = liveness
        .live_across_suspensions()
        .iter()
        .filter(|entry| entry.await_expression() == await_expression)
        .map(|entry| entry.subject())
        .collect::<BTreeSet<_>>();

    if let Some(contract) = dependency_contract.and_then(|id| dependencies.contract(id)) {
        for requirement in contract.requirements() {
            collect_requirement_subjects(requirement, &mut retained);
        }
    }

    retained.into_iter().collect()
}

fn collect_requirement_subjects(
    requirement: &BoundDependencyRequirement,
    subjects: &mut BTreeSet<BoundDependencySubject>,
) {
    let mut pending = vec![requirement];

    while let Some(requirement) = pending.pop() {
        match requirement {
            BoundDependencyRequirement::Direct { subject, .. } => {
                subjects.insert(*subject);
            }
            BoundDependencyRequirement::Guarded(guarded) => {
                subjects.insert(guard_subject(guarded.guard()));
                pending.extend(guarded.requirements());
            }
        }
    }
}

const fn guard_subject(guard: BoundDependencyGuard) -> BoundDependencySubject {
    match guard {
        BoundDependencyGuard::NullablePresent(access)
        | BoundDependencyGuard::ActiveUnionVariant { access, .. } => {
            BoundDependencySubject::StorageAccess(access)
        }
        BoundDependencyGuard::BorrowCapabilityActive(capability) => {
            BoundDependencySubject::BorrowCapability(capability)
        }
        BoundDependencyGuard::ScopedCapabilityLive(capability) => {
            BoundDependencySubject::ScopedCapability(capability)
        }
    }
}

fn inputs_match<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    selections: &CheckedSemanticSelections,
    liveness: &LivenessFacts,
    dependencies: &CheckedDependencyContracts,
    flow: &StorageFlowFacts,
) -> bool
where
    C: CheckerRequestContext + ?Sized,
{
    let unit = request.unit().unit();
    let kind = request.unit().key().kind();

    types.unit() == unit
        && types.kind() == kind
        && selections.unit() == unit
        && selections.kind() == kind
        && liveness.unit() == unit
        && liveness.kind() == kind
        && dependencies.unit() == unit
        && dependencies.kind() == kind
        && flow.unit() == unit
        && flow.kind() == kind
}

fn containing_execution<C>(
    request: CheckerUnitView<'_, C>,
) -> Result<Option<CallableExecution>, CheckerFactError>
where
    C: CheckerRequestContext + CheckerSemanticFactProvider<CallableSignatureFact> + ?Sized,
{
    if let crate::SemanticUnitContext::AnonymousCallable(context) = request.semantic_context() {
        return Ok(Some(context.execution()));
    }

    let Some(callable) = request.containing_callable() else {
        return Ok(None);
    };

    let signature = request
        .symbol_fact(bray_symbols::SymbolFactRequest::<CallableSignatureFact>::new(callable))?;

    let execution = match signature.value().callable_type() {
        TypeExpressionTemplate::Callable(callable) => callable.execution(),
        TypeExpressionTemplate::Resolved(ty) => {
            let data = request.semantic_values().type_data(*ty).map_err(|_| {
                CheckerFactError::Infrastructure(
                    CheckerInfrastructureError::SemanticValueUnavailable,
                )
            })?;

            let TypeData::Callable(callable) = data.as_ref() else {
                return Err(CheckerFactError::Infrastructure(
                    CheckerInfrastructureError::InvalidSemanticSelectionInput,
                ));
            };

            callable.execution()
        }
        _ => {
            return Err(CheckerFactError::Infrastructure(
                CheckerInfrastructureError::InvalidSemanticSelectionInput,
            ));
        }
    };

    Ok(Some(execution))
}

fn local_initializers<C>(
    request: CheckerUnitView<'_, C>,
) -> BTreeMap<AnyLocalSymbolId, BoundExpressionId>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut initializers = BTreeMap::new();

    for (_, block) in request.unit().tree().blocks() {
        collect_block_initializers(block, &mut initializers);
    }

    initializers
}

fn collect_block_initializers(
    block: &BoundBlock,
    initializers: &mut BTreeMap<AnyLocalSymbolId, BoundExpressionId>,
) {
    for item in block.items() {
        match item {
            BoundBlockItem::LocalBinding(binding) => {
                for symbol in binding.bindings() {
                    initializers.insert((*symbol).into(), binding.initializer());
                }
            }
            BoundBlockItem::LocalConstant(constant) => {
                if let Some(symbol) = constant.symbol() {
                    initializers.insert(symbol.into(), constant.initializer());
                }
            }
            BoundBlockItem::Expression(_) => {}
        }
    }
}

fn expression_deferred_calls<C>(
    request: CheckerUnitView<'_, C>,
    selections: &CheckedSemanticSelections,
    types: &CheckedExpressionTypes,
    local_initializers: &BTreeMap<AnyLocalSymbolId, BoundExpressionId>,
    expression: BoundExpressionId,
    memoized: &mut BTreeMap<BoundExpressionId, BTreeSet<BodyBehaviorCall>>,
    active: &mut BTreeSet<BoundExpressionId>,
) -> BTreeSet<BodyBehaviorCall>
where
    C: CheckerRequestContext + ?Sized,
{
    if let Some(calls) = memoized.get(&expression) {
        // The cached set remains available while each caller combines an independent result.
        return calls.clone();
    }

    if !active.insert(expression) {
        return BTreeSet::new();
    }

    let mut calls = BTreeSet::new();

    if let Some(SemanticSelection::Call(call)) = selections.expression(expression)
        && matches!(call.resolution().result(), BoundCallResult::LazyFuture(_))
    {
        let mut contribution =
            BodyBehaviorCall::new(call.target(), BodyBehaviorPhase::DeferredExecution);

        if matches!(call.target(), BoundCallableTarget::Anonymous(_))
            && let Some(unit) = request.anonymous_callable_unit(expression)
        {
            contribution = contribution.with_anonymous_unit(unit);
        }

        calls.insert(contribution);
    }

    if let Some(node) = request.view().expression(expression)
        && is_future_expression(request, types, expression)
    {
        if let BoundExpression::Name(name) = node
            && let bray_bound_tree::BoundReferenceTarget::Local(local) = name.target()
            && let Some(initializer) = local_initializers.get(&local)
        {
            calls.extend(expression_deferred_calls(
                request,
                selections,
                types,
                local_initializers,
                *initializer,
                memoized,
                active,
            ));
        }

        if let BoundExpression::PatternReference(reference) = node
            && let Some(initializer) =
                local_initializers.get(&AnyLocalSymbolId::from(reference.binding()))
        {
            calls.extend(expression_deferred_calls(
                request,
                selections,
                types,
                local_initializers,
                *initializer,
                memoized,
                active,
            ));
        }

        for child in node.child_expressions() {
            calls.extend(expression_deferred_calls(
                request,
                selections,
                types,
                local_initializers,
                child,
                memoized,
                active,
            ));
        }

        for block in node.child_blocks() {
            if let Some(result) = request
                .view()
                .block(block)
                .and_then(|block| block.items().last())
                .and_then(BoundBlockItem::expression)
            {
                calls.extend(expression_deferred_calls(
                    request,
                    selections,
                    types,
                    local_initializers,
                    result,
                    memoized,
                    active,
                ));
            }
        }
    }

    active.remove(&expression);

    // The cached set remains available while this caller takes ownership of its result.
    memoized.insert(expression, calls.clone());

    calls
}

fn is_future_expression<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    expression: BoundExpressionId,
) -> bool
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(result) = types.expression(expression) else {
        return false;
    };

    let Ok(data) = request.semantic_values().type_data(result.ty()) else {
        return false;
    };

    let TypeData::Named { definition, .. } = data.as_ref() else {
        return false;
    };

    let bray_symbols::NamedTypeSymbolId::Struct(definition) = definition else {
        return false;
    };

    request
        .available_compiler_known_symbols()
        .symbol_representation(*definition)
        == Some(RepresentationRole::Future)
}

const fn task_operation_kind(kind: AnalysisTaskOperationKind) -> AsyncTaskOperationKind {
    match kind {
        AnalysisTaskOperationKind::Start => AsyncTaskOperationKind::Start,
        AnalysisTaskOperationKind::Join => AsyncTaskOperationKind::Join,
        AnalysisTaskOperationKind::Cancel => AsyncTaskOperationKind::Cancel,
    }
}

fn selected_task_operation<C>(
    request: CheckerUnitView<'_, C>,
    selection: &SemanticSelection,
) -> Option<AnalysisTaskOperationKind>
where
    C: CheckerRequestContext + ?Sized,
{
    let SemanticSelection::Call(call) = selection else {
        return None;
    };

    let target = call.target().declaration()?;
    let hook = request
        .available_compiler_known_symbols()
        .symbol_implementation(target.symbol())?;

    AnalysisTaskOperationKind::from_implementation_hook(hook)
}

fn add_task_context_diagnostic<C>(
    request: CheckerUnitView<'_, C>,
    execution: Option<CallableExecution>,
    expression: BoundExpressionId,
    kind: AnalysisTaskOperationKind,
    diagnostics: &mut DiagnosticBag,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    if kind != AnalysisTaskOperationKind::Start || execution != Some(CallableExecution::Synchronous)
    {
        return Ok(());
    }

    let span = expression_span(request, expression)?;

    diagnostics.add(
        Diagnostic::new(
            diagnostic_id(diagnostics.len()),
            DiagnosticKind::CheckingTaskStartOutsideAsyncCallable,
            SeverityKind::Error,
        )
        .with_primary_span(span),
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use bray_bound_tree::{
        BoundCallResult, BoundCallableTarget, BoundDependencyRequirement,
        BoundDependencyRequirementKind, BoundDependencySubject, BoundErrorExpression,
        BoundExpression, BoundResolvedCall, BoundUnitId, SelectedCall, SemanticSelection,
    };
    use bray_diagnostics::{DiagnosticBag, DiagnosticKind};
    use bray_symbols::{
        CallableAbi, CallableExecution, TypeCallableMemberSymbolId,
        testing::implementation_instance,
    };

    use super::{
        AnalysisTaskOperationKind, add_task_context_diagnostic, collect_requirement_subjects,
        selected_task_operation,
    };
    use crate::CheckerUnitView;
    use crate::test_support::{
        TestCheckerContext, callable_entry, callable_instance, compiler_known_symbol, error_type,
        expression_unit, push_expression, semantic_values,
    };

    #[test]
    fn selected_compiler_known_calls_publish_task_operation_kinds() {
        let (unit, _) = expression_unit(BoundUnitId::new(70), |_, _| Vec::new());
        let context = TestCheckerContext::new(false);
        let semantic_context = callable_entry(unit.key());

        let request = CheckerUnitView::new(&unit, &semantic_context, &context)
            .unwrap_or_else(|error| panic!("test checker unit must validate: {error:?}"));

        for (key, expected) in [
            ("FutureStart", AnalysisTaskOperationKind::Start),
            ("TaskJoin", AnalysisTaskOperationKind::Join),
            ("TaskCancel", AnalysisTaskOperationKind::Cancel),
        ] {
            let symbol = compiler_known_symbol::<TypeCallableMemberSymbolId>(key);
            let resolution = BoundResolvedCall::new(
                BoundCallableTarget::Declaration(callable_instance(symbol.into())),
                [],
                BoundCallResult::Immediate(error_type()),
            );

            let selection =
                SemanticSelection::Call(SelectedCall::new(resolution, CallableAbi::Bray, [], []));

            assert_eq!(
                selected_task_operation(request, &selection),
                Some(expected),
                "{key}"
            );
        }
    }

    #[test]
    fn starting_tasks_requires_an_async_callable_context() {
        let (unit, expressions) = expression_unit(BoundUnitId::new(71), |tree, origin| {
            vec![push_expression(
                tree,
                BoundExpression::Error(BoundErrorExpression::new(origin, error_type())),
            )]
        });

        let context = TestCheckerContext::new(false);
        let semantic_context = callable_entry(unit.key());

        let request = CheckerUnitView::new(&unit, &semantic_context, &context)
            .unwrap_or_else(|error| panic!("test checker unit must validate: {error:?}"));

        let mut diagnostics = DiagnosticBag::new();

        add_task_context_diagnostic(
            request,
            Some(CallableExecution::Synchronous),
            expressions[0],
            AnalysisTaskOperationKind::Start,
            &mut diagnostics,
        )
        .unwrap_or_else(|error| panic!("test expression span must resolve: {error:?}"));

        assert_eq!(
            diagnostics
                .iter()
                .map(|diagnostic| diagnostic.kind())
                .collect::<Vec<_>>(),
            [DiagnosticKind::CheckingTaskStartOutsideAsyncCallable]
        );
    }

    #[test]
    fn deferred_contracts_retain_non_storage_frame_subjects() {
        let witness = implementation_instance(semantic_values(), 72);
        let subject = BoundDependencySubject::ImplementationWitness(witness);
        let requirement = BoundDependencyRequirement::direct(
            subject,
            BoundDependencyRequirementKind::StorageAlive,
        );

        let mut subjects = BTreeSet::new();

        collect_requirement_subjects(&requirement, &mut subjects);

        assert_eq!(subjects, BTreeSet::from([subject]));
    }
}
