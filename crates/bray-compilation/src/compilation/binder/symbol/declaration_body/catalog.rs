use bray_binder::{BindingQueryContext, BindingQueryError, SymbolQueryProvider, bind_contract_clause_expressions, semantic_unit_context};
use bray_bound_tree::{BoundSourceAnchor, BoundUnitId, BoundUnitKey, CheckedExpressionSemantics, DeclaredValueTypeTemplates};
use bray_checker::{DefaultExpressionSemanticChecker, ExpressionSemanticChecker, ExpressionTypeInput, PatternCheckInput};
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{AnySymbolId, PredicateSemanticSummary};
use bray_syntax::{ExpressionSyntax, SyntaxNodeView};

use crate::compilation::binder::{BindingQueryResult, CompilationBindingContext};
use crate::compilation::checker::checker_result;

/// Check catalog caller predicates with the same binder and normalization as source clauses.
pub(in crate::compilation::binder::symbol) fn checked_catalog_predicates(
    context: &CompilationBindingContext<'_>,
    owner: AnySymbolId,
    clause: SyntaxNodeView<'_>,
    expressions: Vec<ExpressionSyntax>,
) -> BindingQueryResult<DiagnosticResult<Vec<PredicateSemanticSummary>>> {
    let compilation = context.compilation();

    let owner_key = context.symbols().symbol_key(owner)
        .expect("catalog contract owner must have a semantic identity");

    let source = BoundSourceAnchor::new(bray_declarations::SyntaxAnchor::from_node(&clause), clause.source().version());

    let key = BoundUnitKey::contract_clause(owner_key.clone(), source)
        .expect("catalog callable must own its predicate clause");

    let computation = bind_contract_clause_expressions(context, BoundUnitId::new(0), key, expressions)
        .map_err(crate::compilation::unit::map_binding_error)
        .map_err(super::super::binding::binder_error)?;

    let bound = computation.result().value();

    let candidates = crate::compilation::unit::expression_candidates_in_syntax(context, bound, Some(clause))
        .map_err(super::super::binding::binder_error)?;

    let checker = compilation.checker_context(context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let semantic = semantic_unit_context(context.symbols(), bound);
    let request = bray_checker::CheckerUnitView::new(bound, &semantic, &checker).with_source_snapshot(clause.source());
    let callable = bray_symbols::CallableSymbolId::try_from_any(owner).expect("catalog predicate owner is callable");
    let signature = context.resolve_symbol_query(bray_symbols::SymbolQueryRequest::<bray_symbols::CallableSignatureQuery>::new(callable))?;
    let mut evidence = Vec::new();

    for (expression, node) in bound.tree().expressions() {
        let bray_bound_tree::BoundExpression::Name(name) = node else { continue; };

        let template = match name.target() {
            bray_bound_tree::BoundReferenceTarget::Surface(AnySymbolId::CallableParameter(parameter)) => {
                let ordinal = signature.value().parameters().iter().position(|candidate| *candidate == parameter)
                    .expect("bound catalog clause parameter belongs to its signature");

                Some(signature.value().parameter_type_template(parameter, u32::try_from(ordinal).expect("signature ordinal fits"), context.semantic_values())
                    .expect("signature retains every parameter type"))
            }
            bray_bound_tree::BoundReferenceTarget::Local(bray_symbols::AnyLocalSymbolId::PostconditionResult(_)) => Some(signature.value().result().clone()),
            _ => None,
        };

        if let Some(template) = template {
            evidence.push(bray_bound_tree::DeclaredValueTypeEvidence::new(bray_bound_tree::DeclaredValueTypeTerm::Expression(expression), template));
        }
    }

    let declared = DeclaredValueTypeTemplates::new(bound.unit(), bound.key().kind(), evidence, [], None, Some(signature.value().result().clone()));

    let checked = checker_result(DefaultExpressionSemanticChecker.check_expression_semantics(request,
        &declared, &[], candidates.value(), &PatternCheckInput::new(), &ExpressionTypeInput::new()))
        .map_err(super::super::binding::binder_error)?;

    let ((types, selections, literals), diagnostics) = checked.into_parts();

    let semantics = CheckedExpressionSemantics::try_new(types, selections, literals)
        .expect("catalog expression semantics must describe the supplied unit");

    let normalized = bray_checker::predicate_conditions(bound, &semantics, context.semantic_values())
        .map_err(BindingQueryError::SemanticValue)?;

    let inputs = compilation.execution_callable_inputs(owner, context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let boolean = compilation.target_property_type(bray_target::TargetPropertyKind::ScalarBool)
        .map_err(super::super::binding::binder_error)?;

    let dependency = context.semantic_values().empty_dependency_contract_template()
        .map_err(BindingQueryError::SemanticValue)?;

    let predicates = normalized.into_iter().map(|(_, condition, trusted)| {
        bray_checker::execution_condition_term(context.semantic_values(), &condition, &inputs, boolean)
            .map(|term| PredicateSemanticSummary::new(dependency).with_condition(term, trusted))
            .map_err(BindingQueryError::SemanticValue)
    }).collect::<BindingQueryResult<Vec<_>>>()?;

    Ok(DiagnosticResult::new(predicates, diagnostics.merged(computation.result().diagnostics())
        .merged(candidates.diagnostics())))
}
