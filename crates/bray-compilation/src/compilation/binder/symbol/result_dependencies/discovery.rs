use std::collections::{BTreeMap, BTreeSet};

use bray_binder::BindingQueryContext;
use bray_bound_tree::{BoundCallableTarget, BoundUnitKey, BoundUnitKind};
use bray_checker::ResultDependencyOwner;
use bray_symbols::AnySymbolId;

use super::super::binding::binder_error;
use crate::compilation::binder::{BindingQueryResult, CompilationBindingContext};

use super::component::{Component, infer_component};

pub(super) fn source_component(
    context: &CompilationBindingContext<'_>,
    root: &ResultDependencyOwner,
) -> BindingQueryResult<Option<std::sync::Arc<crate::fact::PublishedUnitResult<Component>>>> {
    let compilation = context.compilation();

    let Some(component) = source_members(context, root)? else {
        return Ok(None);
    };

    let owner = component.iter().min().expect("call component is nonempty");

    let key = body_key(compilation, owner)
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
    owner: &ResultDependencyOwner,
) -> Option<std::sync::Arc<[ResultDependencyOwner]>> {
    compilation
        .state
        .result_dependency_membership
        .lock()
        .expect("source component membership index must remain available")
        .get(owner)
        .map(std::sync::Arc::clone)
}

// This index records graph identity only. infer_component still reads every member
// prerequisite inside its semantic publication, preserving query dependencies.
fn source_members(
    context: &CompilationBindingContext<'_>,
    root: &ResultDependencyOwner,
) -> BindingQueryResult<Option<std::sync::Arc<[ResultDependencyOwner]>>> {
    let compilation = context.compilation();

    if let Some(members) = cached_members(compilation, root) {
        return Ok(Some(members));
    }

    let mut units = BTreeSet::new();
    let mut frontier = vec![root.clone()];
    let mut edges = BTreeMap::<ResultDependencyOwner, BTreeSet<ResultDependencyOwner>>::new();
    let mut visited = BTreeSet::new();

    // Each callee's inputs depend only on signatures, so one frontier level is
    // discovered in parallel instead of checking bodies one after another.
    while !frontier.is_empty() {
        context.cancellation.check().map_err(binder_error)?;

        let mut owners = Vec::new();

        for owner in std::mem::take(&mut frontier) {
            // A published component already includes its entire outgoing closure. It cannot
            // return to an undiscovered node in this immutable snapshot.
            if !visited.insert(owner.clone())
                || cached_members(compilation, &owner).is_some()
                || matches!(&owner, ResultDependencyOwner::Declaration(symbol)
                    if context.imported_semantic_address(*symbol)?.is_some())
            {
                continue;
            }

            owners.push(owner);
        }

        let discovered = compilation
            .state
            .fact_runtime
            .map_indexed(owners.len(), |index| discovered_callees(context, &owners[index]))
            .map_err(binder_error)?;

        for (owner, callees) in owners.into_iter().zip(discovered) {
            let Some(callees) = callees? else {
                continue;
            };

            for callee in callees {
                frontier.push(callee.clone());
                edges.entry(owner.clone()).or_default().insert(callee);
            }

            units.insert(owner);
        }
    }

    let components = bray_base::strongly_connected_components(units.iter(), |callable| {
        edges
            .get(callable)
            .into_iter()
            .flatten()
            .filter(|callee| units.contains(*callee))
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
            .any(|member| membership.contains_key(*member))
        {
            // Concurrent publication can prune a suffix during this walk. Its complete
            // component already owns every member of the resulting partial component.
            assert!(
                component
                    .iter()
                    .all(|member| membership.contains_key(*member)),
                "source component publication must cover its complete outgoing closure"
            );

            continue;
        }

        let component = std::sync::Arc::<[ResultDependencyOwner]>::from(
            component.into_iter().cloned().collect::<Vec<_>>(),
        );

        for member in component.iter() {
            membership.insert(member.clone(), std::sync::Arc::clone(&component));
        }
    }

    Ok(membership.get(root).map(std::sync::Arc::clone))
}

/// The callees of one source body, or None when it has no body to analyze.
fn discovered_callees(
    context: &CompilationBindingContext<'_>,
    owner: &ResultDependencyOwner,
) -> BindingQueryResult<Option<Vec<ResultDependencyOwner>>> {
    let compilation = context.compilation();

    let Some(key) = body_key(compilation, owner).map_err(binder_error)? else {
        return Ok(None);
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
        .prepared_body_analysis_with_cancellation(key.clone(), context.cancellation)
        .map_err(binder_error)?;

    let expressions = compilation
        .expression_semantics_with_cancellation(key.clone(), context.cancellation)
        .map_err(binder_error)?;

    let bound = compilation
        .bound_unit_with_cancellation(key, context.cancellation)
        .map_err(binder_error)?;

    Ok(Some(
        body_callees(
            context.semantic_values(),
            bound.result().value(),
            behavior.result().value(),
            prepared.result().value(),
            expressions.result().value(),
        )
        .collect(),
    ))
}

pub(super) fn body_key(
    compilation: &crate::Compilation,
    owner: &ResultDependencyOwner,
) -> Result<Option<BoundUnitKey>, crate::fact::FactQueryError> {
    match owner {
        ResultDependencyOwner::Anonymous(key) => Ok(Some(key.clone())),
        ResultDependencyOwner::Declaration(symbol) => {
            let kind = match symbol {
                AnySymbolId::CallableParameterDefaultProvider(_)
                | AnySymbolId::StructFieldDefaultProvider(_)
                | AnySymbolId::UnionPayloadDefaultProvider(_) => BoundUnitKind::RuntimeDefault,
                AnySymbolId::Static(_) => BoundUnitKind::ConstantTemplate,
                _ => BoundUnitKind::CallableBody,
            };

            compilation.declared_unit_key(*symbol, kind)
        }
    }
}

pub(super) fn body_callees<'a>(
    values: &'a bray_symbols::SemanticValueStore,
    bound: &'a bray_bound_tree::BoundUnit,
    behavior: &'a bray_bound_tree::BodyBehaviorContributions,
    prepared: &'a bray_checker::PreparedBodyAnalysis,
    expressions: &'a bray_bound_tree::CheckedExpressionSemantics,
) -> impl Iterator<Item = ResultDependencyOwner> + 'a {
    behavior
        .calls()
        .iter()
        .filter_map(|call| match call.target() {
            BoundCallableTarget::Declaration(instance) => Some(ResultDependencyOwner::Declaration(
                instance.definition().callable_symbol().into_any(),
            )),
            BoundCallableTarget::Anonymous(_)
            | BoundCallableTarget::Predicate(_)
            | BoundCallableTarget::Indirect(_)
            | BoundCallableTarget::TraitView { .. } => None,
        })
        // Inactive async construction also needs the body's deferred result summary.
        .chain(
            expressions
                .selections()
                .entries()
                .iter()
                .filter_map(|entry| {
                    let bray_bound_tree::SemanticSelection::Call(call) = entry.selection() else {
                        return None;
                    };

                    call.resolution()
                        .declaration_source()
                        .map(|instance| {
                            ResultDependencyOwner::Declaration(
                                instance.definition().callable_symbol().into_any(),
                            )
                        })
                        .or_else(|| {
                            call.resolution()
                                .anonymous_body()
                                .map(|(key, _)| ResultDependencyOwner::Anonymous(key.clone()))
                        })
                }),
        )
        .chain(
            expressions
                .selections()
                .entries()
                .iter()
                .filter_map(|entry| {
                    let bray_bound_tree::SemanticSelection::CallableReference(instance) =
                        entry.selection()
                    else {
                        return None;
                    };

                    (!values
                        .generic_substitution_data(instance.substitution())
                        .bindings()
                        .is_empty())
                    .then_some(ResultDependencyOwner::Declaration(
                        instance.definition().callable_symbol().into_any(),
                    ))
                }),
        )
        .chain(bound.tree().expressions().filter_map(|(_, node)| {
            let bray_bound_tree::BoundExpression::AnonymousCallable(body) = node else {
                return None;
            };

            Some(ResultDependencyOwner::Anonymous(body.unit().clone()))
        }))
        .chain(
            behavior
                .defaults()
                .iter()
                .map(|provider| ResultDependencyOwner::Declaration(provider.symbol())),
        )
        .chain(
            behavior
                .static_dependencies()
                .iter()
                .copied()
                .map(|symbol| ResultDependencyOwner::Declaration(AnySymbolId::Static(symbol))),
        )
        .chain(
            prepared
                .lifecycle_callees()
                .map(|callable| ResultDependencyOwner::Declaration(callable.into_any())),
        )
}
