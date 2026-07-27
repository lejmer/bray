use super::super::call::{ConstantTemplateResolver, EvaluatedConstantCall};
use super::super::limits::EvaluationBudget;
use super::evaluator::TemplateEvaluator;
use super::support::{TemplateEvaluationFailure, recovery_value};
use crate::{CheckerOutcome, CheckerRequestContext, ConstantCallRequest};
use bray_bound_tree::{CheckedTemplate, CheckedTemplateKind};
use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticId, SeverityKind};
use bray_source::SourceSpan;
use bray_symbols::{
    ConcreteGenericSubstitutionId, ConstantValueId, ImplementationInstanceId, TypeId,
};

/// Evaluates one source-independent checked const-callable body.
pub fn evaluate_constant_callable_template<C>(
    context: &C,
    template: &CheckedTemplate,
    request: &ConstantCallRequest,
    resolver: &dyn ConstantTemplateResolver,
    diagnostic_span: Option<SourceSpan>,
) -> CheckerOutcome<EvaluatedConstantCall>
where
    C: CheckerRequestContext + ?Sized,
{
    let substitution = match context
        .semantic_values()
        .require_concrete_substitution(request.callable().substitution())
    {
        Ok(substitution) => substitution,
        Err(_) => {
            return CheckerOutcome::InfrastructureFailure(
                crate::CheckerInfrastructureError::SemanticValueUnavailable,
            );
        }
    };

    let limits = request.limits();

    let mut evaluator = TemplateEvaluator {
        context,
        template,
        substitution,
        selected_implementation: request.selected_implementation(),
        arguments: request.arguments(),
        resolver,
        limits,
        budget: EvaluationBudget::from_limits(limits),
        values: vec![None; template.nodes().len()],
        diagnostics: DiagnosticBag::new(),
    };

    let evaluated = evaluator.evaluate_result(
        CheckedTemplateKind::ConstantCallableBody,
        request.result_type(),
    );

    match evaluated {
        Ok(value) => CheckerOutcome::complete(
            EvaluatedConstantCall::new(value, evaluator.budget.usage(limits)),
            evaluator.diagnostics,
        ),
        Err(TemplateEvaluationFailure::Diagnostic(kind)) => {
            let value = match recovery_value(context.semantic_values(), request.result_type()) {
                Ok(value) => value,
                Err(error) => return CheckerOutcome::InfrastructureFailure(error),
            };

            let mut diagnostic = Diagnostic::new(DiagnosticId::new(0), kind, SeverityKind::Error);

            if let Some(span) = diagnostic_span {
                diagnostic = diagnostic.with_primary_span(span);
            }

            evaluator.diagnostics.add(diagnostic);

            CheckerOutcome::complete(
                EvaluatedConstantCall::new(value, evaluator.budget.usage(limits)),
                evaluator.diagnostics,
            )
        }
        Err(TemplateEvaluationFailure::Cancelled) => CheckerOutcome::Cancelled,
        Err(TemplateEvaluationFailure::Infrastructure(error)) => {
            CheckerOutcome::InfrastructureFailure(error)
        }
    }
}

/// Evaluates one source-independent checked generic predicate.
pub fn evaluate_generic_constraint_template<C>(
    context: &C,
    template: &CheckedTemplate,
    substitution: ConcreteGenericSubstitutionId,
    result_type: TypeId,
    resolver: &dyn ConstantTemplateResolver,
    diagnostic_span: Option<SourceSpan>,
) -> CheckerOutcome<Option<ConstantValueId>>
where
    C: CheckerRequestContext + ?Sized,
{
    let limits = crate::ConstantEvaluationLimits::default();

    let mut evaluator = TemplateEvaluator {
        context,
        template,
        substitution,
        selected_implementation: None::<ImplementationInstanceId>,
        arguments: &[],
        resolver,
        limits,
        budget: EvaluationBudget::from_limits(limits),
        values: vec![None; template.nodes().len()],
        diagnostics: DiagnosticBag::new(),
    };

    let evaluated = evaluator.evaluate_result(CheckedTemplateKind::GenericConstraint, result_type);

    match evaluated {
        Ok(value) => CheckerOutcome::complete(Some(value), evaluator.diagnostics),
        Err(TemplateEvaluationFailure::Diagnostic(kind)) => {
            let mut diagnostic = Diagnostic::new(DiagnosticId::new(0), kind, SeverityKind::Error);

            if let Some(span) = diagnostic_span {
                diagnostic = diagnostic.with_primary_span(span);
            }

            evaluator.diagnostics.add(diagnostic);

            CheckerOutcome::complete(None, evaluator.diagnostics)
        }
        Err(TemplateEvaluationFailure::Cancelled) => CheckerOutcome::Cancelled,
        Err(TemplateEvaluationFailure::Infrastructure(error)) => {
            CheckerOutcome::InfrastructureFailure(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_bound_tree::{
        CheckedTemplateBehavior, CheckedTemplateBuilder, CheckedTemplateCompletion,
        CheckedTemplateExecution, CheckedTemplateKind, CheckedTemplateNode,
        CheckedTemplateOperation,
    };
    use bray_symbols::{
        CallableInstanceData, ConstantTermData, ConstantValueData, ConstantValueKind,
        CurrentRunCancellation, DependencyContractTemplateData, FunctionSymbolId, GenericOwnerId,
        GenericSubstitutionData, SymbolId, TypeData,
    };

    use super::super::super::call::{
        ConstantCallRequest, ConstantCallResolution, ConstantCallResolver, ConstantTemplateResolver,
    };
    use super::super::super::limits::ConstantEvaluationLimits;
    use super::{evaluate_constant_callable_template, evaluate_generic_constraint_template};
    use crate::test_support::{TestCheckerContext, semantic_values};
    use crate::{CheckerFactResult, ConstantReferenceResolution};

    #[test]
    fn constant_body_templates_evaluate_closed_results_and_report_usage() {
        let values = semantic_values();

        let ty = values
            .intern_type(TypeData::tuple([]))
            .unwrap_or_else(|error| panic!("tuple type must intern: {error:?}"));

        let value = values
            .intern_constant_value(ConstantValueData::new(
                ty,
                ConstantValueKind::Tuple(Arc::from([])),
            ))
            .unwrap_or_else(|error| panic!("tuple constant must intern: {error:?}"));

        let term = values
            .intern_constant_term(ConstantTermData::Value(value))
            .unwrap_or_else(|error| panic!("constant term must intern: {error:?}"));

        let dependency_contract = values
            .intern_dependency_contract_template(DependencyContractTemplateData::new([]))
            .unwrap_or_else(|error| panic!("empty dependency contract must intern: {error:?}"));

        let behavior = CheckedTemplateBehavior::new(
            [],
            [],
            [],
            CheckedTemplateExecution::new([], CurrentRunCancellation::NotEntered),
            [],
            dependency_contract,
            [],
        );

        let mut builder =
            CheckedTemplateBuilder::new(CheckedTemplateKind::ConstantCallableBody, behavior);

        let result = builder
            .push_node(CheckedTemplateNode::new(
                CheckedTemplateOperation::Constant {
                    term,
                    usage: bray_bound_tree::CheckedTemplateConstantUsage::new(2, 3, 4),
                },
                ty,
            ))
            .unwrap_or_else(|error| panic!("constant body node must validate: {error:?}"));

        let template = builder
            .finish(result, CheckedTemplateCompletion::Complete)
            .unwrap_or_else(|error| panic!("constant body template must validate: {error:?}"));

        let owner =
            GenericOwnerId::try_new(FunctionSymbolId::from_symbol_id(SymbolId::new(999)).into())
                .unwrap_or_else(|| panic!("function must be a generic owner"));

        let substitution = GenericSubstitutionData::try_new(owner, [], [])
            .unwrap_or_else(|error| panic!("empty substitution must validate: {error:?}"));

        let substitution = values
            .intern_generic_substitution(substitution)
            .unwrap_or_else(|error| panic!("empty substitution must intern: {error:?}"));

        let callable = bray_symbols::CallableDefinitionId::try_new(
            FunctionSymbolId::from_symbol_id(SymbolId::new(999)).into(),
        )
        .unwrap_or_else(|| panic!("test function must be callable"));

        let callable = CallableInstanceData::new(callable, substitution);
        let request = |limits| ConstantCallRequest::new(callable, None, [], ty, limits);

        let outcome = evaluate_constant_callable_template(
            &TestCheckerContext::new(false),
            &template,
            &request(ConstantEvaluationLimits::default()),
            &UnusedTemplateResolver,
            None,
        )
        .into_result()
        .unwrap_or_else(|| panic!("constant body template must evaluate"));

        assert!(outcome.diagnostics().is_empty());
        assert_eq!(outcome.value().value(), value);
        assert_eq!(outcome.value().usage().steps(), 2);
        assert_eq!(outcome.value().usage().aggregate_elements(), 2);
        assert_eq!(outcome.value().usage().literal_bytes(), 3);
        assert_eq!(outcome.value().usage().expansions(), 4);

        let cases = [
            (
                ConstantEvaluationLimits::new(16, 1, 16),
                bray_diagnostics::DiagnosticKind::CheckingConstantAggregateLimitExceeded,
            ),
            (
                ConstantEvaluationLimits::new(16, 16, 2),
                bray_diagnostics::DiagnosticKind::CheckingConstantLiteralSizeLimitExceeded,
            ),
            (
                ConstantEvaluationLimits::new(16, 16, 16).with_expansions(3),
                bray_diagnostics::DiagnosticKind::CheckingConstantExpansionLimitExceeded,
            ),
        ];

        for (limits, expected) in cases {
            let outcome = evaluate_constant_callable_template(
                &TestCheckerContext::new(false),
                &template,
                &request(limits),
                &UnusedTemplateResolver,
                None,
            )
            .into_result()
            .unwrap_or_else(|| panic!("constant body limit failure must recover"));

            assert_eq!(outcome.diagnostics().by_kind(expected).count(), 1);
        }
    }

    #[test]
    fn generic_constraint_templates_evaluate_concrete_predicates() {
        let values = semantic_values();

        let ty = values
            .intern_type(TypeData::tuple([]))
            .unwrap_or_else(|error| panic!("test type must intern: {error:?}"));

        let value = values
            .intern_constant_value(ConstantValueData::new(ty, ConstantValueKind::Boolean(true)))
            .unwrap_or_else(|error| panic!("predicate value must intern: {error:?}"));

        let term = values
            .intern_constant_term(ConstantTermData::Value(value))
            .unwrap_or_else(|error| panic!("predicate term must intern: {error:?}"));

        let dependency_contract = values
            .intern_dependency_contract_template(DependencyContractTemplateData::new([]))
            .unwrap_or_else(|error| panic!("empty dependency contract must intern: {error:?}"));

        let behavior = CheckedTemplateBehavior::new(
            [],
            [],
            [],
            CheckedTemplateExecution::new([], CurrentRunCancellation::NotEntered),
            [],
            dependency_contract,
            [],
        );

        let mut builder =
            CheckedTemplateBuilder::new(CheckedTemplateKind::GenericConstraint, behavior);

        let result = builder
            .push_node(CheckedTemplateNode::new(
                CheckedTemplateOperation::Constant {
                    term,
                    usage: bray_bound_tree::CheckedTemplateConstantUsage::new(1, 0, 0),
                },
                ty,
            ))
            .unwrap_or_else(|error| panic!("predicate node must validate: {error:?}"));

        let template = builder
            .finish(result, CheckedTemplateCompletion::Complete)
            .unwrap_or_else(|error| panic!("predicate template must validate: {error:?}"));

        let owner =
            GenericOwnerId::try_new(FunctionSymbolId::from_symbol_id(SymbolId::new(999)).into())
                .unwrap_or_else(|| panic!("function must be a generic owner"));

        let substitution = GenericSubstitutionData::try_new(owner, [], [])
            .unwrap_or_else(|error| panic!("empty substitution must validate: {error:?}"));

        let substitution = values
            .intern_generic_substitution(substitution)
            .unwrap_or_else(|error| panic!("empty substitution must intern: {error:?}"));

        let substitution = values
            .require_concrete_substitution(substitution)
            .unwrap_or_else(|error| panic!("empty substitution must be concrete: {error:?}"));

        let outcome = evaluate_generic_constraint_template(
            &TestCheckerContext::new(false),
            &template,
            substitution,
            ty,
            &UnusedTemplateResolver,
            None,
        )
        .into_result()
        .unwrap_or_else(|| panic!("generic predicate template must evaluate"));

        assert!(outcome.diagnostics().is_empty());
        assert_eq!(*outcome.value(), Some(value));
    }

    struct UnusedTemplateResolver;

    impl ConstantCallResolver for UnusedTemplateResolver {
        fn is_constant_callable(&self, _callable: CallableInstanceData) -> CheckerFactResult<bool> {
            unreachable!("closed test template must not resolve calls")
        }

        fn resolve(
            &self,
            _request: &ConstantCallRequest,
        ) -> CheckerFactResult<ConstantCallResolution> {
            unreachable!("closed test template must not resolve calls")
        }
    }

    impl ConstantTemplateResolver for UnusedTemplateResolver {
        fn symbol(
            &self,
            _key: &bray_symbols::ExternalSymbolKey,
        ) -> Option<bray_symbols::AnySymbolId> {
            None
        }

        fn resolve_constant(
            &self,
            _instance: bray_symbols::ConstantInstanceKey,
            _limits: crate::ConstantEvaluationLimits,
        ) -> CheckerFactResult<bray_diagnostics::DiagnosticResult<ConstantReferenceResolution>>
        {
            unreachable!("closed test template must not resolve constants")
        }
    }
}
