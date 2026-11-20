use std::sync::Arc;

use bray_bound_tree::{BoundExpressionId, StorageDeferredProducer, StorageOutcomeRows};

use crate::analysis::guarantee::{deferred, state::ExecutionState};
use crate::execution_guarantees::ExecutionDeferredInvocation;
use crate::{CheckerRequestContext, ExecutionCondition, ExecutionPlace};

use super::super::ExecutionFlowDomain;

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(in crate::analysis) fn operation_outcome_rows(
        &self,
        state: &ExecutionState,
        operation: crate::analysis::AnalysisOperationKind,
        task: Option<&super::super::TaskSelection>,
    ) -> Option<StorageOutcomeRows> {
        use crate::analysis::{AnalysisCallPhase, AnalysisOperationKind};

        if let Some(task) = task {
            return Some(task.guard());
        }

        let site = operation.occurrence();

        let await_completion = matches!(operation, AnalysisOperationKind::Call { phase: AnalysisCallPhase::Completion, .. })
            && site.expression().is_some_and(|expression| {
                matches!(self.request.view().expression(expression), Some(bray_bound_tree::BoundExpression::Await(_)))
            });

        let (producer, entry) = match operation {
            _ if operation.awaited_expression().is_some() || await_completion => {
                self.direct_await_body(state, site).map_or((None, None), |(_, producer, frame)| {
                    // Share the admitted producer identity and its portable row selection.
                    (Some(producer.clone()), Some(frame.entry.as_ref()))
                })
            }
            AnalysisOperationKind::Call { phase: AnalysisCallPhase::Attempt | AnalysisCallPhase::Completion, .. }
            | AnalysisOperationKind::Bound(_)
            | AnalysisOperationKind::AssignmentInstall(_)
                if self.dependencies.calls().get(&site).is_some_and(|call| call.captured_values().len() > 1) =>
            {
                (None, state.admitted_call(site))
            }
            _ => return None,
        };

        Some(StorageOutcomeRows {
            producer,
            rows: entry.and_then(|entry| entry.outputs.reached_source_rows()),
            task: None,
        })
    }

    pub(in crate::analysis) fn deferred_alternatives(
        &self,
        state: &ExecutionState,
        operation: crate::analysis::AnalysisOperationKind,
    ) -> Option<(
        Arc<deferred::DeferredValue>,
        Vec<deferred::DeferredValue>,
    )> {
        use crate::analysis::{AnalysisCallPhase, AnalysisOperationKind, AnalysisSuspensionKind};

        if !matches!(
            operation,
            AnalysisOperationKind::Call {
                phase: AnalysisCallPhase::Attempt | AnalysisCallPhase::Completion,
                ..
            } | AnalysisOperationKind::Suspension {
                kind: AnalysisSuspensionKind::Await,
                ..
            } | AnalysisOperationKind::Bound(bray_bound_tree::AnyBoundNodeId::Expression(_))
        ) {
            return None;
        }

        let expression = operation.occurrence().expression()?;

        let completion = matches!(operation, AnalysisOperationKind::Call { phase: AnalysisCallPhase::Completion, .. });

        let value = if let Some(bray_bound_tree::BoundExpression::Await(_)) =
            self.request.view().expression(expression)
        {
            state.admitted_receiver(expression.into())
        } else {
            if completion { return None; }

            let bray_bound_tree::SemanticSelection::Call(call) =
                self.semantics.selections().expression(expression)?
            else {
                return None;
            };

            crate::analysis::AnalysisTaskOperationKind::from_implementation_hook(
                call.implementation_hook()?,
            )?;

            if state.entries.contains_key(&operation.occurrence()) {
                state.admitted_receiver(operation.occurrence())
            } else {
                self.receiver_value(state, expression, call)
            }
        };

        value.deferred_value()?;

        let proof = state.ordinary_proof();
        let mut value = value;

        loop {
            let deferred = value.deferred_value()?;

            let mut feasible = deferred.producers.iter().filter(|(_, candidate)| {
                deferred::possible(&candidate.selection, &proof)
            });

            let first = feasible.next();
            let second = feasible.next();

            let unknown = deferred.unknown.as_ref().filter(|guard| {
                deferred::possible(&guard.selection, &proof)
            });

            if second.is_some() || (first.is_some() && unknown.is_some()) {
                let mut alternatives = first
                    .into_iter()
                    .chain(second)
                    .chain(feasible)
                    .map(
                        |(source, candidate)| deferred::DeferredValue {
                            producers: [(source.clone(), candidate.clone())].into(),
                            unknown: None,
                        },
                    )
                    .collect::<Vec<_>>();

                if let Some(guard) = unknown {
                    alternatives.push(deferred::DeferredValue {
                        producers: Default::default(),
                        unknown: Some(guard.clone()),
                    });
                }

                return Some((Arc::clone(deferred), alternatives));
            }

            let (source, candidate) = first?;

            if !self.is_task_observation(source.clone()) {
                return None;
            }

            value = ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0))
                .value_in(&candidate.entry.arguments)?;
        }
    }

    pub(in crate::analysis) fn select_deferred(
        &self,
        state: &mut ExecutionState,
        frame: Arc<deferred::DeferredValue>,
        alternative: deferred::DeferredValue,
    ) {
        let guard = alternative
            .unknown
            .as_ref()
            .map(|candidate| &candidate.selection)
            .or_else(|| {
                alternative
                    .producers
                    .values()
                    .next()
                    .map(|candidate| &candidate.selection)
            })
            .expect("selected producer retains its ordinary guard");

        state.assumptions.extend(guard.iter().cloned());
        state.replace_deferred_value(&frame, Some(Arc::new(alternative)));
    }

    pub(in crate::analysis) fn visit_deferred_alternatives<State: Clone>(
        &self,
        state: State,
        operation: crate::analysis::AnalysisOperationKind,
        (frame, mut alternatives): (Arc<crate::ExecutionDeferredValue>, Vec<crate::ExecutionDeferredValue>),
        execution: impl Fn(&mut State) -> &mut ExecutionState,
        mut visit: impl FnMut(Option<StorageDeferredProducer>, State),
    ) {
        use crate::analysis::model::{AnalysisCallPhase, AnalysisOperationKind};

        let last = alternatives.pop().expect("a selected frame split has alternatives");

        let mut apply = |mut branch: State, alternative: crate::ExecutionDeferredValue| {
            let producer = alternative.producers.keys().next().cloned();
            let evidence = execution(&mut branch);

            self.select_deferred(evidence, Arc::clone(&frame), alternative);

            if !matches!(operation, AnalysisOperationKind::Call { phase: AnalysisCallPhase::Completion, .. })
                || self.normal_completion(evidence, operation.occurrence())
            {
                visit(producer, branch);
            }
        };

        for alternative in alternatives {
            apply(state.clone(), alternative);
        }

        apply(state, last);
    }

    pub(in crate::analysis) fn is_task_observation(&self, producer: impl Into<StorageDeferredProducer>) -> bool {
        let hook = match producer.into() {
            StorageDeferredProducer::Local(source) => source.expression().filter(|expression| source == (*expression).into()).and_then(|expression|
                match self.semantics.selections().expression(expression)? {
                    bray_bound_tree::SemanticSelection::Call(call) => call.implementation_hook(),
                    _ => None,
                }),
            StorageDeferredProducer::Summary { callee, .. } => callee.as_deref().and_then(|callee| self.dependencies.frame_callees().get(callee)?.hook),
        };

        matches!(hook, Some(bray_compiler_known::ImplementationHook::TaskJoin | bray_compiler_known::ImplementationHook::TaskCancel))
    }

    pub(in crate::analysis::guarantee::flow) fn task_observation_body<'candidate>(
        &self,
        source: &StorageDeferredProducer,
        candidate: &'candidate ExecutionDeferredInvocation,
    ) -> Option<&'candidate Arc<crate::ExecutionDeferredValue>> {
        if !self.is_task_observation(source.clone()) { return None; }

        candidate.entry.arguments.get(&ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0)))?
            .deferred_value()
    }

    pub(in crate::analysis) fn awaited_task_frame(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> Option<Arc<crate::ExecutionDeferredValue>> {
        let Some(bray_bound_tree::BoundExpression::Await(_)) = self.request.view().expression(expression) else {
            return None;
        };

        self.observed_task_frame(state, &state.admitted_receiver(expression.into()))
    }

    pub(in crate::analysis) fn observed_task_frame(&self, state: &ExecutionState, value: &ExecutionCondition)
        -> Option<Arc<crate::ExecutionDeferredValue>>
    {
        let frame = value.deferred_value()?;
        let proof = state.ordinary_proof();

        if frame.unknown_possible(&proof) { return None; }

        let mut feasible = frame.producers.iter().filter(|(_, producer)| deferred::possible(&producer.selection, &proof));

        let (source, producer) = feasible.next()?;

        if feasible.next().is_some() { return None; }

        self.task_observation_body(source, producer).cloned()
    }
}
