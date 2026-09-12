use bray_bound_tree::{
    BoundExpressionId, CheckedBodySemantics, CheckedControlFlow, CheckedExpressionSemantics,
    CheckedMemoryOperations, CheckedPatterns, StoragePlan,
};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticBag,
    DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind, DiagnosticNote, DiagnosticNoteKind,
    SeverityKind,
};
use bray_source::SourceSpan;
use bray_symbols::CallableSignatureQuery;

use crate::{
    CheckerOutcome, CheckerRequestContext, CheckerSemanticQueryProvider, CheckerUnitRoot,
    CheckerUnitView,
};

use super::build::{
    ControlFlowGraphBuildOutcome, ControlFlowGraphBuilder, build_storage_control_flow_graph,
};
use super::cleanup::{CleanupFreeExits, record_cleanup_free_exits};
use super::id::AnalysisBlockId;
use super::model::AnalysisExitKind;
use super::reachability::analyze_reachability;

pub(crate) fn check_callable_result<C>(
    request: CheckerUnitView<'_, C>,
    control_flow: &CheckedControlFlow,
    expressions: &CheckedExpressionSemantics,
    patterns: &CheckedPatterns,
    storage: &StoragePlan,
    memory: &CheckedMemoryOperations,
    semantics: &CheckedBodySemantics,
) -> CheckerOutcome<(), C::UpstreamError>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
{
    let CheckerUnitRoot::CallableBody(_) = request.root() else {
        return CheckerOutcome::without_diagnostics(());
    };

    let Some(result) = expressions.types().callable_result_type() else {
        return CheckerOutcome::without_diagnostics(());
    };

    match crate::representation::type_representation(request, result) {
        Ok(Some(RepresentationRole::Unit)) => return CheckerOutcome::without_diagnostics(()),
        Ok(_) => {}
        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
    }

    let mut diagnostics = match callable_can_fall_through(
        request,
        control_flow,
        expressions,
        patterns,
        storage,
        memory,
        semantics,
    ) {
        CheckerOutcome::Complete(result) => {
            let (can_fall_through, diagnostics) = result.into_parts();

            if !can_fall_through {
                return CheckerOutcome::complete((), diagnostics);
            }

            diagnostics
        }
        CheckerOutcome::Cancelled => return CheckerOutcome::Cancelled,
        CheckerOutcome::InfrastructureFailure(error) => {
            return CheckerOutcome::InfrastructureFailure(error);
        }
        CheckerOutcome::UpstreamFailure(error) => {
            return CheckerOutcome::UpstreamFailure(error);
        }
    };

    let expected = match crate::diagnostic::diagnostic_type(request.context(), result) {
        Ok(bray_diagnostics::DiagnosticType::Error) => {
            return CheckerOutcome::complete((), diagnostics);
        }
        Ok(expected) => expected,
        Err(crate::CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
        Err(crate::CheckerQueryError::Infrastructure(error)) => {
            return CheckerOutcome::InfrastructureFailure(error);
        }
        Err(crate::CheckerQueryError::Upstream(error)) => {
            return CheckerOutcome::UpstreamFailure(error);
        }
    };

    let anchor = request.unit().key().source().syntax();
    let span = SourceSpan::new(anchor.source_id(), anchor.full_range());

    let name = request
        .containing_callable()
        .and_then(|callable| request.symbols().member_name(callable.into_any()))
        .map_or_else(
            || {
                DiagnosticArg::new(
                    DiagnosticArgName::DeclarationName,
                    DiagnosticArgValue::SyntaxKind(anchor.syntax_kind()),
                )
            },
            |name| DiagnosticArg::declaration_name(name.as_str()),
        );

    diagnostics.add(
        Diagnostic::new(
            crate::diagnostic::diagnostic_id(0),
            DiagnosticKind::CheckingCallableResultRequired,
            SeverityKind::Error,
        )
        .with_primary_span(span)
        .with_arg(name)
        .with_arg(DiagnosticArg::expected_type(expected))
        .with_label(DiagnosticLabel::primary(
            DiagnosticLabelKind::CallableResultRequired,
            span,
        ))
        .with_note(DiagnosticNote::new(
            DiagnosticNoteKind::ReturnRequiredResult,
        )),
    );

    CheckerOutcome::complete((), diagnostics)
}

fn callable_can_fall_through<C>(
    request: CheckerUnitView<'_, C>,
    control_flow: &CheckedControlFlow,
    expressions: &CheckedExpressionSemantics,
    patterns: &CheckedPatterns,
    storage: &StoragePlan,
    memory: &CheckedMemoryOperations,
    semantics: &CheckedBodySemantics,
) -> CheckerOutcome<bool, C::UpstreamError>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
{
    let mut cleanup_free = CleanupFreeExits::new();
    record_cleanup_free_exits(&mut cleanup_free, semantics.asynchronous());

    let mut diagnostics = DiagnosticBag::new();
    let mut convergence_bound = None;

    // Removing impossible cleanup failures can make further transfers definite in nested catches.
    // Each refinement proves another exit cleanup-free and never adds a control-flow path.
    for round in 0.. {
        let graph = match build_storage_control_flow_graph(
            request,
            storage,
            expressions.selections(),
            Some((expressions, patterns, &cleanup_free)),
        ) {
            ControlFlowGraphBuildOutcome::Complete(graph) => graph,
            ControlFlowGraphBuildOutcome::Cancelled => return CheckerOutcome::Cancelled,
            ControlFlowGraphBuildOutcome::InfrastructureFailure(error) => {
                return CheckerOutcome::InfrastructureFailure(error);
            }
            ControlFlowGraphBuildOutcome::UpstreamFailure(error) => {
                return CheckerOutcome::UpstreamFailure(error);
            }
        };

        // Every new proof identifies a scope-exit operation in the first graph.
        if round > *convergence_bound.get_or_insert(graph.operations().len()) {
            panic!("finite cleanup refinement exceeded its convergence bound");
        }

        let Some(reachability) = analyze_reachability(&graph, request) else {
            return CheckerOutcome::Cancelled;
        };

        if !reachability.completion().can_complete_normally() {
            return CheckerOutcome::complete(false, diagnostics);
        }

        let refined = match crate::body_semantics::check_body_semantics_with_graph(
            request,
            control_flow,
            expressions,
            patterns,
            storage,
            memory,
            &graph,
        ) {
            CheckerOutcome::Complete(result) => {
                let (semantics, owned) = result.into_parts();

                diagnostics = diagnostics.merged(&owned);

                semantics
            }
            CheckerOutcome::Cancelled => return CheckerOutcome::Cancelled,
            CheckerOutcome::InfrastructureFailure(error) => {
                return CheckerOutcome::InfrastructureFailure(error);
            }
            CheckerOutcome::UpstreamFailure(error) => {
                return CheckerOutcome::UpstreamFailure(error);
            }
        };

        if !record_cleanup_free_exits(&mut cleanup_free, refined.asynchronous()) {
            return CheckerOutcome::complete(true, diagnostics);
        }
    }

    unreachable!("cleanup refinement returns when no new exit is proven cleanup-free")
}

impl<C: CheckerRequestContext + ?Sized> ControlFlowGraphBuilder<'_, C> {
    pub(super) fn build_expression(
        &mut self,
        id: BoundExpressionId,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let completion = self.build_expression_inner(id, current)?;

        let Some(current) = completion else {
            return Some(None);
        };

        if let Some(ty) = self
            .completion_semantics
            .and_then(|(expressions, _, _)| expressions.types().expression(id))
            .map(|entry| entry.ty())
        {
            match crate::representation::type_representation(self.request(), ty) {
                Ok(Some(bray_compiler_known::RepresentationRole::Never)) => {
                    self.push_exit(current, AnalysisExitKind::Divergence, id.into());

                    return Some(None);
                }
                Ok(_) => {}
                Err(error) => self.record_infrastructure_failure(error),
            }
        }

        Some(Some(current))
    }
}
