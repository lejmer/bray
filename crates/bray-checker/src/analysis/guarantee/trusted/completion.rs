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
            .filter_map(|place| place.value_in(&state.current))
            .collect::<Vec<_>>();

        for place in expired {
            state.invalidate_trusted_place(place);
        }

        state
            .current
            .retain(|place, _| !expired.iter().any(|expired| expired.contains(place)));

        let mut pending = values;

        while let Some(value) = pending.pop() {
            if state.result.contains_value(&value)
                || state
                    .current
                    .values()
                    .chain(state.pending_results.values())
                    .any(|live| live.contains_value(&value))
            {
                continue;
            }

            if let ExecutionCondition::Constructed(_, components) = value {
                pending.extend(components.iter().map(|(_, value)| value.clone()));

                continue;
            }

            state.invalidate_trusted(&value);
        }
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
                    expression,
                    kind: AnalysisSuspensionKind::Await,
                } = operation.kind()
                    && let Some(BoundExpression::Await(awaited)) =
                        self.request.view().expression(expression)
                    && let ExecutionCondition::Expression(source) =
                        self.value(state, awaited.operand())
                {
                    if let Some(entry) = state.entries.get_mut(&source) {
                        // Only evidence still valid when execution begins can justify this completion.
                        entry.pending_execution = false;
                    }

                    completions.push((source, expression));
                }
            }
        }

        // A resumed body can have executed opaque code and changed owner epochs.
        state.invalidate_cleanup();

        for (source, expression) in completions {
            self.complete_trusted_call(state, source, expression);
        }
    }

    fn prepare_trusted_borrows(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
        call: &bray_bound_tree::SelectedCall,
        arguments: &mut BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
    ) -> (Vec<ExecutionCondition>, BTreeSet<crate::ExecutionPlace>) {
        let mut carriers = Vec::new();
        let mut owners = BTreeSet::new();

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

            owners.insert(place.clone());

            let preserves = trusted_call_preserves_inputs(call);

            let value = if preserves {
                self.value(state, *operand)
            } else {
                ExecutionCondition::PostState(expression, reference)
            };

            if !preserves {
                state.assign(place, value.clone());
            }

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

            owners.insert(place.clone());

            let preserves = trusted_call_preserves_inputs(call);

            let value = if preserves {
                self.value(state, receiver.expression())
            } else {
                ExecutionCondition::PostState(expression, reference)
            };

            if !preserves {
                state.assign(place, value.clone());
            }

            carriers.push(value.clone());
            arguments.insert(reference.into(), value.clone());

            arguments.insert(
                crate::ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0)),
                value,
            );
        }

        (carriers, owners)
    }

    pub(in crate::analysis::guarantee) fn complete_trusted_call(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
        result_expression: BoundExpressionId,
    ) {
        let Some(contract) = self
            .request
            .trusted_contracts()
            .and_then(|contracts| contracts.calls.get(&expression))
        else {
            return;
        };

        let Some(entry) = state
            .entries
            .get(&expression)
            .filter(|entry| {
                (entry.trusted_boundary
                    || self.trusted_requirements_proven(entry, &contract.requirements))
                    && entry.proves(&contract.preconditions)
            })
            .cloned()
        else {
            return;
        };

        if !contract.completes && expression == result_expression {
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

        let result = ExecutionCondition::Expression(result_expression);
        let mut arguments = entry.arguments;

        let (carriers, owners) = if !contract.guarantees.is_empty()
            && let Some(bray_bound_tree::SemanticSelection::Call(call)) =
                self.semantics.selections().expression(expression)
        {
            self.prepare_trusted_borrows(state, expression, call, &mut arguments)
        } else {
            Default::default()
        };

        for (guarantee, subject) in &subjects {
            let guarantee = guarantee.substitute(
                &|place| {
                    place
                        .value_in(&arguments)
                        .unwrap_or(ExecutionCondition::Unknown)
                },
                &result,
                &mut { ExecutionCondition::WORK_LIMIT },
            );

            // A guarantee discharged solely by its ordinary guard carries no trusted authority.
            if guarantee.prove(&state.assumptions, &mut { ExecutionCondition::WORK_LIMIT })
                == Some(true)
            {
                continue;
            }

            let condition = subject.substitute(
                &|place| {
                    place
                        .value_in(&arguments)
                        .unwrap_or(ExecutionCondition::Unknown)
                },
                &result,
                &mut { ExecutionCondition::WORK_LIMIT },
            );

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
            guarantee
                .substitute(
                    &|place| {
                        place
                            .value_in(&arguments)
                            .unwrap_or(ExecutionCondition::Unknown)
                    },
                    &result,
                    &mut { ExecutionCondition::WORK_LIMIT },
                )
                .assume(true, &mut state.trusted_assumptions);
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
            let condition = postcondition.substitute(
                &|place| {
                    place
                        .value_in(&arguments)
                        .unwrap_or(ExecutionCondition::Unknown)
                },
                &result,
                &mut { ExecutionCondition::WORK_LIMIT },
            );

            condition.assume(true, &mut state.assumptions);
        }

        state.expressions.insert(result_expression, result);
    }
}

fn trusted_call_preserves_inputs(call: &bray_bound_tree::SelectedCall) -> bool {
    call.phase_behaviors()
        .invocation()
        .execution_properties()
        .contains(&bray_symbols::ExecutionProperty::Pure)
        || matches!(
            call.implementation_hook(),
            Some(
                bray_compiler_known::ImplementationHook::AddressOf
                    | bray_compiler_known::ImplementationHook::AddressOfMut
                    | bray_compiler_known::ImplementationHook::RawBufferCapacity
                    | bray_compiler_known::ImplementationHook::RawBufferInitializedCount
                    | bray_compiler_known::ImplementationHook::RawBufferPointer
                    | bray_compiler_known::ImplementationHook::RawBufferSparePointer
            )
        )
}
