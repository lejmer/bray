use bray_bound_tree::{BoundExpressionId, CheckedExpressionSemantics, CheckedPatterns};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticBag,
    DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind, DiagnosticNote, DiagnosticNoteKind,
    SeverityKind,
};
use bray_source::SourceSpan;

use crate::{CheckerOutcome, CheckerRequestContext, CheckerUnitRoot, CheckerUnitView};

use super::build::{
    ControlFlowGraphBuildOutcome, ControlFlowGraphBuilder, build_result_control_flow_graph,
};
use super::id::AnalysisBlockId;
use super::model::AnalysisExitKind;
use super::reachability::analyze_reachability;

pub(crate) fn check_callable_result<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    expressions: &CheckedExpressionSemantics,
    patterns: &CheckedPatterns,
) -> CheckerOutcome<(), C::UpstreamError> {
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

    let graph = match build_result_control_flow_graph(request, expressions, patterns) {
        ControlFlowGraphBuildOutcome::Complete(graph) => graph,
        ControlFlowGraphBuildOutcome::Cancelled => return CheckerOutcome::Cancelled,
        ControlFlowGraphBuildOutcome::InfrastructureFailure(error) => {
            return CheckerOutcome::InfrastructureFailure(error);
        }
        ControlFlowGraphBuildOutcome::UpstreamFailure(error) => match error {},
    };

    let Some(reachability) = analyze_reachability(&graph, request) else {
        return CheckerOutcome::Cancelled;
    };

    if !reachability.completion().can_complete_normally() {
        return CheckerOutcome::without_diagnostics(());
    }

    let expected = match crate::diagnostic::diagnostic_type(request.context(), result) {
        Ok(bray_diagnostics::DiagnosticType::Error) => {
            return CheckerOutcome::without_diagnostics(());
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

    let mut diagnostics = DiagnosticBag::new();

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
            .completion_facts
            .and_then(|(expressions, _)| expressions.types().expression(id))
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
