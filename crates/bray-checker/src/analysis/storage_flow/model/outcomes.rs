use std::borrow::Cow;

use crate::analysis::id::AnalysisBlockId;
use crate::analysis::model::{AnalysisCallPhase, AnalysisOperation, AnalysisOperationKind};
use crate::analysis::storage_flow::check::StorageFlowCollector;
use crate::analysis::outcomes::{FlowOutcomes, OutcomePath, OutcomeStep, apply_call_rows, extend_path};
use crate::CheckerRequestContext;

use super::StorageFlowState;

/// Storage states kept distinct by the edges that delivered actual result values.
#[derive(Clone, Default, Eq, PartialEq)]
pub(in crate::analysis::storage_flow) struct StorageFlowOutcomes(FlowOutcomes<StorageFlowState>);

impl StorageFlowOutcomes {
    pub(super) fn entry(state: StorageFlowState) -> Self {
        Self(FlowOutcomes::entry(state))
    }

    pub(in crate::analysis::storage_flow) fn states(&self) -> impl Iterator<Item = &StorageFlowState> {
        self.0.states()
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (Option<&OutcomePath>, &StorageFlowState)> {
        self.0.iter()
    }

    pub(in crate::analysis::storage_flow) fn apply_operation<C: CheckerRequestContext + ?Sized>(
        &mut self,
        collector: &mut StorageFlowCollector<'_, C>,
        block: AnalysisBlockId,
        operation: &AnalysisOperation,
    ) {
        let (mut budget, incoming) = self.0.take();

        for (path, mut state) in incoming {
            let rows = collector.prepare_operation_outcomes(&mut state, operation.kind());
            let invocation = operation.kind().occurrence();

            apply_call_rows(state, path, invocation, &rows, &mut budget, |mut branch, path, row, budget| {
                if let Some(row) = row {
                    branch.execution.as_mut().expect("a bound output has an execution state")
                        .select_call_output(invocation, row);
                }

                self.apply_selected(collector, block, operation, path, branch, budget);
            });
        }
    }

    fn apply_selected<C: CheckerRequestContext + ?Sized>(
        &mut self,
        collector: &mut StorageFlowCollector<'_, C>,
        block: AnalysisBlockId,
        operation: &AnalysisOperation,
        path: Option<OutcomePath>,
        mut state: StorageFlowState,
        budget: &mut usize,
    ) {
        let kind = operation.kind();
        let site = kind.occurrence();
        let execution = state.execution.as_ref().expect("reached operation retains execution facts");

        if *budget > 0
            && let Some((frame, alternatives)) = collector.execution.deferred_alternatives(execution, kind)
            && alternatives.len() - 1 <= *budget
        {
            *budget -= alternatives.len() - 1;

            let mut alternatives = alternatives.into_iter().peekable();

            while let Some(alternative) = alternatives.next() {
                let producer = alternative.producers.keys().next().cloned();
                let mut branch = if alternatives.peek().is_some() { state.clone() } else { std::mem::take(&mut state) };
                let execution = branch.execution.as_mut().expect("selected frame retains execution facts");

                collector.execution.select_deferred(execution, frame.clone(), alternative);

                if matches!(kind, AnalysisOperationKind::Call { phase: AnalysisCallPhase::Completion, .. })
                    && !collector.execution.normal_completion(execution, site)
                {
                    continue;
                }

                let path = Some(extend_path(path.as_ref(), OutcomeStep::Frame(site, producer)));

                self.apply_selected(collector, block, operation, path, branch, budget);
            }

            return;
        }

        if let Some((frame, producer, rows)) = collector.execution.direct_await_outputs(execution, kind) {
            apply_call_rows(state, path, site, &rows, budget, |mut branch, path, row, _| {
                if let Some(row) = row {
                    branch.execution.as_mut().expect("selected await retains execution facts")
                        .select_frame_output(&frame, &producer, row);
                }

                collector.apply_operation(&mut branch, operation);
                self.merge(Some(block), path, Cow::Owned(branch));
            });

            return;
        }

        if let Some(alternatives) = collector.execution.split_task_completion(execution, kind, *budget + 1) {
            *budget -= alternatives.len().saturating_sub(1);

            let mut alternatives = alternatives.into_iter().peekable();

            while let Some((outcome, execution)) = alternatives.next() {
                let mut branch = if alternatives.peek().is_some() { state.clone() } else { std::mem::take(&mut state) };

                branch.execution = Some(execution);
                collector.apply_operation(&mut branch, operation);

                let path = Some(extend_path(path.as_ref(), OutcomeStep::TaskCompletion(site, outcome)));

                self.merge(Some(block), path, Cow::Owned(branch));
            }

            return;
        }

        collector.apply_operation(&mut state, operation);
        self.merge(Some(block), path, Cow::Owned(state));
    }

    pub(super) fn merge(
        &mut self,
        block: Option<AnalysisBlockId>,
        path: Option<OutcomePath>,
        incoming: Cow<'_, StorageFlowState>,
    ) -> bool {
        if !incoming.reachable {
            return false;
        }

        self.0.merge(path, incoming, |state, incoming| state.merge(incoming, block))
    }
}
