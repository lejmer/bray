use std::collections::{BTreeMap, BTreeSet};

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
use crate::compilation::unit::{checker_unit_view, semantic_unit_context_for};
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

    let empty = context
        .semantic_values()
        .empty_dependency_contract_template()
        .map_err(crate::compilation::binder::semantic_value_binding_error)?;

    let mut templates = BTreeMap::new();
    let mut units = BTreeMap::new();
    let mut pending = vec![root];
    let mut visited = BTreeSet::new();
    let mut diagnostics = DiagnosticBag::new();

    while let Some(callable) = pending.pop() {
        context.cancellation.check().map_err(binder_error)?;

        if !visited.insert(callable) {
            continue;
        }

        templates.insert(callable, empty);

        if let Some(address) = context.imported_semantic_address(callable.into_any())? {
            let dependencies = super::imported::imported_result_dependencies(context, address)?;

            templates.insert(callable, *dependencies.value());
            diagnostics = diagnostics.merged(dependencies.diagnostics());

            continue;
        }

        let Some(key) = compilation
            .declared_unit_key(callable.into_any(), BoundUnitKind::CallableBody)
            .map_err(binder_error)?
        else {
            continue;
        };

        // Related queries and the fixed-point worklist retain the same immutable unit key.
        let bound = compilation
            .bound_unit_with_cancellation(key.clone(), context.cancellation)
            .map_err(binder_error)?;

        let expressions = compilation
            .expression_semantics_with_cancellation(key.clone(), context.cancellation)
            .map_err(binder_error)?;

        let patterns = compilation
            .patterns_with_cancellation(key.clone(), context.cancellation)
            .map_err(binder_error)?;

        for entry in expressions.result().value().selections().entries() {
            if let SemanticSelection::Call(call) = entry.selection()
                && let BoundCallableTarget::Declaration(instance) = call.target()
            {
                pending.push(instance.definition().callable_symbol());
            }
        }

        diagnostics = DiagnosticBag::merged_all([
            &diagnostics,
            bound.result().diagnostics(),
            expressions.result().diagnostics(),
            patterns.result().diagnostics(),
        ]);

        units.insert(callable, (key, bound, expressions, patterns));
    }

    loop {
        context.cancellation.check().map_err(binder_error)?;
        let mut changed = false;

        for (callable, (key, bound, expressions, patterns)) in &units {
            let dependencies = infer_checked_result(
                context,
                key,
                bound.result().value(),
                expressions.result().value(),
                patterns.result().value(),
                &templates,
            )?;

            changed |= templates.insert(*callable, dependencies) != Some(dependencies);
        }

        if !changed {
            break;
        }
    }

    let result = templates.get(&root).copied().ok_or_else(|| {
        BindingQueryError::Binding(bray_binder::BindingError::SymbolRecordUnavailable(
            root.into_any(),
        ))
    })?;

    Ok(DiagnosticResult::new(result, diagnostics))
}

fn infer_checked_result(
    context: &CompilationBindingContext<'_>,
    key: &bray_bound_tree::BoundUnitKey,
    bound: &bray_bound_tree::BoundUnit,
    expressions: &bray_bound_tree::CheckedExpressionSemantics,
    patterns: &bray_bound_tree::CheckedPatterns,
    templates: &BTreeMap<CallableSymbolId, DependencyContractTemplateId>,
) -> BindingQueryResult<DependencyContractTemplateId> {
    let checker = context
        .compilation()
        .checker_context_for(key, context.cancellation)
        .map_err(binder_error)?;

    let semantic = semantic_unit_context_for(checker.symbols(), bound).map_err(binder_error)?;
    let request = checker_unit_view(bound, &semantic, &checker).map_err(binder_error)?;

    bray_checker::infer_result_dependencies(
        request,
        expressions.types(),
        expressions.selections(),
        patterns,
        templates,
    )
    .map_err(|error| match error {
        bray_checker::CheckerQueryError::Cancelled => BindingQueryError::Cancelled,
        bray_checker::CheckerQueryError::Infrastructure(error) => {
            BindingQueryError::CheckerInfrastructure(error)
        }
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

    for entry in expressions.selections().entries() {
        if let SemanticSelection::Call(call) = entry.selection()
            && let BoundCallableTarget::Declaration(instance) = call.target()
        {
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
    )?;

    Ok(DiagnosticResult::new(result, diagnostics))
}
