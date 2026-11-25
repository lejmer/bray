use bray_binder::{BindingQueryContext, semantic_unit_context};
use bray_bound_tree::{
    BoundStructuredExpression, BoundUnit, CheckedExpressionTypes, SelectedScopedUse,
};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticLabel,
    DiagnosticLabelKind, DiagnosticSelectionKind, SeverityKind,
};
use bray_source::SourceSpan;
use bray_symbols::{TypeAssociatedLifecycleSlot, TypeData, TypeId};

use super::super::{Compilation, binder::CompilationBindingContext};
use super::model::{OperationSubject, SemanticResolution};
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    #[expect(
        clippy::too_many_arguments,
        reason = "scoped selection retains its actual subject, types, cancellation and diagnostics"
    )]
    pub(super) fn resolve_scoped_use(
        &self,
        key: &OperationSubject,
        binding_context: &CompilationBindingContext<'_>,
        unit: &BoundUnit,
        types: &CheckedExpressionTypes,
        expression: &BoundStructuredExpression,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<SemanticResolution>, FactQueryError> {
        let [initializer] = expression.operands() else {
            panic!(
                "with occurrence {:?} must retain its sole initializer",
                key.expression()
            );
        };

        let source = types
            .expression(*initializer)
            .expect("scoped initializer has provisional type evidence");

        if source.is_recovered() {
            return Ok(None);
        }

        let subject =
            self.resolve_access_subject_type(binding_context, source.ty(), diagnostics)?;

        let enter = self.selected_lifecycle_signature(
            subject,
            TypeAssociatedLifecycleSlot::ScopeEnter,
            cancellation,
        )?;

        diagnostics.add_range(enter.diagnostics().iter().cloned());

        let Some(enter) = enter.into_parts().0 else {
            diagnostics.add(scoped_selection_diagnostic(
                expression,
                DiagnosticSelectionKind::ScopeEnter,
            ));

            return Ok(None);
        };

        let values = self.semantic_value_store()?;
        let symbols = self.available_compiler_known_symbols();
        let enter_type = enter.1.result();

        let (capability, enter_failure) = symbols.result_payloads(values, enter_type)
            .map_or((enter_type, None), |([success, error], _)| (success, Some(error)));

        let exit = self.selected_lifecycle_signature(
            subject,
            TypeAssociatedLifecycleSlot::ScopeExit,
            cancellation,
        )?;

        diagnostics.add_range(exit.diagnostics().iter().cloned());

        let Some(exit) = exit.into_parts().0 else {
            diagnostics.add(scoped_selection_diagnostic(
                expression,
                DiagnosticSelectionKind::ScopeExit,
            ));

            return Ok(None);
        };

        let [parameter] = exit.1.parameters() else {
            panic!(
                "selected scoped exit at {:?} must retain its sole capability parameter",
                key.expression()
            );
        };

        if parameter.ty() != capability {
            self.scoped_type_mismatch(
                unit,
                expression,
                parameter.ty(),
                capability,
                cancellation,
                diagnostics,
            )?;

            return Ok(None);
        }

        let exit_failure = symbols.result_payloads(values, exit.1.result())
            .map(|([_, error], _)| error);

        if let (Some(enter), Some(exit)) = (enter_failure, exit_failure)
            && enter != exit
        {
            self.scoped_type_mismatch(unit, expression, enter, exit, cancellation, diagnostics)?;

            return Ok(None);
        }

        let failure = enter_failure.or(exit_failure);

        let context = self.checker_context_for(unit.key(), cancellation)?;
        let semantic_context = semantic_unit_context(binding_context.symbols(), unit);
        let request = bray_checker::CheckerUnitView::new(unit, &semantic_context, &context);

        let invocation_result = |signature: &bray_symbols::CallableSignature| {
            let data = values.type_data(signature.callable_type());

            let TypeData::Callable(callable) = data.as_ref() else {
                panic!("selected scoped signature must retain its callable type");
            };

            bray_checker::call_result(request, callable, signature.result())
        };

        let enter_result = invocation_result(&enter.1);
        let exit_result = invocation_result(&exit.1);

        Ok(Some(SemanticResolution::scoped_use(
            SelectedScopedUse::new(
                key.expression(),
                *initializer,
                source.ty(),
                capability,
                failure,
                enter,
                exit,
                enter_result,
                exit_result,
            ),
        )))
    }

    fn scoped_type_mismatch(
        &self,
        unit: &BoundUnit,
        expression: &BoundStructuredExpression,
        expected: TypeId,
        actual: TypeId,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), FactQueryError> {
        let context = self.checker_context_for(unit.key(), cancellation)?;
        let anchor = expression.origin().source_anchor().syntax();
        let span = SourceSpan::new(anchor.source_id(), anchor.full_range());

        diagnostics.add(
            Diagnostic::new(
                DiagnosticId::new(anchor.full_range().start().bytes()),
                DiagnosticKind::CheckingIncompatibleExpressionType,
                SeverityKind::Error,
            )
            .with_primary_span(span)
            .with_label(DiagnosticLabel::primary(
                DiagnosticLabelKind::IncompatibleExpressionType,
                span,
            ))
            .with_arg(DiagnosticArg::expected_type(bray_checker::diagnostic_type(
                &context, expected,
            )?))
            .with_arg(DiagnosticArg::actual_type(bray_checker::diagnostic_type(
                &context, actual,
            )?))
            .with_arg(DiagnosticArg::selection_kind(
                DiagnosticSelectionKind::ScopeExit,
            )),
        );

        Ok(())
    }

}

fn scoped_selection_diagnostic(
    expression: &BoundStructuredExpression,
    kind: DiagnosticSelectionKind,
) -> Diagnostic {
    let anchor = expression.origin().source_anchor().syntax();
    let span = SourceSpan::new(anchor.source_id(), anchor.full_range());

    Diagnostic::new(
        DiagnosticId::new(anchor.full_range().start().bytes()),
        DiagnosticKind::CheckingNoApplicableCandidate,
        SeverityKind::Error,
    )
    .with_primary_span(span)
    .with_label(DiagnosticLabel::primary(
        DiagnosticLabelKind::SelectionFailure,
        span,
    ))
    .with_arg(DiagnosticArg::selection_kind(kind))
}
