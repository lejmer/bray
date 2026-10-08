use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BoundExpressionId, BoundReferenceTarget, CheckedAsync, StorageAccessId,
};
use bray_symbols::{
    CallableExecution, CallableInstanceData, TypeAssociatedLifecycleSlot, TypeData, TypeId,
};

use super::super::model::{AnalysisOperationKind, AnalysisScopeExitPhase};
use super::flow::{ExecutionFlow, ExecutionFlowDomain};
use super::state::ExecutionState;
use crate::{
    CheckerQueryError, CheckerRequestContext, ExecutionCallEvidence, ExecutionCondition,
    ExecutionPlace,
};

impl<C: CheckerRequestContext + ?Sized> ExecutionFlow<'_, '_, C> {
    pub(super) fn candidate(
        &self,
        cleanup: &CheckedAsync,
        collect_cleanup: bool,
    ) -> Result<crate::ExecutionCandidate, CheckerQueryError<C::UpstreamError>> {
        let mut candidate = crate::ExecutionCandidate::default();
        let mut dependencies = BTreeSet::new();
        let mut results = BTreeSet::new();

        let normal_exit_blocks = self
            .domain
            .graph
            .exits()
            .iter()
            .filter(|exit| {
                matches!(
                    exit.kind(),
                    super::super::model::AnalysisExitKind::Return
                        | super::super::model::AnalysisExitKind::NormalFallthrough
                        | super::super::model::AnalysisExitKind::ResultErrorPropagation,
                )
            })
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
                let targets = match operation.kind() {
                    AnalysisOperationKind::ScopeExit {
                        block,
                        exit,
                        phase: AnalysisScopeExitPhase::LifecycleResolution,
                    } if collect_cleanup => cleanup
                        .scope_exit_plan(block, exit)
                        .into_iter()
                        .flat_map(|plan| {
                            plan.lifecycle_resolution()
                                .iter()
                                .map(move |access| (exit, *access))
                        })
                        .collect::<Vec<_>>(),
                    AnalysisOperationKind::Bound(AnyBoundNodeId::Expression(expression))
                        if collect_cleanup =>
                    {
                        cleanup
                            .replacement(expression)
                            .into_iter()
                            .filter(|plan| {
                                plan.parts().is_none()
                                    && matches!(
                                        plan.cleanup(),
                                        bray_bound_tree::AsyncStorageCleanupRequirement::Cleanup(_)
                                    )
                            })
                            .map(|plan| (expression.into(), plan.access()))
                            .collect()
                    }
                    _ => Vec::new(),
                };

                for (node, access) in targets {
                    if let Some((callable, result, evidence)) = self.cleanup_entry(
                        &state,
                        access,
                        TypeAssociatedLifecycleSlot::Finalizer,
                        true,
                    )? {
                        candidate
                            .cleanup
                            .entry((node, access))
                            .and_modify(|(_, _, previous)| {
                                previous.intersect(&evidence);
                            })
                            .or_insert((callable, result, evidence));
                    }

                    state.invalidate_cleanup();
                }

                self.domain.operation(&mut state, operation.kind());
            }

            for operation in block
                .operations()
                .iter()
                .filter_map(|id| self.domain.graph.operation(*id))
            {
                let occurrence = operation.kind().occurrence();

                if let Some(evidence) = state.entries.remove(&occurrence) {
                    candidate.calls.entry(occurrence).or_insert(evidence);
                }
            }

            dependencies.extend(state.completion_dependencies);

            if normal_exit_blocks.contains(&block.id()) {
                results.insert(match state.result {
                    crate::ExecutionCondition::Expression(expression)
                    | crate::ExecutionCondition::Constructed(expression, _) => {
                        match self.domain.semantics.selections().expression(expression) {
                            Some(bray_bound_tree::SemanticSelection::Operation(
                                bray_bound_tree::SelectedOperation::Construction(construction),
                            )) => match construction.target() {
                                bray_bound_tree::ConstructionTarget::UnionVariant(variant) => {
                                    Some(variant)
                                }
                                _ => None,
                            },
                            _ => None,
                        }
                    }
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

        Ok(candidate)
    }

    pub(super) fn cleanup_entry(
        &self,
        state: &ExecutionState,
        access: StorageAccessId,
        slot: TypeAssociatedLifecycleSlot,
        require_synchronous: bool,
    ) -> Result<
        Option<(CallableInstanceData, TypeId, ExecutionCallEvidence)>,
        CheckerQueryError<C::UpstreamError>,
    > {
        let access_type = self
            .domain
            .storage
            .access(access)
            .expect("checked cleanup has committed storage")
            .reached_type();

        self.cleanup_place_entry(
            state,
            ExecutionPlace::storage(self.domain.storage, access),
            access_type,
            slot,
            require_synchronous,
        )
    }

    pub(super) fn cleanup_place_entry(
        &self,
        state: &ExecutionState,
        place: Option<ExecutionPlace>,
        ty: TypeId,
        slot: TypeAssociatedLifecycleSlot,
        require_synchronous: bool,
    ) -> Result<
        Option<(CallableInstanceData, TypeId, ExecutionCallEvidence)>,
        CheckerQueryError<C::UpstreamError>,
    > {
        let request = self.domain.request;
        let selected = request.context().lifecycle_callable(ty, slot)?;

        let Some((callable, signature)) = selected.value() else {
            return Ok(None);
        };

        let Some(receiver) = signature.receiver() else {
            return Ok(None);
        };

        if selected.diagnostics().has_errors()
            || (require_synchronous
                && !matches!(request.semantic_values().type_data(signature.callable_type())
                .as_ref(), TypeData::Callable(callable) if callable.execution() == CallableExecution::Synchronous))
        {
            return Ok(None);
        }

        let mut arguments = BTreeMap::new();
        let receiver = BoundReferenceTarget::Surface(receiver.parameter().into());

        for (observed, value) in &state.current {
            if let Some(place) = &place
                && place.contains(observed)
            {
                let mut input = ExecutionPlace::from(receiver);

                for field in observed.projections.iter().skip(place.projections.len()) {
                    input = input.component(*field);
                }

                // The finalizer candidate retains the current immutable value snapshot.
                arguments.insert(input, value.clone());
            }
        }

        if let Some(value) = place.and_then(|place| place.value_in(&state.current)) {
            arguments.entry(receiver.into()).or_insert(value);
        }

        // A candidate owns its entry assumptions independently of subsequent cleanup.
        Ok(Some((
            *callable,
            signature.result(),
            ExecutionCallEvidence {
                trusted_boundary: !state.trust_boundaries.is_empty(),
                pending_execution: false,
                arguments,
                assumptions: state.assumptions.clone(),
                trusted_assumptions: state.trusted_assumptions.clone(),
            },
        )))
    }
}

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(super) fn call_entry(
        &self,
        state: &ExecutionState,
        invocation: bray_bound_tree::SemanticOccurrence,
    ) -> Option<crate::ExecutionCallEvidence> {
        let expression = invocation
            .expression()
            .expect("selected invocation retains its actual expression owner");

        let selection = self.semantics.selections().expression(expression)?;

        if let bray_bound_tree::SemanticSelection::ScopedUse(scoped) = selection {
            let (input, value) = match invocation {
                bray_bound_tree::SemanticOccurrence::ScopeEnter(_) => (
                    scoped
                        .enter()
                        .1
                        .receiver()
                        .expect("selected scope enter has a receiver")
                        .parameter()
                        .into(),
                    self.value(state, scoped.initializer()),
                ),
                bray_bound_tree::SemanticOccurrence::ScopeExit(_) => (
                    scoped.exit().1.parameters()[0].parameter().into(),
                    ExecutionCondition::ScopedCapability(expression),
                ),
                bray_bound_tree::SemanticOccurrence::Node(_) => return None,
            };

            return Some(crate::ExecutionCallEvidence {
                trusted_boundary: !state.trust_boundaries.is_empty(),
                pending_execution: matches!(
                    scoped.invocation(invocation).2,
                    bray_bound_tree::BoundCallResult::LazyFuture(_)
                ),
                arguments: BTreeMap::from([
                    (
                        crate::ExecutionPlace::from(BoundReferenceTarget::Surface(input)),
                        value.clone(),
                    ),
                    (
                        crate::ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0)),
                        value,
                    ),
                ]),
                assumptions: state.assumptions.clone(),
                trusted_assumptions: state.trusted_assumptions.clone(),
            });
        }

        let bray_bound_tree::SemanticSelection::Call(call) = selection else {
            let contract = self
                .request
                .trusted_contracts()?
                .calls
                .get(&expression.into())?;

            return Some(crate::ExecutionCallEvidence {
                trusted_boundary: !state.trust_boundaries.is_empty(),
                pending_execution: false,
                arguments: contract
                    .arguments
                    .iter()
                    .map(|(input, expression)| {
                        (
                            crate::ExecutionPlace::from(*input),
                            self.invocation_value(state, *expression),
                        )
                    })
                    .collect(),
                assumptions: state.assumptions.clone(),
                trusted_assumptions: state.trusted_assumptions.clone(),
            });
        };

        let mut arguments = BTreeMap::new();

        for argument in call.arguments() {
            if let bray_bound_tree::SelectedArgument::Explicit {
                expression,
                parameter,
                ordinal,
                conversion,
                ..
            } = argument
            {
                let value = if matches!(
                    conversion.target(),
                    bray_bound_tree::ConversionTarget::Identity
                        | bray_bound_tree::ConversionTarget::CallableContract
                ) {
                    state
                        .expressions
                        .get(expression)
                        .cloned()
                        .unwrap_or_else(|| self.value(state, *expression))
                } else {
                    ExecutionCondition::Unknown
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

                arguments.insert(
                    crate::ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(ordinal)),
                    value,
                );
            }
        }

        if let Some(receiver) = call.receiver() {
            arguments.insert(
                BoundReferenceTarget::Surface(receiver.parameter().into()).into(),
                self.value(state, receiver.expression()),
            );

            arguments.insert(
                crate::ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0)),
                self.value(state, receiver.expression()),
            );
        }

        // Call evidence retains the immutable entry conditions independently of subsequent mutation.
        let mut evidence = crate::ExecutionCallEvidence {
            trusted_boundary: !state.trust_boundaries.is_empty(),
            pending_execution: false,
            arguments,
            assumptions: state.assumptions.clone(),
            trusted_assumptions: state.trusted_assumptions.clone(),
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

    pub(super) fn complete_call(&self, state: &mut ExecutionState, expression: BoundExpressionId) {
        self.complete_trusted_call(
            state,
            expression.into(),
            ExecutionCondition::Expression(expression),
        );

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
        ) {
            return;
        }

        // TODO(BRA-500): Carry async completion evidence through the future and await boundary.
        if !matches!(
            call.resolution().result(),
            bray_bound_tree::BoundCallResult::Immediate(_)
        ) {
            return;
        }

        let result = ExecutionCondition::Expression(expression);

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

        let post_state = self.receiver_post_state(state, expression, call, &contracts);

        for contract in contracts {
            for (postcondition, source) in &contract.postconditions {
                let condition = postcondition.substitute(
                    &|input| {
                        input
                            .value_in(&post_state)
                            .unwrap_or(ExecutionCondition::Unknown)
                    },
                    &result,
                    &mut { crate::ExecutionCondition::WORK_LIMIT },
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
