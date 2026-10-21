use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::BoundExpressionId;

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
        scope: bray_bound_tree::BoundBlockId,
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

    pub(in crate::analysis) fn awaited_invocation(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> Option<bray_bound_tree::BoundExecutionSite> {
        let bray_bound_tree::BoundExpression::Await(awaited) =
            self.request.view().expression(expression)?
        else {
            return None;
        };

        let observed = self.value(state, awaited.operand());

        let ExecutionCondition::Expression(source) = observed else {
            return None;
        };

        let site = source.into();

        self.dependencies
            .calls()
            .get(&site)?
            .deferred_effects()
            .map(|_| site)
    }

    fn prepare_call_observations(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
        call: &bray_bound_tree::SelectedCall,
        arguments: &mut BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
        origins: &BTreeMap<bray_bound_tree::BoundExecutionSite, BTreeSet<bray_bound_tree::StoragePath>>,
        retain_carriers: bool,
    ) -> (Vec<ExecutionCondition>, BTreeSet<crate::ExecutionPlace>) {
        let mut carriers = Vec::new();
        let mut owners = BTreeSet::new();

        // Whole-owner guarantees follow the returned value's dependency contract.
        // Address and field guarantees also depend on the borrowed storage that
        // established them, even when the raw value has no ordinary borrow.
        let returned = |operand| {
            retain_carriers
                && self
                    .dependencies
                    .calls()
                    .get(&expression.into())
                    .and_then(|dependencies| dependencies.retained_inputs())
                    .is_none_or(|inputs| inputs.contains(&operand))
        };

        for argument in call.arguments() {
            let (parameter, ordinal, ty) = match argument {
                bray_bound_tree::SelectedArgument::Explicit { parameter, ordinal, conversion, .. } => (*parameter, *ordinal, conversion.target_type()),
                bray_bound_tree::SelectedArgument::Default { parameter, ordinal, ty, .. } => (Some(*parameter), *ordinal, *ty),
            };
            let operand = call.input_site(expression, bray_symbols::DependencySubjectRoot::Parameter(
                bray_symbols::SymbolOrdinal::new(ordinal))).expect("selected argument has its evaluated input");
            let reference =
                crate::representation::reference_kind(self.request, ty);

            if reference.is_none() {
                continue;
            }

            let Some(place) = self.input_place(operand) else {
                // Indexed storage has no scalar field path. Its enclosing owner still bounds
                // the lifetime of a returned address or capability.
                for access in self
                    .storage
                    .occurrence_plans(operand)
                    .filter(|_| returned(operand))
                {
                    if let Some(owner) =
                        crate::ExecutionPlace::storage_owner(self.storage, access.access())
                    {
                        owners.insert(owner);
                    }
                }

                continue;
            };

            if reference == Some(bray_symbols::DependencyReferenceKind::Address) {
                let ordinal = ordinal
                    .checked_add(u32::from(call.receiver().is_some()))
                    .expect("selected argument order must fit");

                let ordinal =
                    crate::ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(ordinal));

                if let Some(current) = place.value_in(&state.current)
                    && current.owner_identity() != &ExecutionCondition::Unknown
                    && arguments
                        .get(&ordinal)
                        .is_some_and(|entry| entry.owner_identity() == current.owner_identity())
                {
                    // Completion clauses observe current contents of the evaluated address.
                    // A reassigned or exposed pointer slot cannot establish this identity.
                    if let Some(parameter) = parameter {
                        arguments.insert(
                            bray_bound_tree::BoundReferenceTarget::Surface(parameter.into())
                                .into(),
                            current.clone(),
                        );
                    }

                    arguments.insert(ordinal, current);
                }

                continue;
            }

            let input = self.current_input(state, operand);

            // A local borrow alias retains the referent, rather than the slot holding
            // the alias. Its returned view can outlive that slot while the input lives.
            if returned(operand) {
                self.retain_trusted_borrow_owners(&mut owners, operand, &place, origins);
            }

            let value = input;

            if retain_carriers {
                carriers.push(value.clone());
            }

            if let Some(parameter) = parameter {
                arguments.insert(
                    bray_bound_tree::BoundReferenceTarget::Surface(parameter.into()).into(),
                    value.clone(),
                );
            }

            let ordinal = ordinal
                .checked_add(u32::from(call.receiver().is_some()))
                .expect("selected argument order must fit");

            arguments.insert(
                crate::ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(ordinal)),
                value,
            );
        }

        if let Some(receiver) = call.receiver()
            && matches!(
                receiver.mode(),
                bray_symbols::ReceiverMode::Shared | bray_symbols::ReceiverMode::Mutable
            )
            && let Some(input) = call.input_site(expression, bray_symbols::DependencySubjectRoot::Receiver)
            && let Some(place) = self.input_place(input)
        {
            let reference =
                bray_bound_tree::BoundReferenceTarget::Surface(receiver.parameter().into());

            let value = self.current_input(state, input);

            if returned(input) {
                self.retain_trusted_borrow_owners(
                    &mut owners,
                    input,
                    &place,
                    origins,
                );
            }

            if retain_carriers {
                carriers.push(value.clone());
            }

            arguments.insert(reference.into(), value.clone());

            arguments.insert(
                crate::ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0)),
                value,
            );
        }

        (carriers, owners)
    }

    fn retain_trusted_borrow_owners(
        &self,
        owners: &mut BTreeSet<crate::ExecutionPlace>,
        expression: bray_bound_tree::BoundExecutionSite,
        place: &crate::ExecutionPlace,
        origins: &BTreeMap<bray_bound_tree::BoundExecutionSite, BTreeSet<bray_bound_tree::StoragePath>>,
    ) {
        if let Some(paths) = origins.get(&expression) {
            owners.extend(
                paths
                    .iter()
                    .filter_map(|path| crate::ExecutionPlace::storage_path(self.storage, path)),
            );
        } else {
            // Provisional execution analysis has no completed storage transition yet.
            owners.insert(place.clone());
        }
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

        let Some(mut entry) = state
            .entries
            .get(&invocation)
            .filter(|entry| {
                // Normal completion establishes ordinary requirements, including runtime
                // checks. Trusted requirements still need authority at the call boundary.
                entry.trusted_boundary
                    || self.trusted_requirements_proven(entry, &contract.requirements)
            })
            .cloned()
        else {
            return;
        };

        if entry.pending_execution
            || !contract.completes
                && matches!(result, ExecutionCondition::Expression(expression) if invocation == expression.into())
        {
            return;
        }

        if !assume_normal_entry(&mut entry, &contract.preconditions) {
            return;
        }

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

        let inferred = self
            .dependencies
            .calls()
            .get(&invocation)
            .into_iter()
            .flat_map(|dependencies| {
                dependencies.carriers().iter().map(|carrier| {
                    let condition = crate::execution_condition_from_type_term(
                        self.request.semantic_values(),
                        carrier.guarantee(),
                        dependencies.input_count(),
                    )
                    .expect("checked carrier condition decodes in the same semantic store");

                    (
                        capture(&condition),
                        super::carriers::result_projection(self.dependencies, carrier.path()),
                    )
                })
            })
            .collect::<Vec<_>>();

        let guarantees = contract
            .guarantees
            .iter()
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
                        .any(|(condition, _)| *condition == guarantee)
                {
                    None
                } else {
                    Some((guarantee, subject.clone()))
                }
            })
            .collect::<Vec<_>>();

        subjects.extend(inferred);

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
            && let bray_bound_tree::BoundExecutionSite::Node(
                bray_bound_tree::AnyBoundNodeId::Expression(expression),
            ) = invocation
            && let Some(bray_bound_tree::SemanticSelection::Call(call)) =
                self.semantics.selections().expression(expression)
        {
            // Completion clauses observe final contents, independently of trusted authority.
            self.prepare_call_observations(
                state,
                expression,
                call,
                &mut arguments,
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

        for (guarantee, subject) in &subjects {
            let guarantee = instantiate(guarantee);

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
                self.carry_result_guarantee(
                    state,
                    &condition,
                    &guarantee,
                    !contract.transferred_inputs.is_empty(),
                );
            }

            for carrier in &carriers {
                if *carrier != ExecutionCondition::Unknown && condition.observes(carrier) {
                    state.carry_guarantee(carrier.clone(), guarantee.clone());
                }
            }
        }

        for guarantee in &guarantees {
            let guarantee = instantiate(guarantee);

            guarantee.assume(true, &mut state.trusted_assumptions);
        }

        if !guarantees.is_empty() && !owners.is_empty() {
            let dependencies = state
                .witness_dependencies
                .entry(result.owner_identity().clone())
                .or_default();

            // The owning storage transition already resolved the required lifetimes.
            // These anchors need not name the physical storage reached by the borrow.
            dependencies.extend(owners);
        }

        for postcondition in &postconditions {
            let condition = instantiate(postcondition);

            condition.assume(true, &mut state.assumptions);
        }

        if matches!(invocation, bray_bound_tree::BoundExecutionSite::CallInput { .. }
            | bray_bound_tree::BoundExecutionSite::Iterate(_) | bray_bound_tree::BoundExecutionSite::Next(_)) {
            let place = self.input_place(invocation).expect("completed input has its place");
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
    ) {
        state.carry_guarantee(carrier.clone(), guarantee.clone());

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
}

fn assume_normal_entry(
    entry: &mut crate::ExecutionCallEvidence,
    preconditions: &[ExecutionCondition],
) -> bool {
    let equalities = ExecutionCondition::equalities(&entry.assumptions, None);

    let preconditions = preconditions
        .iter()
        .map(|condition| {
            condition
                .substitute(
                    &|place| {
                        place
                            .value_in(&entry.arguments)
                            .unwrap_or(ExecutionCondition::Unknown)
                    },
                    &ExecutionCondition::Unknown,
                    &mut { ExecutionCondition::WORK_LIMIT },
                )
                .with_equalities(&equalities)
        })
        .collect::<Vec<_>>();

    // A disproven entry cannot complete normally. Unknown ordinary requirements
    // can pass their runtime checks; capture that entry evidence for completion guards.
    if preconditions.iter().any(|condition| {
        condition.prove(&entry.assumptions, &mut { ExecutionCondition::WORK_LIMIT }) == Some(false)
    }) {
        return false;
    }

    for condition in preconditions {
        condition.assume(true, &mut entry.assumptions);
    }

    true
}
