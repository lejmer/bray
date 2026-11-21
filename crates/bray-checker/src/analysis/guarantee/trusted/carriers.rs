use crate::execution_guarantees::ExecutionObservationKind as ObservationKind;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::super::state::ExecutionState;
use crate::{ExecutionCondition, ExecutionPlace};
use bray_bound_tree::StorageProjection;
use bray_symbols::{ConstantTermId, DependencyProjection, DependencyWitnessCarrier, DependencyWitnessSupport};
use crate::execution_guarantees::{ExecutionGuarantee, ExecutionSubstitution, ExecutionWitness};

pub(in crate::analysis) type ReturnedCarrier = DependencyWitnessCarrier<StorageProjection, ConstantTermId, ExecutionPlace>;

impl<C: crate::CheckerRequestContext + ?Sized> super::super::flow::ExecutionFlowDomain<'_, '_, C> {
    /// A loan's completion contract belongs to that loan, including observations of reached children.
    pub(super) fn borrowed_result_carrier(
        &self,
        result: &ExecutionCondition,
        subject: &ExecutionCondition,
    ) -> Option<ExecutionCondition> {
        let (root, path) = subject.observation_path()?;

        if root != ExecutionCondition::result() {
            return None;
        }

        let mut value = result.clone();

        for projection in path {
            if matches!((value.owner_identity()).kind(), crate::ExecutionConditionKind::Borrowed(_)) {
                return Some(value);
            }

            value = value.component(projection);
        }

        matches!((value.owner_identity()).kind(), crate::ExecutionConditionKind::Borrowed(_)).then_some(value)
    }

    /// Preserve the input loan and any independently retained owning value.
    pub(super) fn input_witness_owners(
        &self,
        state: &ExecutionState,
        template: &ExecutionCondition,
        subject: &ExecutionCondition,
        condition: &ExecutionCondition,
        arguments: &BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
        addresses: &[(ExecutionCondition, ExecutionCondition, bool)],
    ) -> Vec<ExecutionCondition> {
        if addresses.iter()
            .filter(|(guarantee, address, _)| guarantee == template && address == subject)
            .map(|(_, _, owning)| *owning)
            .reduce(|left, right| left || right) == Some(false)
        {
            // The access producer retains these bounds with their non-owning classification.
            return Vec::new();
        }

        // Only a current borrowed input can establish an input owner's contract.
        // A consumed by-value input remains an entry snapshot, not a live owner.
        let borrowed = subject.inputs().into_iter().any(|place| {
            let mut root = place.clone();

            root.projections = std::sync::Arc::new([]);

            let Some(mut value) = root.value_in(arguments) else { return false; };

            for projection in place.projections.iter() {
                if matches!((value.owner_identity()).kind(), crate::ExecutionConditionKind::Borrowed(_)) {
                    return true;
                }

                value = value.component(*projection);
            }

            matches!((value.owner_identity()).kind(), crate::ExecutionConditionKind::Borrowed(_))
        });

        if !borrowed || condition == &ExecutionCondition::unknown() { return Vec::new(); }

        // The typed subject was projected from this same current argument context.
        // A live backing context retains its guarantees when a temporary loan
        // ends. This does not add an owned-result transfer for a borrowed input.
        let referent = condition.clone().into_referent();
        let contexts = state.current.live_values().collect::<crate::execution_guarantees::RetainedSnapshots>();

        // The loan and owning value share immutable completion data, but retain
        // distinct authority. Moving an owner cannot depend on its temporary loan.
        let mut carriers = vec![condition.clone()];

        if referent != *condition
            && contexts.retains_owner(&referent) {
            carriers.push(referent);
        }

        carriers
    }

    /// Potential carriers for checking whether a partial move can escape a complete owner.
    pub(super) fn possible_completion_carriers(
        &self,
        entry: &crate::ExecutionCallEvidence,
        carriers: &[DependencyWitnessCarrier<StorageProjection>],
        contract: &crate::TrustedCallContract,
    ) -> Vec<(ExecutionCondition, ExecutionCondition)> {
        let inferred = carriers.iter().flat_map(|carrier| {
            let condition = entry.formal_condition(self.request.semantic_values(), carrier.guarantee());

            let subject = carrier.path().iter().copied().fold(ExecutionCondition::result(), ExecutionCondition::project);

            condition.conjuncts().map(|condition| (entry.capture_condition(condition), subject.clone())).collect::<Vec<_>>()
        }).collect::<Vec<_>>();

        let mut subjects = contract
            .witness_subjects
            .iter()
            .filter_map(|(guarantee, subject)| {
                let guarantee = entry.capture_condition(guarantee);

                if subject.observes_completion_result()
                    && inferred
                        .iter()
                        // One complete owner must independently support the clause.
                        .any(|(condition, _)| condition.implies(&guarantee))
                {
                    None
                } else {
                    Some((guarantee, subject.clone()))
                }
            })
            .collect::<Vec<_>>();

        subjects.extend(inferred);
        subjects.retain(|(condition, _)| condition.contains_trusted());

        subjects
    }
}

impl super::check::TrustedChecks {
    pub(super) fn exit_carriers<C: crate::CheckerRequestContext + ?Sized>(&self, request: crate::CheckerUnitView<'_, C>,
        state: &ExecutionState, result: &ExecutionCondition,
        return_substitution: &impl Fn(&ExecutionCondition) -> ExecutionCondition,
    ) -> BTreeSet<ReturnedCarrier> {
        let Some(inputs) = request.trusted_contracts() else { return BTreeSet::new(); };

        if state.witnesses.is_empty() { return BTreeSet::new(); }

        let owners = returned_owners(result);
        let mut carriers = BTreeSet::new();

        for ((declared, _), (checked, _)) in inputs.guarantees.iter().zip(&self.guarantees) {
            let Some(term) = portable(request, declared) else { continue; };

            for (path, support) in declared_carriers(request, state, result, &owners, checked, return_substitution) {
                carriers.insert(DependencyWitnessCarrier::new(term, path).with_support(support));
            }
        }

        if inputs.infer_result_witnesses {
            carriers.extend(inferred_carriers(request, state, &owners, return_substitution));
        }

        carriers
    }
}

pub(in crate::analysis) fn portable<C: crate::CheckerRequestContext + ?Sized>(
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

fn inferred_carriers<C: crate::CheckerRequestContext + ?Sized>(
    request: crate::CheckerUnitView<'_, C>,
    state: &ExecutionState,
    owners: &BTreeMap<ExecutionCondition, Vec<StorageProjection>>,
    return_substitution: &impl Fn(&ExecutionCondition) -> ExecutionCondition,
) -> BTreeSet<ReturnedCarrier> {
    let mut result = BTreeSet::new();
    let proof = state.proof();

    for (owner, witness) in &state.witnesses {
        let Some(path) = owners.get(owner) else {
            continue;
        };

        if !state.directly_carries_witness(owner) {
            continue;
        }

        for (condition, support) in witness
            .conditions
            .iter()
            .filter(|(condition, guarantee)| {
                guarantee.witness
                    && proof.prove_trusted(condition)
                        == Some(true)
            })
        {
            // Rename actual returned storage identities, never ordinary field equalities.
            let condition = return_substitution(condition);

            if let Some(term) = portable(request, &condition) {
                let support = portable_support(request, witness, [support], return_substitution);

                result.insert(DependencyWitnessCarrier::new(term, path.iter().copied()).with_support(support));
            }
        }
    }

    result
}

pub(in crate::analysis::guarantee) fn result_projection(
    values: &bray_symbols::SemanticValueStore,
    dependencies: &bray_bound_tree::CheckedDependencyContracts,
    path: &[DependencyProjection],
) -> ExecutionCondition {
    crate::storage::storage_projections(values, dependencies, path)
        .expect("checked result carrier retains its projection")
        .into_iter()
        .fold(ExecutionCondition::result(), |value, projection| {
            value.project(projection)
        })
}

fn declared_carriers<C: crate::CheckerRequestContext + ?Sized>(
    request: crate::CheckerUnitView<'_, C>, state: &ExecutionState, result: &ExecutionCondition,
    owners: &BTreeMap<ExecutionCondition, Vec<StorageProjection>>, guarantee: &ExecutionCondition,
    return_substitution: &impl Fn(&ExecutionCondition) -> ExecutionCondition,
) -> BTreeSet<(Vec<StorageProjection>, Option<DependencyWitnessSupport<ExecutionPlace>>)> {
        let proof = state.proof();

        let guarantee = guarantee.substitute(
            &|input| {
                input
                    .value_in(&state.current)
                    .unwrap_or_else(ExecutionCondition::unknown)
            },
            result,
            &mut { ExecutionCondition::WORK_LIMIT },
        );

        // A false success guard creates no authority and imposes no carrier requirement.
        if guarantee.prove_ordinary(&state.assumptions, Some(&state.proofs))
            == Some(true)
        {
            return BTreeSet::new();
        }

        let mut paths = BTreeSet::new();

        for (carrier, witness) in &state.witnesses {
            let Some(path) = owners.get(carrier) else {
                continue;
            };

            if !state.directly_carries_witness(carrier) {
                continue;
            }

            let mut evidence = BTreeSet::new();
            let mut independent_evidence = BTreeSet::new();
            let mut supports = Vec::new();

            for (condition, support) in witness
                .conditions
                .iter()
                .filter(|(condition, guarantee)| {
                    guarantee.witness
                        && proof.prove_trusted(condition)
                            == Some(true)
                })
            {
                // The temporary proof set owns its conditions independently of the exit state.
                condition.clone().assume(true, &mut evidence);
                supports.push(support);

                if support.addresses.is_empty() {
                    condition.clone().assume(true, &mut independent_evidence);
                }
            }

            // Each published owner must independently support the entire clause.
            // Combining unrelated owners here would let either one outlive the other.
            if guarantee.prove_mixed(&evidence, &state.assumptions, &mut {
                ExecutionCondition::WORK_LIMIT
            }) == Some(true)
            {
                let independent = witness.dependencies.is_empty()
                    && guarantee.prove_mixed(&independent_evidence, &state.assumptions, &mut {
                        ExecutionCondition::WORK_LIMIT
                    }) == Some(true);

                let support = if independent {
                    Some(DependencyWitnessSupport::new([], []))
                } else {
                    portable_support(request, witness, supports, return_substitution)
                };

                paths.insert((path.clone(), support));
            }
        }

        paths
}

fn portable_support<'a, C: crate::CheckerRequestContext + ?Sized>(
    request: crate::CheckerUnitView<'_, C>,
    witness: &ExecutionWitness,
    guarantees: impl IntoIterator<Item = &'a ExecutionGuarantee>,
    rebase: &impl Fn(&ExecutionCondition) -> ExecutionCondition,
) -> Option<DependencyWitnessSupport<ExecutionPlace>> {
    let addresses = guarantees.into_iter().flat_map(|guarantee| &guarantee.addresses).map(|(address, owners)| {
        // The portable clause owns the required places independently of the ledger.
        Some((portable(request, &rebase(address))?, owners.clone()))
    }).collect::<Option<Vec<_>>>()?;

    Some(DependencyWitnessSupport::new(witness.dependencies.iter().cloned(), addresses))
}

fn returned_owners(
    result: &ExecutionCondition,
) -> BTreeMap<ExecutionCondition, Vec<StorageProjection>> {
    let mut owners = BTreeMap::new();

    visit_returned_components(result, true, |value, path, declared_view| {
        // Local component snapshots belong to the complete returned owner.
        owners.insert(value.clone(), path.to_vec());

        if !declared_view {
            owners.insert(value.owner_identity().clone(), path.to_vec());
        }
    });

    owners
}

fn visit_returned_components(
    result: &ExecutionCondition,
    include_referents: bool,
    mut visit: impl FnMut(&ExecutionCondition, &[StorageProjection], bool),
) {
    let mut pending = vec![(result, Vec::new())];

    while let Some((value, path)) = pending.pop() {
        if path.len() >= ExecutionCondition::WORK_LIMIT {
            continue;
        }

        if *value == ExecutionCondition::unknown() || !include_referents && value.is_reference() {
            continue;
        }

        visit(value, &path, false);

        match value.kind() {
            crate::ExecutionConditionKind::CapturedValue { value, .. }
            | crate::ExecutionConditionKind::DeferredValue { value, .. }
            | crate::ExecutionConditionKind::Tagged(_, value) => pending.push((value, path)),
            crate::ExecutionConditionKind::Present(value) => {
                let mut path = path;

                path.push(StorageProjection::NullableValue);
                pending.push((value, path));
            }
            crate::ExecutionConditionKind::Observation { contents, kind: ObservationKind::Owned | ObservationKind::Reference, .. } => {
                // Reached data can carry access promises without transferring its ownership.
                pending.push((contents, path));
            }
            crate::ExecutionConditionKind::Constructed(_, fields) => {
                for (projection, component) in fields.iter() {
                    if !matches!(
                        projection,
                        StorageProjection::ProductField(_)
                            | StorageProjection::TupleElement(_)
                            | StorageProjection::ActiveUnionPayloadField { .. }
                            | StorageProjection::NullableValue
                            | StorageProjection::ElementFromStart(_)
                            | StorageProjection::OwnedTarget
                            | StorageProjection::UninitPayload
                    ) {
                        continue;
                    }

                    let mut child = path.clone();

                    child.push(*projection);

                    if include_referents {
                        // A declared view can retain its containing owner's identity.
                        visit(&value.clone().project(*projection), &child, true);
                    }
                    pending.push((component, child));
                }
            }
            _ => {}
        }
    }
}

/// Convert complete storage paths only when publishing portable carrier contracts.
pub(in crate::analysis) fn portable_path<C: crate::CheckerRequestContext + ?Sized>(
    request: crate::CheckerUnitView<'_, C>,
    path: &[StorageProjection],
) -> Vec<DependencyProjection> {
    path.iter()
        .copied()
        .map(|projection| {
            crate::storage::value_dependency_projection(request, projection)
                .expect("returned owner retains a portable storage projection")
        })
        .collect()
}

impl ExecutionState {
    /// Binds completed owned values to their public result before the frame slot can be reused.
    pub(in crate::analysis::guarantee) fn publish_frame_owners(
        &mut self, result: ExecutionCondition, root: &ExecutionCondition,
    ) -> ExecutionCondition {
        use crate::ExecutionConditionKind as Kind;
        use crate::execution_guarantees::ExecutionInput;

        let mut owners = BTreeMap::new();

        visit_returned_components(&result, false, |value, path, _| {
            if matches!(value.kind(), Kind::Present(_)) { return; }

            let Some((ObservationKind::Owned, owner)) = value.ownership_identity() else { return; };
            let Some((origin, _)) = owner.observation_path() else { return; };
            if !matches!(origin.kind(), Kind::Input(place) if matches!(place.root, ExecutionInput::FrameInput { .. })) { return; }

            let output = path.iter().copied().fold(root.clone(), ExecutionCondition::project);

            if !output.is_unknown() && owner != &output {
                owners.entry(owner.clone()).or_insert(output);
            }
        });

        let substitution = ExecutionSubstitution::new(&owners, &owners);

        self.rename_values(&substitution);

        substitution.value(&result)
    }

    /// Discharges ordinary guards while each original carrier still supplies its own evidence.
    pub(in crate::analysis::guarantee) fn refine_witnesses(&mut self) {
        // Ordinary closure belongs to proof preparation. Publishing it as new
        // assumptions makes joins multiply clauses by their own consequences.
        for witness in self
            .witnesses
            .values_mut()
            .filter(|witness| witness.available)
        {
            witness.refine(&mut self.trusted_assumptions, &mut self.assumptions, &self.proofs);
        }

        self.refine_deferred_witnesses();
    }

    pub(in crate::analysis::guarantee) fn refine_deferred_witnesses(&mut self) {
        let ordinary = self.assumptions.clone();
        let proofs = Arc::clone(&self.proofs);
        let ambient = proofs.ordinary_proof(&ordinary);

        self.update_stored_values(ExecutionCondition::mapping_frames(|_| true, |_, frame| {
            let mut updated = Arc::clone(frame);

            for (source, candidate) in &frame.producers {
                let entry = &candidate.entry;

                if !entry.pending_execution
                    || !entry.witness_inputs.values().any(|witness| witness.has_guards())
                    || !super::super::deferred::possible(&candidate.selection, &ambient)
                {
                    continue;
                }

                let mut assumptions = entry.assumptions.joined_with(&ordinary).into_owned();

                assumptions.extend(candidate.selection.iter().cloned());

                let mut trusted = entry.trusted_assumptions.clone();
                let mut refined_entry = Arc::clone(entry);

                for (owner, witness) in entry.witness_inputs.iter().filter(|(_, witness)| witness.has_guards()) {
                    let mut refined = Arc::clone(witness);

                    refined.refine(&mut trusted, &mut assumptions, &proofs);

                    if !Arc::ptr_eq(witness, &refined) {
                        Arc::make_mut(&mut refined_entry).witness_inputs.insert(owner.clone(), refined);
                    }
                }

                if trusted != entry.trusted_assumptions {
                    Arc::make_mut(&mut refined_entry).trusted_assumptions = trusted;
                }

                if !Arc::ptr_eq(entry, &refined_entry) {
                    let frame = Arc::make_mut(&mut updated);
                    let candidate = frame.producers.get_mut(source).expect("proof refinement retains the selected producer");

                    candidate.entry = refined_entry;
                }
            }

            Some(updated)
        }));
    }
}
