use std::collections::{BTreeMap, BTreeSet};

use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_bound_tree::{BoundCallableTarget, BoundUnitKind};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    AnySymbolId, CallableResultDependenciesQuery, CallableSymbolId, DependencyContractTemplateId,
    SymbolQueryRequest,
};

use super::binding::{CompilationSymbolQueryEvaluator, binder_error};
use super::cache::CompilationSymbolSemantics;
use crate::compilation::binder::{BindingQueryResult, CompilationBindingContext};

use crate::fact::SymbolQueryCache;

/// The body and its dependency summary are settled and published by the same component.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(in crate::compilation) struct BodyAnalysis {
    pub(in crate::compilation) dependencies: DependencyContractTemplateId,
    pub(in crate::compilation) body:
        DiagnosticResult<std::sync::Arc<bray_bound_tree::CheckedBodySemantics>>,
}

type Component = BTreeMap<AnySymbolId, BodyAnalysis>;

pub(in crate::compilation) fn source_body_semantics(
    context: &CompilationBindingContext<'_>,
    key: &bray_bound_tree::BoundUnitKey,
) -> BindingQueryResult<DiagnosticResult<std::sync::Arc<bray_bound_tree::CheckedBodySemantics>>> {
    let owner = context
        .symbols()
        .symbol_for_key(key.declared_owner())
        .expect("declared body retains its symbol");

    let component =
        source_component(context, owner)?.expect("declared body belongs to a source component");

    // The consumer shares the component's immutable body and diagnostic allocations.
    Ok(component.result().value()[&owner].body.clone())
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

    if let Some(result) = source_summary(context, root.into_any())? {
        return Ok(result);
    }

    let empty = context
        .semantic_values()
        .empty_dependency_contract_template()
        .expect("empty result dependency template satisfies the semantic store contract");

    Ok(DiagnosticResult::new(empty, DiagnosticBag::new()))
}

fn source_component(
    context: &CompilationBindingContext<'_>,
    root: AnySymbolId,
) -> BindingQueryResult<Option<std::sync::Arc<crate::fact::PublishedUnitResult<Component>>>> {
    let compilation = context.compilation();

    let Some(component) = source_members(context, root)? else {
        return Ok(None);
    };

    let owner = component.iter().min().expect("call component is nonempty");

    let key = compilation
        .declared_unit_key(*owner, body_kind(*owner))
        .map_err(binder_error)?
        .expect("source component owner retains its body");

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

fn cached_members(
    compilation: &crate::Compilation,
    owner: AnySymbolId,
) -> Option<std::sync::Arc<[AnySymbolId]>> {
    compilation
        .state
        .result_dependency_membership
        .lock()
        .expect("source component membership index must remain available")
        .get(&owner)
        .map(std::sync::Arc::clone)
}

// This index records graph identity only. infer_component still reads every member
// prerequisite inside its semantic publication, preserving query dependencies.
fn source_members(
    context: &CompilationBindingContext<'_>,
    root: AnySymbolId,
) -> BindingQueryResult<Option<std::sync::Arc<[AnySymbolId]>>> {
    let compilation = context.compilation();

    if let Some(members) = cached_members(compilation, root) {
        return Ok(Some(members));
    }

    let mut units = BTreeSet::new();
    let mut pending = vec![root];
    let mut edges = BTreeMap::<AnySymbolId, BTreeSet<AnySymbolId>>::new();
    let mut visited = BTreeSet::new();

    while let Some(owner) = pending.pop() {
        context.cancellation.check().map_err(binder_error)?;

        // A published component already includes its entire outgoing closure. It cannot
        // return to an undiscovered node in this immutable snapshot.
        if !visited.insert(owner)
            || cached_members(compilation, owner).is_some()
            || context.imported_semantic_address(owner)?.is_some()
        {
            continue;
        }

        let Some(key) = compilation
            .declared_unit_key(owner, body_kind(owner))
            .map_err(binder_error)?
        else {
            continue;
        };

        #[cfg(test)]
        compilation
            .state
            .result_dependency_discoveries
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let behavior = compilation
            .selected_body_behavior_with_cancellation(key.clone(), context.cancellation)
            .map_err(binder_error)?;

        let prepared = compilation
            .prepared_body_analysis_with_cancellation(key, context.cancellation)
            .map_err(binder_error)?;

        for callee in body_callees(behavior.result().value(), prepared.result().value()) {
            pending.push(callee);
            edges.entry(owner).or_default().insert(callee);
        }

        units.insert(owner);
    }

    let components = bray_base::strongly_connected_components(units.iter().copied(), |callable| {
        edges
            .get(&callable)
            .into_iter()
            .flatten()
            .filter(|callee| units.contains(callee))
            .copied()
    });

    // Binding and query waits hold no index lock, so independent discovery stays parallel.
    let mut membership = compilation
        .state
        .result_dependency_membership
        .lock()
        .expect("source component membership index must remain available");

    for component in components {
        if component
            .iter()
            .any(|member| membership.contains_key(member))
        {
            // Concurrent publication can prune a suffix during this walk. Its complete
            // component already owns every member of the resulting partial component.
            assert!(
                component
                    .iter()
                    .all(|member| membership.contains_key(member)),
                "source component publication must cover its complete outgoing closure"
            );

            continue;
        }

        let component = std::sync::Arc::<[AnySymbolId]>::from(component);

        for member in component.iter() {
            membership.insert(*member, std::sync::Arc::clone(&component));
        }
    }

    Ok(membership.get(&root).map(std::sync::Arc::clone))
}

fn infer_component(
    compilation: &crate::Compilation,
    component: &[AnySymbolId],
    cancellation: &crate::fact::CancellationToken,
) -> Result<DiagnosticResult<Component>, crate::fact::FactQueryError> {
    use crate::compilation::binder::binding_query_error;

    let context = compilation.binding_context(cancellation)?;

    let empty = context
        .semantic_values()
        .intern_dependency_contract_template(
            bray_symbols::DependencyContractTemplateData::new([])
                .with_effects([])
                .with_normal_completion(false),
        )
        .expect("empty result dependency template satisfies the semantic store contract");

    let members = component.iter().copied().collect::<BTreeSet<_>>();
    let mut units = BTreeMap::new();
    let mut callees = BTreeSet::new();
    let mut cyclic = component.len() > 1;
    let mut diagnostics = DiagnosticBag::new();

    for callable in component {
        let key = compilation
            .declared_unit_key(*callable, body_kind(*callable))?
            .expect("source summary component member retains its body");

        // Read each prerequisite inside the publication owner so incremental reuse
        // follows the member's own selected body and semantic inputs.
        let inputs = compilation.body_analysis_inputs(key.clone(), cancellation)?;

        for callee in body_callees(
            inputs.behavior.result().value(),
            inputs.prepared.result().value(),
        ) {
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

        units.insert(*callable, inputs);
    }

    let mut templates = members
        .iter()
        .map(|callable| (*callable, empty))
        .collect::<BTreeMap<_, _>>();

    for callee in callees {
        let result = summary(&context, callee).map_err(binding_query_error)?;

        templates.insert(callee, *result.value());
        diagnostics = diagnostics.merged(result.diagnostics());
    }

    let recursive = if cyclic {
        members
            .iter()
            .copied()
            .filter_map(CallableSymbolId::try_from_any)
            .collect()
    } else {
        BTreeSet::new()
    };

    let mut bodies = BTreeMap::new();

    loop {
        cancellation.check()?;

        let mut changed = false;

        for (callable, inputs) in &units {
            let body = inputs.check(compilation, cancellation, Some(&templates), &recursive)?;
            let values = context.semantic_values();
            let previous = values.dependency_contract_template_data(templates[callable]);

            let dependencies = values
                .intern_dependency_contract_template(body_result_dependencies(
                    body.value().storage_flow().dependencies(),
                    &previous,
                ))
                .expect("checked component summary has established semantic identities");

            bodies.insert(*callable, BodyAnalysis { dependencies, body });

            changed |= templates.insert(*callable, dependencies) != Some(dependencies);
        }

        if !cyclic || !changed {
            break;
        }
    }

    Ok(DiagnosticResult::new(bodies, diagnostics))
}

pub(in crate::compilation) fn expression_result_dependencies(
    context: &CompilationBindingContext<'_>,
    key: &bray_bound_tree::BoundUnitKey,
) -> BindingQueryResult<DiagnosticResult<DependencyContractTemplateId>> {
    if key.kind() == BoundUnitKind::RuntimeDefault {
        let owner = context
            .symbols()
            .symbol_for_key(key.declared_owner())
            .expect("runtime default retains its provider");

        return source_summary(context, owner)
            .map(|result| result.expect("source default belongs to a summary component"));
    }

    let body = context
        .compilation()
        .body_semantics_with_cancellation(key.clone(), context.cancellation)
        .map_err(binder_error)?;

    let diagnostics = body.result().diagnostics().clone();

    let values = context.semantic_values();

    let result = values
        .intern_dependency_contract_template(body_result_dependencies(
            body.result().value().storage_flow().dependencies(),
            &bray_symbols::DependencyContractTemplateData::new([])
                .with_effects([])
                .with_normal_completion(false),
        ))
        .expect("checked default summary retains its semantic identities");

    Ok(DiagnosticResult::new(result, diagnostics))
}

fn body_result_dependencies(
    checked: &bray_symbols::DependencyContractTemplateData,
    previous: &bray_symbols::DependencyContractTemplateData,
) -> bray_symbols::DependencyContractTemplateData {
    // Normal outcomes and mutations grow. Once a normal outcome exists, exact
    // relations can only survive intersection with every subsequently discovered outcome.
    let borrows = if !previous.normal_completion() {
        checked.borrows()
    } else {
        previous.borrows()
    };

    bray_symbols::DependencyContractTemplateData::new(
        previous
            .requirements()
            .iter()
            .chain(checked.requirements())
            .cloned(),
    )
    .with_borrows(
        borrows
            .iter()
            .filter(|relation| {
                !previous.normal_completion()
                    || !checked.normal_completion()
                    || checked.borrows().binary_search(relation).is_ok()
            })
            .cloned(),
    )
    .with_carriers(
        (if !previous.normal_completion() { checked.carriers() } else { previous.carriers() })
            .iter().filter(|carrier| !previous.normal_completion() || !checked.normal_completion() || checked.carriers().binary_search(carrier).is_ok()).cloned()
    )
    .with_effects(previous.effects().iter().chain(checked.effects()).cloned())
    .with_normal_completion(previous.normal_completion() || checked.normal_completion())
}

fn body_kind(owner: AnySymbolId) -> BoundUnitKind {
    match owner {
        AnySymbolId::CallableParameterDefaultProvider(_)
        | AnySymbolId::StructFieldDefaultProvider(_)
        | AnySymbolId::UnionPayloadDefaultProvider(_) => BoundUnitKind::RuntimeDefault,
        _ => BoundUnitKind::CallableBody,
    }
}

fn body_callees<'a>(
    behavior: &'a bray_bound_tree::BodyBehaviorContributions,
    prepared: &'a bray_checker::PreparedBodyAnalysis,
) -> impl Iterator<Item = AnySymbolId> + 'a {
    behavior
        .calls()
        .iter()
        .filter_map(|call| match call.target() {
            BoundCallableTarget::Declaration(instance) => {
                Some(instance.definition().callable_symbol().into_any())
            }
            _ => None,
        })
        .chain(behavior.defaults().iter().map(|provider| provider.symbol()))
        .chain(prepared.lifecycle_callees().map(CallableSymbolId::into_any))
}

fn source_summary(
    context: &CompilationBindingContext<'_>,
    owner: AnySymbolId,
) -> BindingQueryResult<Option<DiagnosticResult<DependencyContractTemplateId>>> {
    // Diagnostic bags share their persistent collection with the component publication.
    Ok(source_component(context, owner)?.map(|component| {
        DiagnosticResult::new(
            component.result().value()[&owner].dependencies,
            component.result().diagnostics().clone(),
        )
    }))
}

pub(in crate::compilation) fn default_result_dependencies(
    context: &CompilationBindingContext<'_>,
    provider: bray_bound_tree::DefaultValueProvider,
) -> BindingQueryResult<DependencyContractTemplateId> {
    summary(context, provider.symbol()).map(|result| *result.value())
}

fn summary(
    context: &CompilationBindingContext<'_>,
    owner: AnySymbolId,
) -> BindingQueryResult<DiagnosticResult<DependencyContractTemplateId>> {
    use bray_symbols::{
        CallableParameterDefaultQuery, CallableParameterDefaultValue, StructFieldDefaultQuery,
        StructFieldDefaultValue, UnionPayloadDefaultValue, UnionPayloadFieldDefaultQuery,
    };

    // Returned diagnostics share the checked query's persistent bag in every branch.
    if let Some(callable) = CallableSymbolId::try_from_any(owner) {
        let result = context.resolve_symbol_query(SymbolQueryRequest::<
            CallableResultDependenciesQuery,
        >::new(callable))?;

        return Ok(DiagnosticResult::new(
            *result.value(),
            result.diagnostics().clone(),
        ));
    }

    if let Some(result) = source_summary(context, owner)? {
        return Ok(result);
    }

    let subject = context
        .runtime_default_subject(owner)?
        .expect("selected default retains its declaration subject");

    match subject {
        AnySymbolId::CallableParameter(parameter) => {
            let result = context.resolve_symbol_query(SymbolQueryRequest::<
                CallableParameterDefaultQuery,
            >::new(parameter))?;

            let CallableParameterDefaultValue::Valid(surface) = result.value().value() else {
                panic!("selected imported parameter default retains its checked surface");
            };

            Ok(DiagnosticResult::new(
                surface.behavior().result_dependencies(),
                result.diagnostics().clone(),
            ))
        }
        AnySymbolId::StructField(field) => {
            let result = context
                .resolve_symbol_query(SymbolQueryRequest::<StructFieldDefaultQuery>::new(field))?;

            let StructFieldDefaultValue::Valid(surface) = result.value().value() else {
                panic!("selected imported field default retains its checked surface");
            };

            Ok(DiagnosticResult::new(
                surface.behavior().result_dependencies(),
                result.diagnostics().clone(),
            ))
        }
        AnySymbolId::UnionPayloadField(field) => {
            let result = context.resolve_symbol_query(SymbolQueryRequest::<
                UnionPayloadFieldDefaultQuery,
            >::new(field))?;

            let UnionPayloadDefaultValue::Valid(surface) = result.value().value() else {
                panic!("selected imported payload default retains its checked surface");
            };

            Ok(DiagnosticResult::new(
                surface.behavior().result_dependencies(),
                result.diagnostics().clone(),
            ))
        }
        _ => panic!("runtime default {owner:?} has a non-default subject {subject:?}"),
    }
}
