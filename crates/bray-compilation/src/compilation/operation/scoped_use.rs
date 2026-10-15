use bray_binder::{BindingQueryContext, semantic_unit_context};
use bray_bound_tree::{
    BoundStructuredExpression, BoundUnit, CheckedExpressionTypes, SelectedScopedUse,
};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticLabel,
    DiagnosticLabelKind, DiagnosticSelectionKind, SeverityKind,
};
use bray_source::SourceSpan;
use bray_symbols::{
    GenericArgument, NamedTypeSymbolId, TypeAssociatedLifecycleSlot, TypeData, TypeId,
};

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

        let (capability, enter_failure) = self.scoped_result_types(enter.1.result())?;

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

        let (_, exit_failure) = self.scoped_result_types(exit.1.result())?;

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
        let values = self.semantic_value_store()?;

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

    fn scoped_result_types(
        &self,
        result: TypeId,
    ) -> Result<(TypeId, Option<TypeId>), FactQueryError> {
        let values = self.semantic_value_store()?;
        let data = values.type_data(result);

        let TypeData::Named {
            definition,
            substitution,
        } = data.as_ref()
        else {
            return Ok((result, None));
        };

        let available = self.available_compiler_known_symbols();

        let role = match definition {
            NamedTypeSymbolId::Struct(symbol) => available.symbol_representation(*symbol),
            NamedTypeSymbolId::Union(symbol) => available.symbol_representation(*symbol),
        };

        if role != Some(RepresentationRole::Result) {
            return Ok((result, None));
        }

        let substitution = values.generic_substitution_data(*substitution);

        let [success, error] = substitution.bindings() else {
            panic!(
                "selected scope enter Result type {result:?} retains its success and error arguments"
            );
        };

        let GenericArgument::Type(success) = success.argument() else {
            panic!(
                "selected scope enter Result type {result:?} retains a type-valued success argument"
            );
        };

        let GenericArgument::Type(error) = error.argument() else {
            panic!("selected scoped Result type {result:?} retains a type-valued error argument");
        };

        Ok((success, Some(error)))
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
