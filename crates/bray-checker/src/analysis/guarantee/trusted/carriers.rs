use crate::execution_guarantees::ExecutionObservationKind as ObservationKind;
use std::collections::{BTreeMap, BTreeSet};

use super::super::state::ExecutionState;
use crate::ExecutionCondition;
use bray_bound_tree::StorageProjection;
use bray_symbols::{DependencyProjection, DependencyWitnessCarrier};

impl<C: crate::CheckerRequestContext + ?Sized> super::super::flow::ExecutionFlowDomain<'_, '_, C> {
    /// A loan's completion contract belongs to that loan, including observations of reached children.
    pub(super) fn borrowed_result_carrier(
        &self,
        result: &ExecutionCondition,
        subject: &ExecutionCondition,
    ) -> Option<ExecutionCondition> {
        let (root, path) = subject.observation_path()?;

        if root != ExecutionCondition::Result {
            return None;
        }

        let mut value = result.clone();

        for projection in path {
            if matches!(value.owner_identity(), ExecutionCondition::Borrowed(_)) {
                return Some(value);
            }

            value = value.component(projection);
        }

        matches!(value.owner_identity(), ExecutionCondition::Borrowed(_)).then_some(value)
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
                if matches!(value.owner_identity(), ExecutionCondition::Borrowed(_)) {
                    return true;
                }

                value = value.component(*projection);
            }

            matches!(value.owner_identity(), ExecutionCondition::Borrowed(_))
        });

        if !borrowed || condition == &ExecutionCondition::Unknown { return Vec::new(); }

        // The typed subject was projected from this same current argument context.
        // A live backing context retains its guarantees when a temporary loan
        // ends. This does not add an owned-result transfer for a borrowed input.
        let referent = condition.clone().into_referent();
        let contexts = state.current.live_values().collect::<BTreeSet<_>>();

        // The loan and owning value share immutable completion data, but retain
        // distinct authority. Moving an owner cannot depend on its temporary loan.
        let mut carriers = vec![condition.clone()];

        if referent != *condition
            && crate::execution_guarantees::ObservationSet::retains_owner(&contexts, &referent) {
            carriers.push(referent);
        }

        carriers
    }

    /// Combine declared completion clauses with checked portable carrier promises.
    pub(super) fn completion_guarantees(
        &self,
        invocation: bray_bound_tree::BoundExecutionSite,
        contract: &crate::TrustedCallContract,
        capture: &impl Fn(&ExecutionCondition) -> ExecutionCondition,
    ) -> (
        Vec<ExecutionCondition>,
        Vec<(ExecutionCondition, ExecutionCondition)>,
    ) {
        let inferred = self
            .dependencies
            .calls()
            .get(&invocation)
            .into_iter()
            .flat_map(|dependencies| {
                dependencies.carriers().iter().flat_map(|carrier| {
                    let condition = crate::execution_condition_from_type_term(
                        self.request.semantic_values(),
                        carrier.guarantee(),
                        dependencies.input_count(),
                    )
                    .expect("checked carrier condition decodes in the same semantic store");

                    let subject = result_projection(
                        self.request.semantic_values(), self.dependencies, carrier.path(),
                    );

                    condition.conjuncts().map(|condition| (capture(condition), subject.clone())).collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();

        let guarantees = contract
            .guarantees
            .iter()
            .flat_map(ExecutionCondition::conjuncts)
            .map(capture)
            .chain(inferred.iter().map(|(condition, _)| condition.clone()))
            .collect::<Vec<_>>();

        let mut subjects = contract
            .witness_subjects
            .iter()
            .filter_map(|(guarantee, subject)| {
                let guarantee = capture(guarantee);

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

        (guarantees, subjects)
    }
}

impl super::check::TrustedChecks {
    pub(super) fn collect_carriers<C: crate::CheckerRequestContext + ?Sized>(
        &mut self,
        request: crate::CheckerUnitView<'_, C>,
        state: &ExecutionState,
        result: &ExecutionCondition,
    ) {
        let owners = if state.witnesses.is_empty() {
            BTreeMap::new()
        } else {
            returned_owners(result)
        };

        collect(state, result, &owners, &self.guarantees, &mut self.carriers);

        if request
            .trusted_contracts()
            .is_some_and(|inputs| inputs.infer_result_witnesses)
        {
            collect_inferred(
                request,
                state,
                result,
                &owners,
                &self.owned_returns,
                &mut self.inferred,
            );
        }
    }

    pub(in crate::analysis) fn result_carriers<C: crate::CheckerRequestContext + ?Sized>(
        &self,
        request: crate::CheckerUnitView<'_, C>,
    ) -> Vec<DependencyWitnessCarrier<StorageProjection>> {
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

fn collect_inferred<C: crate::CheckerRequestContext + ?Sized>(
    request: crate::CheckerUnitView<'_, C>,
    state: &ExecutionState,
    returned: &ExecutionCondition,
    owners: &BTreeMap<ExecutionCondition, Vec<StorageProjection>>,
    exits: &[super::owned::OwnedReturn],
    previous: &mut Option<BTreeSet<DependencyWitnessCarrier<StorageProjection>>>,
) {
    let mut result = BTreeSet::new();

    for (owner, witness) in &state.witnesses {
        let Some(path) = owners.get(owner) else {
            continue;
        };

        if !state.directly_carries_witness(owner) {
            continue;
        }

        for condition in witness
            .conditions
            .iter()
            .filter(|(condition, guarantee)| {
                guarantee.witness
                    && condition.prove_trusted(&state.trusted_assumptions, &state.assumptions)
                        == Some(true)
            })
            .map(|(condition, _)| condition)
        {
            // Rename actual returned storage identities, never ordinary field equalities.
            let condition = super::super::flow::returned_condition(condition, returned);

            if let Some(term) = portable(request, &condition) {
                result.insert(DependencyWitnessCarrier::new(term, path.iter().copied()));
            }
        }
    }

    match previous {
        Some(previous) => {
            // An inactive payload imposes no promise. Every exit that can carry
            // this owner must still support the same current condition.
            previous.retain(|carrier| {
                result.contains(carrier)
                    || super::owned::activity(returned, carrier.path(), &state.assumptions).0
                        == Some(false)
            });

            for carrier in result {
                if !previous.contains(&carrier)
                    && exits.iter().all(|exit| exit.excludes(carrier.path()))
                {
                    previous.insert(carrier);
                }
            }
        }
        None => *previous = Some(result),
    }
}

pub(in crate::analysis::guarantee) fn result_projection(
    values: &bray_symbols::SemanticValueStore,
    dependencies: &bray_bound_tree::CheckedDependencyContracts,
    path: &[DependencyProjection],
) -> ExecutionCondition {
    crate::storage::storage_projections(values, dependencies, path)
        .expect("checked result carrier retains its projection")
        .into_iter()
        .fold(ExecutionCondition::Result, |value, projection| {
            value.project(projection)
        })
}

fn collect(
    state: &ExecutionState,
    result: &ExecutionCondition,
    owners: &BTreeMap<ExecutionCondition, Vec<StorageProjection>>,
    guarantees: &[(ExecutionCondition, bray_source::SourceSpan)],
    previous: &mut [Option<BTreeSet<Vec<StorageProjection>>>],
) {
    if guarantees.is_empty() {
        return;
    }

    let proof = crate::execution_guarantees::ExecutionProof::new(&state.trusted_assumptions, &state.assumptions);

    for ((guarantee, _), previous) in guarantees.iter().zip(previous) {
        let guarantee = guarantee.substitute(
            &|input| {
                input
                    .value_in(&state.current)
                    .unwrap_or(ExecutionCondition::Unknown)
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

        for (carrier, witness) in &state.witnesses {
            let Some(path) = owners.get(carrier) else {
                continue;
            };

            if !state.directly_carries_witness(carrier) {
                continue;
            }

            let mut evidence = BTreeSet::new();

            for condition in witness
                .conditions
                .iter()
                .filter(|(condition, guarantee)| {
                    guarantee.witness
                        && proof.prove_trusted(condition)
                            == Some(true)
                })
                .map(|(condition, _)| condition)
            {
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
) -> BTreeMap<ExecutionCondition, Vec<StorageProjection>> {
    let mut owners = BTreeMap::new();
    let mut pending = vec![(result, Vec::new())];

    while let Some((value, path)) = pending.pop() {
        if path.len() >= ExecutionCondition::WORK_LIMIT {
            continue;
        }

        if *value == ExecutionCondition::Unknown {
            continue;
        }

        // Publish the complete returned observation as well as its identity. Local
        // component snapshots belong to this owner and need not escape in metadata.
        owners.insert(value.clone(), path.clone());
        owners.insert(value.owner_identity().clone(), path.clone());

        match value {
            ExecutionCondition::Present(value) => {
                let mut path = path;

                path.push(StorageProjection::NullableValue);
                pending.push((value, path));
            }
            ExecutionCondition::Observation { contents, kind: ObservationKind::Owned | ObservationKind::Reference, .. } => {
                // Reached data can carry access promises without transferring its ownership.
                pending.push((contents, path));
            }
            ExecutionCondition::Constructed(_, fields) => {
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

                    // Projection is the declared view of this retained component.
                    // Nullable extraction can retain its containing owner's identity.
                    let projected = value.clone().project(*projection);

                    owners.insert(projected, child.clone());
                    pending.push((component, child));
                }
            }
            _ => {}
        }
    }

    owners
}

/// Convert complete storage paths only when publishing portable carrier contracts.
pub(in crate::analysis) fn portable_path<C: crate::CheckerRequestContext + ?Sized>(
    request: crate::CheckerUnitView<'_, C>,
    path: &[StorageProjection],
) -> Vec<DependencyProjection> {
    path.iter()
        .map(|projection| match projection {
            StorageProjection::ElementFromStart(index) => {
                let values = request.semantic_values();

                let value = values
                    .intern_constant_value(bray_symbols::ConstantValueData::new(
                        crate::representation::representation_type(
                            request,
                            bray_compiler_known::RepresentationRole::ScalarUsize,
                        ),
                        bray_symbols::ConstantValueKind::Integer(
                            bray_symbols::IntegerConstant::from_u64(u64::from(index.raw())),
                        ),
                    ))
                    .expect("checked array index belongs to the same semantic store");

                DependencyProjection::Element(
                    values
                        .intern_constant_term(bray_symbols::ConstantTermData::Value(value))
                        .expect("checked array selector belongs to the same semantic store"),
                )
            }
            projection => crate::storage::dependency_projection(*projection)
                .expect("returned owner retains a portable storage projection"),
        })
        .collect()
}

impl ExecutionState {
    /// Discharges ordinary guards while each original carrier still supplies its own evidence.
    pub(in crate::analysis::guarantee) fn refine_witnesses(&mut self) {
        // Ownership guards also have meaning when no trusted carrier exists.
        // Ordinary premises cannot introduce trusted occurrences into this closure.
        let ordinary = crate::execution_guarantees::trusted_condition_closure(Default::default(), &self.assumptions);

        self.assumptions.extend(ordinary);

        for witness in self
            .witnesses
            .values_mut()
            .filter(|witness| witness.available)
        {
            witness.refine(&mut self.trusted_assumptions, &mut self.assumptions);
        }

        self.refine_deferred_witnesses();
    }

    pub(in crate::analysis::guarantee) fn refine_deferred_witnesses(&mut self) {
        let candidates = self
            .deferred_invocations
            .iter()
            .flat_map(|(owner, entries)| {
                entries
                    .producers
                    .keys()
                    .map(move |source| (owner.clone(), *source))
            })
            .collect::<Vec<_>>();

        for (owner, source) in candidates {
            let candidate = &self.deferred_invocations[&owner].producers[&source];
            let entry = &candidate.entry;

            if !entry.pending_execution
                || !super::super::deferred::possible(&candidate.selection, &self.assumptions)
            {
                continue;
            }

            let owners = self
                .owned_witnesses(entry.owned_arguments())
                .filter(|(_, witness)| witness.has_guards())
                .map(|(owner, _)| owner.clone())
                .collect::<BTreeSet<_>>();

            if owners.is_empty() && entry.copy_sources.is_empty() {
                continue;
            }

            let mut assumptions = entry.assumptions.clone();

            assumptions.extend(self.assumptions.iter().cloned());
            assumptions.extend(candidate.selection.iter().cloned());

            let captured = entry.captured_facts(&assumptions);

            assumptions.extend(captured.iter().cloned());

            let changed = !captured.is_subset(&entry.assumptions);

            let trusted = if owners.is_empty() {
                None
            } else {
                let mut trusted = entry.trusted_assumptions.clone();

                for owner in owners {
                    if let Some(witness) = self.witnesses.get_mut(&owner) {
                        witness.refine(&mut trusted, &mut assumptions);
                    }
                }

                (trusted != entry.trusted_assumptions).then_some(trusted)
            };

            // Captured contents are ordinary immutable values, available to every
            // analysis consumer independently of the child run's trusted authority.
            self.assumptions.extend(captured.iter().cloned());

            let candidate = self
                .deferred_invocations
                .get_mut(&owner)
                .expect("captured owner is retained during refinement")
                .producers
                .get_mut(&source)
                .expect("captured producer is retained during refinement");

            if !changed && trusted.is_none() {
                continue;
            }

            // Refinement adds proof without changing the frame's captured input identities.
            let entry = std::sync::Arc::make_mut(&mut candidate.entry);

            entry.assumptions.extend(captured);

            if let Some(trusted) = trusted {
                entry.trusted_assumptions = trusted;
            }
        }
    }
}
