use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AsyncSuspensionKind, AsyncSuspensionPoint, AsyncTaskOperation, AsyncTaskOperationKind,
    BodyBehaviorCall, BodyBehaviorPhase, BoundCallableTarget,
    BoundExpression, BoundExpressionId, BoundUnitRoot, CheckedAsync,
    CheckedSemanticSelections, Liveness,
    SemanticSelection, StorageFlow, StoragePlan,
};
use bray_diagnostics::{
    Diagnostic, DiagnosticBag, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
    DiagnosticNote, DiagnosticNoteKind, SeverityKind,
};
use bray_symbols::{CallableExecution, CallableSignatureQuery};

use super::cleanup::prepare_cleanup;
use super::diagnostic::check_await_dependencies;

use crate::analysis::{AnalysisOperationKind, AnalysisSuspensionKind, AnalysisTaskOperationKind};
use crate::diagnostic::{diagnostic_id, expression_span};
use crate::{
    CheckerOutcome, CheckerRequestContext, CheckerSemanticQueryProvider,
    CheckerUnitView,
};

#[expect(
    clippy::too_many_arguments,
    reason = "async checking consumes correlated durable inputs and one shared control-flow graph"
)]
pub(crate) fn check_async_analysis_with_graph<C>(
    cleanup_shapes: &mut crate::asynchronous::CleanupShapeResolver<'_, C>,
    request: CheckerUnitView<'_, C>,
    selections: &CheckedSemanticSelections,
    liveness: &Liveness,
    storage: &StoragePlan,
    flow: &StorageFlow,
    graph: &crate::analysis::ControlFlowGraph,
    cleanup: &crate::analysis::guarantee::CleanupPlan,
) -> CheckerOutcome<CheckedAsync, C::UpstreamError>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
{
    if request.is_cancelled() {
        return CheckerOutcome::Cancelled;
    }

    let execution = match request.unit().root() {
        BoundUnitRoot::CallableBody { execution, .. }
        | BoundUnitRoot::AnonymousCallable { execution, .. } => Some(execution),
        BoundUnitRoot::Expression(_) | BoundUnitRoot::ExpressionSequence(_) => None,
    };

    let (storage_requirements, cleanup_types, cleanup_diagnostics) = prepare_cleanup(cleanup_shapes);

    let broadcast_owners = broadcast_owners(&storage_requirements, storage);

    let mut diagnostics = DiagnosticBag::new();

    let mut suspensions =
        BTreeMap::<bray_bound_tree::BoundExecutionSite, AsyncSuspensionPoint>::new();

    let mut task_operations = BTreeMap::new();
    let mut frame_dependencies = BTreeSet::new();

    let mut is_recovered = false;

    for operation in graph.operations() {
        if request.is_cancelled() {
            return CheckerOutcome::Cancelled;
        }

        match operation.kind() {
            AnalysisOperationKind::Suspension { occurrence, kind } => {
                let expression = occurrence
                    .expression()
                    .expect("checked suspension has an expression owner");

                add_suspension_context_diagnostic(
                    request,
                    execution,
                    expression,
                    kind,
                    &mut diagnostics,
                );

                let suspension_state = flow
                    .suspension(occurrence)
                    .expect("checked suspension retains its correlated storage state");

                if !suspension_state.is_reachable() {
                    continue;
                }

                let (kind, calls, syntax_recovered) = match kind {
                    AnalysisSuspensionKind::Await => {
                        let Some(BoundExpression::Await(await_expression)) =
                            request.view().expression(expression)
                        else {
                            panic!(
                                "await suspension retains its bound await expression: {expression:?}"
                            );
                        };

                        let operand = await_expression.operand();

                        (
                            AsyncSuspensionKind::Await { operand },
                            flow.deferred_calls_at(occurrence).cloned().collect(),
                            request
                                .view()
                                .expression(operand)
                                .is_none_or(BoundExpression::is_recovered),
                        )
                    }
                    AnalysisSuspensionKind::ScopedCall => (
                        AsyncSuspensionKind::ScopedCall,
                        BTreeSet::from([scoped_deferred_call(selections, occurrence)]),
                        false,
                    ),
                    AnalysisSuspensionKind::Yield => {
                        (AsyncSuspensionKind::Yield, BTreeSet::new(), false)
                    }
                };

                let retained = liveness.retained_suspension_subjects(
                    storage,
                    occurrence,
                    suspension_state,
                );

                frame_dependencies.extend(retained.iter().copied());

                if matches!(
                    kind,
                    AsyncSuspensionKind::Await { .. } | AsyncSuspensionKind::ScopedCall
                ) {
                    check_await_dependencies(
                        request,
                        storage,
                        expression,
                        suspension_state,
                        syntax_recovered,
                        &mut diagnostics,
                    );
                }

                is_recovered |= syntax_recovered;

                let suspension = AsyncSuspensionPoint::new(
                    occurrence,
                    kind,
                    calls,
                    retained,
                    suspension_state
                        .live()
                        .iter()
                        .filter_map(|identity| broadcast_owners.get(identity).copied()),
                    syntax_recovered,
                );

                // Deferred calls share immutable checked contracts across the exit paths.
                suspensions
                    .entry(occurrence)
                    .and_modify(|previous| previous.merge_path(&suspension))
                    .or_insert(suspension);
            }
            AnalysisOperationKind::Recovery(_) => is_recovered = true,
            AnalysisOperationKind::Result { .. }
            | AnalysisOperationKind::ScopedResult(_)
            | AnalysisOperationKind::AssignmentInstall(_)
            | AnalysisOperationKind::Bound(_)
            | AnalysisOperationKind::PatternObservation(_)
            | AnalysisOperationKind::Call { .. }
            | AnalysisOperationKind::ScopeExit { .. } => {}
        }
    }

    if execution == Some(CallableExecution::Asynchronous)
        || (request.unit().key().kind() == bray_bound_tree::BoundUnitKind::CallableBody
            && request.unit().key().declared_owner().kind() == bray_symbols::SymbolKind::Destructor)
    {
        for suspension in cleanup_suspensions(
            cleanup_deferred_calls(flow.exits(), flow.replacements(), cleanup),
            flow,
            liveness,
            storage,
            &broadcast_owners,
        ) {
            frame_dependencies.extend(suspension.retained_subjects().iter().copied());
            suspensions.insert(suspension.occurrence(), suspension);
        }
    }

    check_selected_async_operations(
        request,
        selections,
        execution,
        cleanup,
        &mut task_operations,
        &mut diagnostics,
    );

    is_recovered |= flow.exits().iter().any(|exit| exit.is_recovered());

    diagnostics.add_range(cleanup_diagnostics);

    let analysis = CheckedAsync::try_new(
        request.unit().unit(),
        request.unit().key().kind(),
        frame_dependencies,
        suspensions.into_values(),
        task_operations.into_values(),
        storage_requirements,
        cleanup_types.into_iter().map(|ty| {
            let finalizer = cleanup.finalizer(ty.ty());

            ty.with_finalizer(finalizer)
        }),
        is_recovered,
    )
    .expect("durable async analysis retains exact unit and storage identities");

    CheckerOutcome::complete(analysis, diagnostics)
}

fn cleanup_suspensions<'a>(
    calls: BTreeMap<bray_bound_tree::AnyBoundNodeId, BTreeSet<BodyBehaviorCall>>,
    flow: &'a StorageFlow,
    liveness: &'a Liveness,
    storage: &'a StoragePlan,
    broadcast_owners: &'a BTreeMap<
        bray_bound_tree::StorageIdentityId,
        bray_bound_tree::StorageAccessId,
    >,
) -> impl Iterator<Item = AsyncSuspensionPoint> + 'a {
    calls.into_iter().filter_map(|(exit, calls)| {
        let occurrence = bray_bound_tree::BoundExecutionSite::Cleanup(exit);

        let state = flow
            .suspension(occurrence)
            .expect("selected cleanup retains its pre-resolution storage state");

        if !state.is_reachable() {
            return None;
        }

        let retained = liveness.retained_suspension_subjects(
            storage,
            occurrence,
            state,
        );

        Some(AsyncSuspensionPoint::new(
            occurrence,
            AsyncSuspensionKind::Cleanup,
            calls,
            retained,
            state
                .live()
                .iter()
                .filter_map(|identity| broadcast_owners.get(identity).copied()),
            false,
        ))
    })
}

fn cleanup_deferred_calls(
    plans: &[bray_bound_tree::StorageExitDecision],
    replacements: &[bray_bound_tree::StorageReplacementDecision],
    cleanup: &crate::analysis::guarantee::CleanupPlan,
) -> BTreeMap<bray_bound_tree::AnyBoundNodeId, BTreeSet<BodyBehaviorCall>> {
    // Suspension entries own their call sets while the cleanup plan remains shared.
    let mut calls = cleanup
        .expression_cleanup_calls
        .iter()
        .map(|(expression, calls)| ((*expression).into(), calls.clone()))
        .collect::<BTreeMap<_, _>>();

    for plan in plans {
        if cleanup.scoped_quiescence.contains_key(&plan.scope()) {
            calls.entry(plan.exit()).or_default();
        }
    }

    let scope_accesses = plans.iter().flat_map(|plan| {
        plan.lifecycle_resolution()
            .iter()
            .map(move |access| (plan.exit(), *access))
    });

    let replacement_accesses = replacements
        .iter()
        .filter(|plan| {
            matches!(plan.cleanup(),
                bray_bound_tree::AsyncStorageCleanupRequirement::Cleanup(phases)
                    if phases.includes_lifecycle())
        })
        .map(|plan| (plan.expression().into(), plan.access()));

    for (exit, access) in scope_accesses.chain(replacement_accesses) {
        if !cleanup.requires_suspension(access) {
            continue;
        }

        let selected = calls.entry(exit).or_default();

        selected.extend(cleanup.deferred_cleanup_calls(access));
    }

    calls
}

fn broadcast_owners(
    requirements: &[bray_bound_tree::AsyncStorageRequirement],
    storage: &StoragePlan,
) -> BTreeMap<bray_bound_tree::StorageIdentityId, bray_bound_tree::StorageAccessId> {
    requirements
        .iter()
        .filter_map(|requirement| {
            let bray_bound_tree::AsyncStorageCleanupRequirement::Cleanup(phases) =
                requirement.cleanup()
            else {
                return None;
            };

            (requirement.owner().is_some() && phases.includes_cancellation()).then(|| {
                (
                    requirement.identity(),
                    storage
                        .root_access(requirement.identity())
                        .expect("owned cancellation storage retains its root access"),
                )
            })
        })
        .collect()
}

fn check_selected_async_operations<C>(
    request: CheckerUnitView<'_, C>,
    selections: &CheckedSemanticSelections,
    execution: Option<CallableExecution>,
    cleanup: &crate::analysis::guarantee::CleanupPlan,
    task_operations: &mut BTreeMap<BoundExpressionId, AsyncTaskOperation>,
    diagnostics: &mut DiagnosticBag,
) where
    C: CheckerRequestContext + ?Sized,
{
    for expression in cleanup.scoped_quiescence.values().copied() {
        let Some(SemanticSelection::ScopedUse(scoped)) = selections.expression(expression) else {
            panic!("selected scoped cleanup retains its invocation: {expression:?}");
        };

        if matches!(
            scoped.exit_result(),
            bray_bound_tree::BoundCallResult::LazyFuture(_)
        ) {
            // The selected asynchronous call already checks its execution context.
            continue;
        }

        add_suspension_context_diagnostic(
            request,
            execution,
            expression,
            AnalysisSuspensionKind::ScopedCall,
            diagnostics,
        );
    }

    for entry in selections.entries() {
        let Some(analysis_kind) = selected_task_operation(entry.selection()) else {
            continue;
        };

        add_task_operation(
            request,
            execution,
            entry.expression(),
            analysis_kind,
            task_operations,
            diagnostics,
        );
    }
}

fn add_task_operation<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    execution: Option<CallableExecution>,
    expression: BoundExpressionId,
    kind: AnalysisTaskOperationKind,
    task_operations: &mut BTreeMap<BoundExpressionId, AsyncTaskOperation>,
    diagnostics: &mut DiagnosticBag,
) {
    let operation = AsyncTaskOperation::new(expression, task_operation_kind(kind));

    if task_operations.insert(expression, operation).is_none() {
        add_task_context_diagnostic(request, execution, expression, kind, diagnostics);
    }
}

const fn task_operation_kind(kind: AnalysisTaskOperationKind) -> AsyncTaskOperationKind {
    match kind {
        AnalysisTaskOperationKind::Start => AsyncTaskOperationKind::Start,
        AnalysisTaskOperationKind::Join => AsyncTaskOperationKind::Join,
        AnalysisTaskOperationKind::Cancel => AsyncTaskOperationKind::Cancel,
    }
}

fn selected_task_operation(selection: &SemanticSelection) -> Option<AnalysisTaskOperationKind> {
    let SemanticSelection::Call(call) = selection else {
        return None;
    };

    call.implementation_hook().and_then(AnalysisTaskOperationKind::from_implementation_hook)
}

fn add_task_context_diagnostic<C>(
    request: CheckerUnitView<'_, C>,
    execution: Option<CallableExecution>,
    expression: BoundExpressionId,
    kind: AnalysisTaskOperationKind,
    diagnostics: &mut DiagnosticBag,
) where
    C: CheckerRequestContext + ?Sized,
{
    if kind != AnalysisTaskOperationKind::Start || execution != Some(CallableExecution::Synchronous)
    {
        return;
    }

    let span = expression_span(request, expression);

    diagnostics.add(invalid_async_operation_diagnostic(
        diagnostic_id(diagnostics.len()),
        DiagnosticKind::CheckingTaskStartOutsideAsyncCallable,
        span,
    ));
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

fn add_suspension_context_diagnostic<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    execution: Option<CallableExecution>,
    expression: BoundExpressionId,
    kind: AnalysisSuspensionKind,
    diagnostics: &mut DiagnosticBag,
) {
    if kind != AnalysisSuspensionKind::Yield && execution == Some(CallableExecution::Synchronous) {
        let span = expression_span(request, expression);

        diagnostics.add(invalid_async_operation_diagnostic(
            diagnostic_id(diagnostics.len()),
            DiagnosticKind::CheckingAwaitOutsideAsyncCallable,
            span,
        ));
    }
}

fn scoped_deferred_call(
    selections: &CheckedSemanticSelections,
    occurrence: bray_bound_tree::BoundExecutionSite,
) -> BodyBehaviorCall {
    let expression = occurrence
        .expression()
        .expect("scoped suspension retains its with occurrence");

    let Some(SemanticSelection::ScopedUse(scoped)) = selections.expression(expression) else {
        panic!("checked scoped suspension retains its lifecycle pair");
    };

    BodyBehaviorCall::new(
        BoundCallableTarget::Declaration(scoped.invocation(occurrence).0),
        BodyBehaviorPhase::DeferredExecution,
    )
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BorrowCapabilityOrigin, BoundAwaitExpression, BoundCallResult, BoundCallableTarget,
        BoundDependencyRequirementKind,
        BoundDependencySubject, BoundErrorExpression, BoundExpression, BoundResolvedCall,
        BoundUnit, BoundUnitId,
        CheckedSemanticSelections, ExpressionTypeResult, ExpressionTypeStatus, Liveness,
        PlannedBorrowCapability, SelectedCall, SemanticSelection, StorageAccess, StorageAccessRoot,
        StorageFlow, StorageIdentity, StoragePlanBuilder, StorageSuspensionState,
    };
    use bray_diagnostics::{DiagnosticArgValue, DiagnosticBag, DiagnosticKind};
    use bray_symbols::{
        BorrowKind, CallableAbi, CallableExecution, LifecycleObligationKind,
        TypeCallableMemberSymbolId, testing::implementation_instance,
    };
    use bray_testing::assert_goal_state_diagnostic_kind;

    use super::{
        AnalysisTaskOperationKind, add_task_context_diagnostic, check_async_analysis_with_graph,
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

        let request = CheckerUnitView::new(&unit, &semantic_context, &context);

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
            error_type(),
                bray_symbols::CallableConstness::Runtime,
                CallableAbi::Bray,
                empty_callable_phase_behaviors(),
                None,
                [],
                [],
            ).with_implementation_hook(request.available_compiler_known_symbols()
                .symbol_implementation(symbol)));

            assert_eq!(
                selected_task_operation(&selection),
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

        let request = CheckerUnitView::new(&unit, &semantic_context, &context);

        let mut diagnostics = DiagnosticBag::new();

        add_task_context_diagnostic(
            request,
            Some(CallableExecution::Synchronous),
            expressions[0],
            AnalysisTaskOperationKind::Start,
            &mut diagnostics,
        );

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
    #[should_panic(expected = "checked suspension retains its correlated storage state")]
    fn await_without_flow_state_exposes_the_missing_semantic_input() {
        await_outcome_with(|_, _, _| Vec::new(), false);
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
                        BorrowCapabilityOrigin::Operation((expressions[0]).into()),
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

                Vec::from([
                    (
                        BoundDependencySubject::Storage(identity),
                        BoundDependencyRequirementKind::StorageAlive,
                    ),
                    (
                        BoundDependencySubject::StorageAccess(access),
                        BoundDependencyRequirementKind::StorageInitialized,
                    ),
                    (
                        BoundDependencySubject::BorrowCapability(borrow),
                        BoundDependencyRequirementKind::BorrowCapabilityActive(BorrowKind::Shared),
                    ),
                    (
                        BoundDependencySubject::ScopedCapability(scoped),
                        BoundDependencyRequirementKind::ScopedCapabilityLive,
                    ),
                    (
                        BoundDependencySubject::ImplementationWitness(witness),
                        BoundDependencyRequirementKind::StorageAlive,
                    ),
                    (
                        BoundDependencySubject::LifecycleObligation(obligation),
                        BoundDependencyRequirementKind::LifecycleObligationAttached(
                            LifecycleObligationKind::Finalization,
                        ),
                    ),
                ])
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

    fn await_diagnostics_with(
        unavailable_dependencies: impl FnOnce(
            &BoundUnit,
            &[bray_bound_tree::BoundExpressionId],
            &mut StoragePlanBuilder,
        ) -> Vec<(BoundDependencySubject, BoundDependencyRequirementKind)>,
        include_suspension_state: bool,
    ) -> DiagnosticBag {
        let CheckerOutcome::Complete(result) =
            await_outcome_with(unavailable_dependencies, include_suspension_state)
        else {
            panic!("test async checking must complete");
        };

        result.into_parts().1
    }

    fn await_outcome_with(
        unavailable_dependencies: impl FnOnce(
            &BoundUnit,
            &[bray_bound_tree::BoundExpressionId],
            &mut StoragePlanBuilder,
        ) -> Vec<(BoundDependencySubject, BoundDependencyRequirementKind)>,
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

        let request = CheckerUnitView::new(&unit, &semantic_context, &context);

        let result = ExpressionTypeResult::new(error_type(), ExpressionTypeStatus::Valid);
        let types = checked_expression_types(&unit, expressions.iter().copied(), result);

        let selections = CheckedSemanticSelections::try_new(&unit, &types, [])
            .unwrap_or_else(|error| panic!("empty test selections must validate: {error:?}"));

        let mut storage = StoragePlanBuilder::new(unit.unit(), unit.key().kind());

        let unavailable = unavailable_dependencies(&unit, &expressions, &mut storage);

        let storage = storage.finish();

        let liveness = Liveness::try_new(unit.unit(), unit.key().kind(), [], [], [], [], [], false)
            .unwrap_or_else(|error| panic!("empty test liveness must validate: {error:?}"));

        let suspension = include_suspension_state.then(|| {
            StorageSuspensionState::new(expressions[1], [], [], [], [], [])
                .with_execution_dependencies(unavailable.iter().map(|(subject, _)| *subject), unavailable.iter().copied())
        });

        let flow = StorageFlow::try_new(
            unit.unit(),
            unit.key().kind(),
            [],
            suspension,
            [],
            [],
            false,
        )
        .unwrap_or_else(|error| panic!("test storage flow must validate: {error:?}"));

        let patterns =
            bray_bound_tree::CheckedPatterns::new(unit.unit(), unit.key().kind(), [], [], [], []);

        let mut cleanup_shapes = crate::asynchronous::CleanupShapeResolver::new(request);

        let graph = match crate::analysis::build_storage_control_flow_graph(
            &mut cleanup_shapes,
            request,
            &storage,
            &selections,
            Some(&patterns),
            None,
        ) {
            crate::analysis::ControlFlowGraphBuildOutcome::Complete(graph) => graph,
            crate::analysis::ControlFlowGraphBuildOutcome::Cancelled => {
                return CheckerOutcome::Cancelled;
            }
            crate::analysis::ControlFlowGraphBuildOutcome::UpstreamFailure(error) => {
                return CheckerOutcome::UpstreamFailure(error);
            }
        };

        if let Err(error) = cleanup_shapes.prepare_lowering_types(&selections, &types, &storage) {
            return CheckerOutcome::from_query_error(error);
        }

        check_async_analysis_with_graph(
            &mut cleanup_shapes,
            request,
            &selections,
            &liveness,
            &storage,
            &flow,
            &graph,
            &crate::analysis::guarantee::CleanupPlan::default(),
        )
    }
}
