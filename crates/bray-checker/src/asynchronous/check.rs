use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AsyncSuspensionKind, AsyncSuspensionPoint, AsyncTaskOperation, AsyncTaskOperationKind,
    BodyBehaviorCall, BodyBehaviorPhase, BoundBlock, BoundBlockItem, BoundCallResult,
    BoundCallableTarget, BoundDependencyContractId, BoundExpression, BoundExpressionId,
    BoundUnitRoot, CheckedAsync, CheckedDependencyContracts, CheckedExpressionTypes,
    CheckedRefinements, CheckedSemanticSelections, Liveness, SemanticSelection, StorageFlow,
    StoragePlan,
};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::{
    Diagnostic, DiagnosticBag, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
    DiagnosticNote, DiagnosticNoteKind, SeverityKind,
};
use bray_symbols::{AnyLocalSymbolId, CallableExecution, CallableSignatureQuery, TypeData};

use super::cleanup::scope_exit_plans;
use super::dependency::retained_suspension_subjects;
use super::diagnostic::{add_unavailable_await_dependency_diagnostic, await_dependency_failure};

use crate::analysis::{
    AnalysisOperationKind, AnalysisSuspensionKind, AnalysisTaskOperationKind,
    ControlFlowGraphBuildOutcome, build_storage_control_flow_graph,
};
use crate::diagnostic::{diagnostic_id, expression_span};
use crate::unit::semantic_inputs_match;
use crate::{
    CheckerInfrastructureError, CheckerOutcome, CheckerQueryError, CheckerRequestContext,
    CheckerSemanticQueryProvider, CheckerUnitView,
};

#[expect(
    clippy::too_many_arguments,
    reason = "async checking consumes independently materialized semantic analysis sets explicitly"
)]
pub(crate) fn check_async_analysis<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    selections: &CheckedSemanticSelections,
    liveness: &Liveness,
    dependencies: &CheckedDependencyContracts,
    storage: &StoragePlan,
    refinements: &CheckedRefinements,
    flow: &StorageFlow,
) -> CheckerOutcome<CheckedAsync, C::UpstreamError>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
{
    if request.is_cancelled() {
        return CheckerOutcome::Cancelled;
    }

    if !inputs_match(
        request,
        types,
        selections,
        liveness,
        dependencies,
        storage,
        refinements,
        flow,
    ) {
        return CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidStorageFlow,
        );
    }

    let graph = match build_storage_control_flow_graph(request, storage, selections) {
        ControlFlowGraphBuildOutcome::Complete(graph) => graph,
        ControlFlowGraphBuildOutcome::Cancelled => return CheckerOutcome::Cancelled,
    };

    check_async_analysis_with_graph(
        request,
        types,
        selections,
        liveness,
        dependencies,
        storage,
        refinements,
        flow,
        &graph,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "async checking consumes correlated durable inputs and one shared control-flow graph"
)]
pub(crate) fn check_async_analysis_with_graph<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    selections: &CheckedSemanticSelections,
    liveness: &Liveness,
    dependencies: &CheckedDependencyContracts,
    storage: &StoragePlan,
    refinements: &CheckedRefinements,
    flow: &StorageFlow,
    graph: &crate::analysis::ControlFlowGraph,
) -> CheckerOutcome<CheckedAsync, C::UpstreamError>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
{
    if request.is_cancelled() {
        return CheckerOutcome::Cancelled;
    }

    let execution = match containing_execution(request).map_err(CheckerQueryError::with_upstream) {
        Ok(execution) => execution,
        Err(CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
        Err(CheckerQueryError::Infrastructure(error)) => {
            return CheckerOutcome::InfrastructureFailure(error);
        }
        Err(CheckerQueryError::Upstream(error)) => {
            return CheckerOutcome::UpstreamFailure(error);
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
            AnalysisOperationKind::Suspension { expression, kind } => {
                let (kind, dependency_contract, calls, syntax_recovered) = match kind {
                    AnalysisSuspensionKind::Await => {
                        let Some(BoundExpression::Await(await_expression)) =
                            request.view().expression(expression)
                        else {
                            is_recovered = true;

                            continue;
                        };

                        if execution == Some(CallableExecution::Synchronous) {
                            let span = match expression_span(request, expression) {
                                Ok(span) => span,
                                Err(error) => {
                                    return CheckerOutcome::InfrastructureFailure(error);
                                }
                            };

                            diagnostics.add(invalid_async_operation_diagnostic(
                                diagnostic_id(diagnostics.len()),
                                DiagnosticKind::CheckingAwaitOutsideAsyncCallable,
                                span,
                            ));
                        }

                        let operand = await_expression.operand();

                        let calls = expression_deferred_calls(
                            request,
                            selections,
                            types,
                            &local_initializers,
                            operand,
                            &mut deferred_calls,
                            &mut active,
                        );

                        (
                            AsyncSuspensionKind::Await { operand },
                            deferred_dependency_contract(
                                request,
                                dependencies,
                                &local_initializers,
                                operand,
                            ),
                            calls,
                            request
                                .view()
                                .expression(operand)
                                .is_none_or(BoundExpression::is_recovered),
                        )
                    }
                    AnalysisSuspensionKind::Yield => {
                        (AsyncSuspensionKind::Yield, None, BTreeSet::new(), false)
                    }
                };

                let retained = retained_suspension_subjects(
                    liveness,
                    dependencies,
                    storage,
                    expression,
                    dependency_contract,
                );

                frame_dependencies.extend(retained.iter().copied());

                let suspension_state = flow.suspension(expression);

                let dependency_failure = match &kind {
                    AsyncSuspensionKind::Await { .. } => match await_dependency_failure(
                        request,
                        dependencies,
                        storage,
                        refinements,
                        expression,
                        dependency_contract,
                        suspension_state,
                        syntax_recovered,
                    ) {
                        Ok(failure) => failure,
                        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
                    },
                    AsyncSuspensionKind::Yield => None,
                };

                if let Some(failure) = dependency_failure.as_ref()
                    && let Err(error) = add_unavailable_await_dependency_diagnostic(
                        request,
                        storage,
                        expression,
                        failure,
                        &mut diagnostics,
                    )
                {
                    return CheckerOutcome::InfrastructureFailure(error);
                }

                let suspension_recovered = syntax_recovered || suspension_state.is_none();

                is_recovered |= suspension_recovered;

                suspensions.push(AsyncSuspensionPoint::new(
                    expression,
                    kind,
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
            AnalysisOperationKind::Bound(_)
            | AnalysisOperationKind::Call { .. }
            | AnalysisOperationKind::ScopeExit { .. } => {}
        }
    }

    if let Err(error) = add_selected_task_operations(
        request,
        selections,
        execution,
        &mut task_operations,
        &mut diagnostics,
    ) {
        return CheckerOutcome::InfrastructureFailure(error);
    }

    let (scope_exits, cleanup_diagnostics) = match scope_exit_plans(request, storage, flow) {
        Ok(plans) => plans,
        Err(CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
        Err(CheckerQueryError::Infrastructure(error)) => {
            return CheckerOutcome::InfrastructureFailure(error);
        }
        Err(CheckerQueryError::Upstream(error)) => {
            return CheckerOutcome::UpstreamFailure(error);
        }
    };

    is_recovered |= scope_exits.iter().any(|exit| exit.is_recovered());

    diagnostics.add_range(cleanup_diagnostics);

    let analysis = match CheckedAsync::try_new(
        request.unit().unit(),
        request.unit().key().kind(),
        frame_dependencies,
        suspensions,
        task_operations.into_values(),
        scope_exits,
        is_recovered,
    ) {
        Ok(analysis) => analysis,
        Err(_) => {
            return CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::InvalidStorageFlow,
            );
        }
    };

    CheckerOutcome::complete(analysis, diagnostics)
}

fn add_selected_task_operations<C>(
    request: CheckerUnitView<'_, C>,
    selections: &CheckedSemanticSelections,
    execution: Option<CallableExecution>,
    task_operations: &mut BTreeMap<BoundExpressionId, AsyncTaskOperation>,
    diagnostics: &mut DiagnosticBag,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    for entry in selections.entries() {
        let Some(analysis_kind) = selected_task_operation(request, entry.selection()) else {
            continue;
        };

        let operation =
            AsyncTaskOperation::new(entry.expression(), task_operation_kind(analysis_kind));

        if task_operations
            .insert(entry.expression(), operation)
            .is_none()
        {
            add_task_context_diagnostic(
                request,
                execution,
                entry.expression(),
                analysis_kind,
                diagnostics,
            )?;
        }
    }

    Ok(())
}

fn inputs_match<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    selections: &CheckedSemanticSelections,
    liveness: &Liveness,
    dependencies: &CheckedDependencyContracts,
    storage: &StoragePlan,
    refinements: &CheckedRefinements,
    flow: &StorageFlow,
) -> bool
where
    C: CheckerRequestContext + ?Sized,
{
    semantic_inputs_match(
        request,
        [
            (types.unit(), types.kind()),
            (selections.unit(), selections.kind()),
            (liveness.unit(), liveness.kind()),
            (dependencies.unit(), dependencies.kind()),
            (storage.unit(), storage.kind()),
            (refinements.unit(), refinements.kind()),
            (flow.unit(), flow.kind()),
        ],
    )
}

fn containing_execution<C>(
    request: CheckerUnitView<'_, C>,
) -> Result<Option<CallableExecution>, CheckerQueryError>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
{
    let execution = match request.unit().root() {
        BoundUnitRoot::CallableBody { execution, .. }
        | BoundUnitRoot::AnonymousCallable { execution, .. } => Some(execution),
        BoundUnitRoot::Expression(_) | BoundUnitRoot::ExpressionSequence(_) => None,
    };

    Ok(execution)
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

fn deferred_dependency_contract<C>(
    request: CheckerUnitView<'_, C>,
    dependencies: &CheckedDependencyContracts,
    local_initializers: &BTreeMap<AnyLocalSymbolId, BoundExpressionId>,
    expression: BoundExpressionId,
) -> Option<BoundDependencyContractId>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut current = expression;
    let mut active = BTreeSet::new();

    loop {
        if let Some(contract) = dependencies.deferred_expression(current) {
            return Some(contract);
        }

        if !active.insert(current) {
            return None;
        }

        current = match request.view().expression(current)? {
            BoundExpression::Name(name) => {
                let bray_bound_tree::BoundReferenceTarget::Local(local) = name.target() else {
                    return None;
                };

                *local_initializers.get(&local)?
            }
            BoundExpression::PatternReference(reference) => {
                *local_initializers.get(&AnyLocalSymbolId::from(reference.binding()))?
            }
            _ => return None,
        };
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

        if let Some(source) = request
            .view()
            .expression(expression)
            .map(|expression| expression.origin().source_anchor())
        {
            contribution = contribution.with_source(source);
        }

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

    diagnostics.add(invalid_async_operation_diagnostic(
        diagnostic_id(diagnostics.len()),
        DiagnosticKind::CheckingTaskStartOutsideAsyncCallable,
        span,
    ));

    Ok(())
}

fn invalid_async_operation_diagnostic(
    id: bray_diagnostics::DiagnosticId,
    kind: DiagnosticKind,
    span: bray_source::SourceSpan,
) -> Diagnostic {
    Diagnostic::new(id, kind, SeverityKind::Error)
        .with_primary_span(span)
        .with_label(DiagnosticLabel::primary(
            DiagnosticLabelKind::InvalidAsyncOperation,
            span,
        ))
        .with_note(DiagnosticNote::new(
            DiagnosticNoteKind::AsynchronousCallableRequired,
        ))
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BorrowCapabilityOrigin, BoundAwaitExpression, BoundCallResult, BoundCallableTarget,
        BoundDependencyContract, BoundDependencyRequirement, BoundDependencyRequirementKind,
        BoundDependencySubject, BoundErrorExpression, BoundExpression, BoundResolvedCall,
        BoundUnit, BoundUnitId, CheckedDependencyContracts, CheckedRefinements,
        CheckedSemanticSelections, ExpressionTypeResult, ExpressionTypeStatus, Liveness,
        PlannedBorrowCapability, SelectedCall, SemanticSelection, StorageAccess, StorageAccessRoot,
        StorageFlow, StorageIdentity, StoragePlanBuilder, StorageSuspensionState,
    };
    use bray_diagnostics::{
        DiagnosticArgValue, DiagnosticBag, DiagnosticDependencyRequirementKind,
        DiagnosticDependencySubjectKind, DiagnosticKind,
    };
    use bray_symbols::{
        BorrowKind, CallableAbi, CallableExecution, LifecycleObligationKind,
        TypeCallableMemberSymbolId, testing::implementation_instance,
    };
    use bray_testing::assert_goal_state_diagnostic_kind;

    use super::{
        AnalysisTaskOperationKind, add_task_context_diagnostic, check_async_analysis,
        selected_task_operation,
    };
    use crate::test_support::{
        TestCheckerContext, callable_entry, callable_instance, checked_expression_types,
        compiler_known_symbol, empty_callable_phase_behaviors, error_type, expression_unit,
        integer_literal_expression, push_expression, semantic_values, test_source_origins,
    };
    use crate::{CheckerOutcome, CheckerUnitView};

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

            let selection = SemanticSelection::Call(SelectedCall::new(
                resolution,
                CallableAbi::Bray,
                empty_callable_phase_behaviors(),
                None,
                [],
                [],
            ));

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

        assert_goal_state_diagnostic_kind(
            &diagnostics,
            DiagnosticKind::CheckingTaskStartOutsideAsyncCallable,
        );
    }

    #[test]
    fn non_recovered_await_without_an_inferred_dependency_contract_is_infrastructure_failure() {
        assert_eq!(
            await_outcome(false, true),
            CheckerOutcome::InfrastructureFailure(
                crate::CheckerInfrastructureError::InvalidStorageFlow
            )
        );
    }

    #[test]
    fn await_without_flow_state_reports_the_missing_semantic_input() {
        let diagnostics = await_diagnostics(true, false);

        assert_goal_state_diagnostic_kind(
            &diagnostics,
            DiagnosticKind::CheckingUnavailableAwaitDependency,
        );

        assert_dependency_problem(
            &diagnostics,
            DiagnosticDependencySubjectKind::SuspensionState,
            DiagnosticDependencyRequirementKind::SuspensionStateAvailable,
        );
    }

    #[test]
    fn unavailable_await_dependencies_keep_every_paired_cause_and_available_origin() {
        let diagnostics = await_diagnostics_with(
            |unit, expressions, storage| {
                let [access_origin, storage_origin, _] = test_source_origins();

                let identity = storage
                    .push_identity(StorageIdentity::Temporary(expressions[0]))
                    .unwrap_or_else(|error| panic!("test storage identity must build: {error:?}"));

                let access = storage
                    .push_access(StorageAccess::new(
                        StorageAccessRoot::Storage(identity),
                        [],
                        error_type(),
                        access_origin.source_anchor(),
                        false,
                    ))
                    .unwrap_or_else(|error| panic!("test storage access must build: {error:?}"));

                let borrow = storage
                    .push_borrow_capability(PlannedBorrowCapability::new(
                        BorrowCapabilityOrigin::Expression(expressions[0]),
                        BorrowKind::Shared,
                        access,
                        None,
                        storage_origin.source_anchor(),
                        false,
                    ))
                    .unwrap_or_else(|error| panic!("test borrow capability must build: {error:?}"));

                let scoped = bray_bound_tree::testing::scoped_capability_id(unit.unit(), 0);
                let obligation = bray_bound_tree::testing::lifecycle_obligation_id(unit.unit(), 0);
                let witness = implementation_instance(semantic_values(), 73);

                Some(BoundDependencyContract::new([
                    BoundDependencyRequirement::direct(
                        BoundDependencySubject::Storage(identity),
                        BoundDependencyRequirementKind::StorageAlive,
                    ),
                    BoundDependencyRequirement::direct(
                        BoundDependencySubject::StorageAccess(access),
                        BoundDependencyRequirementKind::StorageInitialized,
                    ),
                    BoundDependencyRequirement::direct(
                        BoundDependencySubject::BorrowCapability(borrow),
                        BoundDependencyRequirementKind::BorrowCapabilityActive(BorrowKind::Shared),
                    ),
                    BoundDependencyRequirement::direct(
                        BoundDependencySubject::ScopedCapability(scoped),
                        BoundDependencyRequirementKind::ScopedCapabilityLive,
                    ),
                    BoundDependencyRequirement::direct(
                        BoundDependencySubject::ImplementationWitness(witness),
                        BoundDependencyRequirementKind::StorageAlive,
                    ),
                    BoundDependencyRequirement::direct(
                        BoundDependencySubject::LifecycleObligation(obligation),
                        BoundDependencyRequirementKind::LifecycleObligationAttached(
                            LifecycleObligationKind::Finalization,
                        ),
                    ),
                ]))
            },
            true,
        );

        assert_goal_state_diagnostic_kind(
            &diagnostics,
            DiagnosticKind::CheckingUnavailableAwaitDependency,
        );

        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.kind() == DiagnosticKind::CheckingUnavailableAwaitDependency
            })
            .unwrap_or_else(|| panic!("await dependency diagnostic must be produced"));

        assert_eq!(diagnostic.notes().len(), 6);
        assert_eq!(diagnostic.related_locations().len(), 1);

        for note in diagnostic.notes() {
            assert_eq!(note.args().len(), 2);

            assert!(matches!(
                note.args()[0].value(),
                DiagnosticArgValue::DependencySubjectKind(_)
            ));

            assert!(matches!(
                note.args()[1].value(),
                DiagnosticArgValue::DependencyRequirementKind(_)
            ));
        }
    }

    fn await_diagnostics(
        include_dependency_contract: bool,
        include_suspension_state: bool,
    ) -> DiagnosticBag {
        let CheckerOutcome::Complete(result) =
            await_outcome(include_dependency_contract, include_suspension_state)
        else {
            panic!("test async checking must complete");
        };

        result.into_parts().1
    }

    fn await_outcome(
        include_dependency_contract: bool,
        include_suspension_state: bool,
    ) -> CheckerOutcome<bray_bound_tree::CheckedAsync> {
        await_outcome_with(
            |_, _, _| include_dependency_contract.then(|| BoundDependencyContract::new([])),
            include_suspension_state,
        )
    }

    fn await_diagnostics_with(
        build_contract: impl FnOnce(
            &BoundUnit,
            &[bray_bound_tree::BoundExpressionId],
            &mut StoragePlanBuilder,
        ) -> Option<BoundDependencyContract>,
        include_suspension_state: bool,
    ) -> DiagnosticBag {
        let CheckerOutcome::Complete(result) =
            await_outcome_with(build_contract, include_suspension_state)
        else {
            panic!("test async checking must complete");
        };

        result.into_parts().1
    }

    fn await_outcome_with(
        build_contract: impl FnOnce(
            &BoundUnit,
            &[bray_bound_tree::BoundExpressionId],
            &mut StoragePlanBuilder,
        ) -> Option<BoundDependencyContract>,
        include_suspension_state: bool,
    ) -> CheckerOutcome<bray_bound_tree::CheckedAsync> {
        let (unit, expressions) = expression_unit(BoundUnitId::new(72), |tree, origin| {
            let [_, operand_origin, _] = test_source_origins();

            let operand = push_expression(
                tree,
                integer_literal_expression(operand_origin, Some(error_type())),
            );

            let await_expression = push_expression(
                tree,
                BoundExpression::Await(BoundAwaitExpression::pending(origin, operand, false)),
            );

            vec![operand, await_expression]
        });

        let context = TestCheckerContext::new(false);
        let semantic_context = callable_entry(unit.key());

        let request = CheckerUnitView::new(&unit, &semantic_context, &context)
            .unwrap_or_else(|error| panic!("test checker unit must validate: {error:?}"));

        let result = ExpressionTypeResult::new(error_type(), ExpressionTypeStatus::Valid);
        let types = checked_expression_types(&unit, expressions.iter().copied(), result);

        let selections = CheckedSemanticSelections::try_new(&unit, &types, [])
            .unwrap_or_else(|error| panic!("empty test selections must validate: {error:?}"));

        let mut storage = StoragePlanBuilder::new(unit.unit(), unit.key().kind());

        let deferred = build_contract(&unit, &expressions, &mut storage)
            .map(|contract| (expressions[0], contract));

        let storage = storage.finish();

        let dependencies =
            CheckedDependencyContracts::try_new(&unit, &storage, [], deferred, [], [], false)
                .unwrap_or_else(|error| {
                    panic!("test dependency contracts must validate: {error:?}")
                });

        let liveness = Liveness::try_new(unit.unit(), unit.key().kind(), [], [], [], [], false)
            .unwrap_or_else(|error| panic!("empty test liveness must validate: {error:?}"));

        let refinements = CheckedRefinements::try_new(unit.unit(), unit.key().kind(), [], false)
            .unwrap_or_else(|error| panic!("empty test refinements must validate: {error:?}"));

        let suspension = include_suspension_state
            .then(|| StorageSuspensionState::new(expressions[1], [], [], [], []));

        let flow = StorageFlow::try_new(unit.unit(), unit.key().kind(), [], suspension, [], false)
            .unwrap_or_else(|error| panic!("test storage flow must validate: {error:?}"));

        check_async_analysis(
            request,
            &types,
            &selections,
            &liveness,
            &dependencies,
            &storage,
            &refinements,
            &flow,
        )
    }

    fn assert_dependency_problem(
        diagnostics: &DiagnosticBag,
        expected_subject: DiagnosticDependencySubjectKind,
        expected_requirement: DiagnosticDependencyRequirementKind,
    ) {
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.kind() == DiagnosticKind::CheckingUnavailableAwaitDependency
            })
            .unwrap_or_else(|| panic!("await dependency diagnostic must be produced"));

        assert!(diagnostic.args().iter().any(|argument| {
            argument.value() == &DiagnosticArgValue::DependencySubjectKind(expected_subject)
        }));

        assert!(diagnostic.args().iter().any(|argument| {
            argument.value() == &DiagnosticArgValue::DependencyRequirementKind(expected_requirement)
        }));
    }
}
