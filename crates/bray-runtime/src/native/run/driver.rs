use std::panic::{AssertUnwindSafe, catch_unwind};

use bray_runtime_abi::NativeRuntimeStatus;
use triomphe::Arc;

use crate::context::{current_task_execution_lane, with_task_frame_execution};
use crate::frame::{destroy_frame, finish_frame, resolve_failed_frame};
use crate::{
    ExecutionLane, ExecutionLanePlacement, ExecutionWorkload, FrameContext, FrameExecutionState,
    FrameProgress, FrameSuspension, FrameSuspensionKind, ProtectedFrame, RunOutcome, RuntimePanic,
};

use super::{NativeActivation, NativeRun};

const TRANSITION_BUDGET: usize = 64;

pub(super) fn execution_lane(
    execution: &FrameExecutionState,
) -> Result<ExecutionLane, NativeRuntimeStatus> {
    super::super::state::with_runtime(|runtime| runtime.current_execution_lane(execution))?
}

pub(super) fn lane_allows(current: ExecutionLane, required: ExecutionLane) -> bool {
    if required.workload() != ExecutionWorkload::Cooperative
        && current.workload() != required.workload()
    {
        return false;
    }

    let required_placement = required.placement();

    match required_placement {
        ExecutionLanePlacement::Migratable => true,
        ExecutionLanePlacement::MainThread(required)
        | ExecutionLanePlacement::OriginThread(required)
        | ExecutionLanePlacement::PinnedWorker(required) => match current.placement() {
            ExecutionLanePlacement::MainThread(current)
            | ExecutionLanePlacement::OriginThread(current)
            | ExecutionLanePlacement::PinnedWorker(current) => current == required,
            ExecutionLanePlacement::Migratable => {
                !matches!(required_placement, ExecutionLanePlacement::MainThread(_))
                    && bray_platform::current_runtime_thread()
                        .is_some_and(|thread| thread.id() == required)
            }
        },
    }
}

impl NativeRun {
    pub(super) fn drive(&self) -> FrameProgress<usize> {
        for transition in 0..=TRANSITION_BUDGET {
            let execution = match self.execution() {
                Ok(execution) => execution,
                Err(_) => {
                    if self.fail_static_activation(
                        crate::incident::OwnedCleanupIncident::runtime_failure(),
                    ) {
                        continue;
                    }

                    return FrameProgress::RuntimeFailure;
                }
            };

            let Some(current_lane) = current_task_execution_lane() else {
                return FrameProgress::RuntimeFailure;
            };

            let lane = match execution_lane(&execution) {
                Ok(lane) => lane,
                Err(_) => {
                    if self.fail_static_activation(
                        crate::incident::OwnedCleanupIncident::runtime_failure(),
                    ) {
                        continue;
                    }

                    return FrameProgress::RuntimeFailure;
                }
            };

            if transition == TRANSITION_BUDGET || !lane_allows(current_lane, lane) {
                return FrameProgress::Suspended(FrameSuspension::yielding(execution.state()));
            }

            if let Some(progress) = with_task_frame_execution(execution, lane, || self.step()) {
                return progress;
            }
        }

        // A recovery on the last transition still owns the remaining host cleanup phases.
        match self.execution() {
            Ok(execution) if execution_lane(&execution).is_ok() => {
                FrameProgress::Suspended(FrameSuspension::yielding(execution.state()))
            }
            _ => FrameProgress::RuntimeFailure,
        }
    }

    fn step(&self) -> Option<FrameProgress<usize>> {
        if self.lock_state().current.is_none() {
            return self.step_sequence();
        }

        let (mut frame, terminal, requested) = {
            let mut state = self.lock_state();
            let current = &mut state.current;

            let Some(current) = current.as_mut() else {
                return Some(FrameProgress::RuntimeFailure);
            };

            let Some(frame) = current.frame.take() else {
                return Some(FrameProgress::RuntimeFailure);
            };

            let requested = std::mem::take(&mut current.cancellation_requested)
                || crate::current_run_cancellation_observable();

            // The callback borrows its terminal owner outside the activation lock.
            (frame, Arc::clone(&current.terminal), requested)
        };

        let progress = super::super::incident::with_incident_owner(&terminal, || {
            catch_unwind(AssertUnwindSafe(|| {
                frame.as_mut().resume(FrameContext::new(requested))
            }))
        });

        let progress = match progress {
            Ok(progress) => progress,
            Err(payload) if crate::root::is_propagated_cancellation(payload.as_ref()) => {
                FrameProgress::Cancelled
            }
            Err(payload) => FrameProgress::Panicked(RuntimePanic::from_payload(payload)),
        };

        {
            let mut state = self.lock_state();
            let current = &mut state.current;

            // The exclusive dispatch keeps the activation installed throughout its callback.
            let current = current.as_mut().expect("dispatch retains the active frame");
            current.frame = Some(frame);
        }

        match progress {
            FrameProgress::Suspended(suspension) => {
                match self.suspend_activation(suspension) {
                    Ok(true) => None,
                    Ok(false) => {
                        if self
                            .execution()
                            .and_then(|execution| execution_lane(&execution))
                            .is_err()
                        {
                            self.fail_static_or_return(FrameProgress::RuntimeFailure)
                        } else {
                            Some(FrameProgress::Suspended(suspension))
                        }
                    }
                    // Let the task's existing validation preserve the exact unknown local state.
                    Err(_) => self.fail_static_or_return(FrameProgress::Suspended(suspension)),
                }
            }
            FrameProgress::RuntimeFailure if self.lock_state().sequence.is_none() => {
                Some(FrameProgress::RuntimeFailure)
            }
            progress => match self.finish_activation(progress) {
                Ok(progress) => progress,
                Err(panic) => {
                    if self.lock_state().sequence.is_some() {
                        self.fail_static_activation(crate::incident::OwnedCleanupIncident::host(
                            Box::new(panic),
                        ));

                        None
                    } else {
                        Some(FrameProgress::Panicked(panic))
                    }
                }
            },
        }
    }

    fn fail_static_or_return(
        &self,
        progress: FrameProgress<usize>,
    ) -> Option<FrameProgress<usize>> {
        if self.fail_static_activation(crate::incident::OwnedCleanupIncident::runtime_failure()) {
            None
        } else {
            Some(progress)
        }
    }

    fn suspend_activation(&self, suspension: FrameSuspension) -> Result<bool, NativeRuntimeStatus> {
        let mut state = self.lock_state();
        let current = &mut state.current;

        let active = current
            .as_mut()
            .ok_or(NativeRuntimeStatus::RUNTIME_FAILURE)?;

        active.state = suspension.state();
        let execution = active.execution_state()?;

        if suspension.kind() != FrameSuspensionKind::Awaited {
            return Ok(false);
        }

        let Some(mut child) = active.child.take() else {
            // A raw native suspension can be resumed by an external run-directed notification.
            return Ok(false);
        };

        if child.outcome.is_some() {
            // Checked cleanup may await a child whose terminal result has already arrived.
            active.child = Some(child);

            return Ok(true);
        }

        child.retain_parent_execution(&execution);
        child.parent = current.take();
        *current = Some(child);

        Ok(true)
    }

    fn finish_activation(
        &self,
        progress: FrameProgress<usize>,
    ) -> Result<Option<FrameProgress<usize>>, RuntimePanic> {
        let (mut frame, child, terminal) = {
            let mut state = self.lock_state();
            let current = &mut state.current;

            let active = current
                .as_mut()
                .expect("dispatch retains its terminal activation");

            (
                active.frame.take(),
                active.child.take(),
                Arc::clone(&active.terminal),
            )
        };

        NativeRun::release_chain(child, &terminal);

        // Every activation terminalizes once. The task wrapper has no generated lifecycle callbacks.
        let outcome = super::super::incident::with_incident_owner(&terminal, || {
            let outcome = finish_frame(
                frame
                    .as_mut()
                    .expect("terminal callback retains its frame")
                    .as_mut(),
                progress,
            );

            destroy_frame(frame.take(), outcome)
        });

        let (is_root, is_sequence) = {
            let state = self.lock_state();

            (
                state
                    .current
                    .as_ref()
                    .is_some_and(|active| active.parent.is_none()),
                state.sequence.is_some(),
            )
        };

        if is_root {
            if is_sequence {
                self.finish_static_activation(outcome, &terminal);

                return Ok(None);
            }

            terminal.transfer_payload(&self.terminal);
            Self::retain_incidents(&terminal, &self.terminal);

            return Ok(Some(match outcome {
                RunOutcome::Completed(value) => FrameProgress::Completed(value),
                RunOutcome::Cancelled => FrameProgress::Cancelled,
                RunOutcome::Panicked(panic) => FrameProgress::Panicked(panic),
            }));
        }

        let outcome = super::super::state::task_outcome(outcome, &terminal)?;
        let mut state = self.lock_state();
        let current = &mut state.current;

        let mut child = current
            .take()
            .expect("completed child remains owned until result transfer");

        let mut parent = child
            .parent
            .take()
            .expect("non-root activation retains its parent");

        child.outcome = Some(outcome);
        parent.child = Some(child);

        // Incidents follow the immediate continuation so an enclosing catch can retain them.
        let parent_terminal = Arc::clone(&parent.terminal);
        *current = Some(parent);
        drop(state);
        Self::retain_incidents(&terminal, &parent_terminal);

        Ok(None)
    }

    pub(super) fn release_chain(
        mut current: Option<Box<NativeActivation>>,
        boundary: &Arc<super::super::frame::NativeTerminalState>,
    ) {
        while let Some(mut active) = current {
            if let Some(mut child) = active.child.take() {
                // Reverse one ownership edge, so neither traversal nor destruction recurses.
                child.parent = Some(active);
                current = Some(child);
                continue;
            }

            current = active.parent.take();

            let execution = active.execution_state().ok();

            let mut cleanup = || {
                super::super::incident::with_incident_owner(&active.terminal, || {
                    resolve_failed_frame(&mut active.frame)
                })
            };

            let panic = match execution.zip(current_task_execution_lane()) {
                Some((execution, lane)) => with_task_frame_execution(execution, lane, cleanup),
                None => cleanup(),
            };

            if let Some(panic) = panic {
                active.terminal.record_cleanup_incident(
                    crate::incident::OwnedCleanupIncident::host(Box::new(panic)),
                );
            }

            let parent_terminal = current.as_ref().map_or(boundary, |parent| &parent.terminal);
            Self::retain_incidents(&active.terminal, parent_terminal);
            drop(active);
        }
    }

    pub(super) fn retain_incidents(
        terminal: &Arc<super::super::frame::NativeTerminalState>,
        parent: &Arc<super::super::frame::NativeTerminalState>,
    ) {
        if !Arc::ptr_eq(terminal, parent) {
            for incident in terminal.take_cleanup_incidents() {
                parent.record_cleanup_incident(incident);
            }
        }
    }
}
