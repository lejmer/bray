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
    AnalysisTaskOperationKind,
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

        if let Some(SemanticSelection::Call(call)) = self
            .selections()
            .and_then(|selections| selections.expression(expression))
            && call
                .phase_behaviors()
                .invocation()
                .has_runtime_requirements()
            && !call.evaluates_defaults()
            && call.arguments().iter().all(|argument| {
                !matches!(argument,
                    bray_bound_tree::SelectedArgument::Explicit { conversion, .. }
                        if conversion.invokes_source())
            })
        {
            // Input expressions have completed. An entry failure cannot commit the
            // callee's effects. Effectful implicit input evaluation retains its
            // existing conservative combined transition.
            may_fail = selected_call_may_propagate_panic(call);

            let admitted = self.push_block();
            let failed = self.push_block();

            self.push_edge(
                current,
                failed,
                AnalysisEdgeKind::Sequential,
                Some(super::model::AnalysisRefinement::PreconditionFailure(
                    expression.into(),
                )),
            );

            self.push_exit(failed, AnalysisExitKind::Panic, expression.into());
            self.push_exit(failed, AnalysisExitKind::Cancellation, expression.into());
            self.push_edge(current, admitted, AnalysisEdgeKind::Sequential, None);
            current = admitted;
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

        if may_fail {
            self.push_dependency_failures(current, expression.into());
        }

        self.push_call(
            continuation,
            expression.into(),
            AnalysisCallPhase::Completion,
        );

        continuation
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
            Some(super::model::AnalysisRefinement::InvocationFailure(
                id.into(),
            )),
        );

        self.push_exit(cancellation, AnalysisExitKind::Cancellation, id.into());

        let failure = self.push_block();

        self.push_edge(
            suspended,
            failure,
            AnalysisEdgeKind::Panic,
            Some(super::model::AnalysisRefinement::InvocationFailure(
                id.into(),
            )),
        );

        self.push_exit(failure, AnalysisExitKind::Panic, id.into());

        Some(Some(resume))
    }

    pub(super) fn build_call(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundCallExpression,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let operands = std::iter::once(expression.callee()).chain(
            expression
                .arguments()
                .iter()
                .map(|argument| argument.expression()),
        );

        let current = self.build_expressions(operands, current)?;
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

        if self.call_may_propagate_panic(id, hook) {
            return Some(Some(self.push_invocation(id, current, true)));
        }

        match hook.and_then(AnalysisTaskOperationKind::from_implementation_hook) {
            Some(kind) => self.push_task_operation(current, id, kind),
            None => {
                return Some(Some(self.push_invocation(id, current, false)));
            }
        }

        Some(Some(current))
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
                hook_may_panic && operation.may_propagate_synchronous_panic()
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
    selection.evaluates_defaults()
        || (selection.phase_behaviors().invocation().may_unwind()
            && implementation_hook_may_propagate_synchronous_panic(selection.implementation_hook())
            && !selection
                .phase_behaviors()
                .invocation()
                .execution_properties()
                .contains(&bray_symbols::ExecutionProperty::Total)
            && selection.invocation() == CallableInvocation::Bray)
}

/// Whether a compiler hook can propagate synchronous failure on valid inputs.
pub const fn implementation_hook_may_propagate_synchronous_panic(
    hook: Option<ImplementationHook>,
) -> bool {
    matches!(
        hook,
        None | Some(
            ImplementationHook::NativeThreadStart
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

pub(crate) fn exclude_non_unwinding_edges<C: CheckerRequestContext + ?Sized>(
    graph: &super::model::ControlFlowGraph,
    request: crate::CheckerUnitView<'_, C>,
    excluded: &mut std::collections::BTreeSet<super::id::AnalysisEdgeId>,
) {
    excluded.extend(graph.edges().iter().filter_map(|edge| {
        let Some(super::model::AnalysisRefinement::InvocationFailure(
            bray_bound_tree::BoundExecutionSite::Node(bray_bound_tree::AnyBoundNodeId::Expression(
                expression,
            )),
        )) = edge.refinement()
        else {
            return None;
        };

        request.call_cannot_unwind(expression).then_some(edge.id())
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

        assert!(!implementation_hook_may_propagate_synchronous_panic(Some(
            ImplementationHook::FutureStart,
        )));
    }
}
