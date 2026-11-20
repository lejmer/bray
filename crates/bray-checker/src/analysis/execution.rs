use bray_bound_tree::{
    BoundAwaitExpression, BoundCallExpression, BoundExpressionId, SemanticSelection,
};
use bray_compiler_known::ImplementationHook;
use bray_symbols::CallableInvocation;

use crate::CheckerRequestContext;

use super::build::ControlFlowGraphBuilder;
use super::id::AnalysisBlockId;
use super::model::{
    AnalysisCallPhase, AnalysisEdgeKind, AnalysisExitKind, AnalysisSuspensionKind,
};

impl<C> ControlFlowGraphBuilder<'_, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) fn enter_callable_body(&mut self, current: AnalysisBlockId) -> AnalysisBlockId {
        let body = match self.request().unit().root() {
            bray_bound_tree::BoundUnitRoot::CallableBody {
                body,
                execution: bray_symbols::CallableExecution::Asynchronous,
                ..
            }
            | bray_bound_tree::BoundUnitRoot::AnonymousCallable {
                body,
                execution: bray_symbols::CallableExecution::Asynchronous,
                ..
            } => body,
            _ => return current,
        };

        let cancellation = self.push_block();
        let resume = self.push_block();

        self.push_edge(
            current,
            cancellation,
            AnalysisEdgeKind::EntryCancellation,
            None,
        );

        self.push_exit(cancellation, AnalysisExitKind::Cancellation, body.into());
        self.push_edge(current, resume, AnalysisEdgeKind::Sequential, None);

        resume
    }

    pub(super) fn push_invocation(
        &mut self,
        expression: BoundExpressionId,
        current: AnalysisBlockId,
        may_fail: bool,
    ) -> AnalysisBlockId {
        let mut current = current;
        let mut may_fail = may_fail;

        let checks_preconditions = self
            .request()
            .trusted_contracts()
            .and_then(|contracts| contracts.calls.get(&expression.into()))
            .is_some_and(|contract| contract.checks_preconditions)
            || self
                .selections()
                .and_then(|selections| selections.expression(expression))
                .is_some_and(|selection| {
                    matches!(selection, SemanticSelection::Call(call)
                    if call.phase_behaviors().invocation().has_runtime_requirements())
                });

        if checks_preconditions
            && let Some(SemanticSelection::Call(call)) = self
                .selections()
                .and_then(|selections| selections.expression(expression))
        {
            may_fail = selected_call_may_propagate_panic(call);
        }

        current = self.begin_invocation(expression.into(), current, checks_preconditions);

        let reserves_factory = matches!(
            self.selections().and_then(|selections| selections.expression(expression)),
            Some(SemanticSelection::Call(call))
                if matches!(call.resolution().result(), bray_bound_tree::BoundCallResult::LazyFuture(_))
                    && call.implementation_hook()
                        .and_then(super::model::AnalysisTaskOperationKind::from_implementation_hook)
                        .is_none()
        );

        if reserves_factory {
            current = self.push_storage_admission(
                current, expression.into(),
                Some(super::model::AnalysisRefinement::InvocationBackingFailure(expression.into())),
            );
        }

        let continuation = self.push_block();

        self.push_call(current, expression.into(), AnalysisCallPhase::Attempt);

        self.push_edge(
            current,
            continuation,
            AnalysisEdgeKind::Sequential,
            Some(super::model::AnalysisRefinement::InvocationCompletion(
                expression.into(),
            )),
        );

        if may_fail && !reserves_factory {
            self.push_dependency_failures(current, expression.into());
        }

        self.push_call(
            continuation,
            expression.into(),
            AnalysisCallPhase::Completion,
        );

        continuation
    }

    pub(super) fn begin_invocation(
        &mut self,
        invocation: bray_bound_tree::BoundExecutionSite,
        current: AnalysisBlockId,
        checks_preconditions: bool,
    ) -> AnalysisBlockId {
        self.push_call(current, invocation, AnalysisCallPhase::Input);

        if !checks_preconditions {
            return current;
        }

        let admitted = self.push_block();
        let failed = self.push_block();

        self.push_edge(
            current,
            failed,
            AnalysisEdgeKind::Sequential,
            Some(super::model::AnalysisRefinement::InvocationEntry {
                invocation,
                admitted: false,
            }),
        );

        // Predicate evaluation is certified total. A false condition fails entry with a panic.
        self.push_exit(failed, AnalysisExitKind::Panic, invocation.node());

        self.push_edge(current, admitted, AnalysisEdgeKind::Sequential,
            Some(super::model::AnalysisRefinement::InvocationEntry { invocation, admitted: true }));

        admitted
    }

    pub(super) fn push_protocol_invocation(
        &mut self,
        invocation: bray_bound_tree::BoundExecutionSite,
        current: AnalysisBlockId,
    ) -> AnalysisBlockId {
        let checks = self.request().trusted_contracts().is_none_or(|contracts| {
            contracts
                .calls
                .get(&invocation)
                .is_some_and(|contract| contract.checks_preconditions)
        });

        let current = self.begin_invocation(invocation, current, checks);
        let completed = self.push_block();

        self.push_call(current, invocation, AnalysisCallPhase::Attempt);

        self.push_edge(
            current,
            completed,
            AnalysisEdgeKind::Sequential,
            Some(super::model::AnalysisRefinement::InvocationCompletion(
                invocation,
            )),
        );

        self.push_dependency_failures(current, invocation);
        self.push_call(completed, invocation, AnalysisCallPhase::Completion);

        completed
    }

    pub(super) fn build_await(
        &mut self,
        id: BoundExpressionId,
        expression: BoundAwaitExpression,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let current = self.build_expression(expression.operand(), current)?;
        let current = current.unwrap_or_else(|| self.push_block());

        self.push_suspension(current, id.into(), AnalysisSuspensionKind::Await);

        let suspended = self.push_block();
        let resume = self.push_block();

        self.push_edge(current, suspended, AnalysisEdgeKind::Suspension, None);
        self.connect_suspended_invocation(suspended, resume, id.into());
        self.push_call(resume, id.into(), AnalysisCallPhase::Completion);

        Some(Some(self.continue_committed_invocation(resume, id)))
    }

    pub(super) fn connect_suspended_invocation(
        &mut self,
        suspended: AnalysisBlockId,
        resume: AnalysisBlockId,
        invocation: bray_bound_tree::BoundExecutionSite,
    ) {
        self.push_edge(
            suspended,
            resume,
            AnalysisEdgeKind::Resume,
            Some(super::model::AnalysisRefinement::InvocationCompletion(
                invocation,
            )),
        );

        for (kind, edge, exit) in [
            (bray_symbols::DependencyFailureKind::Cancellation, AnalysisEdgeKind::RunCancellation, AnalysisExitKind::Cancellation),
            (bray_symbols::DependencyFailureKind::Panic, AnalysisEdgeKind::Panic, AnalysisExitKind::Panic),
        ] {
            let failure = self.push_block();

            self.push_edge(suspended, failure, edge, Some(super::model::AnalysisRefinement::InvocationFailure(invocation, kind)));
            self.push_exit(failure, exit, invocation.node());
        }
    }

    pub(super) fn continue_committed_invocation(
        &mut self,
        completed: AnalysisBlockId,
        expression: BoundExpressionId,
    ) -> AnalysisBlockId {
        // A newly cancelled parent owns the value already transferred by the child.
        let cancelled = self.push_block();
        let continued = self.push_block();

        self.push_edge(completed, cancelled, AnalysisEdgeKind::RunCancellation, None);
        self.push_exit(cancelled, AnalysisExitKind::Cancellation, expression.into());
        self.push_edge(completed, continued, AnalysisEdgeKind::Sequential, None);

        continued
    }

    pub(super) fn build_call(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundCallExpression,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let current = self.build_call_inputs(id, expression, current)?;
        let hook = self.implementation_hook(id, expression);

        if hook == Some(ImplementationHook::CatastrophicAbort) {
            // Catastrophic termination skips source cleanup. Record it before the
            // later result analysis supplies the never type, matching MIR lowering.
            self.push_bound(current, id.into());
            self.push_exit(current, AnalysisExitKind::Divergence, id.into());

            return Some(None);
        }

        if hook == Some(ImplementationHook::CurrentRunCancellationPropagation) {
            self.push_bound(current, id.into());
            self.push_exit(current, AnalysisExitKind::Cancellation, id.into());

            return Some(None);
        }

        if hook == Some(ImplementationHook::TestingFail) {
            self.push_bound(current, id.into());
            self.push_exit(current, AnalysisExitKind::Panic, id.into());

            return Some(None);
        }

        if matches!(
            hook,
            Some(ImplementationHook::TaskYield | ImplementationHook::TaskEventWait)
        ) {
            self.push_suspension(current, id.into(), AnalysisSuspensionKind::Yield);

            let suspended = self.push_block();
            let resume = self.push_block();
            let cancellation = self.push_block();

            self.push_edge(current, suspended, AnalysisEdgeKind::Suspension, None);

            self.push_edge(
                suspended,
                resume,
                AnalysisEdgeKind::Resume,
                Some(super::model::AnalysisRefinement::InvocationCompletion(
                    id.into(),
                )),
            );

            self.push_call(resume, id.into(), AnalysisCallPhase::Completion);

            self.push_edge(
                suspended,
                cancellation,
                AnalysisEdgeKind::RunCancellation,
                None,
            );

            self.push_exit(cancellation, AnalysisExitKind::Cancellation, id.into());

            return Some(Some(resume));
        }

        Some(Some(self.push_invocation(id, current, self.call_may_propagate_panic(id, hook))))
    }

    fn build_call_inputs(
        &mut self,
        expression: BoundExpressionId,
        call: &BoundCallExpression,
        current: AnalysisBlockId,
    ) -> Option<AnalysisBlockId> {
        let current = self.build_expression(call.callee(), current)?;
        let current = current.unwrap_or_else(|| self.push_block());
        let selections = self.selections();

        let Some(selection) = selections
            .and_then(|selections| selections.expression(expression))
            .filter(|selection| selection.stages_inputs())
        else {
            return self.build_expressions(
                call.arguments()
                    .iter()
                    .map(|argument| argument.expression()),
                current,
            );
        };

        self.build_selected_inputs(expression, selection, current)
    }

    pub(super) fn build_selected_inputs(
        &mut self,
        expression: BoundExpressionId,
        selection: &SemanticSelection,
        mut current: AnalysisBlockId,
    ) -> Option<AnalysisBlockId> {
        for input in selection.inputs() {
            if !matches!(input, bray_bound_tree::SelectedInput::Receiver(_))
                && let Some(source) = input.expression()
            {
                current = self
                    .build_expression(source, current)?
                    .unwrap_or_else(|| self.push_block());
            }

            let ordinal = selection
                .input_ordinal(input.root())
                .expect("selected input has its ordinal");

            current = self.push_call_input(
                expression,
                ordinal.raw(),
                current,
                input.default_provider().is_some(),
            );
        }

        Some(current)
    }

    fn push_call_input(
        &mut self,
        expression: BoundExpressionId,
        ordinal: u32,
        current: AnalysisBlockId,
        may_fail: bool,
    ) -> AnalysisBlockId {
        let invocation = bray_bound_tree::BoundExecutionSite::CallInput {
            expression,
            ordinal: bray_symbols::SymbolOrdinal::new(ordinal),
        };

        let current = self.begin_invocation(invocation, current, false);
        let continuation = self.push_block();

        self.push_call(current, invocation, AnalysisCallPhase::Attempt);

        self.push_edge(
            current,
            continuation,
            AnalysisEdgeKind::Sequential,
            Some(super::model::AnalysisRefinement::InvocationCompletion(
                invocation,
            )),
        );

        if may_fail {
            self.push_dependency_failures(current, invocation);
        }

        self.push_call(continuation, invocation, AnalysisCallPhase::Completion);

        continuation
    }

    fn call_may_propagate_panic(
        &self,
        expression: BoundExpressionId,
        hook: Option<ImplementationHook>,
    ) -> bool {
        let hook_may_panic = implementation_hook_may_propagate_synchronous_panic(hook);

        let Some(selections) = self.selections() else {
            return hook_may_panic;
        };

        match selections.expression(expression) {
            Some(SemanticSelection::Call(selection)) => {
                // Inactive frame construction admits backing before captures move.
                // Its failure is synchronous even though the body executes later.
                selection
                    .phase_behaviors()
                    .invocation()
                    .has_runtime_requirements()
                    || selected_call_may_propagate_panic(selection)
            }
            // Union construction has call syntax and declaration-owned defaults.
            Some(SemanticSelection::Operation(operation)) => {
                hook_may_panic && operation.may_propagate_synchronous_panic(self.request().view().expression(expression)
                    .expect("checked operation retains its bound expression"))
            }
            _ => false,
        }
    }

    fn implementation_hook(
        &self,
        expression: BoundExpressionId,
        call: &BoundCallExpression,
    ) -> Option<ImplementationHook> {
        if let Some(SemanticSelection::Call(selection)) = self
            .selections()
            .and_then(|analysis| analysis.expression(expression))
            && let Some(hook) = selection.implementation_hook()
        {
            return Some(hook);
        }

        let target = call.resolution().resolved()?.target().declaration()?;

        self.request()
            .available_compiler_known_symbols()
            .symbol_implementation(target.symbol())
    }
}

/// Whether evaluation can fail independently of caller-side requirement checks.
fn selected_call_may_propagate_panic(selection: &bray_bound_tree::SelectedCall) -> bool {
    selection.phase_behaviors().invocation().may_unwind()
        && implementation_hook_may_propagate_synchronous_panic(selection.implementation_hook())
        && !selection
            .phase_behaviors()
            .invocation()
            .execution_properties()
            .contains(&bray_symbols::ExecutionProperty::Total)
        && selection.invocation() == CallableInvocation::Bray
}

/// Whether a compiler hook can propagate synchronous failure on valid inputs.
pub const fn implementation_hook_may_propagate_synchronous_panic(
    hook: Option<ImplementationHook>,
) -> bool {
    matches!(
        hook,
        None | Some(
            ImplementationHook::FutureStart
                | ImplementationHook::TaskJoin
                | ImplementationHook::TaskCancel
                | ImplementationHook::NativeThreadStart
                | ImplementationHook::BranchingInlineAssembly
                | ImplementationHook::RawAllocate
                | ImplementationHook::RawBufferAllocate
                | ImplementationHook::RawDeallocate
                | ImplementationHook::Allocate
                | ImplementationHook::Deallocate
                | ImplementationHook::RawBufferRelease
                | ImplementationHook::RawBufferReplace
        )
    )
}

pub(crate) fn exclude_impossible_call_outcomes<C: CheckerRequestContext + ?Sized>(
    graph: &super::model::ControlFlowGraph,
    request: crate::CheckerUnitView<'_, C>,
    excluded: &mut std::collections::BTreeSet<super::id::AnalysisEdgeId>,
) {
    excluded.extend(graph.edges().iter().filter_map(|edge| {
        let super::model::AnalysisRefinement::InvocationFailure(invocation, kind) = edge.refinement()? else { return None; };

        let bray_bound_tree::BoundExecutionSite::Node(bray_bound_tree::AnyBoundNodeId::Expression(
            expression,
        )) = invocation
        else {
            return None;
        };

        (match kind {
            bray_symbols::DependencyFailureKind::Cancellation => request.call_cannot_cancel(expression),
            bray_symbols::DependencyFailureKind::Panic => request.call_cannot_unwind(expression),
        })
        .then_some(edge.id())
    }));
}

#[cfg(test)]
mod tests {
    use bray_compiler_known::ImplementationHook;

    use super::implementation_hook_may_propagate_synchronous_panic;

    #[test]
    fn fallible_compiler_hooks_preserve_synchronous_panics() {
        assert!(implementation_hook_may_propagate_synchronous_panic(Some(
            ImplementationHook::NativeThreadStart,
        )));

        assert!(implementation_hook_may_propagate_synchronous_panic(Some(
            ImplementationHook::BranchingInlineAssembly,
        )));

        for hook in [
            ImplementationHook::FutureStart,
            ImplementationHook::TaskJoin,
            ImplementationHook::TaskCancel,
            ImplementationHook::RawAllocate,
            ImplementationHook::RawBufferAllocate,
            ImplementationHook::RawDeallocate,
            ImplementationHook::Allocate,
            ImplementationHook::Deallocate,
            ImplementationHook::RawBufferRelease,
            ImplementationHook::RawBufferReplace,
        ] {
            assert!(implementation_hook_may_propagate_synchronous_panic(Some(
                hook
            )));
        }

    }
}
