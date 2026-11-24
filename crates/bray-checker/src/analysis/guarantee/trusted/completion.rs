use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::super::flow::ExecutionFlowDomain;
use super::super::state::ExecutionState;
use super::dependencies::{address_value, carry_address_guarantee};
use super::guarantees::CompletionGuarantees;
use crate::execution_guarantees::ExecutionOutput;
use crate::{CheckerRequestContext, ExecutionCondition};

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(in crate::analysis::guarantee) fn complete_trusted_cleanup(
        &self,
        state: &mut ExecutionState,
        place: Option<crate::ExecutionPlace>,
        lifecycle: &super::super::cleanup::Lifecycle,
        entry: &crate::ExecutionCallEvidence,
    ) {
        let Some(place) = place else {
            return;
        };

        let contract = &lifecycle.contract;

        if contract.guarantees.is_empty() && contract.postconditions.is_empty() {
            return;
        }

        let root = place
            .value_in(&state.current)
            .unwrap_or_else(ExecutionCondition::unknown);

        let receiver = crate::ExecutionPlace::from(lifecycle.receiver);
        let arguments = BTreeMap::from([(receiver.clone(), root.clone())]);

        let instantiate = |condition: &ExecutionCondition| condition.substitute(
            &|input| input.value_in(&arguments).unwrap_or_else(ExecutionCondition::unknown),
            &ExecutionCondition::unknown(),
            &mut { ExecutionCondition::WORK_LIMIT },
        ).pointer_observations(&self.pointer_predicates);

        let context = if crate::representation::borrow_target_type(self.request.semantic_values(), lifecycle.call.receiver).is_some() {
            std::borrow::Cow::Owned(BTreeMap::from([(receiver, root.borrowed())]))
        } else {
            std::borrow::Cow::Borrowed(&arguments)
        };

        let support = |state: &ExecutionState, value: &ExecutionCondition, claim: &ExecutionCondition| {
            if contract.address_subjects.is_empty() {
                return crate::execution_guarantees::AddressDependencies::new();
            }

            let mut addresses = state.address_dependencies(value);

            for (address, owners) in entry.addresses.values().flatten() {
                if claim.observes_trusted(address) && owners.iter().all(|owner| state.current.live_value(owner).is_some()) {
                    addresses.entry(address.clone()).or_default().extend(owners.iter().cloned());
                }
            }

            addresses
        };

        for (template, subject) in &contract.witness_subjects {
            let guarantee = instantiate(&entry.capture_condition(template));
            let subject_value = instantiate(subject);

            if guarantee.prove_ordinary(&state.assumptions, Some(&state.proofs)) == Some(true) {
                continue;
            }

            self.carry_input_witnesses(
                state, (template, subject), (&guarantee, &subject_value),
                (&context, &contract.address_subjects), |state| support(state, &subject_value, &guarantee),
            );
        }

        for (template, subject, owning) in &contract.address_subjects {
            if *owning || subject.observes_completion_result() {
                continue;
            }

            let address = address_value(instantiate(subject));

            if address.is_unknown() {
                continue;
            }

            let guarantee = instantiate(&entry.capture_condition(template));
            let addresses = support(state, &address, &guarantee);

            if !addresses.is_empty() {
                carry_address_guarantee(state, &address, &guarantee, &addresses);
            }
        }

        for condition in &contract.guarantees {
            state.assume_contract_condition(instantiate(&entry.capture_condition(condition)));
        }

        for condition in &contract.postconditions {
            instantiate(&entry.capture_condition(condition)).assume(true, &mut state.assumptions);
        }

    }

    pub(in crate::analysis::guarantee) fn expire_trusted_scope(
        &self,
        state: &mut ExecutionState,
        scope: bray_bound_tree::StorageScope,
    ) {
        use bray_bound_tree::{StorageIdentity, StorageScope};

        let abandoned;

        let expired = if let StorageScope::AbandonedTransfer { value, .. } = scope {
            let Some(place) = crate::ExecutionPlace::storage_identity(self.storage, value) else { return; };

            if let Some(bray_bound_tree::AnyBoundNodeId::Expression(expression)) =
                self.storage.identity(value).and_then(|identity| identity.definition_node())
                && self.storage.value_slot(expression) == Some(value)
            {
                state.pending_results.remove(&expression);
            }

            abandoned = [(place, value)];

            abandoned.as_slice()
        } else {
            let Some(expired) = self.scope_places.get(&scope) else { return; };

            expired.as_slice()
        };

        let values = expired
            .iter()
            .filter(|(_, identity)| {
                // Ending a borrow binding does not destroy the reached storage. The
                // retained physical owner below still retires every dependent view.
                !self.storage.identity_type(*identity).is_some_and(|ty| {
                    matches!(
                        self.request.semantic_values().type_data(ty).as_ref(),
                        bray_symbols::TypeData::Borrow { .. }
                    )
                })
            })
            .filter(|(place, _)| {
                state.current.availability(place) != super::super::current::ValueAvailability::Absent
            })
            .filter_map(|(place, _)| place.value_in(&state.current))
            .collect::<Vec<_>>();

        let expired_roots = expired
            .iter()
            .map(|(place, _)| place.root.clone())
            .collect::<BTreeSet<_>>();

        let external = expired.iter().flat_map(|(place, _)| state.current.entry_borrows_under(place))
            .collect::<Vec<_>>();

        state.invalidate_trusted_roots(&expired_roots);

        let lexical = matches!(scope, StorageScope::Block(_) | StorageScope::Iteration(_));

        for (place, identity) in expired {
            if lexical && matches!(self.storage.identity(*identity), Some(
                StorageIdentity::LocalOwned(_) | StorageIdentity::Temporary(_) | StorageIdentity::Evaluated(_)
                    | StorageIdentity::InvocationInput { .. } | StorageIdentity::InvocationResult(_)
            )) && state.current.clear_planned_root(place) {
                continue;
            }

            // Completion can still use formal data or a retained outcome's
            // explicit absence after the storage and its authority end.
            state.current.retire(place);
        }

        if values.is_empty() {
            return;
        }

        // Retirement and unchanged-value checks use the same retained tree.
        // Borrowed inputs describe caller-owned pointees. Ending their local
        // carriers does not retire those pointees.
        let retained = state.current.live_values()
            .chain(match self.request.unit().root() {
                bray_bound_tree::BoundUnitRoot::Expression(root) => Some(self.value(state, root)),
                _ => None,
            })
            .chain(state.pending_results.values().cloned())
            .chain(
                self.borrowed_inputs
                    .iter()
                    .filter(|place| expired_roots.contains(&place.root))
                    .filter_map(|place| place.value_in(&state.current)),
            )
            .chain(external)
            .collect::<crate::execution_guarantees::RetainedSnapshots>();

        let retired = values
            .into_iter()
            .filter(|value| !retained.retains_owned_snapshot(value))
            .collect::<Vec<_>>();

        // An input aggregate can retire after one of its complete children moved
        // into the result. The child's value survives, but dependencies on the
        // expired storage were invalidated above and cannot be restored here.
        state.retire_trusted_values(retired, retained);
    }

    pub(in crate::analysis::guarantee) fn complete_trusted_call(
        &self,
        state: &mut ExecutionState,
        (invocation, publication): (bray_bound_tree::BoundExecutionSite, bray_bound_tree::BoundExecutionSite),
        frame: Option<&Arc<crate::ExecutionCallEvidence>>,
        output: Option<&crate::ExecutionPlace>,
        result: ExecutionCondition,
        observations: &bray_bound_tree::StorageCompletion,
    ) -> ExecutionCondition {
        // Calls without trusted clauses still establish checked result identities.
        // An empty contract contributes no authority or preservation guarantees.
        let ordinary = crate::TrustedCallContract {
            completes: true,
            ..Default::default()
        };

        let contract = self
            .request
            .trusted_contracts()
            .and_then(|contracts| contracts.calls.get(&invocation))
            .unwrap_or(&ordinary);

        let Some(entry) = self.completed_entry(state, invocation, frame, &result, contract) else {
            return result;
        };

        let (row_guarantees, published_guarantees) = CompletionGuarantees::prepare(contract, &entry);

        let admission = frame.is_none().then_some(invocation);

        let completed = self.complete_entry_outputs(state, (Some(invocation), publication), &result, &entry, |state, value, row| {
            if let Some(admission) = admission {
                let completed = state.entries.get_mut(&admission).expect("ordinary completion retains its admitted entry");

                if completed.assumptions != entry.assumptions || completed.trusted_assumptions != entry.trusted_assumptions {
                    let completed = Arc::make_mut(completed);

                    completed.assumptions.clone_from(&entry.assumptions);
                    completed.trusted_assumptions.clone_from(&entry.trusted_assumptions);
                }
            }

            self.complete_admitted_call(state, (invocation, publication), output, (&result, value), observations, (contract, &row_guarantees), &entry, row)
        });

        let Some(result) = completed else { return result; };

        self.publish_reached_contract(
            state, (invocation, publication), (contract, &published_guarantees),
            (&entry, admission), &result,
        );

        result
    }

    fn publish_reached_contract(
        &self,
        state: &mut ExecutionState,
        (invocation, publication): (bray_bound_tree::BoundExecutionSite, bray_bound_tree::BoundExecutionSite),
        (contract, guarantees): (&crate::TrustedCallContract, &CompletionGuarantees<'_>),
        (entry, admission): (&crate::ExecutionCallEvidence, Option<bray_bound_tree::BoundExecutionSite>),
        result: &ExecutionCondition,
    ) {
        let expression = match invocation {
            bray_bound_tree::BoundExecutionSite::Node(bray_bound_tree::AnyBoundNodeId::Expression(expression)) => Some(expression),
            _ => None,
        };

        if guarantees.is_empty() && contract.postconditions.is_empty()
            && expression.is_none_or(|expression| !self.contracts.contains_key(&expression))
        {
            return;
        }

        // Binding a result guard on a single row can erase it. These declarations
        // observe the published result and current borrowed contents together.
        let mutation = bray_bound_tree::StorageMutation::Operation(publication);

        let (arguments, owners) = self.completed_call_observations(state, invocation, mutation, entry, !guarantees.is_empty());

        let instantiate = self.completion_instantiator(invocation, entry, &arguments, result);

        if !guarantees.is_empty() {
            let references = self.dependencies.calls().get(&invocation).map_or_else(Vec::new, |call| {
                let completed = admission.and_then(|site| state.admitted_call(site)).unwrap_or(entry);

                call.storage().reference_outputs(call.captured_values().iter().enumerate().filter_map(|(index, row)| {
                    completed.outputs.is_admitted(index).then_some(row)
                }))
            });

            self.publish_call_guarantees(
                state, (invocation, &references), (contract, entry), guarantees, result,
                (&arguments, &owners), &BTreeSet::new(), &instantiate,
            );
        }

        for condition in &contract.postconditions {
            let condition = instantiate(&entry.capture_condition(condition));

            condition.assume(true, &mut state.assumptions);
        }

        if let Some(expression) = expression {
            self.assume_call_postconditions(state, expression, entry, result, &arguments);
        }
    }

    fn complete_admitted_call(
        &self, state: &mut ExecutionState,
        (invocation, publication): (bray_bound_tree::BoundExecutionSite, bray_bound_tree::BoundExecutionSite),
        output: Option<&crate::ExecutionPlace>, (producer, result): (&ExecutionCondition, ExecutionCondition),
        observations: &bray_bound_tree::StorageCompletion,
        (contract, guarantees): (&crate::TrustedCallContract, &CompletionGuarantees<'_>),
        entry: &crate::ExecutionCallEvidence, row: Option<&ExecutionOutput>,
    ) -> ExecutionCondition {
        let selection = invocation.expression().and_then(|expression| self.semantics.selections().expression(expression));
        let ordinal = |root| selection?.invocation_input_ordinal(invocation, root);
        let checked = self.dependencies.calls().get(&invocation);

        let references = checked.map_or_else(Vec::new, |call| {
            ExecutionOutput::reference_relations(row, call.storage(), call.captured_values())
        });

        let preserved = self.preserved_borrowed_inputs(&references, &entry.arguments, &ordinal);
        let empty_ownership = bray_symbols::DependencyOwnedResult::default();
        let ownership = row.map_or(&empty_ownership, |row| &row.ownership);
        let carriers = row.map_or(&[][..], |row| row.carriers.as_ref());

        let result = self.complete_transferred_ownership(state, publication, checked.map(|call| call.storage()), entry, result, (ownership, preserved));

        state.invalidate_trusted_values(contract.transferred_inputs.iter().map(|input| {
            // The transferred owner values retain their immutable entry observations.
            entry.arguments
                .get(&crate::ExecutionPlace::from(*input))
                .expect("selected storage formation retains its transferred input")
                .clone()
        }));

        let mutation = bray_bound_tree::StorageMutation::Operation(publication);

        let result = match self.dependencies.calls().get(&invocation).and_then(|contract|
            contract.storage().subject(&bray_symbols::DependencySubject::root(bray_symbols::DependencySubjectRoot::Result))
                .map(|(ty, _)| ty))
        {
            Some(ty) => result.coerce(self.request.semantic_values(), ty, ty),
            None => result,
        };

        let result = self.complete_returned_references(state, mutation, output, (producer, result), observations);

        if guarantees.is_empty() && carriers.is_empty() {
            return result;
        }

        {
            let (arguments, owners) = self.completed_call_observations(state, invocation, mutation, entry, true);

            let instantiate = self.completion_instantiator(invocation, entry, &arguments, &result);
            let selected = checked.map(|call| (call.storage(), ordinal));

            let bind_declared = |condition: &ExecutionCondition| {
                contract.guarantees.iter().flat_map(ExecutionCondition::conjuncts)
                    .filter(|template| template.observes_completion_result())
                    .map(|template| entry.capture_condition(template))
                    .find(|template| instantiate(template).implies(condition))
                    .map(|template| self.result_guarantee_owners(
                        (invocation, &references), &entry.arguments, &template, true, &owners,
                        &guarantees.addresses, &instantiate,
                    ))
            };

            let supported = self.complete_returned_carriers(
                state, entry, &result, carriers, (selected, output), (&instantiate, &bind_declared),
            );

            self.publish_call_guarantees(
                state, (invocation, &references), (contract, entry), guarantees, &result,
                (&arguments, &owners), &supported, &instantiate,
            );
        }

        result
    }

    fn completion_instantiator<'a>(
        &'a self,
        invocation: bray_bound_tree::BoundExecutionSite,
        entry: &crate::ExecutionCallEvidence,
        arguments: &'a BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
        result: &'a ExecutionCondition,
    ) -> impl Fn(&ExecutionCondition) -> ExecutionCondition + 'a {
        let transfer_equalities = self.completion_transfer_equalities(invocation, entry);

        move |condition: &ExecutionCondition| {
            let condition = condition.substitute(
                &|place| {
                    place
                        .value_in(arguments)
                        .unwrap_or_else(ExecutionCondition::unknown)
                },
                result,
                &mut { ExecutionCondition::WORK_LIMIT },
            );

            match &transfer_equalities {
                Some(equalities) => condition.with_equalities(equalities),
                None => condition,
            }.pointer_observations(&self.pointer_predicates)
        }
    }

    fn completion_transfer_equalities(
        &self, invocation: bray_bound_tree::BoundExecutionSite, entry: &crate::ExecutionCallEvidence,
    ) -> Option<Arc<BTreeMap<ExecutionCondition, ExecutionCondition>>> {
        match invocation {
            bray_bound_tree::BoundExecutionSite::ScopedExit(expression) => {
                Some(self.proofs.equalities(&entry.assumptions, Some(expression)))
            }
            _ => None,
        }
    }

    pub(in crate::analysis::guarantee) fn complete_returned_carriers(
        &self,
        state: &mut ExecutionState,
        entry: &crate::ExecutionCallEvidence,
        value: &ExecutionCondition,
        carriers: &[bray_symbols::DependencyWitnessCarrier<bray_bound_tree::StorageProjection>],
        (selected, output): (
            Option<(&bray_bound_tree::BoundCallStorage, impl Fn(bray_symbols::DependencySubjectRoot) -> Option<bray_symbols::SymbolOrdinal>)>,
            Option<&crate::ExecutionPlace>,
        ),
        (instantiate, bind_declared): (
            &impl Fn(&ExecutionCondition) -> ExecutionCondition,
            &impl Fn(&ExecutionCondition) -> Option<(BTreeSet<crate::ExecutionPlace>, crate::execution_guarantees::AddressDependencies)>,
        ),
    ) -> BTreeSet<ExecutionCondition> {
        use crate::execution_guarantees::ExecutionObservationKind;

        let mut installed = BTreeSet::new();

        for carrier in carriers {
            let owner = carrier.path().iter().copied().fold(value.clone(), ExecutionCondition::project);

            if !matches!(owner.ownership_identity(), Some((ExecutionObservationKind::Owned, _))) {
                continue;
            }

            let formal = entry.formal_condition(self.request.semantic_values(), carrier.guarantee());
            let condition = instantiate(&entry.capture_condition(&formal));

            if !condition.contains_trusted() || condition.contains_unknown()
                || condition.prove_ordinary(&state.assumptions, Some(&state.proofs)) == Some(true) {
                continue;
            }

            let support = match carrier.support() {
                Some(support) => self.bind_returned_witness_support(
                    state, entry, (value, &condition), support, selected.as_ref(), output,
                ),
                None => bind_declared(&condition),
            };

            let Some(support) = support else {
                continue;
            };

            self.carry_result_guarantee(state, &owner, &condition, false, true, support);
            installed.insert(condition);
        }

        for condition in &installed {
            state.assume_contract_condition(condition.clone());
        }

        installed
    }

    fn bind_returned_witness_support(
        &self,
        state: &ExecutionState,
        entry: &crate::ExecutionCallEvidence,
        (result, condition): (&ExecutionCondition, &ExecutionCondition),
        support: &bray_symbols::DependencyWitnessSupport,
        selected: Option<&(&bray_bound_tree::BoundCallStorage, impl Fn(bray_symbols::DependencySubjectRoot) -> Option<bray_symbols::SymbolOrdinal>)>,
        output: Option<&crate::ExecutionPlace>,
    ) -> Option<(BTreeSet<crate::ExecutionPlace>, crate::execution_guarantees::AddressDependencies)> {
        use bray_symbols::DependencySubjectRoot;

        let values = self.request.semantic_values();

        let bind_owner = |subject: &bray_symbols::DependencySubject| -> Option<BTreeSet<crate::ExecutionPlace>> {
            let root = subject.subject_root();

            if let DependencySubjectRoot::StaticInstance(id, _) = root {
                let path = crate::storage::storage_projections(values, self.dependencies, subject.projections())?;

                let place = crate::ExecutionPlace::static_instance(values.static_reference(id).as_ref().clone()).project(&path)?;

                return Some([place].into());
            }

            if root == DependencySubjectRoot::Result {
                let path = crate::storage::storage_projections(values, self.dependencies, subject.projections())?;

                let value = path.iter().copied().fold(result.clone(), ExecutionCondition::project);

                if value.is_unknown() { return None; }

                return Some([output?.clone().project(&path)?].into());
            }

            let (storage, input_ordinal) = selected?;

            let (_, route) = storage.subject(subject)?;

            let ordinal = input_ordinal(root)?;
            let argument = entry.arguments.get(&crate::ExecutionPlace::argument(ordinal))?;

            // An owned argument is held by this frame. It cannot supply the
            // caller's original storage lifetime after admission.
            if !argument.is_reference()
                || !bray_bound_tree::StoragePath::inline_projections(route.projections())
                || route.steps().len() > 1
                || route.steps().first().is_some_and(|step| !matches!(step,
                    bray_bound_tree::StorageReferenceStep::ReadBorrow { index: 0, .. }))
            {
                return None;
            }

            let retained = entry.borrow_owners.get(&ordinal).into_iter().flat_map(|owners| owners.iter());
            let mut owners = BTreeSet::new();

            for place in retained {
                state.current.live_value(place)?;

                owners.insert(place.clone());
            }

            let address = entry.argument_storage(ordinal)?.known_address()?.project(route.projections());
            let place = crate::ExecutionPlace::storage_path(self.storage, &address.value_observation_path()?)?;

            state.current.live_value(&place)?;

            owners.insert(place);

            Some(owners)
        };

        let bind_owners = |subjects: &BTreeSet<bray_symbols::DependencySubject>| {
            let mut owners = BTreeSet::new();

            for subject in subjects {
                owners.extend(bind_owner(subject)?);
            }

            Some(owners)
        };

        let owners = bind_owners(support.owners())?;
        let mut addresses = crate::execution_guarantees::AddressDependencies::new();

        let bind_address = |subject: &bray_symbols::DependencySubject| -> Option<BTreeSet<crate::ExecutionPlace>> {
            use bray_bound_tree::{StorageProjection, StorageReferenceStep};
            use bray_symbols::DependencyProjection;

            if subject.projections().last() != Some(&DependencyProjection::AddressTarget) {
                return bind_owner(subject);
            }

            let (storage, input_ordinal) = selected?;

            let (_, route) = storage.subject(subject)?;

            let (StorageProjection::AddressTarget, path) = route.projections().split_last()? else { return None; };

            let (StorageReferenceStep::ReadAddress { index, .. }, prior) = route.steps().split_last()? else { return None; };

            if *index != path.len() || !prior.iter().all(|step| matches!(step, StorageReferenceStep::ReadBorrow { .. })) {
                return None;
            }

            let input = crate::ExecutionPlace::argument(input_ordinal(subject.subject_root())?);
            let argument = entry.arguments.get(&input)?;
            let descriptor = path.iter().copied().fold(argument.clone(), ExecutionCondition::project).into_referent();

            entry.addresses.get(&input)?.get(descriptor.descriptor_identity()).cloned()
        };

        // The portable witness may observe an address through a formal receiver.
        // Admission already has the address and the storage that actually bounds it.
        for (address, places) in entry.addresses.values().flatten().filter(|(address, _)| condition.observes_trusted(address)) {
            addresses.entry(address.clone()).or_default().extend(places.iter().cloned());
        }

        for (term, sources) in support.addresses() {
            let descriptor = entry.bind_term(values, *term, result);

            if descriptor.contains_unknown() { return None; }

            let address = descriptor.descriptor_identity();

            if let std::collections::btree_map::Entry::Vacant(entry) = addresses.entry(address.clone()) {
                let mut bound = BTreeSet::new();

                for source in sources {
                    bound.extend(bind_address(source)?);
                }

                entry.insert(bound);
            }
        }

        Some((owners, addresses))
    }

    fn completed_entry(
        &self,
        state: &mut ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
        frame: Option<&Arc<crate::ExecutionCallEvidence>>,
        result: &ExecutionCondition,
        contract: &crate::TrustedCallContract,
    ) -> Option<Arc<crate::ExecutionCallEvidence>> {
        let mut entry = frame.or_else(|| state.entries.get(&invocation))
            .filter(|entry| {
                // Normal completion establishes ordinary requirements, including runtime
                // checks. Trusted requirements still need authority at the call boundary.
                entry.trusted_boundary
                    || self.prove_trusted_requirements(entry, &contract.requirements).is_ok()
            })
            .cloned()?;

        if (frame.is_none() && entry.pending_execution)
            || !contract.completes
                && matches!(result.kind(), crate::ExecutionConditionKind::Expression(expression) if invocation == (*expression).into())
        {
            return None;
        }

        let preconditions = assume_normal_entry(&mut entry, &contract.preconditions, &contract.requirements, &state.proofs)?;

        for (condition, domain) in preconditions.into_iter().filter(|_| frame.is_none()) {
            // A completed runtime check establishes its captured value, not a new
            // capability or a claim about mutable borrowed storage after the call.
            if domain == bray_symbols::PredicateDomain::Value
                && condition != ExecutionCondition::unknown()
                && !condition.observes_live(None)
            {
                condition.assume(true, &mut state.assumptions);
            }
        }

        Some(entry)
    }
}

fn assume_normal_entry(
    entry: &mut Arc<crate::ExecutionCallEvidence>,
    preconditions: &[(ExecutionCondition, bray_symbols::PredicateDomain)],
    requirements: &[ExecutionCondition],
    cache: &crate::execution_guarantees::ExecutionProofCache,
) -> Option<Vec<(ExecutionCondition, bray_symbols::PredicateDomain)>> {
    if preconditions.is_empty() && requirements.is_empty() { return Some(Vec::new()); }

    let equalities = cache.equalities(&entry.assumptions, None);

    let preconditions = preconditions
        .iter()
        .map(|(condition, domain)| (condition, *domain))
        .chain(requirements.iter().map(|condition| (condition, bray_symbols::PredicateDomain::Trusted)))
        .map(|(condition, domain)| (entry.bind_entry_condition(condition, &equalities), domain))
        .collect::<Vec<_>>();

    // A disproven entry cannot complete normally. Unknown ordinary requirements
    // can pass their runtime checks; capture that entry evidence for completion guards.
    if preconditions.iter().any(|(condition, _)| {
        condition.prove_ordinary(&entry.assumptions, Some(cache)) == Some(false)
    }) {
        return None;
    }

    for (condition, domain) in &preconditions {
        // The caller has already established or acknowledged trusted requirements.
        // Preserve that admission in the immutable entry, independently of live storage.
        if *domain == bray_symbols::PredicateDomain::Value {
            condition.clone().assume(true, &mut Arc::make_mut(entry).assumptions);
        } else if *domain == bray_symbols::PredicateDomain::Trusted {
            condition.clone().assume(true, &mut Arc::make_mut(entry).trusted_assumptions);
        }
    }

    Some(preconditions)
}

#[cfg(test)]
mod admission_tests {
    use super::assume_normal_entry;
    use crate::{ExecutionCallEvidence, ExecutionCondition, ExecutionPlace};
    use std::sync::Arc;

    #[test]
    fn completed_admission_keeps_checked_requirements_out_of_pre_entry_evidence() {
        let input = ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0));
        let owner = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(1))));
        let predicate = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>("ValidWrite");
        let substitution = crate::test_support::empty_substitution(predicate.into());

        let requirement = ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(ExecutionCondition::predicate(
            predicate.into(), substitution, vec![ExecutionCondition::new(crate::ExecutionConditionKind::Input(input.clone())).borrowed()],
        ))));

        let mut before = ExecutionCallEvidence::default();

        before.arguments.insert(input, owner);

        let before = Arc::new(before);
        let mut completed = Arc::clone(&before);

        assert!(before.prove_trusted_requirements(std::slice::from_ref(&requirement)).is_err());
        assume_normal_entry(&mut completed, &[], std::slice::from_ref(&requirement), &Default::default()).unwrap();
        assert!(completed.prove_trusted_requirements(std::slice::from_ref(&requirement)).is_ok());
        assert!(before.prove_trusted_requirements(std::slice::from_ref(&requirement)).is_err());

        assert!(assume_normal_entry(&mut completed,
            &[(ExecutionCondition::boolean(false), bray_symbols::PredicateDomain::Value)], &[], &Default::default()).is_none());
    }
}
