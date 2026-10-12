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

/// The body and its callable summary are settled and published by the same component.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(in crate::compilation) struct CallableAnalysis {
    pub(in crate::compilation) dependencies: DependencyContractTemplateId,
    pub(in crate::compilation) body:
        DiagnosticResult<std::sync::Arc<bray_bound_tree::CheckedBodySemantics>>,
}

type Component = BTreeMap<CallableSymbolId, CallableAnalysis>;

pub(in crate::compilation) fn callable_body_semantics(
    context: &CompilationBindingContext<'_>,
    key: &bray_bound_tree::BoundUnitKey,
) -> BindingQueryResult<DiagnosticResult<std::sync::Arc<bray_bound_tree::CheckedBodySemantics>>> {
    let owner = context
        .symbols()
        .symbol_for_key(key.declared_owner())
        .expect("declared callable body retains its symbol");

    let callable = CallableSymbolId::try_from_any(owner)
        .expect("callable body is owned by a callable declaration");

    let component = source_component(context, callable)?
        .expect("declared callable body belongs to a source component");

    Ok(component.result().value()[&callable].body.clone())
}

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
    if let Some(address) = context.imported_semantic_address(root.into_any())? {
        return super::imported::imported_result_dependencies(context, address);
    }

    let Some(component) = source_component(context, root)? else {
        let empty = context
            .semantic_values()
            .empty_dependency_contract_template()
            .expect("empty result dependency template satisfies the semantic store contract");

        return Ok(DiagnosticResult::new(empty, DiagnosticBag::new()));
    };

    Ok(DiagnosticResult::new(
        component.result().value()[&root].dependencies,
        component.result().diagnostics().clone(),
    ))
}

fn source_component(
    context: &CompilationBindingContext<'_>,
    root: CallableSymbolId,
) -> BindingQueryResult<Option<std::sync::Arc<crate::fact::PublishedUnitResult<Component>>>> {
    let compilation = context.compilation();
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

        let prepared = compilation
            .prepared_body_analysis_with_cancellation(key.clone(), context.cancellation)
            .map_err(binder_error)?;

        for callee in prepared.result().value().lifecycle_callees() {
            pending.push(callee);
            edges.entry(callable).or_default().insert(callee);
        }

        units.insert(callable, key);
    }

    if !units.contains_key(&root) {
        return Ok(None);
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

    Ok(Some(published))
}

fn infer_component(
    compilation: &crate::Compilation,
    component: &[CallableSymbolId],
    cancellation: &crate::fact::CancellationToken,
) -> Result<DiagnosticResult<Component>, crate::fact::FactQueryError> {
    use crate::compilation::binder::binding_query_error;

    let context = compilation.binding_context(cancellation)?;

    let empty = context
        .semantic_values()
        .intern_dependency_contract_template(
            bray_symbols::DependencyContractTemplateData::new([]).with_effects([]),
        )
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
        let inputs = compilation.body_analysis_inputs(key.clone(), cancellation)?;

        let selected =
            inputs
                .behavior
                .result()
                .value()
                .calls()
                .iter()
                .filter_map(|call| match call.target() {
                    BoundCallableTarget::Declaration(instance) => {
                        Some(instance.definition().callable_symbol())
                    }
                    _ => None,
                });

        for callee in selected.chain(inputs.prepared.result().value().lifecycle_callees()) {
            cyclic |= callee == *callable;

            if !members.contains(&callee) {
                callees.insert(callee);
            }
        }

        diagnostics = DiagnosticBag::merged_all([
            &diagnostics,
            inputs.bound.result().diagnostics(),
            inputs.expressions.result().diagnostics(),
            inputs.patterns.result().diagnostics(),
            inputs.behavior.result().diagnostics(),
        ]);

        units.insert(*callable, (key, inputs));
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

    let mut bodies = BTreeMap::new();

    loop {
        cancellation.check()?;

        let mut changed = false;

        for (callable, (key, inputs)) in &units {
            let dependencies = infer_checked_result(
                &context,
                key,
                inputs.bound.result().value(),
                inputs.expressions.result().value(),
                inputs.patterns.result().value(),
                &templates,
                &recursive,
            )
            .map_err(binding_query_error)?;

            let body = inputs.check(compilation, cancellation, Some(&templates))?;
            let values = context.semantic_values();
            let template = values.dependency_contract_template_data(dependencies);
            let previous = values.dependency_contract_template_data(templates[callable]);

            let dependencies = values
                .intern_dependency_contract_template(body_result_dependencies(
                    &template,
                    body.value().storage_flow().dependencies(),
                    previous.effects(),
                ))
                .expect("checked component summary has established semantic identities");

            bodies.insert(*callable, CallableAnalysis { dependencies, body });

            changed |= templates.insert(*callable, dependencies) != Some(dependencies);
        }

        if !cyclic || !changed {
            break;
        }
    }

    Ok(DiagnosticResult::new(bodies, diagnostics))
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

    let body = context
        .compilation()
        .body_semantics_with_cancellation(key.clone(), context.cancellation)
        .map_err(binder_error)?;

    diagnostics = diagnostics.merged(body.result().diagnostics());

    let values = context.semantic_values();
    let template = values.dependency_contract_template_data(result);

    let result = values
        .intern_dependency_contract_template(body_result_dependencies(
            &template,
            body.result().value().storage_flow().dependencies(),
            &[],
        ))
        .expect("checked default summary retains its semantic identities");

    Ok(DiagnosticResult::new(result, diagnostics))
}

fn body_result_dependencies(
    inferred: &bray_symbols::DependencyContractTemplateData,
    checked: &bray_symbols::DependencyContractTemplateData,
    previous_effects: &[bray_symbols::DependencyStorageEffect],
) -> bray_symbols::DependencyContractTemplateData {
    bray_symbols::DependencyContractTemplateData::new(
        inferred
            .requirements()
            .iter()
            .chain(checked.requirements())
            .cloned(),
    )
    .with_borrows(checked.borrows().iter().cloned())
    .with_effects(previous_effects.iter().chain(checked.effects()).cloned())
}
