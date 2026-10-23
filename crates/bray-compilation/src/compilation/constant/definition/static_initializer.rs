use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_bound_tree::CheckedTemplateKind;
use bray_checker::{
    ConstantEvaluationLimits, EvaluatedConstantCall, evaluate_static_initializer_template,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_source::SourceSpan;
use bray_symbols::{
    StaticInstanceKey, StaticInstanceTemplateQuery, StaticStorageDuration, SymbolQueryRequest,
};

use crate::compilation::Compilation;
use crate::compilation::binder::{
    binding_query_error, imported_declaration_template, visible_generic_parameters,
};
use crate::compilation::checker::checker_result;
use crate::compilation::constant::call::CompilationConstantTemplateResolver;
use crate::compilation::template::generic_template_inputs;
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
        let binding = self.binding_context(cancellation)?;

        let kind = match duration {
            StaticStorageDuration::Product => CheckedTemplateKind::ProductStaticInitializer,
            StaticStorageDuration::ExactThread => CheckedTemplateKind::ThreadLocalStaticInitializer,
        };

        let (template, span) = if let Some(key) = self.static_initializer_key(declaration)? {
            let declared = binding
                .resolve_symbol_query(SymbolQueryRequest::<StaticInstanceTemplateQuery>::new(
                    declaration,
                ))
                .map_err(binding_query_error)?;

            let span = SourceSpan::new(
                key.source().syntax().source_id(),
                key.source().syntax().full_range(),
            );

            let template = if declared.diagnostics().has_errors() {
                DiagnosticResult::new(None, declared.diagnostics().clone())
            } else {
                let inputs = generic_template_inputs(
                    self,
                    visible_generic_parameters(binding.symbols(), declaration.into()),
                )?;

                let expression = key.source().syntax();

                self.checked_source_template(
                    key,
                    kind,
                    expression,
                    inputs,
                    declared.value().dependency_contract(),
                    cancellation,
                )?
            };

            (template, Some(span))
        } else {
            let address = binding
                .imported_semantic_address(declaration.into())
                .map_err(binding_query_error)?
                .expect("imported static initialization retains its checked semantic address");

            let template = imported_declaration_template(&binding, address, kind)
                .map_err(binding_query_error)?
                .map(|template| template.map(|template| template.template().clone()));

            let span = self
                .dependency_interface_input(address.interface())
                .and_then(crate::request::DependencyInterfaceInput::dependency_span);

            (template, span)
        };

        let imported = self.imported_symbol_skeleton_result_with_cancellation(cancellation)?;

        let mut diagnostics =
            DiagnosticBag::merged_all([template.diagnostics(), imported.diagnostics()]);

        let mut value = None;

        if !diagnostics.has_errors() {
            let template = template
                .value()
                .as_ref()
                .expect("ordinary static initialization retains its checked initializer template");

            let context = self.checker_context(cancellation)?;

            let resolver = CompilationConstantTemplateResolver::new(
                self,
                cancellation,
                imported.value().as_deref(),
            );

            let evaluated = checker_result(evaluate_static_initializer_template(
                &context,
                template,
                kind,
                instance.substitution(),
                result_type,
                &resolver,
                span,
                limits,
            ))?;

            diagnostics.add_range(evaluated.diagnostics().iter().cloned());
            value = evaluated.into_parts().0;
        }

        let value = match value {
            Some(value) => value,
            None => {
                assert!(
                    diagnostics.has_errors(),
                    "static initialization without a value retains its source failure"
                );

                let value = self
                    .semantic_value_store()?
                    .intern_error_constant_value(result_type)?;

                EvaluatedConstantCall::new(value, Default::default())
            }
        };

        Ok(DiagnosticResult::new(value, diagnostics))
    }
}
