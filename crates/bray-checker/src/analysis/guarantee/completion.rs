use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{BoundExpressionId, BoundReferenceTarget};
use bray_symbols::TypeData;

use super::super::id::AnalysisOperationId;
use super::super::model::{AnalysisCallPhase, AnalysisOperation, AnalysisOperationKind, AnalysisScopeExitPhase};
use super::cleanup::ReachedScopeCleanup;
use super::flow::{ExecutionFlow, ExecutionFlowDomain};
use super::state::ExecutionState;
use crate::{CheckerRequestContext, ExecutionCondition, ExecutionPlace};

impl<C: CheckerRequestContext + ?Sized> ExecutionFlow<'_, '_, C> {
    pub(super) fn candidate(
        &self,
        memory: &bray_bound_tree::CheckedMemoryOperations,
        obligation: Option<crate::ExecutionObligation>,
        property: Option<crate::ExecutionProperty>,
    ) -> (crate::ExecutionCandidate, BTreeMap<AnalysisOperationId, ReachedScopeCleanup>) {
        let admission = matches!(
            obligation,
            Some(crate::ExecutionObligation::Property(
                crate::ExecutionProperty::Total,
                _
            ))
        );

        let collect_cleanup = admission
            || matches!(
                obligation,
                Some(
                    crate::ExecutionObligation::Termination
                        | crate::ExecutionObligation::FiniteIteration
                )
            );

        let mut candidate = crate::ExecutionCandidate::default();
        let mut reached_cleanup = BTreeMap::<_, ReachedScopeCleanup>::new();
        let mut dependencies = BTreeSet::new();
        let mut results = BTreeSet::new();

        let normal_exit_blocks = self
            .domain
            .graph
            .completion_exits()
            .map(|exit| exit.block())
            .collect::<BTreeSet<_>>();

        for block in self.domain.graph.blocks() {
            let operations = || block.operations().iter().map(|id| {
                self.domain.graph.operation(*id).expect("proof block retains its operation")
            });

            let only_operation = match block.operations() {
                [id] => self.domain.graph.operation(*id),
                _ => None,
            };

            let use_output = (block.operations().is_empty()
                || only_operation.is_some_and(|operation| self.can_read_candidate_output(operation, collect_cleanup, property.is_some())))
                && self.output_states(block.id()).next().is_some();

            let reached = self.output_states(block.id()).filter(|_| use_output)
                .chain(self.input_states(block.id()).filter(|_| !use_output))
                .map(Cow::Borrowed);

            if normal_exit_blocks.contains(&block.id()) {
                results.extend(self.output_states(block.id()).map(|state| self.domain.returned_value(state).known_variant()));
            }

            'states: for mut state in reached {
                if use_output
                    && !admission
                    && let Some(operation) = only_operation
                    && let AnalysisOperationKind::Call { invocation, phase: AnalysisCallPhase::Completion } = operation.kind()
                    && let Some(evidence) = state.entries.get(&invocation)
                {
                    candidate.calls.entry(invocation).or_default().insert(std::sync::Arc::clone(evidence));
                }

                for operation in operations().filter(|_| !use_output) {
                    let scope_property = property.filter(|_| matches!(operation.kind(), AnalysisOperationKind::ScopeExit { .. }));

                    let observer: &mut super::cleanup::Observer<'_> = &mut |observation| {
                        self.domain.observe_candidate_cleanup(
                            &mut candidate, &mut reached_cleanup, operation, scope_property, collect_cleanup, observation,
                        );
                    };

                    if !self.domain.operation(
                        state.to_mut(),
                        operation,
                        block.id(),
                        (collect_cleanup || scope_property.is_some()).then_some(observer),
                    ) {
                        continue 'states;
                    }

                    if !admission
                        && let super::super::model::AnalysisOperationKind::Call {
                            invocation,
                            phase: super::super::model::AnalysisCallPhase::Completion,
                            ..
                        } = operation.kind()
                        && let Some(evidence) = state.entries.get(&invocation)
                    {
                        // Preservation and completion proofs use successful entry checks.
                        // Totality keeps the pre-admission snapshot, including fallible checks.
                        candidate.calls.entry(invocation).or_default().insert(std::sync::Arc::clone(evidence));
                    }
                }

                for operation in operations() {
                    let occurrence = operation.kind().occurrence();

                    let evidence = match &mut state {
                        Cow::Borrowed(state) => state.entries.get(&occurrence).cloned(),
                        Cow::Owned(state) => state.entries.remove(&occurrence),
                    };

                    if let Some(evidence) = evidence {
                        if !occurrence
                                .expression()
                                .and_then(|expression| memory.operation(expression))
                                .is_some_and(|operation| {
                                    crate::memory::execution::guarantees(
                                        operation.kind(),
                                        crate::ExecutionProperty::Pure,
                                    )
                                })
                            && self.domain.admitted_pure(occurrence, &evidence)
                        {
                            super::operation::collect_invocation_dependency(
                                self.domain.semantics.selections(),
                                occurrence,
                                crate::ExecutionProperty::Pure,
                                &mut candidate.dependencies,
                            );
                        }

                        candidate.calls.entry(occurrence).or_default().insert(evidence);
                    }
                }

                dependencies.extend(state.completion_dependencies.iter().cloned());
            }
        }

        candidate.completion_dependencies = dependencies.into_iter().collect();

        candidate.result_variant = if results.len() == 1 {
            results.pop_first().flatten()
        } else {
            None
        };

        (candidate, reached_cleanup)
    }

    fn can_read_candidate_output(&self, operation: &AnalysisOperation, collect_cleanup: bool, certify_scopes: bool) -> bool {
        // Candidate evidence retains the joined entry on operations that select
        // output rows or complete deferred work.
        if matches!(operation.kind(), AnalysisOperationKind::Call { phase: AnalysisCallPhase::Attempt, .. })
            || operation.kind().awaited_expression().is_some()
        {
            return false;
        }

        let Some(flow) = self.domain.checked_body.map(bray_bound_tree::CheckedBodySemantics::storage_flow) else {
            return true;
        };

        let has_actions = flow.cleanup_entry(operation.id(), None).is_some_and(|entry| !entry.orders().is_empty());

        match operation.kind() {
            AnalysisOperationKind::ScopeExit { phase: AnalysisScopeExitPhase::TaskCancellationBroadcast, .. } if certify_scopes => {
                !has_actions
            }
            AnalysisOperationKind::ScopeExit { scope, exit, phase: AnalysisScopeExitPhase::LifecycleResolution, .. } if collect_cleanup || certify_scopes => {
                !has_actions
                    && flow.outcome_mutation(bray_bound_tree::StorageMutation::ScopeCleanupOrder {
                        operation: operation.id(),
                        point: bray_bound_tree::StorageExitPoint::new(scope, exit),
                    }).is_none()
            }
            AnalysisOperationKind::Bound(bray_bound_tree::AnyBoundNodeId::Expression(_))
            | AnalysisOperationKind::AssignmentInstall(_) if collect_cleanup => !has_actions,
            _ => true,
        }
    }
}

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {

    fn call_argument_value(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
        conversion: &bray_bound_tree::SelectedConversion,
    ) -> ExecutionCondition {
        let value = self.operand_value(state, expression);

        converted_argument(self.request.semantic_values(), value, conversion)
    }

    fn implicit_call_entry(
        &self,
        state: &ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
        selection: &bray_bound_tree::SemanticSelection,
    ) -> Option<crate::ExecutionCallEvidence> {
        let contract = self.request.trusted_contracts()?.calls.get(&invocation)?;
        let mut arguments = BTreeMap::new();
        let mut transferred = BTreeSet::new();

        let plan = self
            .storage
            .implicit_input_plan(invocation)
            .expect("implicit invocation retains its input transfer");

        let consumes = !matches!(
            plan.purpose(),
            bray_bound_tree::StorageAccessPurpose::Borrow(_)
        );

        assert_eq!(
            contract.arguments.len(),
            1,
            "implicit invocation has one actual input"
        );

        for (input, source) in &contract.arguments {
            let input = ExecutionPlace::from(*input);
            let ordinal = ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0));

            let value = if matches!(invocation, bray_bound_tree::BoundExecutionSite::ScopedExit(_)) {
                // Enter can return a fallible wrapper. Exit transfers the
                // current capability reached by the checked exit access.
                let place = self.storage.logical_path(plan.access())
                    .and_then(|path| ExecutionPlace::storage_path(self.storage, path))
                    .expect("scoped exit retains its checked capability place");

                let value = state.current.slot(&place)
                    .filter(|slot| slot.availability != super::current::ValueAvailability::Absent)
                    .map_or_else(ExecutionCondition::unknown, |slot| slot.value);

                self.refresh_captured_references(state, value, None)
            } else {
                self.current_input(state, *source)
            };

            let value = if consumes {
                self.copied_access_value(ExecutionCondition::post_state(
                    bray_bound_tree::StorageMutation::Operation(invocation), ExecutionCondition::new(crate::ExecutionConditionKind::Input(ordinal.clone())),
                ), value, plan)
            } else {
                value.borrowed()
            };

            if consumes {
                transferred.extend([input.clone(), ordinal.clone()]);
            }

            arguments.insert(input, value.clone());
            arguments.insert(ordinal, value);
        }

        let trusted_assumptions =
            self.call_trusted_assumptions(state, invocation, &arguments, &transferred);

        let pending_execution = matches!(selection,
            bray_bound_tree::SemanticSelection::ScopedUse(scoped)
                if matches!(scoped.invocation(invocation).2, bray_bound_tree::BoundCallResult::LazyFuture(_)));

        Some(crate::ExecutionCallEvidence {
            failure_rows: None,
            outputs: Default::default(),
            implementation_witnesses: Default::default(),
            witness_inputs: Default::default(),
            witness_allocations: Default::default(),
            extracted_values: Default::default(),
            owned_inputs: if self.storage.effective_purpose(plan)
                == bray_bound_tree::StorageAccessPurpose::Move
            {
                [bray_symbols::SymbolOrdinal::new(0)].into()
            } else {
                Default::default()
            },
            addresses: Default::default(),
            borrow_owners: Default::default(),
            anonymous_target: None,
            trusted_boundary: !state.trust_boundaries.is_empty(),
            pending_execution,
            arguments,
            assumptions: state.assumptions.clone(),
            trusted_assumptions,
        })
    }

    fn operation_call_entry(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
        selection: &bray_bound_tree::SemanticSelection,
    ) -> Option<crate::ExecutionCallEvidence> {
        let bray_bound_tree::SemanticSelection::Operation(operation) = selection else { return None; };

        let occurrence = expression.into();
        let contract = self.request.trusted_contracts().and_then(|inputs| inputs.calls.get(&occurrence));
        let dependencies = self.dependencies.calls().get(&occurrence);

        if contract.is_none() && dependencies.is_none() { return None; }

        let mut arguments = contract.into_iter().flat_map(|contract| &contract.arguments)
            .map(|(input, expression)| {
                (
                    ExecutionPlace::from(*input),
                    self.invocation_value(state, *expression),
                )
            })
            .collect::<BTreeMap<_, _>>();

        for (root, input) in crate::dependency::operation_inputs(self.request.view(), expression, operation) {
            use crate::dependency::OperationInput;

            let Some(ordinal) = selection.input_ordinal(root) else { continue; };

            let value = input.site().map_or_else(ExecutionCondition::unknown, |site| self.current_input(state, site));

            let value = match input {
                OperationInput::Borrow(..) => value.borrowed(),
                OperationInput::Nullable(Some(_)) => value.present(),
                _ => value,
            };

            arguments.insert(ExecutionPlace::argument(ordinal), value);
        }

        if let bray_bound_tree::SemanticSelection::Operation(
            bray_bound_tree::SelectedOperation::Construction(construction),
        ) = selection
        {
            for input in construction.inputs() {
                if let bray_bound_tree::SelectedConstructionInput::Explicit {
                    input,
                    conversion,
                    ..
                } = input
                {
                    let place = BoundReferenceTarget::Surface(input.symbol()).into();

                    if let Some(value) = arguments.get_mut(&place) {
                        *value = std::mem::replace(value, ExecutionCondition::unknown()).coerce(
                            self.request.semantic_values(),
                            conversion.source_type(),
                            conversion.target_type(),
                        );
                    }
                }
            }
        }

        Some(crate::ExecutionCallEvidence {
            failure_rows: None,
            outputs: Default::default(),
            implementation_witnesses: Default::default(),
            witness_inputs: Default::default(),
            witness_allocations: Default::default(),
            extracted_values: Default::default(),
            owned_inputs: Default::default(),
            addresses: Default::default(),
            borrow_owners: Default::default(),
            anonymous_target: None,
            trusted_boundary: !state.trust_boundaries.is_empty(),
            pending_execution: false,
            arguments,
            assumptions: state.assumptions.clone(),
            trusted_assumptions: state.call_trusted_assumptions([]),
        })
    }

    pub(super) fn call_entry(
        &self,
        state: &ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
        captures: &[bray_bound_tree::StorageValueCapture],
    ) -> Option<crate::ExecutionCallEvidence> {
        let mut entry = self.awaited_entry(state, invocation)
            .or_else(|| self.invocation_entry(state, invocation))?;

        self.bind_entry_values(state, invocation, &mut entry, captures);
        entry.borrow_owners = self.admitted_borrow_owners(invocation, &entry);
        if !entry.pending_execution && let Some(call) = self.dependencies.calls().get(&invocation) {
            entry.extracted_values = self.extracted_input_values(call.storage(), call.captured_values(), &entry);
        }

        entry.witness_inputs = state
            .owned_witnesses(entry.witness_sources())
            .filter(|(_, witness)| witness.available)
            .map(|(owner, witness)| (owner.clone(), std::sync::Arc::clone(witness)))
            .collect();

        entry.witness_allocations = state.allocation_owners.iter()
            .filter(|(_, owner)| entry.witness_inputs.contains_key(*owner))
            .map(|(condition, owner)| (condition.clone(), owner.clone())).collect();

        if let Some(contract) = self
            .request
            .trusted_contracts()
            .and_then(|contracts| contracts.calls.get(&invocation))
        {
            self.preserve_integer_entry_conditions(state, &contract.preconditions, &mut entry);
        }

        entry.addresses = entry
            .arguments
            .iter()
            .filter(|(input, _)| {
                matches!(
                    input.root,
                    crate::execution_guarantees::ExecutionInput::Argument(_)
                )
            })
            .filter_map(|(input, value)| {
                let addresses = state.address_dependencies(value);

                (!addresses.is_empty()).then(|| (input.clone(), addresses))
            })
            .collect();

        Some(entry)
    }

    fn anonymous_call_target(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
        call: &bray_bound_tree::SelectedCall,
    ) -> Option<bray_bound_tree::BoundUnitKey> {
        if let Some((key, _)) = call.resolution().anonymous_body() {
            return Some(key.clone());
        }

        if !matches!(
            call.target(),
            bray_bound_tree::BoundCallableTarget::Indirect(_)
        ) {
            return None;
        }

        let Some(bray_bound_tree::BoundExpression::Call(source)) =
            self.request.view().expression(expression)
        else {
            return None;
        };

        let value = self.value(state, source.callee());

        let crate::ExecutionConditionKind::Expression(origin) = value.kind() else {
            return None;
        };

        let Some(bray_bound_tree::BoundExpression::AnonymousCallable(callable)) =
            self.request.view().expression(*origin)
        else {
            return None;
        };

        Some(callable.unit().clone())
    }

    fn invocation_entry(
        &self,
        state: &ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
    ) -> Option<crate::ExecutionCallEvidence> {
        if matches!(
            invocation,
            bray_bound_tree::BoundExecutionSite::CallInput { .. }
        ) {
            return Some(self.input_entry(state, invocation));
        }

        let expression = invocation
            .expression()
            .expect("selected invocation retains its actual expression owner");

        let selection = self.semantics.selections().expression(expression)?;

        if matches!(
            selection,
            bray_bound_tree::SemanticSelection::ScopedUse(_)
                | bray_bound_tree::SemanticSelection::Iteration(_)
        ) {
            return self.implicit_call_entry(state, invocation, selection);
        }

        let bray_bound_tree::SemanticSelection::Call(call) = selection else {
            return self.operation_call_entry(state, expression, selection);
        };

        let staged = call.stages_inputs();

        let mut arguments = if staged {
            self.staged_arguments(state, expression, selection, None)
        } else {
            BTreeMap::new()
        };

        let mut transferred = BTreeSet::new();

        for argument in call.arguments() {
            if let bray_bound_tree::SelectedArgument::Explicit {
                expression,
                parameter,
                ordinal,
                conversion,
                ..
            } = argument
            {
                let value = if staged {
                    arguments
                        .get(&ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(
                            ordinal
                                .checked_add(u32::from(call.receiver().is_some()))
                                .expect("selected argument ordinal fits"),
                        )))
                        .cloned()
                        .expect("staged argument has its value")
                } else {
                    self.call_argument_value(state, *expression, conversion)
                };

                if let Some(parameter) = parameter {
                    arguments.insert(
                        BoundReferenceTarget::Surface((*parameter).into()).into(),
                        value.clone(),
                    );
                }

                let ordinal = ordinal
                    .checked_add(u32::from(call.receiver().is_some()))
                    .expect("selected call argument ordinal must fit receiver-first order");

                let ordinal = ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(ordinal));

                if !matches!(
                    self.request
                        .semantic_values()
                        .type_data(conversion.target_type())
                        .as_ref(),
                    TypeData::Borrow { .. }
                ) {
                    if let Some(parameter) = parameter {
                        transferred
                            .insert(BoundReferenceTarget::Surface((*parameter).into()).into());
                    }

                    transferred.insert(ordinal.clone());
                }

                arguments.insert(ordinal, value);
            }
        }

        if let Some(receiver) = call.receiver() {
            let reference =
                ExecutionPlace::from(BoundReferenceTarget::Surface(receiver.parameter().into()));

            let ordinal = ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0));

            let value = if staged {
                arguments
                    .get(&ordinal)
                    .cloned()
                    .expect("staged receiver has its value")
            } else {
                self.value(state, receiver.expression())
            };

            if matches!(
                receiver.mode(),
                bray_symbols::ReceiverMode::Consuming
                    | bray_symbols::ReceiverMode::ConsumingMutable
            ) {
                transferred.insert(reference.clone());
                transferred.insert(ordinal.clone());
            }

            arguments.insert(reference, value.clone());
            arguments.insert(ordinal, value);
        }

        for argument in call.arguments() {
            if let bray_bound_tree::SelectedArgument::Default {
                parameter,
                ordinal,
                ty,
                ..
            } = argument
                && !matches!(
                    self.request.semantic_values().type_data(*ty).as_ref(),
                    TypeData::Borrow { .. }
                )
            {
                transferred.insert(BoundReferenceTarget::Surface((*parameter).into()).into());

                let ordinal = call
                    .input_ordinal(bray_symbols::DependencySubjectRoot::Parameter(
                        bray_symbols::SymbolOrdinal::new(*ordinal),
                    ))
                    .expect("default has a formal ordinal");

                transferred.insert(ExecutionPlace::argument(ordinal));
            }
        }

        // Call evidence retains the immutable entry conditions independently of subsequent mutation.
        let trusted_assumptions =
            self.call_trusted_assumptions(state, invocation, &arguments, &transferred);

        let owned_inputs = self.owned_input_ordinals(selection);

        Some(crate::ExecutionCallEvidence {
            failure_rows: None,
            outputs: Default::default(),
            implementation_witnesses: call.resolution().implementation_witnesses().into(),
            witness_inputs: Default::default(),
            witness_allocations: Default::default(),
            extracted_values: Default::default(),
            owned_inputs,
            addresses: Default::default(),
            borrow_owners: Default::default(),
            anonymous_target: self.anonymous_call_target(state, expression, call),
            trusted_boundary: !state.trust_boundaries.is_empty(),
            pending_execution: matches!(
                call.resolution().result(),
                bray_bound_tree::BoundCallResult::LazyFuture(_)
            ),
            arguments,
            assumptions: state.assumptions.clone(),
            trusted_assumptions,
        })
    }

    pub(super) fn complete_call(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
        observations: &bray_bound_tree::StorageCompletion,
    ) {
        let result = self.invocation_value(state, expression.into());

        let invocation = expression.into();
        let result = self.complete_trusted_call(state, (invocation, invocation), None, self.invocation_result_place(invocation).as_ref(), result, observations);

        state.expressions.insert(expression, result);
    }

    pub(super) fn assume_call_postconditions(&self, state: &mut ExecutionState, expression: BoundExpressionId,
        entry: &crate::ExecutionCallEvidence, result: &ExecutionCondition,
        post_state: &BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
    ) {
        let Some(bray_bound_tree::SemanticSelection::Call(call)) =
            self.semantics.selections().expression(expression)
        else {
            return;
        };

        if matches!(
            call.target(),
            bray_bound_tree::BoundCallableTarget::Predicate(_)
        ) || !matches!(
            call.resolution().result(),
            bray_bound_tree::BoundCallResult::Immediate(_)
        ) {
            return;
        }

        let contracts = self
            .contracts
            .get(&expression)
            .into_iter()
            .flatten()
            .filter(|contract| entry.prove_requirements(&contract.entry).is_ok())
            .collect::<Vec<_>>();

        if contracts.is_empty() {
            return;
        }

        for contract in contracts {
            for (postcondition, source) in &contract.postconditions {
                let condition = postcondition.substitute(
                    &|input| {
                        input
                            .value_in(post_state)
                            .unwrap_or_else(ExecutionCondition::unknown)
                    },
                    result,
                    &mut { ExecutionCondition::WORK_LIMIT },
                );

                if condition == ExecutionCondition::unknown() {
                    continue;
                }

                condition.assume(true, &mut state.assumptions);

                state
                    .completion_dependencies
                    .insert(crate::ExecutionCompletionDependency {
                        target: call.target(),
                        source: *source,
                        node: expression.into(),
                    });
            }
        }

    }
}

pub(super) fn converted_argument(
    values: &bray_symbols::SemanticValueStore,
    value: ExecutionCondition,
    conversion: &bray_bound_tree::SelectedConversion,
) -> ExecutionCondition {
    if conversion_transports_value(conversion) {
        value.coerce(values, conversion.source_type(), conversion.target_type())
    } else {
        ExecutionCondition::unknown()
    }
}

pub(super) fn conversion_transports_value(conversion: &bray_bound_tree::SelectedConversion) -> bool {
    matches!(conversion.target(), bray_bound_tree::ConversionTarget::Identity
        | bray_bound_tree::ConversionTarget::CallableContract
        | bray_bound_tree::ConversionTarget::NullablePresent)
}
