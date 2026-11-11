use std::collections::{BTreeMap, BTreeSet};

use super::super::flow::ExecutionFlowDomain;
use super::super::state::ExecutionState;
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

        let root = place
            .value_in(&state.current)
            .unwrap_or(ExecutionCondition::Unknown);

        let arguments = BTreeMap::from([(lifecycle.receiver.into(), root)]);

        for (conditions, trusted) in [
            (&contract.guarantees, true),
            (&contract.postconditions, false),
        ] {
            for condition in conditions {
                let condition = condition.capture_entry(
                    &|place| {
                        place
                            .value_in(&entry.arguments)
                            .unwrap_or(ExecutionCondition::Unknown)
                    },
                    &entry.trusted_assumptions,
                    &entry.assumptions,
                    &mut { ExecutionCondition::WORK_LIMIT },
                );

                let condition = condition.substitute(
                    &|input| {
                        input
                            .value_in(&arguments)
                            .unwrap_or(ExecutionCondition::Unknown)
                    },
                    &ExecutionCondition::Unknown,
                    &mut { ExecutionCondition::WORK_LIMIT },
                ).pointer_observations(&self.pointer_predicates);

                if trusted {
                    condition.assume(true, &mut state.trusted_assumptions);
                } else {
                    condition.assume(true, &mut state.assumptions);
                }
            }
        }
    }

    pub(in crate::analysis::guarantee) fn expire_trusted_scope(
        &self,
        state: &mut ExecutionState,
        scope: bray_bound_tree::StorageScope,
    ) {
        let Some(expired) = self.scope_places.get(&scope) else {
            return;
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

        state.invalidate_trusted_roots(&expired_roots);

        // Terminal observations remain readable by completion clauses after the
        // storage and its access authority end.
        for (place, _) in expired {
            state.current.retire(place);
        }

        // Index retained owners once instead of scanning all live aggregates for
        // every expired value and each of its owned components.
        // Borrowed inputs describe caller-owned pointees. Ending their local
        // carriers does not retire those pointees.
        let retained = state.current.live_values()
            .chain(match self.request.unit().root() {
                bray_bound_tree::BoundUnitRoot::Expression(root) => Some(self.value(state, root)),
                _ => None,
            })
            .chain(state.pending_results.values().cloned())
            .flat_map(|value| state.owned_values(&value))
            .chain(
                self.borrowed_inputs
                    .iter()
                    .chain(&self.disposal.borrowed_inputs)
                    .filter(|place| expired_roots.contains(&place.root))
                    .filter_map(|place| place.value_in(&state.current))
                    .flat_map(|value| state.owned_values(&value)),
            )
            .collect::<BTreeSet<_>>();

        let retired = values
            .into_iter()
            .filter(|value| !retained.contains(value))
            .collect::<Vec<_>>();

        // An input aggregate can retire after one of its complete children moved
        // into the result. The child's value survives, but dependencies on the
        // expired storage were invalidated above and cannot be restored here.
        state.retire_trusted_values(retired, &retained);
    }

    pub(in crate::analysis::guarantee) fn complete_trusted_call(
        &self,
        state: &mut ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
        result: ExecutionCondition,
        observations: &bray_bound_tree::StorageCompletion,
    ) -> (ExecutionCondition, Option<BTreeMap<crate::ExecutionPlace, ExecutionCondition>>) {
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

        let Some(entry) = self.completed_entry(state, invocation, &result, contract) else {
            return (result, None);
        };

        let capture = |condition: &ExecutionCondition| {
            condition.capture_entry(
                &|place| {
                    place
                        .value_in(&entry.arguments)
                        .unwrap_or(ExecutionCondition::Unknown)
                },
                &entry.trusted_assumptions,
                &entry.assumptions,
                &mut { ExecutionCondition::WORK_LIMIT },
            )
        };

        let (guarantees, subjects) = self.completion_guarantees(invocation, contract, &capture);

        let address_subjects = contract
            .address_subjects
            .iter()
            .map(|(guarantee, subject, owning)| (capture(guarantee), subject.clone(), *owning))
            .collect::<Vec<_>>();

        let postconditions = contract
            .postconditions
            .iter()
            .map(capture)
            .collect::<Vec<_>>();

        let transfer_equalities = match invocation {
            bray_bound_tree::BoundExecutionSite::ScopedExit(expression) => Some(
                ExecutionCondition::equalities(&entry.assumptions, Some(expression)),
            ),
            _ => None,
        };

        // Publish the admitted entry snapshot independently of current mutable contents.
        state.resume_owned_witnesses(&entry);

        let completed = state
            .entries
            .get_mut(&invocation)
            .expect("completed invocation retains its captured entry");

        completed.assumptions = entry.assumptions;
        completed.trusted_assumptions = entry.trusted_assumptions;
        self.guard_returned_inputs(state, invocation, &entry.arguments, &result);

        let outside = self.surviving_call_values(state, invocation);

        self.retire_consumed_inputs(state, invocation, &entry.arguments, &result, &outside);

        // Ownership leaves with the entry value. Borrowed completion clauses below
        // instead observe the storage contents left by the call.
        let result = self.complete_returned_values(invocation, state, result, &entry.arguments, &outside);

        let mut arguments = entry.arguments;

        state.invalidate_trusted_values(contract.transferred_inputs.iter().map(|input| {
            // The transferred owner values retain their immutable entry observations.
            arguments
                .get(&crate::ExecutionPlace::from(*input))
                .expect("selected storage formation retains its transferred input")
                .clone()
        }));

        let owners = if let Some((_, call)) = self.observation_call(invocation)
        {
            // Completion clauses observe final contents, independently of trusted authority.
            self.prepare_call_observations(
                state,
                invocation,
                call,
                &mut arguments,
                &entry.addresses,
                observations,
                !guarantees.is_empty(),
            )
        } else {
            Default::default()
        };

        let result = self.complete_returned_references(invocation, result, &arguments);

        let instantiate = |condition: &ExecutionCondition| {
            let condition = condition.substitute(
                &|place| {
                    place
                        .value_in(&arguments)
                        .unwrap_or(ExecutionCondition::Unknown)
                },
                &result,
                &mut { ExecutionCondition::WORK_LIMIT },
            );

            match &transfer_equalities {
                Some(equalities) => condition.with_equalities(equalities),
                None => condition,
            }.pointer_observations(&self.pointer_predicates)
        };

        let carry_result = |state: &mut ExecutionState,
                            template: &ExecutionCondition,
                            guarantee: &ExecutionCondition,
                            carrier: &ExecutionCondition,
                            owning: bool| {
            let sources = self.result_guarantee_owners(
                invocation,
                &state
                    .entries
                    .get(&invocation)
                    .expect("completed invocation retains its captured arguments")
                    .arguments,
                template,
                owning,
                &owners,
                &address_subjects,
                &instantiate,
            );

            self.carry_result_guarantee(
                state,
                carrier,
                guarantee,
                !contract.transferred_inputs.is_empty(),
                owning,
                sources,
            );
        };

        for (template, subject) in &subjects {
            let guarantee = instantiate(template);
            let condition = instantiate(subject);

            // A guarantee discharged solely by its ordinary guard carries no trusted authority.
            if condition == ExecutionCondition::Unknown
                || guarantee.prove(&state.assumptions, &mut { ExecutionCondition::WORK_LIMIT }) == Some(true)
            {
                continue;
            }

            if subject.observes_completion_result() {
                let carrier = self.borrowed_result_carrier(&result, subject).unwrap_or(condition);

                carry_result(state, template, &guarantee, &carrier, true);
                continue;
            }

            let carriers = self.input_witness_owners(
                state, template, subject, &condition, &arguments, &address_subjects,
            );

            if carriers.is_empty() { continue; }

            let addresses = self.input_guarantee_addresses(
                invocation,
                &state
                    .entries
                    .get(&invocation)
                    .expect("completed invocation retains its captured arguments")
                    .arguments,
                template,
                &owners,
                &address_subjects,
                &instantiate,
            );

            for carrier in carriers {
                state
                    .witnesses
                    .entry(carrier.owner_identity().clone())
                    .or_default()
                    .carry(&guarantee, true, &addresses);
            }
        }

        self.carry_input_access_guarantees(
            invocation,
            state,
            &address_subjects,
            &owners,
            &instantiate,
        );

        for template in guarantees
            .iter()
            .filter(|guarantee| guarantee.observes_completion_result())
        {
            if subjects.iter().any(|(guarantee, subject)| {
                guarantee == template && subject.observes_completion_result()
            }) {
                continue;
            }

            let guarantee = instantiate(template);

            if guarantee.prove(&state.assumptions, &mut { ExecutionCondition::WORK_LIMIT })
                != Some(true)
            {
                carry_result(state, template, &guarantee, &result, false);
            }
        }

        for guarantee in &guarantees {
            let guarantee = instantiate(guarantee);

            guarantee.assume(true, &mut state.trusted_assumptions);
        }

        for postcondition in &postconditions {
            let condition = instantiate(postcondition);

            condition.assume(true, &mut state.assumptions);
        }

        (result, Some(arguments))
    }
    fn carry_result_guarantee(
        &self,
        state: &mut ExecutionState,
        carrier: &ExecutionCondition,
        guarantee: &ExecutionCondition,
        transferred: bool,
        owning: bool,
        sources: (
            BTreeSet<crate::ExecutionPlace>,
            crate::execution_guarantees::AddressDependencies,
        ),
    ) {
        let (dependencies, addresses) = sources;

        let witness = state
            .witnesses
            .entry(carrier.owner_identity().clone())
            .or_default();

        if !dependencies.is_subset(&witness.dependencies) {
            std::sync::Arc::make_mut(witness)
                .dependencies
                .extend(dependencies);
        }

        witness.carry(guarantee, owning, &addresses);

        // Copyable raw-address facts do not own allocation authority. Only a complete
        // owning carrier restricts which transferred values may supply that authority.
        if !owning {
            return;
        }

        let mut pending = vec![guarantee];

        while let Some(predicate) = pending.pop() {
            match predicate {
                ExecutionCondition::Trusted(predicate) => pending.push(predicate),
                ExecutionCondition::Operation(_, operands) => pending.extend(operands.iter()),
                ExecutionCondition::Predicate(symbol, _, _)
                    if Some(*symbol) == self.owned_allocation
                        && (predicate.observes(carrier) || transferred) =>
                {
                    state
                        .allocation_owners
                        .insert(predicate.clone(), carrier.owner_identity().clone());
                }
                _ => {}
            }
        }
    }
    fn completed_entry(
        &self,
        state: &mut ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
        result: &ExecutionCondition,
        contract: &crate::TrustedCallContract,
    ) -> Option<crate::ExecutionCallEvidence> {
        let mut entry = state
            .entries
            .get(&invocation)
            .filter(|entry| {
                // Normal completion establishes ordinary requirements, including runtime
                // checks. Trusted requirements still need authority at the call boundary.
                entry.trusted_boundary
                    || self.prove_trusted_requirements(entry, &contract.requirements).is_ok()
            })
            .cloned()?;

        // Completion guards use the same admitted copy observations as entry
        // proof. Keep this projection local to proof, outside live witness state.
        if !entry.copy_sources.is_empty() {
            entry.trusted_assumptions = entry.proof_facts(&entry.trusted_assumptions).collect();
        }

        if entry.pending_execution
            || !contract.completes
                && matches!(result, ExecutionCondition::Expression(expression) if invocation == (*expression).into())
        {
            return None;
        }

        let preconditions = assume_normal_entry(&mut entry, &contract.preconditions, &contract.requirements)?;

        for (condition, domain) in preconditions {
            // A completed runtime check establishes its captured value, not a new
            // capability or a claim about mutable borrowed storage after the call.
            if domain == bray_symbols::PredicateDomain::Value
                && condition != ExecutionCondition::Unknown
                && !condition.observes_live(None)
            {
                condition.assume(true, &mut state.assumptions);
            }
        }

        Some(entry)
    }
}

fn assume_normal_entry(
    entry: &mut crate::ExecutionCallEvidence,
    preconditions: &[(ExecutionCondition, bray_symbols::PredicateDomain)],
    requirements: &[ExecutionCondition],
) -> Option<Vec<(ExecutionCondition, bray_symbols::PredicateDomain)>> {
    let equalities = ExecutionCondition::equalities(&entry.assumptions, None);

    let preconditions = preconditions
        .iter()
        .map(|(condition, domain)| (condition, *domain))
        .chain(requirements.iter().map(|condition| (condition, bray_symbols::PredicateDomain::Trusted)))
        .map(|(condition, domain)| {
            let condition = condition
                .substitute(
                    &|place| {
                        place
                            .value_in(&entry.arguments)
                            .unwrap_or(ExecutionCondition::Unknown)
                    },
                    &ExecutionCondition::Unknown,
                    &mut { ExecutionCondition::WORK_LIMIT },
                )
                .with_equalities(&equalities);

            (condition, domain)
        })
        .collect::<Vec<_>>();

    // A disproven entry cannot complete normally. Unknown ordinary requirements
    // can pass their runtime checks; capture that entry evidence for completion guards.
    if preconditions.iter().any(|(condition, _)| {
        condition.prove(&entry.assumptions, &mut { ExecutionCondition::WORK_LIMIT }) == Some(false)
    }) {
        return None;
    }

    for (condition, domain) in &preconditions {
        // The caller has already established or acknowledged trusted requirements.
        // Preserve that admission in the immutable entry, independently of live storage.
        if *domain == bray_symbols::PredicateDomain::Value {
            condition.clone().assume(true, &mut entry.assumptions);
        } else if *domain == bray_symbols::PredicateDomain::Trusted {
            condition.clone().assume(true, &mut entry.trusted_assumptions);
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
        let owner = ExecutionCondition::Input(ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(1)));
        let predicate = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>("ValidWrite");
        let substitution = crate::test_support::empty_substitution(predicate.into());
        let requirement = ExecutionCondition::Trusted(Arc::new(ExecutionCondition::predicate(
            predicate.into(), substitution, vec![ExecutionCondition::Input(input.clone()).borrowed()],
        )));
        let mut before = ExecutionCallEvidence::default();
        before.arguments.insert(input, owner);
        let mut completed = before.clone();
        assert!(before.prove_trusted_requirements(std::slice::from_ref(&requirement)).is_err());
        assume_normal_entry(&mut completed, &[], std::slice::from_ref(&requirement)).unwrap();
        assert!(completed.prove_trusted_requirements(std::slice::from_ref(&requirement)).is_ok());
        assert!(before.prove_trusted_requirements(std::slice::from_ref(&requirement)).is_err());

        assert!(assume_normal_entry(&mut completed,
            &[(ExecutionCondition::Boolean(false), bray_symbols::PredicateDomain::Value)], &[]).is_none());
    }
}
