use bray_bound_tree::{BoundExpressionId, CheckedExpressionSemantics};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticBag,
    DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind, DiagnosticNote, DiagnosticNoteKind,
    SeverityKind,
};
use bray_source::SourceSpan;

use crate::{CheckerOutcome, CheckerRequestContext, CheckerUnitRoot, CheckerUnitView};

use super::build::ControlFlowGraphBuilder;
use super::id::AnalysisBlockId;
use super::model::AnalysisExitKind;

pub(crate) fn check_callable_result<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    expressions: &CheckedExpressionSemantics,
    can_fall_through: bool,
) -> CheckerOutcome<(), C::UpstreamError> {
    let CheckerUnitRoot::CallableBody(_) = request.root() else {
        return CheckerOutcome::without_diagnostics(());
    };

    let Some(result) = expressions.types().callable_result_type() else {
        return CheckerOutcome::without_diagnostics(());
    };

    if let Some(RepresentationRole::Unit) =
        crate::representation::type_representation(request, result)
    {
        return CheckerOutcome::without_diagnostics(());
    }

    if !can_fall_through {
        return CheckerOutcome::without_diagnostics(());
    }

    let mut diagnostics = DiagnosticBag::new();

    let expected = match crate::diagnostic::diagnostic_type(request.context(), result) {
        Ok(bray_diagnostics::DiagnosticType::Error) => {
            return CheckerOutcome::complete((), diagnostics);
        }
        Ok(expected) => expected,
        Err(crate::CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
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

impl<C: CheckerRequestContext + ?Sized> ControlFlowGraphBuilder<'_, C> {
    pub(super) fn push_result(
        &mut self,
        block: AnalysisBlockId,
        expression: BoundExpressionId,
        phase: super::model::AnalysisResultPhase,
    ) {
        if self.view().node_is_recovered(expression.into()) == Some(false) {
            self.storage.push_operation(
                block,
                super::model::AnalysisOperationKind::Result { expression, phase },
            );
        } else {
            self.push_recovery(block, expression.into());
        }
    }

    pub(super) fn build_expression(
        &mut self,
        id: BoundExpressionId,
        current: AnalysisBlockId,
    ) -> Option<Option<AnalysisBlockId>> {
        let result_region = self
            .view()
            .expression(id)
            .is_some_and(bray_bound_tree::BoundExpression::has_result_region);

        let loop_header = matches!(self.view().expression(id),
            Some(bray_bound_tree::BoundExpression::Structured(operation))
                if matches!(operation.kind(),
                    bray_bound_tree::BoundStructuredExpressionKind::While
                        | bray_bound_tree::BoundStructuredExpressionKind::Loop)
        );

        if result_region && !loop_header {
            self.push_result(current, id, super::model::AnalysisResultPhase::Begin);
        }

        let completion = self.build_expression_inner(id, current)?;

        let Some(current) = completion else {
            return Some(None);
        };

        if result_region {
            self.push_result(current, id, super::model::AnalysisResultPhase::Completion);
        }

        if let Some(ty) = self
            .completion_semantics
            .and_then(|expressions| expressions.types().expression(id))
            .map(|entry| entry.ty())
            && let Some(bray_compiler_known::RepresentationRole::Never) =
                crate::representation::type_representation(self.request(), ty)
        {
            self.push_exit(current, AnalysisExitKind::Divergence, id.into());

            return Some(None);
        }

        Some(Some(current))
    }
}
