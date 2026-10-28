use super::super::call::{ConstantTemplateResolver, EvaluatedConstantCall};
use super::super::limits::EvaluationBudget;
use super::evaluator::TemplateEvaluator;
use super::support::{TemplateEvaluationFailure, recovery_value};
use crate::{CheckerOutcome, CheckerQueryError, CheckerRequestContext, ConstantCallRequest};
use bray_bound_tree::{CheckedTemplate, CheckedTemplateKind};
use bray_diagnostics::{Diagnostic, DiagnosticId};
use bray_source::SourceSpan;
use bray_symbols::{ConstantValueId, GenericSubstitutionId, TypeId};

/// Evaluates caller requirements once before a fixed operation or checked callable body.
pub fn evaluate_constant_call<C>(
    context: &C,
    template: Option<&CheckedTemplate>,
    request: &ConstantCallRequest,
    resolver: &dyn ConstantTemplateResolver<UpstreamError = C::UpstreamError>,
    diagnostic_span: Option<SourceSpan>,
    requirements: &[bray_symbols::PredicateSemanticSummary],
    intrinsic: impl FnOnce() -> CheckerOutcome<Option<EvaluatedConstantCall>, C::UpstreamError>,
) -> CheckerOutcome<Option<EvaluatedConstantCall>, C::UpstreamError>
where
    C: CheckerRequestContext + ?Sized,
{
    let substitution = request.target().substitution();
    let limits = request.limits();
    let mut budget = EvaluationBudget::from_limits(limits);

    let mut evaluator = TemplateEvaluator::new(
        context,
        template,
        Some(substitution),
        resolver,
        limits,
        &mut budget,
    );

    evaluator.selected_implementation = request.selected_implementation();
    evaluator.arguments = request.arguments();

    let evaluated = (|| {
        if template.is_some() {
            evaluator.evaluate_generic_inputs()?;
        }

        if !evaluator.evaluate_requirements(request.target(), requirements)? {
            return Ok(Some(recovery_value(
                context.semantic_values(),
                request.result_type(),
            )));
        }

        let intrinsic = match intrinsic() {
            CheckerOutcome::Complete(result) => result,
            CheckerOutcome::Cancelled => return Err(TemplateEvaluationFailure::Cancelled),
            CheckerOutcome::UpstreamFailure(error) => {
                evaluator.upstream_failure = Some(error);

                return Err(TemplateEvaluationFailure::Upstream);
            }
        };

        evaluator
            .diagnostics
            .extend(intrinsic.diagnostics().iter().cloned());

        if evaluator.diagnostics.has_errors() {
            return Err(TemplateEvaluationFailure::Diagnosed);
        }

        if let Some(value) = intrinsic.value() {
            evaluator
                .budget
                .try_charge_usage(value.usage())
                .map_err(TemplateEvaluationFailure::Diagnostic)?;

            return Ok(Some(value.value()));
        }

        if template.is_none() {
            return Ok(None);
        }

        evaluator
            .evaluate_result(
                match request.target() {
                    crate::ConstantCallTarget::Default(_, _) => CheckedTemplateKind::RuntimeDefault,
                    crate::ConstantCallTarget::Predicate(_) => {
                        CheckedTemplateKind::PredicateDefinition
                    }
                    crate::ConstantCallTarget::Declaration(_)
                    | crate::ConstantCallTarget::Anonymous(_, _) => {
                        CheckedTemplateKind::ConstantCallableBody
                    }
                },
                request.result_type(),
            )
            .map(Some)
    })();

    crate::diagnostic::anchor_diagnostics(
        &mut evaluator.diagnostics,
        bray_diagnostics::DiagnosticLabelKind::InvalidConstantExpression,
        || diagnostic_span,
    );

    let mut states = Vec::new();

    let evaluated = evaluated.and_then(|value| {
        if request.is_observation() && !evaluator.diagnostics.has_errors() {
            let mut ordinal = 0;

            for input in template.into_iter().flat_map(|template| template.inputs()) {
                if !matches!(
                    input.kind(),
                    bray_bound_tree::CheckedTemplateInputKind::Receiver
                        | bray_bound_tree::CheckedTemplateInputKind::Parameter(_)
                ) {
                    continue;
                }

                if let Some(output) = input.state_output() {
                    states.push((ordinal, evaluator.evaluate_node(output)?));
                }

                ordinal += 1;
            }
        }

        Ok(value)
    });

    match evaluated {
        Ok(value) => CheckerOutcome::complete(
            value.map(|value| {
                EvaluatedConstantCall::new(value, evaluator.budget.usage(limits))
                    .with_states(states)
            }),
            evaluator.diagnostics,
        ),
        Err(TemplateEvaluationFailure::Diagnosed) => CheckerOutcome::complete(
            Some(EvaluatedConstantCall::new(
                recovery_value(context.semantic_values(), request.result_type()),
                evaluator.budget.usage(limits),
            )),
            evaluator.diagnostics,
        ),
        Err(TemplateEvaluationFailure::Diagnostic(problem)) => {
            let value = recovery_value(context.semantic_values(), request.result_type());

            let diagnostic = match template_failure_diagnostic(
                context,
                request.result_type(),
                problem,
                diagnostic_span,
            ) {
                Ok(diagnostic) => diagnostic,
                Err(CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
                Err(CheckerQueryError::Upstream(error)) => {
                    return CheckerOutcome::UpstreamFailure(error);
                }
            };

            evaluator.diagnostics.add(diagnostic);

            CheckerOutcome::complete(
                Some(EvaluatedConstantCall::new(
                    value,
                    evaluator.budget.usage(limits),
                )),
                evaluator.diagnostics,
            )
        }
        Err(TemplateEvaluationFailure::Cancelled) => CheckerOutcome::Cancelled,
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
            result.map(|evaluated| evaluated.map(|evaluated| evaluated.value())),
        ),
        CheckerOutcome::Cancelled => CheckerOutcome::Cancelled,
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
#[expect(
    clippy::too_many_arguments,
    reason = "initializer evaluation preserves its checked template identity, substitution, result and limits"
)]
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
        panic!(
            "Constant-evaluation inputs do not describe the requested bound unit. in evaluate_static_initializer_template"
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
    let mut budget = EvaluationBudget::from_limits(limits);

    let mut evaluator = TemplateEvaluator::new(
        context,
        Some(template),
        Some(substitution),
        resolver,
        limits,
        &mut budget,
    );

    let evaluated = evaluator
        .evaluate_generic_inputs()
        .and_then(|()| evaluator.instantiate_type(result_type))
        .and_then(|ty| evaluator.evaluate_result(kind, ty));

    complete_closed_evaluation(evaluator, evaluated, result_type, diagnostic_span)
}

/// Evaluates one closed checked term in its instantiated result type.
pub fn evaluate_constant_term<C>(
    context: &C,
    term: bray_symbols::ConstantTermId,
    result_type: TypeId,
    resolver: &dyn ConstantTemplateResolver<UpstreamError = C::UpstreamError>,
    diagnostic_span: Option<SourceSpan>,
) -> CheckerOutcome<Option<EvaluatedConstantCall>, C::UpstreamError>
where
    C: CheckerRequestContext + ?Sized,
{
    let limits = crate::ConstantEvaluationLimits::default();
    let mut budget = EvaluationBudget::from_limits(limits);
    let mut evaluator = TemplateEvaluator::new(context, None, None, resolver, limits, &mut budget);

    let evaluated = evaluator.evaluate_typed_term(term, result_type);

    complete_closed_evaluation(evaluator, evaluated, result_type, diagnostic_span)
}

fn complete_closed_evaluation<C>(
    mut evaluator: TemplateEvaluator<'_, C>,
    evaluated: Result<ConstantValueId, TemplateEvaluationFailure>,
    result_type: TypeId,
    diagnostic_span: Option<SourceSpan>,
) -> CheckerOutcome<Option<EvaluatedConstantCall>, C::UpstreamError>
where
    C: CheckerRequestContext + ?Sized,
{
    let context = evaluator.context;
    let limits = evaluator.limits;

    crate::diagnostic::anchor_diagnostics(
        &mut evaluator.diagnostics,
        bray_diagnostics::DiagnosticLabelKind::InvalidConstantExpression,
        || diagnostic_span,
    );

    match evaluated {
        Ok(value) => CheckerOutcome::complete(
            Some(EvaluatedConstantCall::new(
                value,
                evaluator.budget.usage(limits),
            )),
            evaluator.diagnostics,
        ),
        Err(TemplateEvaluationFailure::Diagnosed) => {
            CheckerOutcome::complete(None, evaluator.diagnostics)
        }
        Err(TemplateEvaluationFailure::Diagnostic(problem)) => {
            let diagnostic =
                match template_failure_diagnostic(context, result_type, problem, diagnostic_span) {
                    Ok(diagnostic) => diagnostic,
                    Err(CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
                    Err(CheckerQueryError::Upstream(error)) => {
                        return CheckerOutcome::UpstreamFailure(error);
                    }
                };

            evaluator.diagnostics.add(diagnostic);

            CheckerOutcome::complete(None, evaluator.diagnostics)
        }
        Err(TemplateEvaluationFailure::Cancelled) => CheckerOutcome::Cancelled,
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
        CallableInstanceData, ConstantField, ConstantProjection, ConstantProjectionKind,
        ConstantTermData, ConstantValueData, ConstantValueKind, CurrentRunCancellation,
        DependencyContractTemplateData, FunctionSymbolId, GenericOwnerId, GenericSubstitutionData,
        ModulePathKey, PackageIdentity, StaticInstanceKey, StaticInstanceTemplateId,
        StaticReferenceSelection, StaticSymbolId, StructFieldSymbolId, SymbolId, SymbolKey,
        SymbolKind, SymbolOrdinal, SymbolRootKey, TypeData,
    };
    use bray_testing::assert_goal_state_diagnostic_kind;

    use super::super::super::call::{
        ConstantCallRequest, ConstantCallResolution, ConstantCallResolver,
        ConstantTemplateResolver, EvaluatedConstantCall,
    };
    use super::super::super::limits::{ConstantEvaluationLimits, ConstantEvaluationUsage};
    use super::{
        evaluate_constant_call, evaluate_constant_definition_template,
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

        let request = |limits| {
            ConstantCallRequest::new(
                crate::ConstantCallTarget::Declaration(callable),
                None,
                [],
                ty,
                limits,
            )
        };

        let diagnostic_span = SourceSpan::new(
            SourceId::new(0),
            TextRange::new(TextSize::new(0), TextSize::new(1)),
        );

        let outcome = evaluate_constant_call(
            &TestCheckerContext::new(false),
            Some(&template),
            &request(ConstantEvaluationLimits::default()),
            &UnusedTemplateResolver,
            None,
            &[],
            || crate::CheckerOutcome::without_diagnostics(None),
        )
        .into_result()
        .unwrap_or_else(|| panic!("constant body template must evaluate"));

        assert!(outcome.diagnostics().is_empty());

        assert_eq!(
            outcome
                .value()
                .as_ref()
                .expect("checked body evaluates")
                .value(),
            value
        );

        assert_eq!(
            outcome
                .value()
                .as_ref()
                .expect("checked body evaluates")
                .usage()
                .steps(),
            2
        );

        assert_eq!(
            outcome
                .value()
                .as_ref()
                .expect("checked body evaluates")
                .usage()
                .aggregate_elements(),
            2
        );

        assert_eq!(
            outcome
                .value()
                .as_ref()
                .expect("checked body evaluates")
                .usage()
                .literal_bytes(),
            3
        );

        assert_eq!(
            outcome
                .value()
                .as_ref()
                .expect("checked body evaluates")
                .usage()
                .expansions(),
            4
        );

        let sequenced = bray_bound_tree::CheckedTemplate::from_checked_parts(
            template.kind(),
            template.inputs().iter().cloned(),
            [
                template.nodes()[0].clone(),
                CheckedTemplateNode::new(
                    CheckedTemplateOperation::Constant {
                        term,
                        usage: bray_bound_tree::CheckedTemplateConstantUsage::new(0, 2, 1),
                    },
                    ty,
                ),
                CheckedTemplateNode::new(
                    CheckedTemplateOperation::Sequence {
                        before: bray_bound_tree::CheckedTemplateNodeId::new(1),
                        value: bray_bound_tree::CheckedTemplateNodeId::new(0),
                    },
                    ty,
                ),
                CheckedTemplateNode::new(
                    CheckedTemplateOperation::Sequence {
                        before: bray_bound_tree::CheckedTemplateNodeId::new(2),
                        value: bray_bound_tree::CheckedTemplateNodeId::new(0),
                    },
                    ty,
                ),
            ],
            [],
            bray_bound_tree::CheckedTemplateNodeId::new(3),
            template.behavior().clone(),
        );

        let evaluate_sequence = |limits| {
            evaluate_constant_call(
                &TestCheckerContext::new(false),
                Some(&sequenced),
                &request(limits),
                &UnusedTemplateResolver,
                Some(diagnostic_span),
                &[],
                || crate::CheckerOutcome::without_diagnostics(None),
            )
            .into_result()
            .expect("sequenced constant body must evaluate")
        };

        let ordered = evaluate_sequence(ConstantEvaluationLimits::default());

        assert!(ordered.diagnostics().is_empty());

        assert_eq!(
            ordered
                .value()
                .as_ref()
                .expect("checked body evaluates")
                .value(),
            value
        );

        assert_eq!(
            ordered
                .value()
                .as_ref()
                .expect("checked body evaluates")
                .usage()
                .literal_bytes(),
            5
        );

        assert_eq!(
            ordered
                .value()
                .as_ref()
                .expect("checked body evaluates")
                .usage()
                .expansions(),
            5
        );

        let first_failure = evaluate_sequence(ConstantEvaluationLimits::new(16, 1, 1));

        assert_goal_state_diagnostic_kind(
            first_failure.diagnostics(),
            bray_diagnostics::DiagnosticKind::CheckingConstantLiteralSizeLimitExceeded,
        );

        let aggregate_outcome = evaluate_constant_call(
            &TestCheckerContext::new(false),
            Some(&template),
            &request(ConstantEvaluationLimits::new(16, 1, 16)),
            &UnusedTemplateResolver,
            Some(diagnostic_span),
            &[],
            || crate::CheckerOutcome::without_diagnostics(None),
        )
        .into_result()
        .unwrap_or_else(|| panic!("constant aggregate limit failure must recover"));

        assert_goal_state_diagnostic_kind(
            aggregate_outcome.diagnostics(),
            bray_diagnostics::DiagnosticKind::CheckingConstantAggregateLimitExceeded,
        );

        let literal_outcome = evaluate_constant_call(
            &TestCheckerContext::new(false),
            Some(&template),
            &request(ConstantEvaluationLimits::new(16, 16, 2)),
            &UnusedTemplateResolver,
            Some(diagnostic_span),
            &[],
            || crate::CheckerOutcome::without_diagnostics(None),
        )
        .into_result()
        .unwrap_or_else(|| panic!("constant literal limit failure must recover"));

        assert_goal_state_diagnostic_kind(
            literal_outcome.diagnostics(),
            bray_diagnostics::DiagnosticKind::CheckingConstantLiteralSizeLimitExceeded,
        );

        let expansion_outcome = evaluate_constant_call(
            &TestCheckerContext::new(false),
            Some(&template),
            &request(ConstantEvaluationLimits::new(16, 16, 16).with_expansions(3)),
            &UnusedTemplateResolver,
            Some(diagnostic_span),
            &[],
            || crate::CheckerOutcome::without_diagnostics(None),
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

        let outcome = evaluate_constant_call(
            &TestCheckerContext::new(false),
            Some(&template),
            &ConstantCallRequest::new(
                crate::ConstantCallTarget::Declaration(CallableInstanceData::new(
                    callable,
                    substitution,
                )),
                None,
                [value],
                ty,
                ConstantEvaluationLimits::default(),
            ),
            &UnusedTemplateResolver,
            None,
            &[],
            || crate::CheckerOutcome::without_diagnostics(None),
        )
        .into_result()
        .unwrap_or_else(|| panic!("callable argument template must evaluate"));

        assert!(outcome.diagnostics().is_empty());

        assert_eq!(
            outcome
                .value()
                .as_ref()
                .expect("checked body evaluates")
                .value(),
            value
        );
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

        let outcome = evaluate_constant_call(
            &TestCheckerContext::new(false),
            Some(&template),
            &ConstantCallRequest::new(
                crate::ConstantCallTarget::Declaration(callable),
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
            &[],
            || crate::CheckerOutcome::without_diagnostics(None),
        )
        .into_result()
        .unwrap_or_else(|| panic!("typed constant body template must evaluate"));

        assert!(outcome.diagnostics().is_empty());

        let result = values.constant_value_data(
            outcome
                .value()
                .as_ref()
                .expect("checked body evaluates")
                .value(),
        );

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
                CheckedTemplateOperation::construct(
                    None,
                    [
                        ConstantField::new(second.clone(), second_value),
                        ConstantField::new(first.clone(), first_value),
                    ],
                ),
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
            .as_ref()
            .unwrap_or_else(|| panic!("product evaluation must produce a value"));

        let data = values.constant_value_data(result.value());

        let ConstantValueKind::Product(fields) = data.kind() else {
            panic!("product template must materialize a product value");
        };

        assert_eq!(fields[0].field(), &resolver.second_id());
        assert_eq!(fields[1].field(), &resolver.first_id());
    }

    #[test]
    fn imported_definition_template_rejects_discarded_nested_static_borrow() {
        let values = semantic_values();

        let unit = values
            .intern_type(TypeData::tuple([]))
            .expect("fixture type must be interned");

        let borrowed = values
            .intern_type(TypeData::Borrow {
                kind: bray_symbols::BorrowKind::Shared,
                target: unit,
            })
            .expect("fixture type must be interned");

        let tuple = values
            .intern_type(TypeData::tuple([borrowed, unit]))
            .expect("fixture type must be interned");

        let static_symbol = StaticSymbolId::from_symbol_id(SymbolId::new(998));

        let owner = GenericOwnerId::try_new(static_symbol.into())
            .expect("fixture generic owner id must be valid");

        let substitution = values
            .intern_generic_substitution(
                GenericSubstitutionData::try_new(owner, [], [])
                    .expect("fixture generic substitution data must be valid"),
            )
            .expect("fixture generic substitution must be interned");

        let target = bray_target::TargetIdentity::try_new("test")
            .expect("fixture target identity must be valid");

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
            .expect("fixture constant value must be interned");

        let unit_value = values
            .intern_constant_value(ConstantValueData::new(
                unit,
                ConstantValueKind::Tuple(Arc::from([])),
            ))
            .expect("fixture constant value must be interned");

        let tuple_value = values
            .intern_constant_value(ConstantValueData::new(
                tuple,
                ConstantValueKind::Tuple(Arc::from([borrowed_value, unit_value])),
            ))
            .expect("fixture constant value must be interned");

        let term = values
            .intern_constant_term(ConstantTermData::Value(tuple_value))
            .expect("fixture constant term must be interned");

        let term = values
            .intern_constant_term(ConstantTermData::typed(term, tuple))
            .expect("fixture constant term must be interned");

        let term = values
            .intern_constant_term(ConstantTermData::Projection(ConstantProjection::new(
                term,
                ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1)),
            )))
            .expect("fixture constant term must be interned");

        let dependency_contract = values
            .intern_dependency_contract_template(DependencyContractTemplateData::new([]))
            .expect("fixture dependency contract must be interned");

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
                unit,
            ))
            .expect("fixture node must fit its identity space");

        let template = builder
            .finish(result, CheckedTemplateCompletion::Complete)
            .expect("fixture operation must finish successfully");

        let span = SourceSpan::new(
            SourceId::new(0),
            TextRange::new(TextSize::new(0), TextSize::new(1)),
        );

        let outcome = evaluate_constant_definition_template(
            &TestCheckerContext::new(false),
            &template,
            substitution,
            unit,
            &UnusedTemplateResolver,
            Some(span),
            ConstantEvaluationLimits::default(),
        )
        .into_result()
        .expect("fixture `into_result` call must succeed");

        assert!(outcome.value().is_none());

        assert_goal_state_diagnostic_kind(
            outcome.diagnostics(),
            bray_diagnostics::DiagnosticKind::CheckingNonMaterializableConstant,
        );

        let diagnostic = outcome
            .diagnostics()
            .by_kind(bray_diagnostics::DiagnosticKind::CheckingNonMaterializableConstant)
            .next()
            .expect("fixture iterator must provide its expected item");

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

        fn contract(
            &self,
            _target: &crate::ConstantCallTarget,
        ) -> CheckerQueryResult<
            bray_diagnostics::DiagnosticResult<Option<crate::ConstantCallContract>>,
        > {
            Ok(bray_diagnostics::DiagnosticResult::without_diagnostics(
                Some(crate::ConstantCallContract::new(
                    [],
                    self.expected_result_type,
                    [],
                )),
            ))
        }

        fn requirement_origin(
            &self,
            _target: &crate::ConstantCallTarget,
            _index: usize,
        ) -> CheckerQueryResult<Option<SourceSpan>> {
            panic!("this fixture has no failing callable requirements")
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
        fn requirement_template(
            &self,
            _target: &crate::ConstantCallTarget,
            _index: usize,
        ) -> CheckerQueryResult<
            bray_diagnostics::DiagnosticResult<Option<bray_bound_tree::CheckedTemplate>>,
        > {
            panic!("this fixture does not invoke a contracted callable");
        }

        fn symbol(&self, _key: &SymbolKey) -> Option<bray_symbols::AnySymbolId> {
            None
        }

        fn resolve_constant(
            &self,
            _instance: bray_symbols::ConstantInstanceKey,
            _limits: ConstantEvaluationLimits,
        ) -> CheckerQueryResult<bray_diagnostics::DiagnosticResult<ConstantReferenceResolution>>
        {
            unreachable!("typed call template must not resolve constants")
        }

        fn resolve_static(
            &self,
            _declaration: StaticSymbolId,
            _substitution: bray_symbols::GenericSubstitutionId,
        ) -> CheckerQueryResult<bray_diagnostics::DiagnosticResult<StaticReferenceSelection>>
        {
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

        fn contract(
            &self,
            _target: &crate::ConstantCallTarget,
        ) -> CheckerQueryResult<
            bray_diagnostics::DiagnosticResult<Option<crate::ConstantCallContract>>,
        > {
            panic!("this fixture evaluates checked nodes without semantic contract queries")
        }

        fn requirement_origin(
            &self,
            _target: &crate::ConstantCallTarget,
            _index: usize,
        ) -> CheckerQueryResult<Option<SourceSpan>> {
            panic!("this fixture has no failing callable requirements")
        }

        fn resolve(
            &self,
            _request: &ConstantCallRequest,
        ) -> CheckerQueryResult<ConstantCallResolution> {
            unreachable!("product template must not resolve calls")
        }
    }

    impl ConstantTemplateResolver for ProductResolver {
        fn requirement_template(
            &self,
            _target: &crate::ConstantCallTarget,
            _index: usize,
        ) -> CheckerQueryResult<
            bray_diagnostics::DiagnosticResult<Option<bray_bound_tree::CheckedTemplate>>,
        > {
            panic!("this fixture does not invoke a contracted callable");
        }

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
            _limits: ConstantEvaluationLimits,
        ) -> CheckerQueryResult<bray_diagnostics::DiagnosticResult<ConstantReferenceResolution>>
        {
            unreachable!("product template must not resolve constants")
        }

        fn resolve_static(
            &self,
            _declaration: StaticSymbolId,
            _substitution: bray_symbols::GenericSubstitutionId,
        ) -> CheckerQueryResult<bray_diagnostics::DiagnosticResult<StaticReferenceSelection>>
        {
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

        fn contract(
            &self,
            _target: &crate::ConstantCallTarget,
        ) -> CheckerQueryResult<
            bray_diagnostics::DiagnosticResult<Option<crate::ConstantCallContract>>,
        > {
            panic!("this fixture evaluates checked nodes without semantic contract queries")
        }

        fn requirement_origin(
            &self,
            _target: &crate::ConstantCallTarget,
            _index: usize,
        ) -> CheckerQueryResult<Option<SourceSpan>> {
            panic!("this fixture has no failing callable requirements")
        }

        fn resolve(
            &self,
            _request: &ConstantCallRequest,
        ) -> CheckerQueryResult<ConstantCallResolution> {
            unreachable!("closed test template must not resolve calls")
        }
    }

    impl ConstantTemplateResolver for UnusedTemplateResolver {
        fn requirement_template(
            &self,
            _target: &crate::ConstantCallTarget,
            _index: usize,
        ) -> CheckerQueryResult<
            bray_diagnostics::DiagnosticResult<Option<bray_bound_tree::CheckedTemplate>>,
        > {
            panic!("this fixture does not invoke a contracted callable");
        }

        fn symbol(&self, _key: &SymbolKey) -> Option<bray_symbols::AnySymbolId> {
            None
        }

        fn resolve_constant(
            &self,
            _instance: bray_symbols::ConstantInstanceKey,
            _limits: ConstantEvaluationLimits,
        ) -> CheckerQueryResult<bray_diagnostics::DiagnosticResult<ConstantReferenceResolution>>
        {
            unreachable!("closed test template must not resolve constants")
        }

        fn resolve_static(
            &self,
            _declaration: StaticSymbolId,
            _substitution: bray_symbols::GenericSubstitutionId,
        ) -> CheckerQueryResult<bray_diagnostics::DiagnosticResult<StaticReferenceSelection>>
        {
            unreachable!("closed test template must not resolve statics")
        }
    }
}
