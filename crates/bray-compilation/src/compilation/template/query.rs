use bray_binder::BindingQueryContext;
use bray_bound_tree::{BoundReferenceTarget, BoundUnitKey, CheckedTemplate, CheckedTemplateKind};
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};

use super::{SourceTemplateInput, SourceTemplateRequest, build_source_template};
use crate::compilation::Compilation;
use crate::fact::FactQueryError;

impl Compilation {
    pub(in crate::compilation) fn checked_source_template(
        &self,
    key: BoundUnitKey,
    kind: CheckedTemplateKind,
    expression: SyntaxAnchor,
    mut inputs: Vec<SourceTemplateInput>,
    dependency: bray_symbols::DependencyContractTemplateId,
    cancellation: &crate::fact::CancellationToken,
) -> Result<DiagnosticResult<Option<CheckedTemplate>>, FactQueryError> {

    let bound = self
        .bound_unit_with_cancellation(key.clone(), cancellation)?;

    let semantics = self
        .expression_semantics_with_cancellation(key.clone(), cancellation)?;

    let context = self
        .binding_context(cancellation)?;

    let declared = self
        .declared_value_type_templates_with_cancellation(key.clone(), cancellation)?;

    let mut diagnostics = DiagnosticBag::new();

    for input in &mut inputs {
        let Some(BoundReferenceTarget::Surface(owner)) = input.target else {
            continue;
        };

        let term =
            bray_bound_tree::DeclaredValueTypeTerm::Value(BoundReferenceTarget::Surface(owner));

        let evidence = declared.result().value().evidence();
        let index = evidence.partition_point(|entry| entry.term() < term);

        if let Some(entry) = evidence.get(index).filter(|entry| entry.term() == term) {
            let constants = self.checked_constant_terms_for_templates_with_cancellation([entry.template()], cancellation)?;

            diagnostics.add_range(constants.diagnostics().clone());

            if constants.diagnostics().has_errors() {
                continue;
            }

            input.ty = bray_checker::resolve_type_expression_template(self.semantic_value_store()?, entry.template(), constants.value())
                .expect("checked source input type resolves from its established constant terms");
        }
    }

    let result_dependencies =
        crate::compilation::binder::expression_result_dependencies(&context, &key)
            .map_err(crate::compilation::binder::binding_query_error)?;

    let patterns = self
        .patterns_with_cancellation(key.clone(), cancellation)?;

    let body = self
        .body_behavior_with_cancellation(key, cancellation)?;

    for bag in [bound.result().diagnostics(), semantics.result().diagnostics(), declared.result().diagnostics(), result_dependencies.diagnostics(), patterns.result().diagnostics(), body.result().diagnostics()] {
        diagnostics.add_range(bag.clone());
    }

    if diagnostics.has_errors() {
        return Ok(DiagnosticResult::new(None, diagnostics));
    }

    assert!(!body.result().value().is_recovered(), "complete checked source template retains its body behavior");

    let behavior = body.result().value();

    let key_for = |symbol| -> Result<bray_symbols::SymbolKey, FactQueryError> {
        Ok(context.symbol_key(symbol)
            .map_err(crate::compilation::binder::binding_query_error)?
            .expect("checked source template declaration has stable identity")
            .clone())
    };
    let native_behavior = bray_bound_tree::CheckedTemplateBehavior::new(
        behavior.effects().iter().map(|requirement| key_for(requirement.declaration()).map(bray_bound_tree::CheckedTemplateEffect::new)).collect::<Result<Vec<_>, _>>()?,
        behavior.capabilities().iter().map(|requirement| key_for(requirement.declaration()).map(bray_bound_tree::CheckedTemplateCapability::new)).collect::<Result<Vec<_>, _>>()?,
        behavior.trusted_capabilities().iter().map(|requirement| key_for((*requirement).into()).map(bray_bound_tree::CheckedTemplateTrustedObligation::new)).collect::<Result<Vec<_>, _>>()?,
        bray_bound_tree::CheckedTemplateExecution::new(
            behavior.execution_requirements().iter().map(|requirement| key_for(requirement.declaration()).map(bray_bound_tree::CheckedTemplateExecutionRequirement::new)).collect::<Result<Vec<_>, _>>()?,
            behavior.current_run_cancellation(),
        ),
        behavior.lifecycle_obligations().iter().copied(),
        dependency,
        *result_dependencies.value(),
        [],
    );

    let semantics = semantics.result().value();

    let template = build_source_template(
        &context,
        SourceTemplateRequest {
            kind,
            unit: bound.result().value(),
            patterns: patterns.result().value(),
            types: semantics.types(),
            selections: semantics.selections(),
            literals: semantics.literals(),
            expression,
            inputs,
            behavior: native_behavior,
        },
    )?;

    Ok(DiagnosticResult::new(Some(template), diagnostics))
}
}
