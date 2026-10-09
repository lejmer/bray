use std::collections::{BTreeMap, BTreeSet};

use bray_binder::{
    BindingQueryContext, BindingQueryError, bind_callable_type_clause, semantic_unit_context,
};
use bray_bound_tree::{
    BoundSourceAnchor, BoundUnitId, BoundUnitKey, CheckedExpressionSemantics,
    DeclaredValueTypeEvidence, DeclaredValueTypeTemplates, DeclaredValueTypeTerm,
};
use bray_checker::{
    DefaultExpressionSemanticChecker, ExpressionSemanticChecker, ExpressionTypeInput,
    PatternCheckInput,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    AnySymbolId, CallablePhaseBehaviors, PredicateSemanticSummary, TypeData, TypeExpressionTemplate,
};
use bray_syntax::SyntaxNodeView;

use crate::compilation::binder::{BindingQueryResult, CompilationBindingContext};
use crate::compilation::checker::checker_result;

pub(in crate::compilation) fn checked_callable_type_contracts(
    context: &CompilationBindingContext<'_>,
    owner: AnySymbolId,
    syntax: SyntaxNodeView<'_>,
    template: &TypeExpressionTemplate,
) -> BindingQueryResult<DiagnosticResult<CallablePhaseBehaviors>> {
    let compilation = context.compilation();

    let constants = compilation
        .checked_constant_terms_for_templates_with_cancellation([template], context.cancellation)
        .map_err(super::super::binding::binder_error)?;

    let ty = bray_checker::resolve_type_expression_template(
        context.semantic_values(),
        template,
        constants.value(),
    );

    let Some(ty) = ty else {
        return Err(BindingQueryError::DependencyUnavailable);
    };

    let data = context.semantic_values().type_data(ty);

    let TypeData::Callable(callable) = data.as_ref() else {
        panic!("callable clause checking must receive a callable type");
    };

    let (parameter_list, clauses) = callable_type_clauses(syntax);

    let parameters = callable
        .parameters()
        .iter()
        .zip(parameter_list.parameters())
        .map(|(parameter, source)| {
            (
                parameter.name().clone(),
                parameter.ty(),
                bray_declarations::SyntaxAnchor::from_node(&source),
            )
        })
        .collect::<Vec<_>>();

    let mut requirements = Vec::new();
    let mut guarantees = Vec::new();
    let mut diagnostics = constants.into_parts().1;

    for (clause, expressions, postcondition, guards) in clauses {
        let owner_key = context
            .symbols()
            .symbol_key(owner)
            .expect("contract type has a lexical owner");

        let source = BoundSourceAnchor::new(clause, syntax.source().version());

        let key = BoundUnitKey::contract_clause(owner_key.clone(), source)
            .expect("source declaration can own type clauses");

        let guard_count = guards.len();
        let expressions = guards.into_iter().chain(expressions).collect();

        let (computation, references) = bind_callable_type_clause(
            context,
            BoundUnitId::new(0),
            key,
            &parameters,
            callable.result(),
            expressions,
        )
        .map_err(crate::compilation::unit::map_binding_error)
        .map_err(super::super::binding::binder_error)?;

        let bound = computation.result().value();

        let guard_expressions = callable_type_guard_expressions(bound, guard_count);

        let candidates = crate::compilation::unit::expression_candidates(context, bound)
            .map_err(super::super::binding::binder_error)?;

        let checker = compilation
            .checker_context(context.cancellation)
            .map_err(super::super::binding::binder_error)?;

        let semantic = semantic_unit_context(context.symbols(), bound);
        let request = bray_checker::CheckerUnitView::new(bound, &semantic, &checker);

        let evidence = bound
            .tree()
            .expressions()
            .filter_map(|(expression, bound)| {
                let bray_bound_tree::BoundExpression::Name(name) = bound else {
                    return None;
                };

                references
                    .iter()
                    .position(|reference| *reference == name.target())
                    .map(|ordinal| {
                        DeclaredValueTypeEvidence::new(
                            DeclaredValueTypeTerm::Expression(expression),
                            TypeExpressionTemplate::Resolved(parameters[ordinal].1),
                        )
                    })
            });

        let declared = DeclaredValueTypeTemplates::new(
            bound.unit(),
            bound.key().kind(),
            evidence,
            [],
            None,
            postcondition.then_some(TypeExpressionTemplate::Resolved(callable.result())),
        );

        let checked = checker_result(DefaultExpressionSemanticChecker.check_expression_semantics(
            request,
            &declared,
            &[],
            candidates.value(),
            &PatternCheckInput::new(),
            &ExpressionTypeInput::new(),
        ))
        .map_err(super::super::binding::binder_error)?;

        let ((types, selections, literals), checked_diagnostics) = checked.into_parts();

        let semantics = CheckedExpressionSemantics::try_new(types, selections, literals)
            .expect("contract type clause semantics have matching unit identity");

        let inputs = references
            .iter()
            .map(|reference| {
                let place = bray_checker::ExecutionPlace::from(*reference);

                (
                    place.clone(),
                    bray_checker::ExecutionCondition::Input(place),
                )
            })
            .collect::<BTreeMap<_, _>>();

        let conditions = bray_checker::predicate_conditions_with_inputs(
            bound,
            &semantics,
            context.semantic_values(),
            &inputs,
        )
        .map_err(BindingQueryError::SemanticValue)?;

        let boolean = compilation
            .target_property_type(bray_target::TargetPropertyKind::ScalarBool)
            .map_err(super::super::binding::binder_error)?;

        let dependency = context
            .semantic_values()
            .empty_dependency_contract_template()
            .map_err(BindingQueryError::SemanticValue)?;

        let mut guards = Vec::new();
        let mut predicates = Vec::new();

        for (expression, condition, trusted) in conditions {
            let term = bray_checker::execution_condition_term(
                context.semantic_values(),
                &condition,
                &references,
                boolean,
            )
            .map_err(BindingQueryError::SemanticValue)?;

            if guard_expressions.contains(&expression) {
                guards.push(term);
            } else {
                predicates.push((term, trusted));
            }
        }

        for (mut term, trusted) in predicates {
            for guard in &guards {
                term = super::super::contract::guarded_postcondition(context, *guard, term)?;
            }

            let predicate = PredicateSemanticSummary::new(dependency).with_condition(term, trusted);

            if postcondition {
                guarantees.push(predicate);
            } else {
                requirements.push(predicate);
            }
        }

        diagnostics.add_range(
            DiagnosticBag::merged_all([
                computation.result().diagnostics(),
                candidates.diagnostics(),
                &checked_diagnostics,
            ])
            .into_iter(),
        );
    }

    Ok(DiagnosticResult::new(
        callable
            .phase_behaviors()
            .clone()
            .with_predicates(requirements, guarantees),
        diagnostics,
    ))
}

fn callable_type_clauses(
    syntax: SyntaxNodeView<'_>,
) -> (
    bray_syntax::ParameterListSyntax,
    Vec<(
        bray_declarations::SyntaxAnchor,
        Vec<bray_syntax::ExpressionSyntax>,
        bool,
        Vec<bray_syntax::ExpressionSyntax>,
    )>,
) {
    let mut parameter_list = None;
    let mut clauses = Vec::new();

    bray_syntax::walk_direct_child_nodes(&syntax, |child| {
        if let Some(parameters) = child.cast::<bray_syntax::ParameterListSyntax>() {
            parameter_list = Some(parameters);
        } else if let Some(clause) = child.cast::<bray_syntax::RequiresClauseSyntax>() {
            clauses.push((
                bray_declarations::SyntaxAnchor::from_node(&clause),
                clause.expressions().collect::<Vec<_>>(),
                false,
                Vec::new(),
            ));
        } else if let Some(clause) = child.cast::<bray_syntax::EnsuresClauseSyntax>() {
            clauses.push((
                bray_declarations::SyntaxAnchor::from_node(&clause),
                clause.expressions().collect::<Vec<_>>(),
                true,
                Vec::new(),
            ));
        }

        bray_syntax::SyntaxWalkControl::SkipChildren
    });

    for (guards, clause) in super::super::contract::conditional_postconditions(syntax) {
        clauses.push((
            bray_declarations::SyntaxAnchor::from_node(&clause),
            clause.expressions().collect::<Vec<_>>(),
            true,
            guards.into_iter().map(|guard| guard.condition()).collect(),
        ));
    }

    let parameter_list = parameter_list.expect("callable type has parameters");

    (parameter_list, clauses)
}

fn callable_type_guard_expressions(
    bound: &bray_bound_tree::BoundUnit,
    count: usize,
) -> BTreeSet<bray_bound_tree::BoundExpressionId> {
    let bray_bound_tree::BoundUnitRoot::ExpressionSequence(block) = bound.root() else {
        panic!("callable type clause must retain its expression roots");
    };

    let mut pending = bound
        .view()
        .block(block)
        .expect("bound clause owns its root block")
        .items()
        .iter()
        .filter_map(|item| item.expression())
        .take(count)
        .collect::<Vec<_>>();

    let mut guard_expressions = BTreeSet::new();

    while let Some(expression) = pending.pop() {
        if guard_expressions.insert(expression) {
            pending.extend(
                bound
                    .view()
                    .expression(expression)
                    .expect("guard owns its descendants")
                    .child_expressions(),
            );
        }
    }

    guard_expressions
}
