use std::collections::{BTreeMap, BTreeSet};

use super::super::state::ExecutionState;
use crate::ExecutionCondition;
use bray_symbols::{DependencyProjection, DependencyWitnessCarrier};

impl super::check::TrustedChecks {
    pub(super) fn collect_carriers<C: crate::CheckerRequestContext + ?Sized>(
        &mut self,
        request: crate::CheckerUnitView<'_, C>,
        dependencies: &bray_bound_tree::CheckedDependencyContracts,
        state: &ExecutionState,
        result: &ExecutionCondition,
    ) {
        let owners = returned_owners(result);

        collect(state, result, &owners, &self.guarantees, &mut self.carriers);

        if request
            .trusted_contracts()
            .is_some_and(|inputs| inputs.infer_result_witnesses)
        {
            collect_inferred(request, dependencies, state, &owners, &mut self.inferred);
        }
    }

    pub(in crate::analysis) fn result_carriers<C: crate::CheckerRequestContext + ?Sized>(
        &self,
        request: crate::CheckerUnitView<'_, C>,
    ) -> Vec<DependencyWitnessCarrier> {
        let mut result = self
            .inferred
            .iter()
            .flatten()
            .cloned()
            .collect::<BTreeSet<_>>();

        if let Some(inputs) = request.trusted_contracts() {
            for ((condition, _), paths) in inputs.guarantees.iter().zip(&self.carriers) {
                if paths.as_ref().is_some_and(|paths| !paths.is_empty())
                    && let Some(term) = portable(request, condition)
                {
                    result.extend(
                        paths
                            .iter()
                            .flatten()
                            .map(|path| DependencyWitnessCarrier::new(term, path.iter().copied())),
                    );
                }
            }
        }

        result.into_iter().collect()
    }
}

fn portable<C: crate::CheckerRequestContext + ?Sized>(
    request: crate::CheckerUnitView<'_, C>,
    condition: &ExecutionCondition,
) -> Option<bray_symbols::ConstantTermId> {
    let inputs = request.trusted_contracts()?;

    crate::execution_condition_term(
        request.semantic_values(),
        condition,
        &inputs.inputs,
        crate::representation::representation_type(
            request,
            bray_compiler_known::RepresentationRole::ScalarBool,
        ),
    )
    .expect("checked carrier conditions intern in the same semantic store")
}

fn collect_inferred<C: crate::CheckerRequestContext + ?Sized>(
    request: crate::CheckerUnitView<'_, C>,
    dependencies: &bray_bound_tree::CheckedDependencyContracts,
    state: &ExecutionState,
    owners: &BTreeMap<ExecutionCondition, Vec<DependencyProjection>>,
    previous: &mut Option<BTreeSet<DependencyWitnessCarrier>>,
) {
    let identities = owners
        .iter()
        .map(|(owner, path)| (owner.clone(), result_projection(dependencies, path)))
        .collect::<BTreeMap<_, _>>();

    let mut result = BTreeSet::new();

    for (owner, conditions) in &state.witness_carriers {
        let Some(path) = owners.get(owner) else {
            continue;
        };

        if !state.directly_carries_witness(owner) {
            continue;
        }

        for condition in conditions {
            // Rename actual returned storage identities, never ordinary field equalities.
            let condition = condition.with_joined_values(&identities, &identities);

            if let Some(term) = portable(request, &condition) {
                result.insert(DependencyWitnessCarrier::new(term, path.iter().copied()));
            }
        }
    }

    match previous {
        Some(previous) => previous.retain(|carrier| result.contains(carrier)),
        None => *previous = Some(result),
    }
}

pub(in crate::analysis::guarantee) fn result_projection(
    dependencies: &bray_bound_tree::CheckedDependencyContracts,
    path: &[DependencyProjection],
) -> ExecutionCondition {
    crate::storage::storage_projections(dependencies, path)
        .expect("checked result carrier retains its projection")
        .into_iter()
        .fold(ExecutionCondition::Result, |value, projection| {
            value.project(projection)
        })
}

fn collect(
    state: &ExecutionState,
    result: &ExecutionCondition,
    owners: &BTreeMap<ExecutionCondition, Vec<DependencyProjection>>,
    guarantees: &[(ExecutionCondition, bray_source::SourceSpan)],
    previous: &mut [Option<BTreeSet<Vec<DependencyProjection>>>],
) {
    if guarantees.is_empty() {
        return;
    }

    for ((guarantee, _), previous) in guarantees.iter().zip(previous) {
        let guarantee = guarantee.substitute(
            &|input| {
                input
                    .value_in(&state.current)
                    .unwrap_or_else(|| ExecutionCondition::Input(input.clone()))
            },
            result,
            &mut { ExecutionCondition::WORK_LIMIT },
        );

        // A false success guard creates no authority and imposes no carrier requirement.
        if guarantee.prove(&state.assumptions, &mut { ExecutionCondition::WORK_LIMIT })
            == Some(true)
        {
            continue;
        }

        let mut paths = BTreeSet::new();

        for (carrier, conditions) in &state.witness_carriers {
            let Some(path) = owners.get(carrier) else {
                continue;
            };

            if !state.directly_carries_witness(carrier) {
                continue;
            }

            let mut evidence = BTreeSet::new();

            for condition in conditions {
                // The temporary proof set owns its conditions independently of the exit state.
                condition.clone().assume(true, &mut evidence);
            }

            // Each published owner must independently support the entire clause.
            // Combining unrelated owners here would let either one outlive the other.
            if guarantee.prove_mixed(&evidence, &state.assumptions, &mut {
                ExecutionCondition::WORK_LIMIT
            }) == Some(true)
            {
                paths.insert(path.clone());
            }
        }

        // Each nonvacuous return must preserve the same complete owners. A failed proof
        // retains the conservative declaration carrier at every call site.
        match previous {
            Some(previous) if *previous != paths => previous.clear(),
            Some(_) => {}
            None => *previous = Some(paths),
        }
    }
}

fn returned_owners(
    result: &ExecutionCondition,
) -> BTreeMap<ExecutionCondition, Vec<DependencyProjection>> {
    let mut owners = BTreeMap::new();
    let mut pending = vec![(result, Vec::new())];
    let mut budget = ExecutionCondition::WORK_LIMIT;

    while let Some((value, path)) = pending.pop() {
        let Some(remaining) = budget.checked_sub(1) else {
            return BTreeMap::new();
        };

        budget = remaining;

        if *value == ExecutionCondition::Unknown {
            continue;
        }

        owners.insert(value.owner_identity().clone(), path.clone());

        match value {
            ExecutionCondition::Present(value) => {
                let mut path = path;

                path.push(DependencyProjection::NullableValue);
                pending.push((value, path));
            }
            ExecutionCondition::Constructed(_, fields) => {
                for (projection, value) in fields.iter() {
                    let projection = match projection {
                        bray_bound_tree::StorageProjection::ProductField(field) => {
                            DependencyProjection::ProductField(*field)
                        }
                        bray_bound_tree::StorageProjection::TupleElement(index) => {
                            DependencyProjection::TupleElement(*index)
                        }
                        bray_bound_tree::StorageProjection::ActiveUnionPayloadField {
                            field,
                            ..
                        } => DependencyProjection::UnionPayloadField(*field),
                        bray_bound_tree::StorageProjection::NullableValue => {
                            DependencyProjection::NullableValue
                        }
                        _ => continue,
                    };

                    let mut child = path.clone();

                    child.push(projection);
                    pending.push((value, child));
                }
            }
            _ => {}
        }
    }

    owners
}

impl ExecutionState {
    /// Discharges ordinary guards while each original carrier still supplies its own evidence.
    pub(in crate::analysis::guarantee) fn refine_witnesses(&mut self) {
        for conditions in self.witness_carriers.values_mut() {
            if !conditions.iter().any(|condition| {
                matches!(
                    condition,
                    ExecutionCondition::Operation(bray_bound_tree::BoundOperator::LogicalOr, _)
                )
            }) {
                continue;
            }

            let mut evidence = BTreeSet::new();

            for condition in conditions.iter() {
                // This proof set is independent of the immutable carried clauses.
                condition.clone().assume(true, &mut evidence);
            }

            let known =
                crate::execution_guarantees::trusted_condition_closure(evidence, &self.assumptions);

            let derived = conditions
                .iter()
                .flat_map(ExecutionCondition::observations)
                .filter(|condition| matches!(condition, ExecutionCondition::Trusted(_)))
                .filter(|condition| known.contains(&((*condition).clone(), true)))
                .cloned()
                .collect::<Vec<_>>();

            for condition in derived {
                // The carrier and current proof set independently retain this established fact.
                condition
                    .clone()
                    .assume(true, &mut self.trusted_assumptions);

                conditions.insert(condition);
            }
        }
    }
}

/// An exact entry snapshot can transport an unknown witness only as a complete owned value.
pub(super) fn returned_input<C: crate::CheckerRequestContext + ?Sized>(
    domain: &super::super::flow::ExecutionFlowDomain<'_, '_, C>,
    result: &ExecutionCondition,
) -> Option<bray_symbols::ConstantTermId> {
    let inputs = domain.request.trusted_contracts()?;

    if !inputs.infer_result_witnesses {
        return None;
    }

    let ty = domain.semantics.types().callable_result_type()?;

    if domain.storage.copyable_types().contains(&ty) {
        return None;
    }

    let mut input = result;

    while let ExecutionCondition::Projection(_, value) = input {
        input = value;
    }

    let ExecutionCondition::Input(place) = input else {
        return None;
    };

    if !place.is_callable_input()
        || domain
            .borrowed_inputs
            .iter()
            .any(|borrow| borrow.contains(place))
    {
        return None;
    }

    portable(domain.request, result)
}
