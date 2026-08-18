use std::collections::BTreeMap;

use bray_bound_tree::CheckedTemplateKind;
use bray_checker::{
    CheckerUnitView, ConstantEvaluationInput, ConstantEvaluationLimits, ConstantEvaluator,
    DefaultConstantEvaluator, EvaluatedConstantCall, evaluate_static_initializer_template,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{StaticInstanceKey, StaticStorageDuration};

use super::support::substitute_expression_types;
use crate::compilation::Compilation;
use crate::compilation::binder::{binding_query_error, imported_declaration_template};
use crate::compilation::checker::checker_result;
use crate::compilation::constant::call::{
    CompilationConstantCallResolver, CompilationConstantTemplateResolver,
};
use crate::compilation::unit::semantic_unit_context_for;
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(in crate::compilation) fn evaluate_static_initializer(
        &self,
        instance: &StaticInstanceKey,
        duration: StaticStorageDuration,
        result_type: bray_symbols::TypeId,
        limits: ConstantEvaluationLimits,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<EvaluatedConstantCall>, FactQueryError> {
        let declaration = instance.template().declaration();

        let Some(key) = self.static_initializer_key(declaration)? else {
            return self.evaluate_imported_static_initializer(
                instance,
                duration,
                result_type,
                limits,
                cancellation,
            );
        };

        let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;
        let semantics = self.expression_semantics_with_cancellation(key.clone(), cancellation)?;
        let context = self.checker_context_for(&key, cancellation)?;

        let semantic_context =
            semantic_unit_context_for(context.symbols(), bound.result().value())?;

        let types = substitute_expression_types(
            self.semantic_value_store()?,
            &semantics.result().value().0,
            instance.substitution().substitution(),
        )?;

        let (references, dependency_diagnostics) = self.concrete_call_references(
            bound.result().value(),
            &semantics.result().value().1,
            instance.substitution().substitution(),
            None,
            &BTreeMap::new(),
            limits,
            cancellation,
        )?;

        let resolver = CompilationConstantCallResolver::new(self, cancellation);

        let input = ConstantEvaluationInput::new(&types, &semantics.result().value().1)
            .with_references(references)
            .with_call_resolver(&resolver)
            .with_static_address_borrows()
            .with_limits(limits);

        let unit = CheckerUnitView::new(bound.result().value(), &semantic_context, &context)
            .map_err(|error| {
                FactQueryError::CheckerInfrastructure(
                    bray_checker::CheckerInfrastructureError::InvalidUnitView(error),
                )
            })?;

        let evaluated = checker_result(
            DefaultConstantEvaluator.evaluate_constant_with_references(unit, &input),
        )?;

        Ok(DiagnosticResult::new(
            EvaluatedConstantCall::new(evaluated.value().value(), evaluated.value().usage()),
            dependency_diagnostics.merged(evaluated.diagnostics()),
        ))
    }

    fn evaluate_imported_static_initializer(
        &self,
        instance: &StaticInstanceKey,
        duration: StaticStorageDuration,
        result_type: bray_symbols::TypeId,
        limits: ConstantEvaluationLimits,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<EvaluatedConstantCall>, FactQueryError> {
        let declaration = instance.template().declaration();
        let binding_context = self.binding_context(cancellation)?;

        let address = binding_context
            .imported_semantic_address(declaration.into())
            .map_err(binding_query_error)?
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let kind = match duration {
            StaticStorageDuration::Product => CheckedTemplateKind::ProductStaticInitializer,
            StaticStorageDuration::ExactThread => CheckedTemplateKind::ThreadLocalStaticInitializer,
        };

        let template = imported_declaration_template(&binding_context, address, kind)
            .map_err(binding_query_error)?;

        let imported = template
            .value()
            .as_ref()
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let imported_symbols =
            self.imported_symbol_skeleton_result_with_cancellation(cancellation)?;

        let imported_symbols = imported_symbols
            .value()
            .as_deref()
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let context = self.checker_context(cancellation)?;

        let resolver =
            CompilationConstantTemplateResolver::new(self, cancellation, imported_symbols);

        let diagnostic_span = self
            .dependency_interface_input(address.interface())
            .and_then(crate::request::DependencyInterfaceInput::dependency_span);

        let evaluated = checker_result(evaluate_static_initializer_template(
            &context,
            imported.template(),
            kind,
            instance.substitution(),
            result_type,
            &resolver,
            diagnostic_span,
            limits,
        ))?;

        let diagnostics =
            DiagnosticBag::merged_all([template.diagnostics(), evaluated.diagnostics()]);

        if let Some(evaluated) = evaluated.value() {
            return Ok(DiagnosticResult::new(*evaluated, diagnostics));
        }

        let value = self
            .semantic_value_store()?
            .intern_error_constant_value(result_type)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        Ok(DiagnosticResult::new(
            EvaluatedConstantCall::new(value, Default::default()),
            diagnostics,
        ))
    }
}
