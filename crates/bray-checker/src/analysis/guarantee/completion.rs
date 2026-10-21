use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{AnyBoundNodeId, BoundExpressionId, BoundReferenceTarget, StorageAccessId};
use bray_symbols::TypeData;

use super::flow::{ExecutionFlow, ExecutionFlowDomain};
use super::state::ExecutionState;
use crate::{CheckerRequestContext, ExecutionCondition, ExecutionPlace};

impl<C: CheckerRequestContext + ?Sized> ExecutionFlow<'_, '_, C> {
    pub(super) fn candidate(
        &self,
        memory: &bray_bound_tree::CheckedMemoryOperations,
        collect_cleanup: bool,
    ) -> crate::ExecutionCandidate {
        let mut candidate = crate::ExecutionCandidate::default();
        let mut dependencies = BTreeSet::new();
        let mut results = BTreeSet::new();

        let normal_exit_blocks = self
            .domain
            .graph
            .completion_exits()
            .map(|exit| exit.block())
            .collect::<BTreeSet<_>>();

        'blocks: for block in self.domain.graph.blocks() {
            let Some(Some(state)) = self.states.state(block.id()) else {
                continue;
            };

            // Each replay retains independently mutable value observations at the cleanup boundary.
            let mut state = state.clone();

            for operation in block
                .operations()
                .iter()
                .filter_map(|id| self.domain.graph.operation(*id))
            {
                if let super::super::model::AnalysisOperationKind::ScopeExit {
                    block, exit, phase: super::super::model::AnalysisScopeExitPhase::LifecycleResolution, ..
                } = operation.kind()
                    && let Some((cleanup, _)) = self.domain.disposal.selected
                    && let Some(plan) = cleanup.scope_exit_plan(block, exit)
                {
                    for access in plan.lifecycle_resolution() {
                        let inert = self.domain.cleanup_preserves_inputs(&state, *access);
                        candidate.inert_cleanup.entry((exit, *access))
                            .and_modify(|previous| *previous &= inert).or_insert(inert);
                    }
                }

                if !collect_cleanup {
                    if !self.domain.operation(&mut state, operation.kind(), None) {
                        continue 'blocks;
                    }

                    continue;
                }

                if !self.domain.operation(
                    &mut state,
                    operation.kind(),
                    Some(&mut |access, part, slot, lifecycle, whole, evidence, _| {
                        let node = match operation.kind() {
                            super::super::model::AnalysisOperationKind::ScopeExit {
                                exit, ..
                            } => exit,
                            operation => operation.node(),
                        };

                        {
                            let normal = operation.kind().is_normal_cleanup();

                            candidate
                                .cleanup
                                .entry((node, access, part, slot, normal))
                                .and_modify(|previous| {
                                    previous.evidence.intersect(evidence);
                                })
                                // Different control-flow exits own independently mutable premises.
                                .or_insert_with(|| crate::ExecutionCleanup {
                                    callable: lifecycle.call.callable,
                                    result: lifecycle.call.result.completion_type(),
                                    synchronous: lifecycle.call.result.execution()
                                        == bray_symbols::CallableExecution::Synchronous,
                                    caller_continuation: self.domain.storage.is_root_access(access)
                                        && self.domain.storage.root_identity(access).is_some_and(|identity| {
                                            bray_bound_tree::storage_identity_is_destructor_receiver(
                                                self.domain.request.unit(), self.domain.storage, identity,
                                            )
                                        })
                                        && matches!(operation.kind(), super::super::model::AnalysisOperationKind::ScopeExit { .. }),
                                    whole,
                                    evidence: evidence.clone(),
                                });
                        }

                        // Whole-value completion can omit an async finalizer without
                        // constructing its future. Invocation certification still distinguishes
                        // synchronous execution from deferred execution.
                        if lifecycle.call.result.execution()
                            != bray_symbols::CallableExecution::Synchronous
                        {
                            return;
                        }

                        candidate
                            .disposal
                            .entry((node.into(), lifecycle.call.callable))
                            .and_modify(|previous| {
                                previous.intersect(evidence);
                            })
                            .or_insert_with(|| evidence.clone());
                    }),
                ) { continue 'blocks; }
            }

            for operation in block
                .operations()
                .iter()
                .filter_map(|id| self.domain.graph.operation(*id))
            {
                let occurrence = operation.kind().occurrence();

                if let Some(evidence) = state.entries.remove(&occurrence) {
                    if let Some(contract) = self
                        .domain
                        .request
                        .trusted_contracts()
                        .and_then(|contracts| contracts.calls.get(&occurrence))
                        && contract.preserves_inputs
                        && !occurrence
                            .expression()
                            .and_then(|expression| memory.operation(expression))
                            .is_some_and(|operation| {
                                crate::memory::execution::guarantees(
                                    operation.kind(),
                                    crate::ExecutionProperty::Pure,
                                )
                            })
                        && evidence.proves(&contract.preconditions)
                        && (evidence.trusted_boundary
                            || self
                                .domain
                                .trusted_requirements_proven(&evidence, &contract.requirements))
                    {
                        let target = match self.domain.semantics.selections().expression(
                            occurrence
                                .expression()
                                .expect("selected call has an expression"),
                        ) {
                            Some(bray_bound_tree::SemanticSelection::Call(call)) => call
                                .resolution()
                                .declaration_source()
                                .map(bray_bound_tree::BoundCallableTarget::Declaration)
                                .unwrap_or(call.target()),
                            Some(bray_bound_tree::SemanticSelection::ScopedUse(scoped)) => {
                                bray_bound_tree::BoundCallableTarget::Declaration(
                                    scoped.invocation(occurrence).0,
                                )
                            }
                            _ => panic!("preserving invocation retains its selected callable"),
                        };

                        candidate.dependencies.push(
                            crate::execution_guarantees::ExecutionDependency {
                                target: target.into(),
                                property: crate::execution_guarantees::ExecutionProperty::Pure,
                                occurrence,
                            },
                        );
                    }

                    candidate.calls.entry(occurrence).or_insert(evidence);
                }
            }

            dependencies.extend(state.completion_dependencies);

            if normal_exit_blocks.contains(&block.id()) {
                let mut result = &state.result;

                while let ExecutionCondition::Constructed(base, _) = result {
                    result = base;
                }

                results.insert(match result {
                    ExecutionCondition::Tagged(variant, _) => Some(*variant),
                    _ => None,
                });
            }
        }

        candidate.completion_dependencies = dependencies.into_iter().collect();

        candidate.result_variant = if results.len() == 1 {
            results.pop_first().flatten()
        } else {
            None
        };

        candidate
    }
}

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(super) fn cleanup_preserves_inputs(
        &self,
        state: &ExecutionState,
        access: StorageAccessId,
    ) -> bool {
        let ty = self
            .storage
            .access(access)
            .expect("cleanup retains its checked storage access")
            .reached_type();

        if crate::representation::type_representation(self.request, ty)
            != Some(bray_compiler_known::RepresentationRole::String)
        {
            return false;
        }

        let value = ExecutionPlace::storage(self.storage, access)
            .and_then(|place| place.value_in(&state.current))
            .or_else(|| {
                let node = self
                    .storage
                    .identity(self.storage.root_identity(access)?)?
                    .definition_node()?;

                let AnyBoundNodeId::Expression(expression) = node else {
                    return None;
                };

                Some(self.value(state, expression))
            });

        // Materialized string constants have static backing and no reference-counted owner.
        // Disposing their local representation cannot execute storage release or user cleanup.
        matches!(value, Some(ExecutionCondition::Literal(value))
            if matches!(value.kind(), bray_symbols::ConstantValueKind::String(_)))
    }

    fn call_argument_value(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
        conversion: &bray_bound_tree::SelectedConversion,
    ) -> ExecutionCondition {
        let value = state.expressions.get(&expression).cloned()
            .unwrap_or_else(|| self.value(state, expression));
        converted_argument(value, conversion)
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
        let plan = self.storage.occurrence_plans(invocation)
            .find(|plan| plan.purpose() != bray_bound_tree::StorageAccessPurpose::Initialize)
            .expect("implicit invocation retains its input transfer");
        let consumes = !matches!(plan.purpose(), bray_bound_tree::StorageAccessPurpose::Borrow(_));
        assert_eq!(contract.arguments.len(), 1, "implicit invocation has one actual input");
        for (input, source) in &contract.arguments {
            let input = ExecutionPlace::from(*input);
            let ordinal = ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0));
            let value = self.current_input(state, *source);
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
            returned: Default::default(),
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
        let contract = self
            .request
            .trusted_contracts()?
            .calls
            .get(&expression.into())?;

        let mut arguments = contract
            .arguments
            .iter()
            .map(|(input, expression)| {
                (
                    ExecutionPlace::from(*input),
                    self.invocation_value(state, *expression),
                )
            })
            .collect::<BTreeMap<_, _>>();

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
                        *value = std::mem::replace(value, ExecutionCondition::Unknown).coerce(
                            self.request.semantic_values(),
                            conversion.source_type(),
                            conversion.target_type(),
                        );
                    }
                }
            }
        }

        return Some(crate::ExecutionCallEvidence {
            returned: Default::default(),
            anonymous_target: None,
            trusted_boundary: !state.trust_boundaries.is_empty(),
            pending_execution: false,
            arguments,
            assumptions: state.assumptions.clone(),
            trusted_assumptions: state.call_trusted_assumptions([]),
        });
    }

    pub(super) fn call_entry(
        &self,
        state: &ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
    ) -> Option<crate::ExecutionCallEvidence> {
        let mut entry = self.invocation_entry(state, invocation)?;
        if !entry.pending_execution {
            entry.returned = self.returned_value_evidence(state, invocation, &entry.arguments);
        }
        Some(entry)
    }

    fn invocation_entry(
        &self,
        state: &ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
    ) -> Option<crate::ExecutionCallEvidence> {
        if matches!(invocation, bray_bound_tree::BoundExecutionSite::CallInput { .. }) {
            return Some(self.input_entry(state, invocation));
        }
        let expression = invocation
            .expression()
            .expect("selected invocation retains its actual expression owner");

        let selection = self.semantics.selections().expression(expression)?;

        if matches!(selection, bray_bound_tree::SemanticSelection::ScopedUse(_)
            | bray_bound_tree::SemanticSelection::Iteration(_))
        {
            return self.implicit_call_entry(state, invocation, selection);
        }

        let bray_bound_tree::SemanticSelection::Call(call) = selection else {
            return self.operation_call_entry(state, expression, selection);
        };

        let staged = call.evaluates_defaults();
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
                    arguments.get(&ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(
                        ordinal.checked_add(u32::from(call.receiver().is_some())).expect("selected argument ordinal fits"),
                    ))).cloned().expect("staged argument has its value")
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
                arguments.get(&ordinal).cloned().expect("staged receiver has its value")
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
            if let bray_bound_tree::SelectedArgument::Default { parameter, ordinal, ty, .. } = argument
                && !matches!(self.request.semantic_values().type_data(*ty).as_ref(), TypeData::Borrow { .. })
            {
                transferred.insert(BoundReferenceTarget::Surface((*parameter).into()).into());
                let ordinal = call.input_ordinal(bray_symbols::DependencySubjectRoot::Parameter(
                    bray_symbols::SymbolOrdinal::new(*ordinal))).expect("default has a formal ordinal");
                transferred.insert(ExecutionPlace::argument(ordinal));
            }
        }

        // Call evidence retains the immutable entry conditions independently of subsequent mutation.
        let trusted_assumptions =
            self.call_trusted_assumptions(state, invocation, &arguments, &transferred);

        let mut evidence = crate::ExecutionCallEvidence {
            returned: Default::default(),
            anonymous_target: call
                .resolution()
                .anonymous_body()
                .map(|(key, _)| key.clone())
                .or_else(|| match self.request.view().expression(expression) {
                    Some(bray_bound_tree::BoundExpression::Call(source))
                        if matches!(
                            call.target(),
                            bray_bound_tree::BoundCallableTarget::Indirect(_)
                        ) =>
                    {
                        match self.value(state, source.callee()) {
                            ExecutionCondition::Expression(origin) => {
                                match self.request.view().expression(origin) {
                                    Some(bray_bound_tree::BoundExpression::AnonymousCallable(
                                        callable,
                                    )) => Some(callable.unit().clone()),
                                    _ => None,
                                }
                            }
                            _ => None,
                        }
                    }
                    _ => None,
                }),
            trusted_boundary: !state.trust_boundaries.is_empty(),
            pending_execution: matches!(
                call.resolution().result(),
                bray_bound_tree::BoundCallResult::LazyFuture(_)
            ),
            arguments,
            assumptions: state.assumptions.clone(),
            trusted_assumptions,
        };

        if call.implementation_hook()
            == Some(bray_compiler_known::ImplementationHook::RawBufferRelocate)
        {
            let operands = call
                .arguments()
                .iter()
                .filter_map(|argument| match argument {
                    bray_bound_tree::SelectedArgument::Explicit { expression, .. } => {
                        crate::execution_guarantees::expression_place(
                            self.request.unit(),
                            self.semantics,
                            self.request.semantic_values(),
                            *expression,
                        )
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();

            if let [source, destination] = operands.as_slice()
                && !source.overlaps(destination)
                && let Some(contract) = self
                    .request
                    .trusted_contracts()
                    .and_then(|contracts| contracts.calls.get(&expression.into()))
            {
                let key =
                    bray_compiler_known::CompilerKnownDeclarationKey::try_new("NonOverlapping")
                        .expect("closed memory predicate key must be valid");

                let predicate = self
                    .request
                    .context()
                    .available_compiler_known_symbols()
                    .provider()
                    .declaration_symbol::<bray_symbols::PredicateSymbolId>(&key)
                    .map(bray_symbols::PredicateDefinitionSymbolId::from);

                // Separate exclusive accesses to protected buffers retain separate allocation ownership.
                // The selected intrinsic contract supplies the initialized ranges within those owners.
                for requirement in &contract.requirements {
                    if matches!(requirement, ExecutionCondition::Trusted(condition)
                        if matches!(condition.as_ref(), ExecutionCondition::Predicate(identity, _, _) if Some(*identity) == predicate))
                    {
                        let condition = requirement.substitute(
                            &|place| {
                                place
                                    .value_in(&evidence.arguments)
                                    .unwrap_or(ExecutionCondition::Unknown)
                            },
                            &ExecutionCondition::Unknown,
                            &mut { ExecutionCondition::WORK_LIMIT },
                        );

                        condition.assume(true, &mut evidence.trusted_assumptions);
                    }
                }
            }
        }

        Some(evidence)
    }

    pub(super) fn complete_call(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
        observations: &BTreeMap<bray_bound_tree::BoundExecutionSite, BTreeSet<bray_bound_tree::StoragePath>>,
    ) {
        let result = self.invocation_value(state, expression.into());

        self.complete_trusted_call(state, expression.into(), result, observations);

        let Some(entry) = state.entries.get(&expression.into()) else {
            return;
        };

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

        let result = self.value(state, expression);

        let contracts = self
            .contracts
            .get(&expression)
            .into_iter()
            .flatten()
            .filter(|contract| entry.proves(&contract.entry))
            .collect::<Vec<_>>();

        if contracts.is_empty() {
            return;
        }

        let post_state = self.call_post_state(state, expression, call);

        for contract in contracts {
            for (postcondition, source) in &contract.postconditions {
                let condition = postcondition.substitute(
                    &|input| {
                        input
                            .value_in(&post_state)
                            .unwrap_or(ExecutionCondition::Unknown)
                    },
                    &result,
                    &mut { ExecutionCondition::WORK_LIMIT },
                );

                if condition == ExecutionCondition::Unknown {
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

        state.expressions.insert(expression, result);
    }
}


pub(super) fn converted_argument(
    value: ExecutionCondition,
    conversion: &bray_bound_tree::SelectedConversion,
) -> ExecutionCondition {
    match conversion.target() {
        bray_bound_tree::ConversionTarget::Identity
        | bray_bound_tree::ConversionTarget::CallableContract => value,
        bray_bound_tree::ConversionTarget::NullablePresent => value.present(),
        _ => ExecutionCondition::Unknown,
    }
}
