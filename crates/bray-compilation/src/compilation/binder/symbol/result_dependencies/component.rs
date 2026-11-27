use crate::compilation::binder::binding_query_error;
use std::collections::{BTreeMap, BTreeSet};

use bray_binder::BindingQueryContext;
use bray_checker::ResultDependencyOwner;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    CallableSymbolId, DependencyContractTemplateId,
};

use super::discovery::{body_callees, body_key};
use super::summary::summary;

/// The body and its dependency summary are settled and published by the same component.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(in crate::compilation) struct BodyAnalysis {
    pub(in crate::compilation) dependencies: DependencyContractTemplateId,
    pub(in crate::compilation) body:
        DiagnosticResult<std::sync::Arc<bray_bound_tree::CheckedBodySemantics>>,
}

pub(super) type Component = BTreeMap<ResultDependencyOwner, BodyAnalysis>;

pub(super) fn infer_component(
    compilation: &crate::Compilation,
    component: &[ResultDependencyOwner],
    cancellation: &crate::fact::CancellationToken,
) -> Result<DiagnosticResult<Component>, crate::fact::FactQueryError> {

    let context = compilation.binding_context(cancellation)?;

    let empty = context
        .semantic_values()
        .intern_dependency_contract_template(
            bray_symbols::DependencyContractTemplateData::new([])
                .with_possible_effects([])
                .with_normal_completion(false),
        )
        .expect("empty result dependency template satisfies the semantic store contract");

    let members = component.iter().cloned().collect::<BTreeSet<_>>();
    let mut units = BTreeMap::new();
    let mut callees = BTreeSet::new();
    let mut member_callees = BTreeMap::<_, Vec<_>>::new();
    let mut cyclic = component.len() > 1;
    let mut diagnostics = DiagnosticBag::new();

    for callable in component {
        let key = body_key(compilation, callable)?
            .expect("source summary component member retains its body");

        // Read each prerequisite inside the publication owner so incremental reuse
        // follows the member's own selected body and semantic inputs.
        let inputs = compilation.body_analysis_inputs(key.clone(), cancellation)?;

        for callee in body_callees(
            context.semantic_values(),
            inputs.bound.result().value(),
            inputs.behavior.result().value(),
            inputs.prepared.result().value(),
            inputs.expressions.result().value(),
        ) {
            cyclic |= callee == *callable;

            if members.contains(&callee) {
                member_callees.entry(callable.clone()).or_default().push(callee);
            } else {
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

        units.insert(callable.clone(), inputs);
    }

    let mut templates = members
        .iter()
        .map(|callable| (callable.clone(), empty))
        .collect::<BTreeMap<_, _>>();

    for callee in callees {
        let result = summary(&context, &callee).map_err(binding_query_error)?;

        templates.insert(callee, *result.value());
        diagnostics = diagnostics.merged(result.diagnostics());
    }

    let recursive = if cyclic {
        members
            .iter()
            .filter_map(|owner| match owner {
                ResultDependencyOwner::Declaration(symbol) => {
                    CallableSymbolId::try_from_any(*symbol)
                }
                ResultDependencyOwner::Anonymous(_) => None,
            })
            .collect()
    } else {
        BTreeSet::new()
    };

    // Callees are solved before their callers, and a member is checked again
    // only after the summary of a member it calls has changed.
    let order = callee_first_order(&members, &member_callees);

    let rank = order
        .iter()
        .enumerate()
        .map(|(rank, member)| (*member, rank))
        .collect::<BTreeMap<_, _>>();

    let mut callers = BTreeMap::<_, Vec<_>>::new();

    for (caller, callees) in &member_callees {
        for callee in callees {
            callers.entry(callee).or_default().push(rank[caller]);
        }
    }

    let mut pending = (0..order.len()).collect::<BTreeSet<_>>();
    let mut bodies = BTreeMap::new();

    while let Some(next) = pending.pop_first() {
        cancellation.check()?;

        let callable = order[next];
        let body = units[callable].check(compilation, cancellation, Some(&templates), &recursive)?;
        let values = context.semantic_values();
        let checked = body.value().storage_flow().dependencies();
        let previous = templates[callable];

        let template = if previous == empty {
            // The first solved body already owns the complete portable contract.
            checked.clone()
        } else {
            let previous = values.dependency_contract_template_data(previous);

            body_result_dependencies(checked, &previous)
        };

        let dependencies = values
            .intern_dependency_contract_template(template)
            .expect("checked component summary has established semantic identities");

        bodies.insert(callable.clone(), BodyAnalysis { dependencies, body });

        if templates.insert(callable.clone(), dependencies) != Some(dependencies) {
            pending.extend(callers.get(callable).into_iter().flatten());
        }
    }

    Ok(DiagnosticResult::new(bodies, diagnostics))
}

/// Orders component members so each one follows the members it calls,
/// except where a cycle forces a back edge.
fn callee_first_order<'a>(
    members: &'a BTreeSet<ResultDependencyOwner>,
    callees: &'a BTreeMap<ResultDependencyOwner, Vec<ResultDependencyOwner>>,
) -> Vec<&'a ResultDependencyOwner> {
    let mut order = Vec::with_capacity(members.len());
    let mut visited = BTreeSet::new();

    for root in members {
        if !visited.insert(root) {
            continue;
        }

        let mut stack = vec![(root, 0)];

        while let Some((member, next)) = stack.last_mut() {
            match callees.get(*member).and_then(|callees| callees.get(*next)) {
                Some(callee) => {
                    *next += 1;

                    if visited.insert(callee) {
                        stack.push((callee, 0));
                    }
                }
                None => {
                    order.push(*member);
                    stack.pop();
                }
            }
        }
    }

    order
}

fn body_result_dependencies(
    checked: &bray_symbols::DependencyContractTemplateData,
    previous: &bray_symbols::DependencyContractTemplateData,
) -> bray_symbols::DependencyContractTemplateData {
    // Normal outcomes and mutations grow. Value and authority promises hold
    // only where the same widening retains their supporting observations.
    let outcomes = combined_result_values(checked, previous);

    let failures = checked.failure_outcomes().map(|checked| {
        bray_symbols::CapturedFailureOutcome::join_reachable(previous.failure_outcomes().unwrap_or_default(), checked)
    });

    bray_symbols::DependencyContractTemplateData::new(
        previous
            .requirements()
            .iter()
            .chain(checked.requirements())
            .cloned(),
    )
    .with_captured_values(outcomes)
    .with_failure_outcomes(failures)
    .with_possible_effects(previous.effects().iter().chain(checked.effects()).cloned())
    .with_synchronous_cleanup(
        previous
            .synchronous_cleanup()
            .iter()
            .chain(checked.synchronous_cleanup())
            .cloned(),
    )
    .with_normal_completion(previous.normal_completion() || checked.normal_completion())
}

fn combined_result_values(
    checked: &bray_symbols::DependencyContractTemplateData,
    previous: &bray_symbols::DependencyContractTemplateData,
) -> Vec<bray_symbols::CapturedValueOutcome> {
    if !previous.normal_completion() { return checked.captured_values().to_vec(); }
    if !checked.normal_completion() { return previous.captured_values().to_vec(); }

    bray_symbols::CapturedValueOutcome::join_reachable(previous.captured_values(), checked.captured_values(), bray_symbols::DependencySubject::contains).to_vec()
}
