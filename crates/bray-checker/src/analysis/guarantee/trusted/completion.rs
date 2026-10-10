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
        contract: &crate::TrustedCallContract,
        entry: &crate::ExecutionCallEvidence,
    ) {
        let Some(place) = place else {
            return;
        };

        // Cleanup keeps the receiver's owner, but its fields describe a new initialized state.
        for value in entry.arguments.values() {
            state
                .assumptions
                .retain(|(condition, _)| !condition.observes(value));
        }

        let root = ExecutionCondition::Input(place.clone());

        state.assign(place, root.clone());

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
            .map(|(place, _)| place.root)
            .collect::<BTreeSet<_>>();

        state.invalidate_trusted_roots(&expired_roots);

        state
            .current
            .retain(|place, _| !expired_roots.contains(&place.root));

        // Index retained owners once instead of scanning all live aggregates for
        // every expired value and each of its owned components.
        let retained = std::iter::once(&state.result)
            .chain(state.current.values())
            .chain(state.pending_results.values())
            .flat_map(ExecutionCondition::owned_values)
            .cloned()
            .collect::<BTreeSet<_>>();

        let mut pending = values;
        let mut visited = BTreeSet::new();
        let mut retired = Vec::new();

        while let Some(value) = pending.pop() {
            if retained.contains(&value) || !visited.insert(value.clone()) {
                continue;
            }

            if let ExecutionCondition::Constructed(_, components) = value {
                pending.extend(components.iter().map(|(_, value)| value.clone()));

                continue;
            }

            retired.push(value);
        }

        state.invalidate_trusted_values(retired);
    }

    pub(in crate::analysis::guarantee) fn complete_trusted_await(
        &self,
        state: &mut ExecutionState,
        edge: &super::super::super::model::AnalysisEdge,
    ) {
        use super::super::super::model::{
            AnalysisEdgeKind, AnalysisOperationKind, AnalysisSuspensionKind,
        };

        use bray_bound_tree::BoundExpression;

        if edge.kind() != AnalysisEdgeKind::Resume {
            return;
        }

        let suspended = self
            .graph
            .block(edge.source())
            .expect("resume edge has a suspended source block");

        let suspended_from = suspended
            .predecessors()
            .iter()
            .filter_map(|id| self.graph.edge(*id))
            .find(|incoming| incoming.kind() == AnalysisEdgeKind::Suspension);

        let mut completions = Vec::new();

        if let Some(block) = suspended_from.and_then(|edge| self.graph.block(edge.source())) {
            for operation in block
                .operations()
                .iter()
                .filter_map(|id| self.graph.operation(*id))
            {
                if let AnalysisOperationKind::Suspension {
                    occurrence,
                    kind: AnalysisSuspensionKind::ScopedCall,
                } = operation.kind()
                    && let Some(entry) = state.entries.get_mut(&occurrence)
                {
                    // The resumed invocation keeps only evidence that survived deferred creation.
                    entry.pending_execution = false;
                }

                if let AnalysisOperationKind::Suspension {
                    occurrence:
                        bray_bound_tree::BoundExecutionSite::Node(
                            bray_bound_tree::AnyBoundNodeId::Expression(expression),
                        ),
                    kind: AnalysisSuspensionKind::Await,
                } = operation.kind()
                    && let Some(BoundExpression::Await(awaited)) =
                        self.request.view().expression(expression)
                    && let ExecutionCondition::Expression(source) =
                        self.value(state, awaited.operand())
                {
                    if let Some(entry) = state.entries.get_mut(&source.into()) {
                        // Only evidence still valid when execution begins can justify this completion.
                        entry.pending_execution = false;
                    }

                    completions.push((source, expression));
                }
            }
        }

        // A resumed body can have executed opaque code and changed owner epochs.
        state.invalidate_cleanup(None);

        for (source, expression) in completions {
            self.complete_trusted_call(
                state,
                source.into(),
                ExecutionCondition::Expression(expression),
            );
        }
    }

    fn prepare_trusted_borrows(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
        call: &bray_bound_tree::SelectedCall,
        preserves_inputs: bool,
        arguments: &mut BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
    ) -> (Vec<ExecutionCondition>, BTreeSet<crate::ExecutionPlace>) {
        let mut carriers = Vec::new();
        let mut owners = BTreeSet::new();
        let mut replacements = BTreeMap::new();
        let mut assignments = BTreeMap::new();

        let preserves = preserves_inputs
            || super::super::super::storage_invalidation::call_preserves_storage(
                call,
                &self.copied_types,
            );

        let mut complete_input =
            |input: ExecutionCondition, place: &crate::ExecutionPlace, reference| {
                if preserves {
                    input
                } else {
                    let post = ExecutionCondition::PostState(expression, reference);

                    // The promise describes the same borrowed storage after the call. Keep
                    // the borrow around the new referent, including through local aliases.
                    match input {
                        ExecutionCondition::Borrowed(referent) => {
                            let post = replacements
                                .entry(referent.as_ref().clone())
                                .or_insert(post);

                            if let ExecutionCondition::Input(place) = referent.as_ref() {
                                assignments.insert(place.clone(), post.clone());
                            }

                            post.clone().borrowed()
                        }
                        ExecutionCondition::Unknown => ExecutionCondition::Unknown,
                        input => {
                            let place = match &input {
                                ExecutionCondition::Input(place) => place.clone(),
                                _ => place.clone(),
                            };

                            let post = replacements.entry(input).or_insert(post).clone();

                            assignments.insert(place, post.clone());

                            post
                        }
                    }
                }
            };

        for argument in call.arguments() {
            let bray_bound_tree::SelectedArgument::Explicit {
                expression: operand,
                ordinal,
                parameter,
                conversion,
                ..
            } = argument
            else {
                continue;
            };

            if !matches!(
                self.request
                    .semantic_values()
                    .type_data(conversion.target_type())
                    .as_ref(),
                bray_symbols::TypeData::Borrow { .. }
            ) {
                continue;
            }

            let Some(place) = crate::execution_guarantees::expression_place(
                self.request.unit(),
                self.semantics,
                self.request.semantic_values(),
                *operand,
            ) else {
                // Indexed storage has no scalar field path. Its enclosing owner still bounds
                // the lifetime of a returned address or capability.
                for access in self.storage.expression_plans(*operand) {
                    if let Some(owner) =
                        crate::ExecutionPlace::storage_owner(self.storage, access.access())
                    {
                        owners.insert(owner);
                    }
                }

                continue;
            };

            let reference = parameter
                .map(|parameter| bray_bound_tree::BoundReferenceTarget::Surface(parameter.into()))
                .or_else(|| place.reference())
                .expect("borrowed call input retains its binding identity");

            let input = self.value(state, *operand);

            // A local borrow alias retains the referent, rather than the slot holding
            // the alias. Its returned view can outlive that slot while the input lives.
            self.retain_trusted_borrow_owners(&mut owners, *operand, &input, &place);

            let value = complete_input(input, &place, reference);

            carriers.push(value.clone());

            if let Some(parameter) = parameter {
                arguments.insert(
                    bray_bound_tree::BoundReferenceTarget::Surface((*parameter).into()).into(),
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
            && let Some(place) = crate::execution_guarantees::expression_place(
                self.request.unit(),
                self.semantics,
                self.request.semantic_values(),
                receiver.expression(),
            )
        {
            let reference =
                bray_bound_tree::BoundReferenceTarget::Surface(receiver.parameter().into());

            let input = self.value(state, receiver.expression());

            self.retain_trusted_borrow_owners(&mut owners, receiver.expression(), &input, &place);

            let value = complete_input(input, &place, reference);

            carriers.push(value.clone());
            arguments.insert(reference.into(), value.clone());

            arguments.insert(
                crate::ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0)),
                value,
            );
        }

        // Update reached owners and aliases together in one pass. Entry evidence and
        // old facts stay immutable; only the declared completion promises restore authority.
        if !replacements.is_empty() {
            for value in state.current.values_mut() {
                *value = value.with_joined_values(&replacements);
            }
        }

        // Parameters initially have implicit Input values. Materialize their new
        // state and retire stale field snapshots beneath each updated place.
        if !assignments.is_empty() {
            let changed = assignments.keys().cloned().collect::<BTreeSet<_>>();

            state.current.retain(|place, _| {
                !(0..=place.projections.len()).any(|length| {
                    let mut prefix = place.clone();

                    prefix.projections = place.projections[..length].into();

                    changed.contains(&prefix)
                })
            });

            state.current.extend(assignments);
        }

        (carriers, owners)
    }

    fn retain_trusted_borrow_owners(
        &self,
        owners: &mut BTreeSet<crate::ExecutionPlace>,
        expression: BoundExpressionId,
        input: &ExecutionCondition,
        place: &crate::ExecutionPlace,
    ) {
        // Retained borrow aliases name local slots, but their capabilities reach the
        // original storage. Use the same resolved roots as ordinary borrow checking.
        let mut retained = false;

        let borrowed = self.storage.expression_plans(expression).any(|plan| {
            matches!(
                plan.purpose(),
                bray_bound_tree::StorageAccessPurpose::Borrow(_)
            )
        });

        for plan in self.storage.expression_plans(expression).filter(|plan| {
            !borrowed
                || matches!(
                    plan.purpose(),
                    bray_bound_tree::StorageAccessPurpose::Borrow(_)
                )
        }) {
            for root in self.storage.retained_roots(plan.access()) {
                if let Some(owner) = crate::ExecutionPlace::storage_identity(self.storage, root) {
                    owners.insert(owner);
                    retained = true;
                }
            }
        }

        if !retained {
            owners.insert(match input {
                ExecutionCondition::Input(owner) => owner.clone(),
                _ => place.clone(),
            });
        }
    }

    pub(in crate::analysis::guarantee) fn complete_trusted_call(
        &self,
        state: &mut ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
        result: ExecutionCondition,
    ) {
        let Some(contract) = self
            .request
            .trusted_contracts()
            .and_then(|contracts| contracts.calls.get(&invocation))
        else {
            return;
        };

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

        if !contract.completes
            && matches!(result, ExecutionCondition::Expression(expression) if invocation == expression.into())
        {
            return;
        }

        let equalities = ExecutionCondition::equalities(&entry.assumptions, None);

        let preconditions = contract
            .preconditions
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
            condition.prove(&entry.assumptions, &mut { ExecutionCondition::WORK_LIMIT })
                == Some(false)
        }) {
            return;
        }

        for condition in preconditions {
            condition.assume(true, &mut entry.assumptions);
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

        let guarantees = contract.guarantees.iter().map(capture).collect::<Vec<_>>();

        let subjects = contract
            .witness_subjects
            .iter()
            .map(|(guarantee, subject)| (capture(guarantee), subject))
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

        let mut arguments = entry.arguments;

        state.invalidate_trusted_values(contract.transferred_inputs.iter().map(|input| {
            // The transferred owner values retain their immutable entry observations.
            arguments
                .get(&crate::ExecutionPlace::from(*input))
                .expect("selected storage formation retains its transferred input")
                .clone()
        }));

        let (carriers, owners) = if !contract.guarantees.is_empty()
            && let bray_bound_tree::BoundExecutionSite::Node(
                bray_bound_tree::AnyBoundNodeId::Expression(expression),
            ) = invocation
            && let Some(bray_bound_tree::SemanticSelection::Call(call)) =
                self.semantics.selections().expression(expression)
        {
            // The entry above already proves the valid domain of this preservation promise.
            self.prepare_trusted_borrows(
                state,
                expression,
                call,
                contract.preserves_inputs,
                &mut arguments,
            )
        } else {
            Default::default()
        };

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

            if contract.result_is_witness && condition.observes(&result) {
                state.witness_carriers.insert(result.clone());
            }

            for carrier in &carriers {
                if *carrier != ExecutionCondition::Unknown && condition.observes(carrier) {
                    state.witness_carriers.insert(carrier.clone());
                }
            }
        }

        for guarantee in &guarantees {
            let guarantee = instantiate(guarantee);

            if contract.result_is_witness {
                let mut pending = vec![&guarantee];

                while let Some(condition) = pending.pop() {
                    match condition {
                        ExecutionCondition::Trusted(condition) => pending.push(condition),
                        ExecutionCondition::Operation(_, operands) => {
                            pending.extend(operands.iter())
                        }
                        ExecutionCondition::Predicate(predicate, _, _)
                            if Some(*predicate) == self.owned_allocation
                                && (condition.observes(&result)
                                    || !contract.transferred_inputs.is_empty()) =>
                        {
                            // The allocation condition follows the complete linear value, not copies of its fields.
                            state
                                .allocation_owners
                                .insert(condition.clone(), result.clone());

                            state.witness_carriers.insert(result.clone());
                        }
                        _ => {}
                    }
                }
            }

            guarantee.assume(true, &mut state.trusted_assumptions);
        }

        if !contract.guarantees.is_empty() && !owners.is_empty() {
            let dependencies = state
                .witness_dependencies
                .entry(result.clone())
                .or_default();

            // The ordinary dependency checker validates the reached storage and borrow capability.
            // Keep those same input owners attached to the trusted conditions on the returned view.
            dependencies.extend(owners);
        }

        for postcondition in &postconditions {
            let condition = instantiate(postcondition);

            condition.assume(true, &mut state.assumptions);
        }

        if let ExecutionCondition::Expression(expression) = result {
            state.expressions.insert(expression, result);
        }
    }
}
