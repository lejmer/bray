use std::collections::{BTreeMap, BTreeSet};

use bray_binder::semantic_unit_context;
use bray_binder::{BindingQueryContext, BindingQueryError, SymbolQueryProvider};
use bray_bound_tree::{BoundCallableTarget, BoundUnitKind, SemanticSelection};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    CallableResultDependenciesQuery, CallableSymbolId, DependencyContractTemplateId,
    SymbolQueryRequest,
};

use super::binding::{CompilationSymbolQueryEvaluator, binder_error};
use super::cache::CompilationSymbolSemantics;
use crate::compilation::binder::{BindingQueryResult, CompilationBindingContext};

use crate::fact::SymbolQueryCache;

impl CompilationSymbolQueryEvaluator<CallableResultDependenciesQuery>
    for CompilationSymbolSemantics
{
    fn cache(&self) -> &SymbolQueryCache<CallableResultDependenciesQuery> {
        &self.callable_result_dependencies
    }

    fn bind(
        &self,
        context: &CompilationBindingContext<'_>,
        request: SymbolQueryRequest<CallableResultDependenciesQuery>,
    ) -> BindingQueryResult<DiagnosticResult<DependencyContractTemplateId>> {
        infer_reachable_results(context, request.owner())
    }
}

fn infer_reachable_results(
    context: &CompilationBindingContext<'_>,
    root: CallableSymbolId,
) -> BindingQueryResult<DiagnosticResult<DependencyContractTemplateId>> {
    let compilation = context.compilation();

    if let Some(address) = context.imported_semantic_address(root.into_any())? {
        return super::imported::imported_result_dependencies(context, address);
    }

    let mut units = BTreeMap::new();
    let mut pending = vec![root];
    let mut edges = BTreeMap::<CallableSymbolId, BTreeSet<CallableSymbolId>>::new();
    let mut visited = BTreeSet::new();

    while let Some(callable) = pending.pop() {
        context.cancellation.check().map_err(binder_error)?;

        if !visited.insert(callable)
            || context
                .imported_semantic_address(callable.into_any())?
                .is_some()
        {
            continue;
        }

        let Some(key) = compilation
            .declared_unit_key(callable.into_any(), BoundUnitKind::CallableBody)
            .map_err(binder_error)?
        else {
            continue;
        };

        let behavior = compilation
            .selected_body_behavior_with_cancellation(key.clone(), context.cancellation)
            .map_err(binder_error)?;

        for call in behavior.result().value().calls() {
            if let BoundCallableTarget::Declaration(instance) = call.target() {
                let callee = instance.definition().callable_symbol();

                pending.push(callee);
                edges.entry(callable).or_default().insert(callee);
            }
        }

        units.insert(callable, key);
    }

    if !units.contains_key(&root) {
        let empty = context
            .semantic_values()
            .empty_dependency_contract_template()
            .expect("empty result dependency template satisfies the semantic store contract");

        return Ok(DiagnosticResult::new(empty, DiagnosticBag::new()));
    }

    let components = bray_base::strongly_connected_components(units.keys().copied(), |callable| {
        edges.get(&callable).into_iter().flatten().copied()
    });

    let component = components
        .into_iter()
        .find(|component| component.contains(&root))
        .expect("a demanded source callable belongs to its reachable call component");

    let owner = component.iter().min().expect("call component is nonempty");

    let key = units
        .get(owner)
        .expect("source component owner retains its body")
        .clone();

    let published = compilation
        .unit_query(
            &compilation.state.result_dependency_components,
            crate::fact::CompilationFactKey::ResultDependencyComponent(key.clone()),
            key,
            context.cancellation,
            |cancellation| {
                let result = infer_component(compilation, &component, cancellation)?;

                Ok((result, Box::new([])))
            },
        )
        .map_err(binder_error)?;

    Ok(DiagnosticResult::new(
        *published
            .result()
            .value()
            .get(&root)
            .expect("settled component publishes every member"),
        published.result().diagnostics().clone(),
    ))
}

fn infer_component(
    compilation: &crate::Compilation,
    component: &[CallableSymbolId],
    cancellation: &crate::fact::CancellationToken,
) -> Result<
    DiagnosticResult<BTreeMap<CallableSymbolId, DependencyContractTemplateId>>,
    crate::fact::FactQueryError,
> {
    use crate::compilation::binder::binding_query_error;

    let context = compilation.binding_context(cancellation)?;

    let empty = context
        .semantic_values()
        .empty_dependency_contract_template()
        .expect("empty result dependency template satisfies the semantic store contract");

    let members = component.iter().copied().collect::<BTreeSet<_>>();
    let mut units = BTreeMap::new();
    let mut callees = BTreeSet::new();
    let mut cyclic = component.len() > 1;
    let mut diagnostics = DiagnosticBag::new();

    for callable in component {
        let key = compilation
            .declared_unit_key(callable.into_any(), BoundUnitKind::CallableBody)?
            .expect("source summary component member retains its body");

        // Read each prerequisite inside the publication owner so incremental reuse
        // follows the member's own selected body and semantic inputs.
        let bound = compilation.bound_unit_with_cancellation(key.clone(), cancellation)?;

        let expressions =
            compilation.expression_semantics_with_cancellation(key.clone(), cancellation)?;

        let patterns = compilation.patterns_with_cancellation(key.clone(), cancellation)?;

        let behavior =
            compilation.selected_body_behavior_with_cancellation(key.clone(), cancellation)?;

        for call in behavior.result().value().calls() {
            if let BoundCallableTarget::Declaration(instance) = call.target() {
                let callee = instance.definition().callable_symbol();

                cyclic |= callee == *callable;

                if !members.contains(&callee) {
                    callees.insert(callee);
                }
            }
        }

        diagnostics = DiagnosticBag::merged_all([
            &diagnostics,
            bound.result().diagnostics(),
            expressions.result().diagnostics(),
            patterns.result().diagnostics(),
            behavior.result().diagnostics(),
        ]);

        units.insert(*callable, (key, bound, expressions, patterns));
    }

    let mut templates = members
        .iter()
        .map(|callable| (*callable, empty))
        .collect::<BTreeMap<_, _>>();

    for callee in callees {
        let result = context
            .resolve_symbol_query(SymbolQueryRequest::<CallableResultDependenciesQuery>::new(
                callee,
            ))
            .map_err(binding_query_error)?;

        templates.insert(callee, *result.value());
        diagnostics = diagnostics.merged(result.diagnostics());
    }

    let recursive = if cyclic {
        members.clone()
    } else {
        BTreeSet::new()
    };

    loop {
        cancellation.check()?;

        let mut changed = false;

        for (callable, (key, bound, expressions, patterns)) in &units {
            let dependencies = infer_checked_result(
                &context,
                key,
                bound.result().value(),
                expressions.result().value(),
                patterns.result().value(),
                &templates,
                &recursive,
            )
            .map_err(binding_query_error)?;

            changed |= templates.insert(*callable, dependencies) != Some(dependencies);
        }

        if !cyclic || !changed {
            break;
        }
    }

    templates.retain(|callable, _| members.contains(callable));

    Ok(DiagnosticResult::new(templates, diagnostics))
}

fn infer_checked_result(
    context: &CompilationBindingContext<'_>,
    key: &bray_bound_tree::BoundUnitKey,
    bound: &bray_bound_tree::BoundUnit,
    expressions: &bray_bound_tree::CheckedExpressionSemantics,
    patterns: &bray_bound_tree::CheckedPatterns,
    templates: &BTreeMap<CallableSymbolId, DependencyContractTemplateId>,
    recursive_callees: &BTreeSet<CallableSymbolId>,
) -> BindingQueryResult<DependencyContractTemplateId> {
    let checker = context
        .compilation()
        .checker_context_for(key, context.cancellation)
        .map_err(binder_error)?;

    let semantic = semantic_unit_context(checker.symbols(), bound);
    let request = bray_checker::CheckerUnitView::new(bound, &semantic, &checker);

    let static_roots = expressions
        .selections()
        .entries()
        .iter()
        .filter_map(|entry| match entry.selection() {
            SemanticSelection::StaticReference(selection) => {
                Some(selection.template().declaration())
            }
            _ => None,
        })
        .map(|declaration| {
            super::declaration_body::static_dependency_root(context, declaration)
                .map(|root| (declaration, root))
        })
        .collect::<BindingQueryResult<BTreeMap<_, _>>>()?;

    bray_checker::infer_result_dependencies(
        request,
        expressions.types(),
        expressions.selections(),
        patterns,
        templates,
        recursive_callees,
        &static_roots,
    )
    .map_err(|error| match error {
        bray_checker::CheckerQueryError::Cancelled => BindingQueryError::Cancelled,
        bray_checker::CheckerQueryError::Upstream(error) => binder_error(error),
    })
}

pub(in crate::compilation) fn expression_result_dependencies(
    context: &CompilationBindingContext<'_>,
    key: &bray_bound_tree::BoundUnitKey,
    bound: &bray_bound_tree::BoundUnit,
    expressions: &bray_bound_tree::CheckedExpressionSemantics,
) -> BindingQueryResult<DiagnosticResult<DependencyContractTemplateId>> {
    let mut templates = BTreeMap::new();
    let mut diagnostics = DiagnosticBag::new();

    let behavior = context
        .compilation()
        .selected_body_behavior_with_cancellation(key.clone(), context.cancellation)
        .map_err(binder_error)?;

    diagnostics = diagnostics.merged(behavior.result().diagnostics());

    for call in behavior.result().value().calls() {
        if let BoundCallableTarget::Declaration(instance) = call.target() {
            let callable = instance.definition().callable_symbol();

            if templates.contains_key(&callable) {
                continue;
            }

            let result = context.resolve_symbol_query(SymbolQueryRequest::<
                CallableResultDependenciesQuery,
            >::new(callable))?;

            templates.insert(callable, *result.value());
            diagnostics = diagnostics.merged(result.diagnostics());
        }
    }

    // The query retains its immutable unit key independently of the enclosing default query.
    let patterns = context
        .compilation()
        .patterns_with_cancellation(key.clone(), context.cancellation)
        .map_err(binder_error)?;

    diagnostics = diagnostics.merged(patterns.result().diagnostics());

    let result = infer_checked_result(
        context,
        key,
        bound,
        expressions,
        patterns.result().value(),
        &templates,
        &BTreeSet::new(),
    )?;

    Ok(DiagnosticResult::new(result, diagnostics))
}
