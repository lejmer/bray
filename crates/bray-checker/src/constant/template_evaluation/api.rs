use super::super::call::{ConstantTemplateResolver, EvaluatedConstantCall};
use super::super::limits::EvaluationBudget;
use super::evaluator::TemplateEvaluator;
use super::support::{TemplateEvaluationFailure, recovery_value};
use crate::{CheckerOutcome, CheckerQueryError, CheckerRequestContext, ConstantCallRequest};
use bray_bound_tree::{CheckedTemplate, CheckedTemplateKind};
use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticId};
use bray_source::SourceSpan;
use bray_symbols::{ConstantValueId, GenericSubstitutionId, ImplementationInstanceId, TypeId};

/// Evaluates one source-independent checked const-callable body.
pub fn evaluate_constant_callable_template<C>(
    context: &C,
    template: &CheckedTemplate,
    request: &ConstantCallRequest,
    resolver: &dyn ConstantTemplateResolver<UpstreamError = C::UpstreamError>,
    diagnostic_span: Option<SourceSpan>,
) -> CheckerOutcome<EvaluatedConstantCall, C::UpstreamError>
where
    C: CheckerRequestContext + ?Sized,
{
    let substitution = request.callable().substitution();

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
        upstream_failure: None,
        static_initializer: false,
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
        Err(TemplateEvaluationFailure::Diagnostic(problem)) => {
            let value = match recovery_value(context.semantic_values(), request.result_type()) {
                Ok(value) => value,
                Err(error) => return CheckerOutcome::InfrastructureFailure(error),
            };

            let diagnostic = match template_failure_diagnostic(
                context,
                request.result_type(),
                problem,
                diagnostic_span,
            ) {
                Ok(diagnostic) => diagnostic,
                Err(CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
                Err(CheckerQueryError::Infrastructure(error)) => {
                    return CheckerOutcome::InfrastructureFailure(error);
                }
                Err(CheckerQueryError::Upstream(error)) => {
                    return CheckerOutcome::UpstreamFailure(error);
                }
            };

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
        Err(TemplateEvaluationFailure::Upstream) => {
            CheckerOutcome::UpstreamFailure(evaluator.take_upstream_failure())
        }
    }
}

/// Evaluates one source-independent checked generic predicate.
pub fn evaluate_generic_constraint_template<C>(
    context: &C,
    template: &CheckedTemplate,
    substitution: GenericSubstitutionId,
    result_type: TypeId,
    resolver: &dyn ConstantTemplateResolver<UpstreamError = C::UpstreamError>,
    diagnostic_span: Option<SourceSpan>,
) -> CheckerOutcome<Option<ConstantValueId>, C::UpstreamError>
where
    C: CheckerRequestContext + ?Sized,
{
    match evaluate_closed_template(
        context,
        template,
        CheckedTemplateKind::GenericConstraint,
        substitution,
        result_type,
        resolver,
        diagnostic_span,
        crate::ConstantEvaluationLimits::default(),
    ) {
        CheckerOutcome::Complete(result) => CheckerOutcome::Complete(
            result.map(|evaluated| evaluated.map(EvaluatedConstantCall::value)),
        ),
        CheckerOutcome::Cancelled => CheckerOutcome::Cancelled,
        CheckerOutcome::InfrastructureFailure(error) => {
            CheckerOutcome::InfrastructureFailure(error)
        }
        CheckerOutcome::UpstreamFailure(error) => CheckerOutcome::UpstreamFailure(error),
    }
}

/// Evaluates one source-independent constant definition.
pub fn evaluate_constant_definition_template<C>(
    context: &C,
    template: &CheckedTemplate,
    substitution: GenericSubstitutionId,
    result_type: TypeId,
    resolver: &dyn ConstantTemplateResolver<UpstreamError = C::UpstreamError>,
    diagnostic_span: Option<SourceSpan>,
    limits: crate::ConstantEvaluationLimits,
) -> CheckerOutcome<Option<EvaluatedConstantCall>, C::UpstreamError>
where
    C: CheckerRequestContext + ?Sized,
{
    evaluate_closed_template(
        context,
        template,
        CheckedTemplateKind::ConstantDefinition,
        substitution,
        result_type,
        resolver,
        diagnostic_span,
        limits,
    )
}

/// Evaluates one source-independent static initializer.
pub fn evaluate_static_initializer_template<C>(
    context: &C,
    template: &CheckedTemplate,
    kind: CheckedTemplateKind,
    substitution: GenericSubstitutionId,
    result_type: TypeId,
    resolver: &dyn ConstantTemplateResolver<UpstreamError = C::UpstreamError>,
    diagnostic_span: Option<SourceSpan>,
    limits: crate::ConstantEvaluationLimits,
) -> CheckerOutcome<Option<EvaluatedConstantCall>, C::UpstreamError>
where
    C: CheckerRequestContext + ?Sized,
{
    if !matches!(
        kind,
        CheckedTemplateKind::ProductStaticInitializer
            | CheckedTemplateKind::ThreadLocalStaticInitializer
    ) {
        return CheckerOutcome::InfrastructureFailure(
            crate::CheckerInfrastructureError::InvalidConstantEvaluationInput,
        );
    }

    evaluate_closed_template(
        context,
        template,
        kind,
        substitution,
        result_type,
        resolver,
        diagnostic_span,
        limits,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "template evaluation requires each validated semantic input independently"
)]
fn evaluate_closed_template<C>(
    context: &C,
    template: &CheckedTemplate,
    kind: CheckedTemplateKind,
    substitution: GenericSubstitutionId,
    result_type: TypeId,
    resolver: &dyn ConstantTemplateResolver<UpstreamError = C::UpstreamError>,
    diagnostic_span: Option<SourceSpan>,
    limits: crate::ConstantEvaluationLimits,
) -> CheckerOutcome<Option<EvaluatedConstantCall>, C::UpstreamError>
where
    C: CheckerRequestContext + ?Sized,
{
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
        upstream_failure: None,
        static_initializer: false,
    };

    let evaluated = evaluator.evaluate_result(kind, result_type);

    match evaluated {
        Ok(value) => CheckerOutcome::complete(
            Some(EvaluatedConstantCall::new(
                value,
                evaluator.budget.usage(limits),
            )),
            evaluator.diagnostics,
        ),
        Err(TemplateEvaluationFailure::Diagnostic(problem)) => {
            let diagnostic =
                match template_failure_diagnostic(context, result_type, problem, diagnostic_span) {
                    Ok(diagnostic) => diagnostic,
                    Err(CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
                    Err(CheckerQueryError::Infrastructure(error)) => {
                        return CheckerOutcome::InfrastructureFailure(error);
                    }
                    Err(CheckerQueryError::Upstream(error)) => {
                        return CheckerOutcome::UpstreamFailure(error);
                    }
                };

            evaluator.diagnostics.add(diagnostic);

            CheckerOutcome::complete(None, evaluator.diagnostics)
        }
        Err(TemplateEvaluationFailure::Cancelled) => CheckerOutcome::Cancelled,
        Err(TemplateEvaluationFailure::Infrastructure(error)) => {
            CheckerOutcome::InfrastructureFailure(error)
        }
        Err(TemplateEvaluationFailure::Upstream) => {
            CheckerOutcome::UpstreamFailure(evaluator.take_upstream_failure())
        }
    }
}

fn template_failure_diagnostic<C>(
    context: &C,
    result_type: TypeId,
    problem: super::super::diagnostic::ConstantDiagnostic,
    span: Option<SourceSpan>,
) -> Result<Diagnostic, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    problem.render(context, DiagnosticId::new(0), span, || Ok(result_type))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_bound_tree::{
        CheckedTemplateBehavior, CheckedTemplateBuilder, CheckedTemplateCompletion,
        CheckedTemplateExecution, CheckedTemplateKind, CheckedTemplateNode,
        CheckedTemplateOperation,
    };
    use bray_source::{SourceId, SourceSpan, TextRange, TextSize};
    use bray_symbols::{
        CallableInstanceData, ConstantField, ConstantTermData, ConstantValueData,
        ConstantValueKind, CurrentRunCancellation, DependencyContractTemplateData,
        FunctionSymbolId, GenericOwnerId, GenericSubstitutionData, ModulePathKey, PackageIdentity,
        StaticInstanceKey, StaticInstanceTemplateId, StaticReferenceSelection, StaticSymbolId,
        StructFieldSymbolId, SymbolId, SymbolKey, SymbolKind, SymbolOrdinal, SymbolRootKey,
        TypeData,
    };
    use bray_testing::assert_goal_state_diagnostic_kind;

    use super::super::super::call::{
        ConstantCallRequest, ConstantCallResolution, ConstantCallResolver,
        ConstantTemplateResolver, EvaluatedConstantCall,
    };
    use super::super::super::limits::{ConstantEvaluationLimits, ConstantEvaluationUsage};
    use super::{
        evaluate_constant_callable_template, evaluate_constant_definition_template,
        evaluate_generic_constraint_template, evaluate_static_initializer_template,
    };
    use crate::test_support::{TestCheckerContext, semantic_values};
    use crate::{CheckerQueryResult, ConstantReferenceResolution};

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

        let diagnostic_span = SourceSpan::new(
            SourceId::new(0),
            TextRange::new(TextSize::new(0), TextSize::new(1)),
        );

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

        let aggregate_outcome = evaluate_constant_callable_template(
            &TestCheckerContext::new(false),
            &template,
            &request(ConstantEvaluationLimits::new(16, 1, 16)),
            &UnusedTemplateResolver,
            Some(diagnostic_span),
        )
        .into_result()
        .unwrap_or_else(|| panic!("constant aggregate limit failure must recover"));

        assert_goal_state_diagnostic_kind(
            aggregate_outcome.diagnostics(),
            bray_diagnostics::DiagnosticKind::CheckingConstantAggregateLimitExceeded,
        );

        let literal_outcome = evaluate_constant_callable_template(
            &TestCheckerContext::new(false),
            &template,
            &request(ConstantEvaluationLimits::new(16, 16, 2)),
            &UnusedTemplateResolver,
            Some(diagnostic_span),
        )
        .into_result()
        .unwrap_or_else(|| panic!("constant literal limit failure must recover"));

        assert_goal_state_diagnostic_kind(
            literal_outcome.diagnostics(),
            bray_diagnostics::DiagnosticKind::CheckingConstantLiteralSizeLimitExceeded,
        );

        let expansion_outcome = evaluate_constant_callable_template(
            &TestCheckerContext::new(false),
            &template,
            &request(ConstantEvaluationLimits::new(16, 16, 16).with_expansions(3)),
            &UnusedTemplateResolver,
            Some(diagnostic_span),
        )
        .into_result()
        .unwrap_or_else(|| panic!("constant expansion limit failure must recover"));

        assert_goal_state_diagnostic_kind(
            expansion_outcome.diagnostics(),
            bray_diagnostics::DiagnosticKind::CheckingConstantExpansionLimitExceeded,
        );
    }

    #[test]
    fn constant_body_templates_resolve_callable_arguments() {
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
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .unwrap_or_else(|error| panic!("callable argument term must intern: {error:?}"));

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
            dependency_contract,
            [],
        );

        let mut builder =
            CheckedTemplateBuilder::new(CheckedTemplateKind::ConstantCallableBody, behavior);

        let result = builder
            .push_node(CheckedTemplateNode::new(
                CheckedTemplateOperation::Constant {
                    term,
                    usage: bray_bound_tree::CheckedTemplateConstantUsage::new(1, 0, 0),
                },
                ty,
            ))
            .unwrap_or_else(|error| panic!("callable argument node must validate: {error:?}"));

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

        let outcome = evaluate_constant_callable_template(
            &TestCheckerContext::new(false),
            &template,
            &ConstantCallRequest::new(
                CallableInstanceData::new(callable, substitution),
                None,
                [value],
                ty,
                ConstantEvaluationLimits::default(),
            ),
            &UnusedTemplateResolver,
            None,
        )
        .into_result()
        .unwrap_or_else(|| panic!("callable argument template must evaluate"));

        assert!(outcome.diagnostics().is_empty());
        assert_eq!(outcome.value().value(), value);
    }

    #[test]
    fn typed_nested_terms_preserve_their_call_result_type() {
        let values = semantic_values();

        let inner_type = values
            .intern_type(TypeData::tuple([]))
            .unwrap_or_else(|error| panic!("inner type must intern: {error:?}"));

        let outer_type = values
            .intern_type(TypeData::tuple([inner_type]))
            .unwrap_or_else(|error| panic!("outer type must intern: {error:?}"));

        let inner_value = values
            .intern_constant_value(ConstantValueData::new(
                inner_type,
                ConstantValueKind::Tuple(Arc::from([])),
            ))
            .unwrap_or_else(|error| panic!("inner value must intern: {error:?}"));

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

        let callable_id = values
            .intern_callable_instance(callable)
            .unwrap_or_else(|error| panic!("callable instance must intern: {error:?}"));

        let call = values
            .intern_constant_term(ConstantTermData::call(callable_id, None, []))
            .unwrap_or_else(|error| panic!("call term must intern: {error:?}"));

        let call = values
            .intern_constant_term(ConstantTermData::typed(call, inner_type))
            .unwrap_or_else(|error| panic!("typed call term must intern: {error:?}"));

        let result = values
            .intern_constant_term(ConstantTermData::tuple([call]))
            .unwrap_or_else(|error| panic!("tuple term must intern: {error:?}"));

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
            dependency_contract,
            [],
        );

        let mut builder =
            CheckedTemplateBuilder::new(CheckedTemplateKind::ConstantCallableBody, behavior);

        let result = builder
            .push_node(CheckedTemplateNode::new(
                CheckedTemplateOperation::Constant {
                    term: result,
                    usage: bray_bound_tree::CheckedTemplateConstantUsage::new(1, 1, 0),
                },
                outer_type,
            ))
            .unwrap_or_else(|error| panic!("constant body node must validate: {error:?}"));

        let template = builder
            .finish(result, CheckedTemplateCompletion::Complete)
            .unwrap_or_else(|error| panic!("constant body template must validate: {error:?}"));

        let outcome = evaluate_constant_callable_template(
            &TestCheckerContext::new(false),
            &template,
            &ConstantCallRequest::new(
                callable,
                None,
                [],
                outer_type,
                ConstantEvaluationLimits::default(),
            ),
            &TypedCallResolver {
                result: inner_value,
                expected_result_type: inner_type,
            },
            None,
        )
        .into_result()
        .unwrap_or_else(|| panic!("typed constant body template must evaluate"));

        assert!(outcome.diagnostics().is_empty());

        let result = values.constant_value_data(outcome.value().value());

        assert_eq!(result.ty(), outer_type);

        assert_eq!(
            result.kind(),
            &ConstantValueKind::Tuple(Arc::from([inner_value]))
        );
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

    #[test]
    fn static_initializer_templates_materialize_field_identified_products() {
        let values = semantic_values();

        let ty = values
            .intern_type(TypeData::tuple([]))
            .unwrap_or_else(|error| panic!("test type must intern: {error:?}"));

        let value = values
            .intern_constant_value(ConstantValueData::new(ty, ConstantValueKind::Unit))
            .unwrap_or_else(|error| panic!("field value must intern: {error:?}"));

        let term = values
            .intern_constant_term(ConstantTermData::Value(value))
            .unwrap_or_else(|error| panic!("field term must intern: {error:?}"));

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
            dependency_contract,
            [],
        );

        let mut builder =
            CheckedTemplateBuilder::new(CheckedTemplateKind::ProductStaticInitializer, behavior);

        let first_value = builder
            .push_node(CheckedTemplateNode::new(
                CheckedTemplateOperation::Constant {
                    term,
                    usage: Default::default(),
                },
                ty,
            ))
            .unwrap_or_else(|error| panic!("first field node must validate: {error:?}"));

        let second_value = builder
            .push_node(CheckedTemplateNode::new(
                CheckedTemplateOperation::Constant {
                    term,
                    usage: Default::default(),
                },
                ty,
            ))
            .unwrap_or_else(|error| panic!("second field node must validate: {error:?}"));

        let package = PackageIdentity::try_new("example.package")
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        let path = ModulePathKey::try_new(["api"])
            .unwrap_or_else(|| panic!("test module path must be valid"));

        let owner = SymbolKey::module(SymbolRootKey::Package(package), path);

        let first = SymbolKey::source_declaration(
            owner.clone(),
            SymbolKind::StructField,
            bray_declarations::DeclarationId::new(0),
        )
        .unwrap_or_else(|| panic!("first field key must be valid"));

        let second = SymbolKey::source_declaration(
            owner,
            SymbolKind::StructField,
            bray_declarations::DeclarationId::new(1),
        )
        .unwrap_or_else(|| panic!("second field key must be valid"));

        let result = builder
            .push_node(CheckedTemplateNode::new(
                CheckedTemplateOperation::product([
                    ConstantField::new(second.clone(), second_value),
                    ConstantField::new(first.clone(), first_value),
                ]),
                ty,
            ))
            .unwrap_or_else(|error| panic!("product node must validate: {error:?}"));

        let template = builder
            .finish(result, CheckedTemplateCompletion::Complete)
            .unwrap_or_else(|error| panic!("product template must validate: {error:?}"));

        let generic_owner =
            GenericOwnerId::try_new(FunctionSymbolId::from_symbol_id(SymbolId::new(999)).into())
                .unwrap_or_else(|| panic!("function must be a generic owner"));

        let substitution = values
            .intern_generic_substitution(
                GenericSubstitutionData::try_new(generic_owner, [], [])
                    .unwrap_or_else(|error| panic!("empty substitution must validate: {error:?}")),
            )
            .unwrap_or_else(|error| panic!("empty substitution must intern: {error:?}"));

        let resolver = ProductResolver { first, second };

        let outcome = evaluate_static_initializer_template(
            &TestCheckerContext::new(false),
            &template,
            CheckedTemplateKind::ProductStaticInitializer,
            substitution,
            ty,
            &resolver,
            None,
            ConstantEvaluationLimits::default(),
        )
        .into_result()
        .unwrap_or_else(|| panic!("product template must evaluate"));

        let result = outcome
            .value()
            .unwrap_or_else(|| panic!("product evaluation must produce a value"));

        let data = values.constant_value_data(result.value());

        let ConstantValueKind::Product(fields) = data.kind() else {
            panic!("product template must materialize a product value");
        };

        assert_eq!(fields[0].field(), &resolver.second_id());
        assert_eq!(fields[1].field(), &resolver.first_id());
    }

    #[test]
    fn imported_definition_template_rejects_nested_static_borrow() {
        let values = semantic_values();
        let unit = values.intern_type(TypeData::tuple([])).unwrap();

        let borrowed = values
            .intern_type(TypeData::Borrow {
                kind: bray_symbols::BorrowKind::Shared,
                target: unit,
            })
            .unwrap();

        let tuple = values.intern_type(TypeData::tuple([borrowed])).unwrap();
        let static_symbol = StaticSymbolId::from_symbol_id(SymbolId::new(998));
        let owner = GenericOwnerId::try_new(static_symbol.into()).unwrap();

        let substitution = values
            .intern_generic_substitution(GenericSubstitutionData::try_new(owner, [], []).unwrap())
            .unwrap();

        let target = bray_target::TargetIdentity::try_new("test").unwrap();

        let instance = StaticInstanceKey::new(
            StaticInstanceTemplateId::new(static_symbol),
            substitution,
            [],
            target,
        );

        let borrowed_value = values
            .intern_constant_value(ConstantValueData::new(
                borrowed,
                ConstantValueKind::StaticAddress(StaticReferenceSelection::Closed(instance)),
            ))
            .unwrap();

        let tuple_value = values
            .intern_constant_value(ConstantValueData::new(
                tuple,
                ConstantValueKind::Tuple(Arc::from([borrowed_value])),
            ))
            .unwrap();

        let term = values
            .intern_constant_term(ConstantTermData::Value(tuple_value))
            .unwrap();

        let dependency_contract = values
            .intern_dependency_contract_template(DependencyContractTemplateData::new([]))
            .unwrap();

        let behavior = CheckedTemplateBehavior::new(
            [],
            [],
            [],
            CheckedTemplateExecution::new([], CurrentRunCancellation::NotEntered),
            [],
            dependency_contract,
            dependency_contract,
            [],
        );

        let mut builder =
            CheckedTemplateBuilder::new(CheckedTemplateKind::ConstantDefinition, behavior);

        let result = builder
            .push_node(CheckedTemplateNode::new(
                CheckedTemplateOperation::Constant {
                    term,
                    usage: Default::default(),
                },
                tuple,
            ))
            .unwrap();

        let template = builder
            .finish(result, CheckedTemplateCompletion::Complete)
            .unwrap();

        let span = SourceSpan::new(
            SourceId::new(0),
            TextRange::new(TextSize::new(0), TextSize::new(1)),
        );

        let outcome = evaluate_constant_definition_template(
            &TestCheckerContext::new(false),
            &template,
            substitution,
            tuple,
            &UnusedTemplateResolver,
            Some(span),
            ConstantEvaluationLimits::default(),
        )
        .into_result()
        .unwrap();

        assert!(outcome.value().is_none());

        assert_goal_state_diagnostic_kind(
            outcome.diagnostics(),
            bray_diagnostics::DiagnosticKind::CheckingNonMaterializableConstant,
        );

        let diagnostic = outcome
            .diagnostics()
            .by_kind(bray_diagnostics::DiagnosticKind::CheckingNonMaterializableConstant)
            .next()
            .unwrap();

        assert_eq!(diagnostic.primary_span(), Some(span));
    }

    struct UnusedTemplateResolver;

    struct TypedCallResolver {
        result: bray_symbols::ConstantValueId,
        expected_result_type: bray_symbols::TypeId,
    }

    struct ProductResolver {
        first: SymbolKey,
        second: SymbolKey,
    }

    impl ProductResolver {
        fn first_id(&self) -> StructFieldSymbolId {
            StructFieldSymbolId::from_symbol_id(SymbolId::new(1000))
        }

        fn second_id(&self) -> StructFieldSymbolId {
            StructFieldSymbolId::from_symbol_id(SymbolId::new(1001))
        }
    }

    impl ConstantCallResolver for TypedCallResolver {
        type UpstreamError = std::convert::Infallible;

        fn is_constant_callable(
            &self,
            _callable: CallableInstanceData,
        ) -> CheckerQueryResult<bool> {
            Ok(true)
        }

        fn resolve(
            &self,
            request: &ConstantCallRequest,
        ) -> CheckerQueryResult<ConstantCallResolution> {
            assert_eq!(request.result_type(), self.expected_result_type);

            Ok(ConstantCallResolution::Evaluated(
                bray_diagnostics::DiagnosticResult::without_diagnostics(
                    EvaluatedConstantCall::new(self.result, ConstantEvaluationUsage::default()),
                ),
            ))
        }
    }

    impl ConstantTemplateResolver for TypedCallResolver {
        fn symbol(&self, _key: &bray_symbols::SymbolKey) -> Option<bray_symbols::AnySymbolId> {
            None
        }

        fn resolve_constant(
            &self,
            _instance: bray_symbols::ConstantInstanceKey,
            _limits: crate::ConstantEvaluationLimits,
        ) -> CheckerQueryResult<bray_diagnostics::DiagnosticResult<ConstantReferenceResolution>>
        {
            unreachable!("typed call template must not resolve constants")
        }

        fn resolve_static(
            &self,
            _declaration: bray_symbols::StaticSymbolId,
            _substitution: bray_symbols::GenericSubstitutionId,
        ) -> CheckerQueryResult<
            bray_diagnostics::DiagnosticResult<bray_symbols::StaticReferenceSelection>,
        > {
            unreachable!("typed call template must not resolve statics")
        }
    }

    impl ConstantCallResolver for ProductResolver {
        type UpstreamError = std::convert::Infallible;

        fn is_constant_callable(
            &self,
            _callable: CallableInstanceData,
        ) -> CheckerQueryResult<bool> {
            unreachable!("product template must not resolve calls")
        }

        fn resolve(
            &self,
            _request: &ConstantCallRequest,
        ) -> CheckerQueryResult<ConstantCallResolution> {
            unreachable!("product template must not resolve calls")
        }
    }

    impl ConstantTemplateResolver for ProductResolver {
        fn symbol(&self, key: &SymbolKey) -> Option<bray_symbols::AnySymbolId> {
            if key == &self.first {
                Some(self.first_id().into())
            } else if key == &self.second {
                Some(self.second_id().into())
            } else {
                None
            }
        }

        fn resolve_constant(
            &self,
            _instance: bray_symbols::ConstantInstanceKey,
            _limits: crate::ConstantEvaluationLimits,
        ) -> CheckerQueryResult<bray_diagnostics::DiagnosticResult<ConstantReferenceResolution>>
        {
            unreachable!("product template must not resolve constants")
        }

        fn resolve_static(
            &self,
            _declaration: bray_symbols::StaticSymbolId,
            _substitution: bray_symbols::GenericSubstitutionId,
        ) -> CheckerQueryResult<
            bray_diagnostics::DiagnosticResult<bray_symbols::StaticReferenceSelection>,
        > {
            unreachable!("product template must not resolve statics")
        }
    }

    impl ConstantCallResolver for UnusedTemplateResolver {
        type UpstreamError = std::convert::Infallible;

        fn is_constant_callable(
            &self,
            _callable: CallableInstanceData,
        ) -> CheckerQueryResult<bool> {
            unreachable!("closed test template must not resolve calls")
        }

        fn resolve(
            &self,
            _request: &ConstantCallRequest,
        ) -> CheckerQueryResult<ConstantCallResolution> {
            unreachable!("closed test template must not resolve calls")
        }
    }

    impl ConstantTemplateResolver for UnusedTemplateResolver {
        fn symbol(&self, _key: &bray_symbols::SymbolKey) -> Option<bray_symbols::AnySymbolId> {
            None
        }

        fn resolve_constant(
            &self,
            _instance: bray_symbols::ConstantInstanceKey,
            _limits: crate::ConstantEvaluationLimits,
        ) -> CheckerQueryResult<bray_diagnostics::DiagnosticResult<ConstantReferenceResolution>>
        {
            unreachable!("closed test template must not resolve constants")
        }

        fn resolve_static(
            &self,
            _declaration: bray_symbols::StaticSymbolId,
            _substitution: bray_symbols::GenericSubstitutionId,
        ) -> CheckerQueryResult<
            bray_diagnostics::DiagnosticResult<bray_symbols::StaticReferenceSelection>,
        > {
            unreachable!("closed test template must not resolve statics")
        }
    }
}
