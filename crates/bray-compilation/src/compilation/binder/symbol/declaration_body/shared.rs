use crate::compilation::binder::{
    BindingQueryResult, semantic_contract_binding_error as binding_contract,
};
use bray_binder::{BindingQueryContext, BindingQueryError};
use bray_bound_tree::{
    BoundUnit, BoundUnitKey, BoundUnitRoot, CheckedBodySemantics, CheckedExpressionSemantics,
    SemanticSelection, StoragePlan,
};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{CallableExecutionRequirement, DependencyContractTemplateId, TypeId};

use super::dependency::portable_dependency_contract;
use crate::compilation::binder::CompilationBindingContext;
use crate::fact::PublishedUnitResult;

pub(super) fn checked_bound_contract(
    context: &CompilationBindingContext<'_>,
    bound: &BoundUnit,
    declared: &bray_bound_tree::DeclaredValueTypeTemplates,
    syntax: Option<bray_syntax::SyntaxNodeView<'_>>,
) -> BindingQueryResult<
    bray_diagnostics::DiagnosticResult<(
        CheckedExpressionSemantics,
        bray_bound_tree::CheckedPatterns,
    )>,
> {
    use bray_checker::{ExpressionSemanticChecker, PatternChecker};

    let compilation = context.compilation();

    let candidates =
        crate::compilation::unit::expression_candidates_in_syntax(context, bound, syntax)
            .map_err(super::super::binding::binder_error)?;

    let checker = compilation
        .checker_context(context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let semantic = bray_binder::semantic_unit_context(context.symbols(), bound);
    let mut request = bray_checker::CheckerUnitView::new(bound, &semantic, &checker);

    if let Some(syntax) = syntax {
        request = request.with_source_snapshot(syntax.source());
    }

    let operations =
        crate::compilation::operation::operation_expressions(context, bound, context.cancellation)
            .map_err(super::super::binding::binder_error)?;

    let mut resolutions = Vec::new();

    loop {
        let operation_input = crate::compilation::operation::selection_type_input(&resolutions);

        let checked = crate::compilation::checker::checker_result(
            bray_checker::DefaultExpressionSemanticChecker.check_expression_semantics(
                request,
                declared,
                &[],
                candidates.value(),
                &bray_checker::PatternCheckInput::new(),
                &operation_input,
            ),
        )
        .map_err(super::super::binding::binder_error)?;

        let ((types, selections, literals), diagnostics) = checked.into_parts();

        let semantics = CheckedExpressionSemantics::try_new(types, selections, literals)
            .expect("contract expression semantics describe the supplied bound unit");

        let (next, operation_diagnostics) = compilation
            .semantic_selection_inputs(
                bound.key(),
                bound,
                &semantics,
                &operations,
                &resolutions,
                context.cancellation,
            )
            .map_err(super::super::binding::binder_error)?;

        if next == resolutions {
            let (patterns, pattern_diagnostics) = crate::compilation::checker::checker_result(
                bray_checker::DefaultPatternChecker.check_patterns(
                    request,
                    semantics.types(),
                    &bray_checker::PatternCheckInput::new(),
                ),
            )
            .map_err(super::super::binding::binder_error)?
            .into_parts();

            return Ok(bray_diagnostics::DiagnosticResult::new(
                (semantics, patterns),
                DiagnosticBag::merged_all([
                    &diagnostics,
                    &pattern_diagnostics,
                    candidates.diagnostics(),
                    &operation_diagnostics,
                ]),
            ));
        }

        resolutions = next;
    }
}

pub(in crate::compilation::binder::symbol) struct CheckedSourceExpression {
    pub(in crate::compilation::binder::symbol) result: TypeId,
    pub(in crate::compilation::binder::symbol) dependency_contract: DependencyContractTemplateId,
    pub(in crate::compilation::binder::symbol) result_dependencies: DependencyContractTemplateId,
    pub(in crate::compilation::binder::symbol) diagnostics: DiagnosticBag,
    pub(in crate::compilation::binder::symbol) is_recovered: bool,
}

pub(in crate::compilation::binder::symbol) struct CheckedSourcePredicateSequence {
    pub(in crate::compilation::binder::symbol) predicates:
        Vec<bray_symbols::PredicateSemanticSummary>,
    pub(in crate::compilation::binder::symbol) execution_requirements:
        Vec<CallableExecutionRequirement>,
    pub(in crate::compilation::binder::symbol) diagnostics: DiagnosticBag,
}

pub(in crate::compilation::binder::symbol) fn checked_source_expression(
    context: &CompilationBindingContext<'_>,
    key: BoundUnitKey,
) -> BindingQueryResult<CheckedSourceExpression> {
    let compilation = context.compilation();

    let bound = compilation
        .bound_unit_with_cancellation(key.clone(), context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let BoundUnitRoot::Expression(root) = bound.result().value().root() else {
        panic!(
            "checked expression unit {key:?} has unexpected root {:?}",
            bound.result().value().root()
        );
    };

    let semantics = compilation
        .expression_semantics_with_cancellation(key.clone(), context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let storage = compilation
        .storage_plan_with_cancellation(key.clone(), context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let body = compilation
        .body_semantics_with_cancellation(key.clone(), context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let result = semantics
        .result()
        .value()
        .types()
        .expression(root)
        .ok_or_else(|| {
            binding_contract(
                crate::compilation::SemanticQueryContext::Expression {
                    unit: key.clone(),
                    expression: root,
                },
                crate::compilation::SemanticQueryViolation::Missing(
                    crate::compilation::SemanticDataKind::Type,
                ),
            )
        })?;

    let expression = bound
        .result()
        .value()
        .view()
        .expression(root)
        .ok_or_else(|| {
            binding_contract(
                crate::compilation::SemanticQueryContext::Unit(key.clone()),
                crate::compilation::SemanticQueryViolation::MissingBoundNode(root.into()),
            )
        })?;

    let dependencies = body.result().value().dependencies();

    let contract = dependencies
        .expression(root)
        .and_then(|contract| dependencies.contract(contract))
        .unwrap_or_else(|| {
            panic!("checked expression {root:?} in {key:?} retains its dependency contract")
        });

    let dependency_contract =
        portable_dependency_contract(context, storage.result().value(), contract)?;

    let returned =
        super::super::result_dependencies::expression_result_dependencies(context, &key)?;

    let diagnostics = checked_source_diagnostics(&bound, &semantics, &storage, &body)
        .merged(returned.diagnostics());

    Ok(CheckedSourceExpression {
        result: result.ty(),
        dependency_contract,
        result_dependencies: *returned.value(),
        diagnostics,
        is_recovered: expression.is_recovered()
            || result.is_recovered()
            || dependencies.is_recovered(),
    })
}

pub(in crate::compilation::binder::symbol) fn checked_source_predicate_sequence(
    context: &CompilationBindingContext<'_>,
    key: BoundUnitKey,
) -> BindingQueryResult<CheckedSourcePredicateSequence> {
    let compilation = context.compilation();

    let bound = compilation
        .bound_unit_with_cancellation(key.clone(), context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let (BoundUnitRoot::ExpressionSequence(_) | BoundUnitRoot::Expression(_)) =
        bound.result().value().root()
    else {
        panic!(
            "checked predicate unit {key:?} has unexpected root {:?}",
            bound.result().value().root()
        );
    };

    let semantics = compilation
        .expression_semantics_with_cancellation(key.clone(), context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let storage = compilation
        .storage_plan_with_cancellation(key.clone(), context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let body = compilation
        .body_semantics_with_cancellation(key.clone(), context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let dependencies = body.result().value().dependencies();

    let owner = context
        .symbols()
        .symbol_for_key(key.declared_owner())
        .expect("checked predicate clause must retain its declaration owner");

    let inputs = compilation
        .execution_callable_inputs(owner, context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let boolean = compilation
        .target_property_type(bray_target::TargetPropertyKind::ScalarBool)
        .map_err(super::super::binding::binder_error)?;

    let mut diagnostics = checked_source_diagnostics(&bound, &semantics, &storage, &body);

    let constants = compilation
        .execution_constant_inputs(
            bound.result().value(),
            semantics.result().value(),
            context.cancellation,
            &mut diagnostics,
        )
        .map_err(super::super::binding::binder_error)?;

    let patterns = compilation
        .patterns_with_cancellation(key.clone(), context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let predicate_definitions = compilation
        .execution_predicate_inputs(
            semantics.result().value(),
            context.cancellation,
            &mut diagnostics,
        )
        .map_err(super::super::binding::binder_error)?;

    let normalized = bray_checker::predicate_conditions_with_inputs(
        bound.result().value(),
        semantics.result().value(),
        patterns.result().value(),
        context.semantic_values(),
        &constants,
        Some((
            storage.result().value(),
            body.result().value().storage_flow(),
        )),
        Some(&predicate_definitions),
    )
    .map_err(|error| BindingQueryError::Binding(bray_binder::BindingError::SemanticValue(error)))?;

    let mut predicates = Vec::new();
    let mut execution_requirements = Vec::new();

    for (expression, condition, is_trusted) in normalized {
        let contract = dependencies
            .expression(expression)
            .and_then(|contract| dependencies.contract(contract));

        let Some(contract) = contract else {
            panic!("checked predicate {expression:?} in {key:?} retains its dependency contract");
        };

        let dependency = portable_dependency_contract(context, storage.result().value(), contract)?;

        let term = bray_checker::execution_condition_term(
            context.semantic_values(),
            &condition,
            &inputs,
            boolean,
        )
        .map_err(|error| {
            BindingQueryError::Binding(bray_binder::BindingError::SemanticValue(error))
        })?;

        predicates.push(
            bray_symbols::PredicateSemanticSummary::new(dependency).with_condition(
                term,
                bray_symbols::PredicateDomain::checked_condition(
                    term,
                    is_trusted,
                    context.semantic_values(),
                    compilation.available_compiler_known_symbols(),
                ),
            ),
        );

        if let Some(requirement) = execution_requirement(
            semantics
                .result()
                .value()
                .selections()
                .expression(expression),
        ) {
            execution_requirements.push(requirement);
        }
    }

    Ok(CheckedSourcePredicateSequence {
        predicates,
        execution_requirements,
        diagnostics,
    })
}

fn execution_requirement(
    selection: Option<&SemanticSelection>,
) -> Option<CallableExecutionRequirement> {
    let SemanticSelection::Call(call) = selection? else {
        return None;
    };

    if !call.implementation_hook()?.is_execution_requirement() {
        return None;
    }

    let bray_bound_tree::BoundCallableTarget::Predicate(instance) = call.target() else {
        return None;
    };

    Some(CallableExecutionRequirement::new(
        instance.definition().into_any(),
    ))
}

fn checked_source_diagnostics(
    bound: &PublishedUnitResult<BoundUnit>,
    semantics: &PublishedUnitResult<CheckedExpressionSemantics>,
    storage: &PublishedUnitResult<StoragePlan>,
    body: &PublishedUnitResult<std::sync::Arc<CheckedBodySemantics>>,
) -> DiagnosticBag {
    DiagnosticBag::merged_all([
        bound.result().diagnostics(),
        semantics.result().diagnostics(),
        storage.result().diagnostics(),
        body.result().diagnostics(),
    ])
}

pub(super) fn syntax_diagnostics(
    context: &CompilationBindingContext<'_>,
    anchor: bray_declarations::SyntaxAnchor,
) -> DiagnosticBag {
    let mut diagnostics = DiagnosticBag::new();

    diagnostics.add_range(
        context
            .compilation()
            .syntax_tree_result()
            .diagnostics()
            .iter()
            .filter(|diagnostic| {
                diagnostic.primary_span().is_some_and(|span| {
                    span.source_id() == anchor.source_id()
                        && anchor.full_range().contains_range(span.range())
                })
            })
            .cloned(),
    );

    diagnostics
}
