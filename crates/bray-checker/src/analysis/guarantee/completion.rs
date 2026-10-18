use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{AnyBoundNodeId, BoundExpressionId, BoundReferenceTarget, StorageAccessId};
use bray_symbols::{TypeAssociatedLifecycleSlot, TypeData};

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

        for block in self.domain.graph.blocks() {
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
                if !collect_cleanup {
                    self.domain.operation(&mut state, operation.kind(), None);
                    continue;
                }

                self.domain.operation(
                    &mut state,
                    operation.kind(),
                    Some(&mut |access, part, slot, lifecycle, whole, evidence, _| {
                        let node = match operation.kind() {
                            super::super::model::AnalysisOperationKind::ScopeExit {
                                exit, ..
                            } => exit,
                            operation => operation.node(),
                        };

                        if slot == TypeAssociatedLifecycleSlot::Finalizer {
                            let normal = operation.kind().is_normal_cleanup();

                            candidate
                                .cleanup
                                .entry((node, access, part, normal))
                                .and_modify(|previous| {
                                    previous.evidence.intersect(evidence);
                                })
                                // Different control-flow exits own independently mutable premises.
                                .or_insert_with(|| crate::ExecutionCleanup {
                                    callable: lifecycle.call.callable,
                                    result: lifecycle.call.result.completion_type(),
                                    synchronous: (lifecycle.call.result.execution()
                                        == bray_symbols::CallableExecution::Synchronous),
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
                );
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
                            Some(bray_bound_tree::SemanticSelection::Call(call)) => call.target(),
                            Some(bray_bound_tree::SemanticSelection::ScopedUse(scoped)) => {
                                bray_bound_tree::BoundCallableTarget::Declaration(
                                    scoped.invocation(occurrence).0,
                                )
                            }
                            _ => panic!("preserving invocation retains its selected callable"),
                        };

                        candidate.dependencies.push(
                            crate::execution_guarantees::ExecutionDependency {
                                target,
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
        let value = if matches!(
            conversion.target(),
            bray_bound_tree::ConversionTarget::Identity
                | bray_bound_tree::ConversionTarget::CallableContract
                | bray_bound_tree::ConversionTarget::NullablePresent
        ) {
            state
                .expressions
                .get(&expression)
                .cloned()
                .unwrap_or_else(|| self.value(state, expression))
        } else {
            ExecutionCondition::Unknown
        };

        if *conversion.target() == bray_bound_tree::ConversionTarget::NullablePresent {
            value.present()
        } else {
            value
        }
    }

    fn scoped_call_entry(
        &self,
        state: &ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
        expression: BoundExpressionId,
        scoped: &bray_bound_tree::SelectedScopedUse,
    ) -> Option<crate::ExecutionCallEvidence> {
        let (input, value, consumes) = match invocation {
            bray_bound_tree::BoundExecutionSite::ScopedEnter(_) => (
                scoped
                    .enter()
                    .1
                    .receiver()
                    .expect("selected scope enter has a receiver")
                    .parameter()
                    .into(),
                self.value(state, scoped.initializer()),
                scoped.enter().1.receiver().is_some_and(|receiver| {
                    matches!(
                        receiver.mode(),
                        bray_symbols::ReceiverMode::Consuming
                            | bray_symbols::ReceiverMode::ConsumingMutable
                    )
                }),
            ),
            bray_bound_tree::BoundExecutionSite::ScopedExit(_) => (
                scoped.exit().1.parameters()[0].parameter().into(),
                ExecutionCondition::ScopedCapability(expression),
                !matches!(
                    self.request
                        .semantic_values()
                        .type_data(scoped.capability_type())
                        .as_ref(),
                    TypeData::Borrow { .. }
                ),
            ),
            bray_bound_tree::BoundExecutionSite::Node(_)
            | bray_bound_tree::BoundExecutionSite::Cleanup(_) => return None,
        };

        let input = ExecutionPlace::from(BoundReferenceTarget::Surface(input));
        let ordinal = ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0));

        let transferred = if consumes {
            BTreeSet::from([input.clone(), ordinal.clone()])
        } else {
            BTreeSet::new()
        };

        let arguments = BTreeMap::from([(input, value.clone()), (ordinal, value)]);

        let trusted_assumptions =
            self.call_trusted_assumptions(state, invocation, &arguments, &transferred);

        return Some(crate::ExecutionCallEvidence {
            returned: Default::default(),
            anonymous_target: None,
            trusted_boundary: !state.trust_boundaries.is_empty(),
            pending_execution: matches!(
                scoped.invocation(invocation).2,
                bray_bound_tree::BoundCallResult::LazyFuture(_)
            ),
            arguments,
            assumptions: state.assumptions.clone(),
            trusted_assumptions,
        });
    }

    pub(super) fn call_entry(
        &self,
        state: &ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
    ) -> Option<crate::ExecutionCallEvidence> {
        let expression = invocation
            .expression()
            .expect("selected invocation retains its actual expression owner");

        let selection = self.semantics.selections().expression(expression)?;

        if let bray_bound_tree::SemanticSelection::ScopedUse(scoped) = selection {
            return self.scoped_call_entry(state, invocation, expression, scoped);
        }

        let bray_bound_tree::SemanticSelection::Call(call) = selection else {
            let contract = self
                .request
                .trusted_contracts()?
                .calls
                .get(&expression.into())?;

            return Some(crate::ExecutionCallEvidence {
                returned: Default::default(),
                anonymous_target: None,
                trusted_boundary: !state.trust_boundaries.is_empty(),
                pending_execution: false,
                arguments: contract
                    .arguments
                    .iter()
                    .map(|(input, expression)| {
                        (
                            ExecutionPlace::from(*input),
                            self.invocation_value(state, *expression),
                        )
                    })
                    .collect(),
                assumptions: state.assumptions.clone(),
                trusted_assumptions: state.call_trusted_assumptions([]),
            });
        };

        let mut arguments = BTreeMap::new();
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
                let value = self.call_argument_value(state, *expression, conversion);

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
            let value = self.value(state, receiver.expression());

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

        if !evidence.pending_execution {
            evidence.returned =
                self.returned_value_evidence(state, invocation, &evidence.arguments);
        }

        Some(evidence)
    }

    pub(super) fn complete_call(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
        observations: &BTreeMap<BoundExpressionId, BTreeSet<bray_bound_tree::StoragePath>>,
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

        let post_state = self.call_post_state(state, call);

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
