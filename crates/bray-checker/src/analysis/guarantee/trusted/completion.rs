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

        let arguments = entry
            .arguments
            .keys()
            .map(|input| {
                let mut value = root.clone();

                for field in input.projections.iter() {
                    value = value.component(*field);
                }

                (input.clone(), value)
            })
            .collect::<BTreeMap<_, _>>();

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
                );

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
            .filter_map(|(place, _)| place.value_in(&state.current))
            .collect::<Vec<_>>();

        let expired_roots = expired
            .iter()
            .map(|(place, _)| place.root.clone())
            .collect::<BTreeSet<_>>();

        // A formal carrier's stored reference names caller-owned storage. Retain its
        // checked final value for completion clauses after disposing the carrier.
        let borrowed = self
            .borrowed_inputs
            .iter()
            .chain(&self.disposal.borrowed_inputs)
            .filter(|place| expired_roots.contains(&place.root))
            .filter_map(|place| {
                place
                    .value_in(&state.current)
                    .map(|value| (place.clone(), value))
            })
            .collect::<BTreeMap<_, _>>();

        // Normal postconditions read the final parameter observations. Retiring the
        // parameter owner must revoke its authority without making lookup fall back
        // to the entry value. Restore these snapshots only after owner retirement.
        let completed_inputs = state
            .current
            .iter()
            .filter(|(place, _)| place.is_callable_input() && expired_roots.contains(&place.root))
            .map(|(place, value)| (place.clone(), value.clone()))
            .collect::<BTreeMap<_, _>>();

        state.invalidate_trusted_roots(&expired_roots);

        state
            .current
            .retain(|place, _| !expired_roots.contains(&place.root));

        state.current.extend(borrowed);

        // Index retained owners once instead of scanning all live aggregates for
        // every expired value and each of its owned components.
        let retained = std::iter::once(&state.result)
            .chain(state.current.values())
            .chain(state.pending_results.values())
            .flat_map(ExecutionCondition::owned_values)
            .cloned()
            .collect::<BTreeSet<_>>();

        let retired = values
            .into_iter()
            .filter(|value| !retained.contains(value))
            .collect::<Vec<_>>();

        // An input aggregate can retire after one of its complete children moved
        // into the result. The child's value survives, but dependencies on the
        // expired storage were invalidated above and cannot be restored here.
        state.retire_trusted_values(retired, &retained);
        state.current.extend(completed_inputs);
    }

    pub(in crate::analysis::guarantee) fn complete_trusted_call(
        &self,
        state: &mut ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
        result: ExecutionCondition,
        observations: &BTreeMap<
            bray_bound_tree::BoundExecutionSite,
            BTreeSet<bray_bound_tree::StoragePath>,
        >,
    ) {
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
            return;
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
        state
            .entries
            .get_mut(&invocation)
            .expect("completed invocation retains its captured entry")
            .assumptions = entry.assumptions;

        state.restore_returned_evidence(entry.returned);

        let mut arguments = entry.arguments;

        state.invalidate_trusted_values(contract.transferred_inputs.iter().map(|input| {
            // The transferred owner values retain their immutable entry observations.
            arguments
                .get(&crate::ExecutionPlace::from(*input))
                .expect("selected storage formation retains its transferred input")
                .clone()
        }));

        let (carriers, owners) = if (!guarantees.is_empty() || !postconditions.is_empty())
            && let Some((expression, call)) = self.observation_call(invocation)
        {
            // Completion clauses observe final contents, independently of trusted authority.
            self.prepare_call_observations(
                state,
                expression,
                call,
                &mut arguments,
                &entry.addresses,
                observations,
                !guarantees.is_empty(),
            )
        } else {
            Default::default()
        };

        let result_expression = match result {
            ExecutionCondition::Expression(expression) => Some(expression),
            _ => None,
        };

        let result = self.complete_returned_values(invocation, state, result, &arguments);

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
            }
        };

        let carry_result = |state: &mut ExecutionState,
                            template: &ExecutionCondition,
                            guarantee: &ExecutionCondition,
                            carrier: &ExecutionCondition,
                            owning: bool| {
            let sources = self.result_guarantee_owners(
                invocation,
                template,
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

            // A guarantee discharged solely by its ordinary guard carries no trusted authority.
            if guarantee.prove(&state.assumptions, &mut { ExecutionCondition::WORK_LIMIT })
                == Some(true)
            {
                continue;
            }

            let condition = instantiate(subject);

            if condition == ExecutionCondition::Unknown {
                continue;
            }

            if subject.observes_completion_result() {
                carry_result(state, template, &guarantee, &condition, true);
            }

            for carrier in &carriers {
                if *carrier != ExecutionCondition::Unknown && condition.observes(carrier) {
                    let addresses = self.input_guarantee_addresses(
                        invocation,
                        template,
                        &owners,
                        &address_subjects,
                        &instantiate,
                    );

                    state
                        .witnesses
                        .entry(carrier.owner_identity().clone())
                        .or_default()
                        .carry(&guarantee, true, &addresses);
                }
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

        if matches!(
            invocation,
            bray_bound_tree::BoundExecutionSite::CallInput { .. }
                | bray_bound_tree::BoundExecutionSite::Iterate(_)
                | bray_bound_tree::BoundExecutionSite::Next(_)
        ) {
            let place = self
                .input_place(invocation)
                .expect("completed input has its place");

            state.assign(place, result);
        } else if let Some(expression) = result_expression {
            state.expressions.insert(expression, result);
        }
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

        witness.dependencies.extend(dependencies);

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
                        .insert(predicate.clone(), carrier.clone());
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
                    || self.trusted_requirements_proven(entry, &contract.requirements)
            })
            .cloned()?;

        if entry.pending_execution
            || !contract.completes
                && matches!(result, ExecutionCondition::Expression(expression) if invocation == (*expression).into())
        {
            return None;
        }

        let preconditions = assume_normal_entry(&mut entry, &contract.preconditions)?;

        for (condition, domain) in preconditions {
            // A completed runtime check establishes its captured value, not a new
            // capability or a claim about mutable borrowed storage after the call.
            if domain == bray_symbols::PredicateDomain::Value
                && condition != ExecutionCondition::Unknown
                && !condition.observes_borrowed(None)
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
) -> Option<Vec<(ExecutionCondition, bray_symbols::PredicateDomain)>> {
    let equalities = ExecutionCondition::equalities(&entry.assumptions, None);

    let preconditions = preconditions
        .iter()
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

            (condition, *domain)
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
        // Normal return proves runtime value checks in their immutable entry snapshot.
        // Authority and execution requirements retain their separate checked premises.
        if *domain == bray_symbols::PredicateDomain::Value {
            condition.clone().assume(true, &mut entry.assumptions);
        }
    }

    Some(preconditions)
}
