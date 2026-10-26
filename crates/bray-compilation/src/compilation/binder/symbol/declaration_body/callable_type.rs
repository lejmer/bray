use std::collections::BTreeSet;

use bray_binder::{BindingQueryContext, BindingQueryError, bind_callable_type_clause};
use bray_bound_tree::{BoundSourceAnchor, BoundUnitId, BoundUnitKey};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    AnySymbolId, CallablePhaseBehaviors, PredicateSemanticSummary, TypeData, TypeExpressionTemplate,
};
use bray_syntax::SyntaxNodeView;

use crate::compilation::binder::{BindingQueryResult, CompilationBindingContext};

pub(in crate::compilation) fn checked_callable_type_contracts(
    context: &CompilationBindingContext<'_>,
    owner: AnySymbolId,
    syntax: SyntaxNodeView<'_>,
    template: &TypeExpressionTemplate,
    selected_clauses: Option<&[bray_declarations::SyntaxAnchor]>,
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

    let (parameters, mut clauses) = callable_type_clauses(syntax);

    if let Some(selected) = selected_clauses {
        clauses.retain(|(anchor, ..)| selected.contains(anchor));

        for anchor in selected {
            if anchor.syntax_kind() == bray_syntax::SyntaxKind::WhenClause {
                let guard = anchor
                    .find_descendant::<bray_syntax::WhenClauseSyntax>(context.syntax())
                    .expect("selected guard has its source syntax");

                clauses.push((*anchor, vec![guard.condition()], false, Vec::new()));
            }
        }
    }

    let mut requirements = Vec::new();
    let mut guarantees = Vec::new();
    let mut diagnostics = constants.into_parts().1;

    for (clause, _, postcondition, guards) in clauses {
        let owner_key = context
            .symbols()
            .symbol_key(owner)
            .expect("contract type has a lexical owner");

        let source = BoundSourceAnchor::new(clause, syntax.source().version());

        let key = BoundUnitKey::callable_type_clause(
            owner_key.clone(),
            BoundSourceAnchor::new(
                bray_declarations::SyntaxAnchor::from_node(&syntax),
                syntax.source().version(),
            ),
            source,
        );

        let guard_count = guards.len();

        let computation = compilation
            .bound_unit_with_cancellation(key.clone(), context.cancellation)
            .map_err(super::super::binding::binder_error)?;

        let bound = computation.result().value();

        let references = compilation
            .execution_unit_inputs(&key, context.cancellation)
            .map_err(super::super::binding::binder_error)?;

        let guard_expressions = callable_type_guard_expressions(bound, guard_count);

        let checked = compilation
            .expression_semantics_with_cancellation(key.clone(), context.cancellation)
            .map_err(super::super::binding::binder_error)?;

        let patterns = compilation
            .patterns_with_cancellation(key.clone(), context.cancellation)
            .map_err(super::super::binding::binder_error)?;

        let semantics = checked.result().value();
        let patterns = patterns.result().value();

        let eligibility = compilation
            .check_predicate_eligibility(&key, context.cancellation)
            .map_err(super::super::binding::binder_error)?;

        diagnostics.add_range(eligibility);
        diagnostics.add_range(checked.result().diagnostics().iter().cloned());

        let mut inputs = compilation
            .execution_constant_inputs(bound, semantics, context.cancellation, &mut diagnostics)
            .map_err(super::super::binding::binder_error)?;

        inputs.extend(references.iter().map(|reference| {
            let place = bray_checker::ExecutionPlace::from(*reference);

            (
                place.clone(),
                bray_checker::ExecutionCondition::Input(place),
            )
        }));

        let predicate_definitions = compilation
            .execution_predicate_inputs(semantics, context.cancellation, &mut diagnostics)
            .map_err(super::super::binding::binder_error)?;

        let conditions = bray_checker::predicate_conditions_with_inputs(
            bound,
            semantics,
            patterns,
            context.semantic_values(),
            &inputs,
            None,
            Some(&predicate_definitions),
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

        for (expression, condition, domain) in conditions {
            let term = bray_checker::execution_condition_term(
                context.semantic_values(),
                &condition,
                &references,
                boolean,
            )
            .map_err(BindingQueryError::SemanticValue)?;

            if guard_expressions.contains(&expression) {
                guards.push((term, domain));
            } else {
                predicates.push((term, domain));
            }
        }

        for (mut term, mut domain) in predicates {
            for (guard, guard_domain) in &guards {
                term = super::super::contract::guarded_postcondition(context, *guard, term)?;
                domain = domain.max(*guard_domain);
            }

            let predicate = PredicateSemanticSummary::new(dependency).with_condition(term, domain);

            if postcondition {
                guarantees.push(predicate);
            } else {
                requirements.push(predicate);
            }
        }

        diagnostics.add_range(
            DiagnosticBag::merged_all([
                computation.result().diagnostics(),
                checked.result().diagnostics(),
            ]),
        );
    }

    let capabilities = super::super::contract::bind_trusted_capability_clauses(
        context,
        owner,
        super::super::contract::direct_contract_clauses(syntax)
            .into_iter()
            .filter_map(|clause| match clause {
                super::super::contract::ContractClauseSyntax::Uses(clause)
                    if selected_clauses.is_none_or(|selected| {
                        selected.contains(&bray_declarations::SyntaxAnchor::from_node(&clause))
                    }) =>
                {
                    Some(clause)
                }
                _ => None,
            }),
        Some(&parameters),
    )?;

    diagnostics.add_range(capabilities.diagnostics().iter().cloned());

    super::super::contract::validate_trusted_capabilities(
        context,
        owner,
        callable.trust(),
        capabilities.value(),
        None,
        false,
        &mut diagnostics,
    )?;

    let phases = callable
        .phase_behaviors()
        .clone()
        .with_predicates(requirements, guarantees)
        .with_trusted_capabilities(
            capabilities
                .value()
                .iter()
                .map(|capability| capability.requirement()),
        );

    let normalized = bray_checker::normalize_numeric_phase_requirements(
        &crate::compilation::checker::CompilationCheckerContext::new(*context),
        &phases,
    )
    .map_err(crate::fact::FactQueryError::from)
    .map_err(super::super::binding::binder_error)?;

    Ok(DiagnosticResult::new(
        normalized.unwrap_or(phases),
        diagnostics,
    ))
}

type CallableTypeClause = (
    bray_declarations::SyntaxAnchor,
    Vec<bray_syntax::ExpressionSyntax>,
    bool,
    Vec<bray_syntax::ExpressionSyntax>,
);

fn callable_type_clauses(
    syntax: SyntaxNodeView<'_>,
) -> (bray_syntax::ParameterListSyntax, Vec<CallableTypeClause>) {
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

/// Reconstructs only the outer callable header, before its predicate contract is established.
pub(in crate::compilation) fn callable_clause_header(
    context: &CompilationBindingContext<'_>,
    key: &BoundUnitKey,
) -> BindingQueryResult<DiagnosticResult<bray_symbols::CallableTypeData>> {
    let anchor = key
        .callable_type_header()
        .expect("type clause retains its callable header");

    let syntax = context
        .compilation()
        .execution_declaration_node(anchor.syntax())
        .map_err(super::super::binding::binder_error)?;

    let owner = context
        .symbols()
        .symbol_for_key(key.declared_owner())
        .expect("type clause retains its lexical declaration");

    let template =
        super::super::environment::type_binder(context, owner)?.bind_callable_header(syntax)?;

    let constants = context
        .compilation()
        .checked_constant_terms_for_templates_with_cancellation(
            [template.value()],
            context.cancellation,
        )
        .map_err(super::super::binding::binder_error)?;

    let ty = bray_checker::resolve_type_expression_template(
        context.semantic_values(),
        template.value(),
        constants.value(),
    )
    .expect("bound callable header retains its checked type constants");

    let data = context.semantic_values().type_data(ty);

    let TypeData::Callable(callable) = data.as_ref() else {
        panic!("callable clause header retains a callable type");
    };

    Ok(DiagnosticResult::new(
        callable.clone(),
        DiagnosticBag::merged_all([template.diagnostics(), constants.diagnostics()]),
    ))
}

/// Binds a callable-type clause as an ordinary independently analyzed semantic unit.
pub(in crate::compilation) fn bind_callable_clause_unit(
    context: &CompilationBindingContext<'_>,
    unit: BoundUnitId,
    key: BoundUnitKey,
) -> Result<
    bray_binder::BoundUnitComputation,
    bray_binder::BoundUnitBindingError<crate::fact::FactQueryError>,
> {
    let header = callable_clause_header(context, &key)
        .map_err(crate::compilation::binder::binding_query_error)
        .map_err(bray_binder::BoundUnitBindingError::Upstream)?;

    let syntax = context
        .compilation()
        .execution_declaration_node(
            key.callable_type_header()
                .expect("clause retains its header")
                .syntax(),
        )
        .map_err(bray_binder::BoundUnitBindingError::Upstream)?;

    let (parameters, clauses) = callable_type_clauses(syntax);

    let inputs = header
        .value()
        .parameters()
        .iter()
        .zip(parameters.parameters())
        .map(|(parameter, syntax)| {
            (
                parameter.name().clone(),
                parameter.ty(),
                bray_declarations::SyntaxAnchor::from_node(&syntax),
            )
        })
        .collect::<Vec<_>>();

    let anchor = key.source().syntax();

    let expressions = if anchor.syntax_kind() == bray_syntax::SyntaxKind::WhenClause {
        let guard = anchor
            .find_descendant::<bray_syntax::WhenClauseSyntax>(context.syntax())
            .expect("guard retains its syntax");

        vec![guard.condition()]
    } else {
        let (_, expressions, _, guards) = clauses
            .into_iter()
            .find(|(clause, ..)| *clause == anchor)
            .expect("header owns its selected clause");

        guards.into_iter().chain(expressions).collect()
    };

    let (bound, _) = bind_callable_type_clause(
        context,
        unit,
        key,
        &inputs,
        header.value().result(),
        expressions,
    )?;

    let (result, dependencies) = bound.into_parts();

    let (bound, diagnostics) = result.into_parts();

    Ok(bray_binder::BoundUnitComputation::new(
        DiagnosticResult::new(bound, diagnostics.merged(header.diagnostics())),
        dependencies,
    ))
}

pub(in crate::compilation) fn callable_requirement_source(
    syntax: SyntaxNodeView<'_>,
    index: usize,
) -> (
    bray_declarations::SyntaxAnchor,
    bray_declarations::SyntaxAnchor,
) {
    callable_type_clauses(syntax)
        .1
        .into_iter()
        .filter(|(_, _, postcondition, _)| !postcondition)
        .flat_map(|(clause, expressions, _, _)| {
            expressions.into_iter().map(move |expression| {
                (
                    clause,
                    bray_declarations::SyntaxAnchor::from_node(&expression),
                )
            })
        })
        .nth(index)
        .expect("selected callable requirement retains its source expression")
}
