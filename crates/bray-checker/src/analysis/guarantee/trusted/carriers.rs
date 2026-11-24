use crate::execution_guarantees::ExecutionObservationKind as ObservationKind;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::super::state::ExecutionState;
use crate::execution_guarantees::{ExecutionGuarantee, ExecutionSubstitution, ExecutionWitness};
use crate::{ExecutionCondition, ExecutionPlace};
use bray_bound_tree::StorageProjection;
use bray_symbols::{
    ConstantTermId, DependencyProjection, DependencyWitnessCarrier, DependencyWitnessSupport,
};

pub(in crate::analysis) type ReturnedCarrier =
    DependencyWitnessCarrier<StorageProjection, ConstantTermId, ExecutionPlace>;

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
            if value.is_reference() {
                return Some(value);
            }

            value = value.component(projection);
        }

        value.is_reference().then_some(value)
    }

    /// Preserve the input loan and any independently retained owning value.
    pub(super) fn input_witness_owners(
        &self,
        state: &ExecutionState,
        entry: &crate::ExecutionCallEvidence,
        template: &ExecutionCondition,
        subject: &ExecutionCondition,
        condition: &ExecutionCondition,
        arguments: &BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
        addresses: &[crate::TrustedAddressSubject],
        direct_owners: Option<&BTreeMap<ExecutionPlace, ExecutionCondition>>,
    ) -> Vec<ExecutionCondition> {
        if addresses
            .iter()
            .filter(|address| &address.condition == template && &address.subject == subject)
            .map(|address| address.owning)
            .reduce(|left, right| left || right)
            == Some(false)
        {
            // The access producer retains these bounds with their non-owning classification.
            return Vec::new();
        }

        let direct_owner = subject.observation_path().and_then(|(root, path)| {
            let crate::ExecutionConditionKind::Input(root) = root.kind() else {
                return None;
            };

            let mut place = root.clone();

            place.projections = path.into();

            direct_owners?.get(&place)
        });

        // Only a current borrowed input can establish an input owner's contract.
        // A consumed by-value input remains an entry snapshot, not a live owner.
        let mut borrowed_inputs = BTreeSet::new();

        let mut borrowed = false;

        for place in subject.inputs() {
            let mut root = place.clone();

            root.projections = std::sync::Arc::new([]);

            let Some(mut value) = root.value_in(arguments) else {
                continue;
            };

            for projection in place.projections.iter() {
                if value.is_reference() {
                    if let Some(ordinal) = entry.input_ordinal(&root) {
                        borrowed_inputs.insert(ordinal);
                    }

                    borrowed = true;
                    break;
                }

                value = value.component(*projection);
            }

            if value.is_reference()
                && let Some(ordinal) = entry.input_ordinal(&root)
            {
                borrowed_inputs.insert(ordinal);
                borrowed = true;
            }
        }

        if !borrowed || condition == &ExecutionCondition::unknown() {
            return direct_owner.cloned().into_iter().collect();
        }

        // The loan and owning value share immutable completion data, but retain
        // distinct authority. Moving an owner cannot depend on its temporary loan.
        let mut carriers = vec![condition.clone()];

        let reached = state.address_dependencies(condition);

        for owner in reached.values().flatten() {
            if let Some(carrier) = owner.value_in(&state.current)
                && !carriers.contains(&carrier)
            {
                carriers.push(carrier);
            }
        }

        for owner in borrowed_inputs.iter().flat_map(|ordinal| {
            entry
                .borrow_owners
                .get(ordinal)
                .into_iter()
                .flat_map(|owners| owners.iter())
        }) {
            if let Some(carrier) = owner.value_in(&state.current)
                && !carriers.contains(&carrier)
            {
                carriers.push(carrier);
            }
        }

        if let Some(owner) = direct_owner {
            if !carriers.contains(owner) {
                carriers.push(owner.clone());
            }
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
        let inferred = carriers
            .iter()
            .flat_map(|carrier| {
                let condition =
                    entry.formal_condition(self.request.semantic_values(), carrier.guarantee());

                let subject = carrier
                    .path()
                    .iter()
                    .copied()
                    .fold(ExecutionCondition::result(), ExecutionCondition::project);

                condition
                    .conjuncts()
                    .map(|condition| (entry.capture_condition(condition), subject.clone()))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

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
    pub(super) fn exit_carriers<C: crate::CheckerRequestContext + ?Sized>(
        &self,
        domain: &super::super::flow::ExecutionFlowDomain<'_, '_, C>,
        state: &ExecutionState,
        result: &ExecutionCondition,
        return_substitution: &impl Fn(&ExecutionCondition) -> ExecutionCondition,
    ) -> BTreeSet<ReturnedCarrier> {
        let request = domain.request;

        let Some(inputs) = request.trusted_contracts() else {
            return BTreeSet::new();
        };

        assert_eq!(
            inputs.proof_guarantees().len(),
            self.guarantees.len(),
            "completion retains every checked trusted guarantee"
        );

        let owners = returned_owners(result);
        let mut carriers = BTreeSet::new();

        for (index, (declared, _)) in inputs.declared_guarantees.iter().enumerate() {
            let Some(term) = portable(request, declared) else {
                continue;
            };

            let subjects = inputs
                .guarantee_witness_subjects
                .iter()
                .filter(|(guarantee, subject)| {
                    guarantee == declared && subject.observes_completion_result()
                })
                .map(|(_, subject)| subject);

            let declared_paths = declared_result_paths(domain, state, result, &owners, subjects);

            let completed = domain.completion_condition(state, declared, result);

            // A false success guard creates no authority and imposes no carrier requirement.
            if completed.prove_ordinary(&state.assumptions, Some(&state.proofs)) == Some(true) {
                continue;
            }

            let checked = self
                .guarantees
                .get(index)
                .map_or(declared, |(condition, _)| condition);

            let mut published = checked_declared_carriers(
                domain,
                state,
                result,
                &owners,
                checked,
                &declared_paths,
                return_substitution,
            );

            if inputs.declared_guarantees_are_authoritative {
                for path in declared_paths {
                    if !published.iter().any(|(carried, _)| *carried == path) {
                        // Admission binds declaration-only support from the actual call.
                        published.insert((path, None));
                    }
                }
            }

            for (path, support) in published {
                carriers.insert(DependencyWitnessCarrier::new(term, path).with_support(support));
            }
        }

        if inputs.infer_result_witnesses {
            carriers.extend(inferred_carriers(
                request,
                state,
                &owners,
                return_substitution,
            ));
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

        for (condition, support) in witness.conditions.iter().filter(|(condition, guarantee)| {
            guarantee.witness && proof.prove_trusted(condition) == Some(true)
        }) {
            // Rename actual returned storage identities, never ordinary field equalities.
            let path = if let Some((StorageProjection::NullableValue, parent)) = path.split_last() {
                condition
                    .observations()
                    .find_map(|observation| {
                        let whole = observation.current_referent().unwrap_or(observation);

                        (matches!(whole.kind(), crate::ExecutionConditionKind::Present(_))
                            && whole.owner_identity() == owner
                            && owners.get(whole).is_some_and(|observed| observed == parent))
                        .then_some(parent)
                    })
                    .unwrap_or(path)
            } else {
                path
            };

            let condition = return_substitution(condition);

            if let Some(term) = portable(request, &condition) {
                let support = portable_support(request, witness, [support], return_substitution);

                result.insert(
                    DependencyWitnessCarrier::new(term, path.iter().copied()).with_support(support),
                );
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

fn declared_result_paths<'a, C: crate::CheckerRequestContext + ?Sized>(
    domain: &super::super::flow::ExecutionFlowDomain<'_, '_, C>,
    state: &ExecutionState,
    result: &ExecutionCondition,
    owners: &BTreeMap<ExecutionCondition, Vec<StorageProjection>>,
    subjects: impl IntoIterator<Item = &'a ExecutionCondition>,
) -> BTreeSet<Vec<StorageProjection>> {
    subjects
        .into_iter()
        .filter_map(|subject| {
            let subject = domain.completion_condition(state, subject, result);

            owners
                .get(&subject)
                .or_else(|| owners.get(subject.owner_identity()))
                .cloned()
        })
        .collect()
}

fn checked_declared_carriers<C: crate::CheckerRequestContext + ?Sized>(
    domain: &super::super::flow::ExecutionFlowDomain<'_, '_, C>,
    state: &ExecutionState,
    result: &ExecutionCondition,
    owners: &BTreeMap<ExecutionCondition, Vec<StorageProjection>>,
    guarantee: &ExecutionCondition,
    declared_paths: &BTreeSet<Vec<StorageProjection>>,
    return_substitution: &impl Fn(&ExecutionCondition) -> ExecutionCondition,
) -> BTreeSet<(
    Vec<StorageProjection>,
    Option<DependencyWitnessSupport<ExecutionPlace>>,
)> {
    let proof = state.proof();
    let guarantee = domain.completion_condition(state, guarantee, result);

    let mut paths = BTreeSet::new();

    for (carrier, witness) in &state.witnesses {
        if !witness.available {
            continue;
        }

        let returned_path = owners.get(carrier);

        if declared_paths.is_empty()
            && (returned_path.is_none() || !state.directly_carries_witness(carrier))
        {
            continue;
        }

        let mut evidence = BTreeSet::new();
        let mut independent_evidence = BTreeSet::new();
        let mut supports = Vec::new();

        for (condition, support) in witness.conditions.iter().filter(|(condition, guarantee)| {
            (guarantee.witness || !declared_paths.is_empty())
                && proof.prove_trusted(condition) == Some(true)
        }) {
            // An authored result owner can adopt a checked address fact. Without
            // that result subject, only an existing owning witness can publish.
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
                portable_support(domain.request, witness, supports, return_substitution)
            };

            if declared_paths.is_empty() {
                paths.insert((
                    returned_path
                        .expect("a fallback declared carrier is returned")
                        .clone(),
                    support,
                ));
            } else {
                paths.extend(
                    declared_paths
                        .iter()
                        .cloned()
                        .map(|path| (path, support.clone())),
                );
            }
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
    let addresses = guarantees
        .into_iter()
        .flat_map(|guarantee| &guarantee.addresses)
        .map(|(address, owners)| {
            // The portable clause owns the required places independently of the ledger.
            Some((portable(request, &rebase(address))?, owners.clone()))
        })
        .collect::<Option<Vec<_>>>()?;

    Some(DependencyWitnessSupport::new(
        witness.dependencies.iter().cloned(),
        addresses,
    ))
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
            crate::ExecutionConditionKind::Observation {
                contents,
                kind: ObservationKind::Owned | ObservationKind::Reference,
                ..
            } => {
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
            crate::storage::dependency_projection(request, projection)
                .expect("returned owner retains a portable storage projection")
        })
        .collect()
}

impl ExecutionState {
    /// Binds completed owned values to their public result before the frame slot can be reused.
    pub(in crate::analysis::guarantee) fn publish_frame_owners(
        &mut self,
        result: ExecutionCondition,
        root: &ExecutionCondition,
    ) -> ExecutionCondition {
        use crate::ExecutionConditionKind as Kind;
        use crate::execution_guarantees::ExecutionInput;

        let mut owners = BTreeMap::new();

        visit_returned_components(&result, false, |value, path, _| {
            if matches!(value.kind(), Kind::Present(_)) {
                return;
            }

            let Some((ObservationKind::Owned, owner)) = value.ownership_identity() else {
                return;
            };

            let Some((origin, _)) = owner.observation_path() else {
                return;
            };

            if !matches!(origin.kind(), Kind::Input(place) if matches!(place.root, ExecutionInput::FrameInput { .. }))
            {
                return;
            }

            let output = path
                .iter()
                .copied()
                .fold(root.clone(), ExecutionCondition::project);

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
            witness.refine(
                &mut self.trusted_assumptions,
                &mut self.assumptions,
                &self.proofs,
            );
        }

        self.refine_deferred_witnesses();
    }

    pub(in crate::analysis::guarantee) fn refine_deferred_witnesses(&mut self) {
        let ordinary = self.assumptions.clone();
        let proofs = Arc::clone(&self.proofs);
        let ambient = proofs.ordinary_proof(&ordinary);

        self.update_stored_values(ExecutionCondition::mapping_frames(
            |_| true,
            |_, frame| {
                let mut updated = Arc::clone(frame);

                for (source, candidate) in &frame.producers {
                    let entry = &candidate.entry;

                    if !entry.pending_execution
                        || !entry
                            .witness_inputs
                            .values()
                            .any(|witness| witness.has_guards())
                        || !super::super::deferred::possible(&candidate.selection, &ambient)
                    {
                        continue;
                    }

                    let mut assumptions = entry.assumptions.joined_with(&ordinary).into_owned();

                    assumptions.extend(candidate.selection.iter().cloned());

                    let mut trusted = entry.trusted_assumptions.clone();
                    let mut refined_entry = Arc::clone(entry);

                    for (owner, witness) in entry
                        .witness_inputs
                        .iter()
                        .filter(|(_, witness)| witness.has_guards())
                    {
                        let mut refined = Arc::clone(witness);

                        refined.refine(&mut trusted, &mut assumptions, &proofs);

                        if !Arc::ptr_eq(witness, &refined) {
                            Arc::make_mut(&mut refined_entry)
                                .witness_inputs
                                .insert(owner.clone(), refined);
                        }
                    }

                    if trusted != entry.trusted_assumptions {
                        Arc::make_mut(&mut refined_entry).trusted_assumptions = trusted;
                    }

                    if !Arc::ptr_eq(entry, &refined_entry) {
                        let frame = Arc::make_mut(&mut updated);
                        let candidate = frame
                            .producers
                            .get_mut(source)
                            .expect("proof refinement retains the selected producer");

                        candidate.entry = refined_entry;
                    }
                }

                Some(updated)
            },
        ));
    }
}
