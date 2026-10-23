use bray_binder::{
    BindingQueryContext, BindingQueryError, PredicateClauseBindingContext,
    bind_predicate_clause_expressions,
};
use bray_bound_tree::{BoundSourceAnchor, BoundUnitId, BoundUnitKey};
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{AnySymbolId, PredicateSemanticSummary};
use bray_syntax::{ExpressionSyntax, SyntaxNodeView};

use crate::compilation::binder::{BindingQueryResult, CompilationBindingContext};

/// Checks a predicate sequence through the shared binder, checker and condition normalization.
pub(in crate::compilation::binder::symbol) fn checked_bound_predicates(
    context: &CompilationBindingContext<'_>,
    owner: AnySymbolId,
    clause: SyntaxNodeView<'_>,
    expressions: Vec<ExpressionSyntax>,
    kind: PredicateClauseBindingContext,
) -> BindingQueryResult<DiagnosticResult<Vec<PredicateSemanticSummary>>> {
    let compilation = context.compilation();

    let owner_key = context
        .symbols()
        .symbol_key(owner)
        .expect("checked predicate owner has a semantic identity");

    let source = BoundSourceAnchor::new(
        bray_declarations::SyntaxAnchor::from_node(&clause),
        clause.source().version(),
    );

    // The bound unit retains its immutable declaration identity independently.
    let key = match kind {
        PredicateClauseBindingContext::GenericConstraint => {
            BoundUnitKey::constraint(owner_key.clone(), source)
        }
        PredicateClauseBindingContext::CallableContract(_) => {
            BoundUnitKey::contract_clause(owner_key.clone(), source)
        }
    }
    .expect("checked declaration owns its predicate clause");

    let computation =
        bind_predicate_clause_expressions(context, BoundUnitId::new(0), key, expressions)
            .map_err(crate::compilation::unit::map_binding_error)
            .map_err(super::super::binding::binder_error)?;

    let bound = computation.result().value();
    let declared = crate::compilation::binder::bind_declared_value_type_templates(context, bound)?;

    let ((semantics, patterns), mut diagnostics) =
        super::shared::checked_bound_contract(context, bound, declared.value(), Some(clause))?
            .into_parts();

    diagnostics.add_range(declared.into_parts().1);

    let constants = compilation
        .execution_constant_inputs(bound, &semantics, context.cancellation, &mut diagnostics)
        .map_err(super::super::binding::binder_error)?;

    let predicate_definitions = compilation
        .execution_predicate_inputs(&semantics, context.cancellation, &mut diagnostics)
        .map_err(super::super::binding::binder_error)?;

    let normalized = bray_checker::predicate_conditions_with_inputs(
        bound,
        &semantics,
        &patterns,
        context.semantic_values(),
        &constants,
        None,
        Some(&predicate_definitions),
    )
    .map_err(BindingQueryError::SemanticValue)?;

    let inputs = compilation
        .execution_callable_inputs(owner, context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let boolean = compilation
        .target_property_type(bray_target::TargetPropertyKind::ScalarBool)
        .map_err(super::super::binding::binder_error)?;

    let dependency = context
        .semantic_values()
        .empty_dependency_contract_template()
        .map_err(BindingQueryError::SemanticValue)?;

    let predicates = normalized
        .into_iter()
        .map(|(_, condition, domain)| {
            bray_checker::execution_condition_term(
                context.semantic_values(),
                &condition,
                &inputs,
                boolean,
            )
            .map(|term| PredicateSemanticSummary::new(dependency).with_condition(term, domain))
            .map_err(BindingQueryError::SemanticValue)
        })
        .collect::<BindingQueryResult<Vec<_>>>()?;

    Ok(DiagnosticResult::new(
        predicates,
        diagnostics.merged(computation.result().diagnostics()),
    ))
}
