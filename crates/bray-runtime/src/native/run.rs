use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};

use bray_runtime_abi::{
    NativeInactiveFrame, NativeRunOutcome, NativeRunState, NativeRuntimeStatus,
};
use bray_runtime_model::{
    ExecutionLaneRequirement, ProtectedFrameAffinity, ProtectedFrameDescriptor,
    ProtectedFrameStateId,
};

use crate::context::{current_task_execution_lane, with_task_frame_execution};
use crate::frame::{terminalize_frame, terminalize_frame_after_broadcast};
use crate::{
    ExecutionLane, ExecutionLanePlacement, ExecutionWorkload, FrameContext, FrameExecutionState,
    FrameExit, FrameProgress, FrameSuspension, FrameSuspensionKind, ProtectedFrame, RunOutcome,
    RuntimePanic,
};

use super::frame::{NativeFrame, NativeTerminalState};
use super::state::{current_native_task, with_runtime};

const TRANSITION_BUDGET: usize = 64;

pub(super) struct NativeRun {
    descriptor: ProtectedFrameDescriptor,
    terminal: Arc<NativeTerminalState>,
    current: Mutex<Option<Box<NativeActivation>>>,
}

struct NativeActivation {
    frame: Option<Pin<Box<NativeFrame>>>,
    descriptor: ProtectedFrameDescriptor,
    state: ProtectedFrameStateId,
    parent: Option<Box<Self>>,
    child: Option<Box<Self>>,
    retired: Option<Box<Self>>,
    next_retired: Option<Box<Self>>,
    terminal: Arc<NativeTerminalState>,
    outcome: Option<NativeRunOutcome>,
    outgoing: crate::outgoing::OutgoingRecords,
    broadcasted: bool,
    cleanup: bool,
    deferred_progress: Option<FrameProgress<usize>>,
    retained_lanes: [bool; ExecutionLaneRequirement::ALL.len()],
    retained_affinity: ProtectedFrameAffinity,
    origin: Option<bray_platform::RuntimeThreadId>,
    retained_origin: Option<bray_platform::RuntimeThreadId>,
}

impl NativeActivation {
    fn new(frame: NativeFrame, outgoing: crate::outgoing::OutgoingRecords) -> Self {
        let terminal = frame.terminal_state();
        let descriptor = frame.descriptor().clone();

        Self {
            frame: Some(Box::pin(frame)),
            descriptor,
            state: ProtectedFrameStateId::new(0),
            parent: None,
            child: None,
            retired: None,
            next_retired: None,
            terminal,
            outcome: None,
            outgoing,
            broadcasted: false,
            cleanup: false,
            deferred_progress: None,
            retained_lanes: [false; ExecutionLaneRequirement::ALL.len()],
            retained_affinity: ProtectedFrameAffinity::Movable,
            origin: bray_platform::current_runtime_thread().map(|thread| thread.id()),
            retained_origin: None,
        }
    }

    fn retain_parent_execution(&mut self, execution: &FrameExecutionState) {
        for (retained, requirement) in self
            .retained_lanes
            .iter_mut()
            .zip(ExecutionLaneRequirement::ALL)
        {
            *retained = execution
                .descriptor()
                .lane_requirements()
                .contains(&requirement);
        }

        self.retained_affinity = execution.descriptor().affinity();
        self.retained_origin = execution.origin();
    }

    fn discard_parent_execution(&mut self) {
        self.retained_lanes = [false; ExecutionLaneRequirement::ALL.len()];
        self.retained_affinity = ProtectedFrameAffinity::Movable;
        self.retained_origin = None;
    }

    fn execution_state(&self) -> Result<FrameExecutionState, NativeRuntimeStatus> {
        self.execution_at(self.state)
    }

    fn execution_at(
        &self,
        state: ProtectedFrameStateId,
    ) -> Result<FrameExecutionState, NativeRuntimeStatus> {
        let local = self
            .descriptor
            .state(state)
            .ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?;

        let affinities = [local.affinity(), self.retained_affinity];

        if affinities == [ProtectedFrameAffinity::OriginThread; 2]
            && self
                .origin
                .zip(self.retained_origin)
                .is_some_and(|(own, parent)| own != parent)
        {
            return Err(NativeRuntimeStatus::INVALID_ARGUMENT);
        }

        let affinity = if affinities.contains(&ProtectedFrameAffinity::OriginThread) {
            ProtectedFrameAffinity::OriginThread
        } else if affinities.contains(&ProtectedFrameAffinity::MainThread) {
            ProtectedFrameAffinity::MainThread
        } else {
            ProtectedFrameAffinity::Movable
        };

        let lanes = ExecutionLaneRequirement::ALL
            .into_iter()
            .zip(self.retained_lanes)
            .filter_map(|(requirement, retained)| {
                (retained
                    || local.lane_requirements().contains(&requirement)
                    || (requirement == ExecutionLaneRequirement::MainThread
                        && affinity == ProtectedFrameAffinity::MainThread))
                    .then_some(requirement)
            });

        let state = local.clone().with_execution_requirements(lanes, affinity);

        let execution = FrameExecutionState::new(self.descriptor.frame(), state);

        let origin = if self.retained_affinity == ProtectedFrameAffinity::OriginThread {
            self.retained_origin.or(self.origin)
        } else {
            self.origin
        };

        Ok(
            match origin.filter(|_| affinity == ProtectedFrameAffinity::OriginThread) {
                Some(origin) => execution.with_origin(origin),
                None => execution,
            },
        )
    }
}

impl NativeRun {
    fn active(current: &mut Option<Box<NativeActivation>>) -> &mut NativeActivation {
        current
            .as_deref_mut()
            .unwrap_or_else(|| panic!("dispatch retains the active activation"))
    }

    pub(super) fn new(frame: NativeFrame, outgoing: crate::outgoing::OutgoingRecords) -> Arc<Self> {
        let activation = Box::new(NativeActivation::new(frame, outgoing));

        Arc::new(Self {
            descriptor: activation.descriptor.clone(),
            terminal: Arc::clone(&activation.terminal),
            current: Mutex::new(Some(activation)),
        })
    }

    pub(super) fn terminal_state(&self) -> Arc<NativeTerminalState> {
        Arc::clone(&self.terminal)
    }

    fn lock_current(&self) -> MutexGuard<'_, Option<Box<NativeActivation>>> {
        self.current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn execution(&self) -> Result<FrameExecutionState, NativeRuntimeStatus> {
        self.lock_current()
            .as_ref()
            .ok_or(NativeRuntimeStatus::RUNTIME_FAILURE)?
            .execution_state()
    }

    pub(super) fn execution_at(
        &self,
        state: ProtectedFrameStateId,
    ) -> Result<FrameExecutionState, NativeRuntimeStatus> {
        self.lock_current()
            .as_ref()
            .ok_or(NativeRuntimeStatus::RUNTIME_FAILURE)?
            .execution_at(state)
    }

    pub(super) fn compose(&self, frame: NativeInactiveFrame) -> NativeRuntimeStatus {
        {
            let current = self.lock_current();

            if current.as_ref().is_none_or(|current| {
                current.frame.is_some()
                    || current.child.is_some()
                    || current.cleanup
                    || current.deferred_progress.is_some()
            }) {
                return NativeRuntimeStatus::RUNTIME_FAILURE;
            }
        }

        let Ok(mut outgoing) = crate::outgoing::OutgoingRecords::admit(6) else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

        let parent_execution = match self.execution() {
            Ok(execution) => execution,
            Err(status) => return status,
        };

        let Some(mut frame) = NativeFrame::try_new(frame.into_protected()) else {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        };

        frame.outgoing = outgoing.take(4);

        let mut child = Box::new(NativeActivation::new(frame, outgoing));

        let local_status = child
            .execution_state()
            .and_then(|execution| {
                execution_lane(&execution).map_err(|_| NativeRuntimeStatus::RUNTIME_FAILURE)
            })
            .err();

        if let Some(status) = local_status {
            Self::release_chain(Some(child), &self.terminal);

            return status;
        }

        child.retain_parent_execution(&parent_execution);

        let composed_status = child
            .execution_state()
            .and_then(|execution| {
                execution_lane(&execution).map_err(|_| NativeRuntimeStatus::RUNTIME_FAILURE)
            })
            .err();

        if composed_status.is_some() {
            child.discard_parent_execution();
            child.cleanup = true;
        }

        let mut current = self.lock_current();

        let parent = Self::active(&mut current);

        if parent.frame.is_some()
            || parent.child.is_some()
            || parent.cleanup
            || parent.deferred_progress.is_some()
        {
            drop(current);
            Self::release_chain(Some(child), &self.terminal);

            return NativeRuntimeStatus::RUNTIME_FAILURE;
        }

        parent.child = Some(child);

        composed_status.unwrap_or(NativeRuntimeStatus::SUCCESS)
    }

    pub(super) fn resolve_completion(&self) -> Result<usize, NativeRuntimeStatus> {
        let mut current = self.lock_current();

        let parent = current
            .as_mut()
            .ok_or(NativeRuntimeStatus::RUNTIME_FAILURE)?;

        let mut child = parent
            .child
            .take()
            .ok_or(NativeRuntimeStatus::UNKNOWN_TASK)?;

        let outcome = child.outcome.as_ref().ok_or(NativeRuntimeStatus::PENDING)?;

        if outcome.state() != NativeRunState::COMPLETED || outcome.payload() == 0 {
            parent.child = Some(child);

            return Err(NativeRuntimeStatus::RUNTIME_FAILURE);
        }

        let payload = outcome.payload();
        Self::transfer_incidents(&mut child);
        child.next_retired = parent.retired.take();
        parent.retired = Some(child);

        Ok(payload)
    }

    fn drive(&self) -> FrameProgress<usize> {
        for _ in 0..TRANSITION_BUDGET {
            let Ok(execution) = self.execution() else {
                return FrameProgress::RuntimeFailure;
            };

            let Some(current_lane) = current_task_execution_lane() else {
                return FrameProgress::RuntimeFailure;
            };

            let Ok(required_lane) = execution_lane(&execution) else {
                let mut current = self.lock_current();

                let active = Self::active(&mut current);

                if active.parent.is_some() && !active.cleanup {
                    active.discard_parent_execution();
                    active.cleanup = true;

                    active
                        .parent
                        .as_mut()
                        .unwrap_or_else(|| panic!("composed activation retains its parent"))
                        .deferred_progress = Some(FrameProgress::RuntimeFailure);

                    continue;
                }

                return FrameProgress::RuntimeFailure;
            };

            if !lane_allows(current_lane, required_lane) {
                return FrameProgress::Suspended(FrameSuspension::yielding(execution.state()));
            }

            let Some(progress) =
                with_task_frame_execution(&execution, required_lane, || self.step())
            else {
                return FrameProgress::RuntimeFailure;
            };

            if let Some(progress) = progress {
                return progress;
            }
        }

        self.execution()
            .map_or(FrameProgress::RuntimeFailure, |execution| {
                FrameProgress::Suspended(FrameSuspension::yielding(execution.state()))
            })
    }

    fn step(&self) -> Option<FrameProgress<usize>> {
        let (cleanup, deferred_progress) = {
            let mut current = self.lock_current();

            let active = Self::active(&mut current);

            (active.cleanup, active.deferred_progress.take())
        };

        if cleanup {
            return self.cleanup_activation();
        }

        if let Some(progress) = deferred_progress {
            return self.process_progress(progress);
        }

        let mut frame = {
            let mut current = self.lock_current();

            Self::active(&mut current)
                .frame
                .take()
                .unwrap_or_else(|| panic!("dispatch retains the active frame"))
        };

        let progress = catch_unwind(AssertUnwindSafe(|| {
            frame.as_mut().resume(FrameContext::new(
                crate::current_run_cancellation_observable(),
            ))
        }));

        let mut current = self.lock_current();
        let active = Self::active(&mut current);

        let progress = progress.unwrap_or_else(|payload| {
            FrameProgress::Panicked(RuntimePanic::from_payload(payload, &mut active.outgoing))
        });

        active.frame = Some(frame);
        drop(current);

        let progress = self.begin_pending_cleanup(progress)?;

        self.process_progress(progress)
    }

    fn process_progress(&self, progress: FrameProgress<usize>) -> Option<FrameProgress<usize>> {
        match progress {
            FrameProgress::Suspended(suspension) => self.suspend_activation(suspension),
            FrameProgress::RuntimeFailure => Some(FrameProgress::RuntimeFailure),
            progress => self.finish_activation(progress),
        }
    }

    fn begin_pending_cleanup(
        &self,
        progress: FrameProgress<usize>,
    ) -> Option<FrameProgress<usize>> {
        let mut current = self.lock_current();

        let active = Self::active(&mut current);

        if !active.child.as_ref().is_some_and(|child| child.cleanup) {
            return Some(progress);
        }

        active.deferred_progress = Some(progress);

        let mut child = active
            .child
            .take()
            .unwrap_or_else(|| panic!("active activation retains its cleanup child"));

        child.parent = current.take();
        *current = Some(child);

        None
    }

    fn suspend_activation(&self, suspension: FrameSuspension) -> Option<FrameProgress<usize>> {
        let mut current = self.lock_current();

        let active = Self::active(&mut current);

        active.state = suspension.state();

        if suspension.kind() != FrameSuspensionKind::Awaited {
            return Some(FrameProgress::Suspended(suspension));
        }

        let Some(mut child) = active.child.take() else {
            return Some(FrameProgress::Suspended(suspension));
        };

        if child.outcome.is_some() {
            active.child = Some(child);

            return None;
        }

        let execution = active
            .execution_state()
            .unwrap_or_else(|_| panic!("suspension state belongs to the active native frame"));

        child.retain_parent_execution(&execution);
        child.parent = current.take();
        *current = Some(child);

        None
    }

    fn finish_activation(&self, progress: FrameProgress<usize>) -> Option<FrameProgress<usize>> {
        let mut progress = Some(progress);

        {
            let mut current = self.lock_current();

            let active = Self::active(&mut current);

            if active
                .child
                .as_ref()
                .is_some_and(|child| child.frame.is_some())
            {
                active.deferred_progress = progress.take();

                let mut child = active
                    .child
                    .take()
                    .unwrap_or_else(|| panic!("terminal activation retains its pending child"));

                child.cleanup = true;
                child.parent = current.take();
                *current = Some(child);

                return None;
            }
        }

        let progress =
            progress.unwrap_or_else(|| panic!("terminal progress was not deferred to a child"));

        let (frame, child, mut outgoing, terminal, is_root) = {
            let mut current = self.lock_current();

            let active = Self::active(&mut current);

            (
                active.frame.take(),
                active.child.take(),
                std::mem::take(&mut active.outgoing),
                Arc::clone(&active.terminal),
                active.parent.is_none(),
            )
        };

        Self::release_chain(child, &terminal);

        let outcome = terminalize_frame(
            frame.unwrap_or_else(|| panic!("terminal activation retains its frame")),
            progress,
            &mut outgoing,
        );

        if is_root {
            return Some(match outcome {
                Some(RunOutcome::Completed(payload)) => FrameProgress::Completed(payload),
                Some(RunOutcome::Cancelled) => FrameProgress::Cancelled,
                Some(RunOutcome::Panicked(panic)) => FrameProgress::Panicked(panic),
                None => FrameProgress::RuntimeFailure,
            });
        }

        let native = match outcome {
            Some(RunOutcome::Completed(payload)) => {
                NativeRunOutcome::new(NativeRunState::COMPLETED, payload)
            }
            Some(RunOutcome::Cancelled) => NativeRunOutcome::new(NativeRunState::CANCELLED, 0),
            Some(RunOutcome::Panicked(panic)) => NativeRunOutcome::panicked(panic.into_native()),
            None => NativeRunOutcome::new(
                NativeRunState::RUNTIME_FAILURE,
                NativeRuntimeStatus::RUNTIME_FAILURE.code() as usize,
            ),
        };

        let mut current = self.lock_current();

        let mut child = current
            .take()
            .unwrap_or_else(|| panic!("dispatch retains the completed child activation"));

        let mut parent = child
            .parent
            .take()
            .unwrap_or_else(|| panic!("composed child retains its parent activation"));

        child.outcome = Some(native);
        child.outgoing = outgoing;
        parent.child = Some(child);
        *current = Some(parent);

        None
    }

    fn cleanup_activation(&self) -> Option<FrameProgress<usize>> {
        let (frame, mut outgoing, terminal, parent_terminal) = {
            let mut current = self.lock_current();

            let active = Self::active(&mut current);

            assert!(
                active.cleanup,
                "only cleanup activations use cleanup dispatch"
            );

            let parent_terminal = active
                .parent
                .as_ref()
                .map(|parent| Arc::clone(&parent.terminal))
                .unwrap_or_else(|| Arc::clone(&self.terminal));

            (
                active
                    .frame
                    .take()
                    .unwrap_or_else(|| panic!("cleanup activation retains its frame")),
                std::mem::take(&mut active.outgoing),
                Arc::clone(&active.terminal),
                parent_terminal,
            )
        };

        let _ = terminalize_frame(frame, FrameProgress::RuntimeFailure, &mut outgoing);

        let mut current = self.lock_current();

        let mut child = current
            .take()
            .unwrap_or_else(|| panic!("dispatch retains the cleanup child activation"));

        child.outgoing = outgoing;

        if !Self::transfer_incidents(&mut child) {
            Self::forward_incidents(&terminal, &parent_terminal);
        }

        let parent = child
            .parent
            .take()
            .unwrap_or_else(|| panic!("cleanup child retains its parent activation"));

        drop(child);
        *current = Some(parent);

        None
    }

    fn release_chain(
        mut current: Option<Box<NativeActivation>>,
        boundary: &Arc<NativeTerminalState>,
    ) {
        NativeRun::broadcast_pending_frames(&mut current);

        while let Some(mut active) = current {
            if let Some(mut child) = active.child.take() {
                child.parent = Some(active);
                current = Some(child);
                continue;
            }

            if let Some(mut retired) = active.retired.take() {
                retired.parent = Some(active);
                current = Some(retired);
                continue;
            }

            let mut parent = active.parent.take();

            if let Some(frame) = active.frame.take() {
                let _ = terminalize_frame_after_broadcast(
                    frame,
                    FrameProgress::RuntimeFailure,
                    &mut active.outgoing,
                );
            }

            let parent_terminal = parent.as_ref().map_or_else(
                || Arc::clone(boundary),
                |parent| Arc::clone(&parent.terminal),
            );

            let is_boundary = parent.is_none() && Arc::ptr_eq(&active.terminal, boundary);

            if !is_boundary && !Self::transfer_incidents(&mut active) {
                Self::forward_incidents(&active.terminal, &parent_terminal);
            }

            if let Some(mut sibling) = active.next_retired.take() {
                sibling.parent = parent.take();
                current = Some(sibling);
            } else {
                current = parent;
            }
        }
    }

    fn broadcast_pending_frames(current: &mut Option<Box<NativeActivation>>) {
        let mut cursor = current.as_deref_mut();

        while let Some(active) = cursor {
            Self::broadcast_activation(active);

            if let Some(child) = active.child.as_deref_mut() {
                Self::broadcast_activation(child);
            }

            cursor = active.parent.as_deref_mut();
        }
    }

    fn broadcast_activation(active: &mut NativeActivation) {
        if active.broadcasted {
            return;
        }

        if let Some(frame) = active.frame.as_mut() {
            frame.as_mut().broadcast_tasks();
        }

        active.broadcasted = true;
    }

    fn forward_incidents(terminal: &Arc<NativeTerminalState>, parent: &Arc<NativeTerminalState>) {
        if Arc::ptr_eq(terminal, parent) {
            return;
        }

        for incident in terminal.take_cleanup_incidents().into_iter().flatten() {
            parent.record_cleanup_report(incident);
        }
    }

    fn transfer_incidents(active: &mut NativeActivation) -> bool {
        let Some(task) = current_native_task() else {
            return false;
        };

        let origin = crate::CleanupIncidentOrigin::new(active.descriptor.frame(), active.state);
        let incidents = active.terminal.take_cleanup_incidents();

        with_runtime(|runtime| {
            for incident in incidents.into_iter().flatten() {
                runtime.cleanup_reports.transfer(
                    crate::CleanupIncidentProducer::Task(
                        runtime
                            .with_started(task, |started| started.task.id())
                            .unwrap_or_else(|_| panic!("active native run retains its task")),
                    ),
                    origin,
                    incident,
                    &mut active.outgoing,
                );
            }
        })
        .is_ok()
    }
}

impl ProtectedFrame for Arc<NativeRun> {
    type Output = usize;

    fn descriptor(&self) -> &ProtectedFrameDescriptor {
        &self.descriptor
    }

    fn execution_state(&self, _: ProtectedFrameStateId) -> Option<FrameExecutionState> {
        self.execution().ok()
    }

    fn resume(self: Pin<&mut Self>, _: FrameContext) -> FrameProgress<Self::Output> {
        self.drive()
    }

    fn broadcast_tasks(self: Pin<&mut Self>) {
        let mut current = self.lock_current().take();

        NativeRun::broadcast_pending_frames(&mut current);

        *self.lock_current() = current;
    }

    fn resolve_lifecycle(self: Pin<&mut Self>, _: FrameExit) {
        let current = self.lock_current().take();
        NativeRun::release_chain(current, &self.terminal);
    }
}

fn execution_lane(execution: &FrameExecutionState) -> Result<ExecutionLane, NativeRuntimeStatus> {
    let task = current_native_task().ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?;

    with_runtime(|runtime| {
        runtime.with_started(task, |task| {
            task.registration
                .execution_lane(execution)
                .map_err(|_| NativeRuntimeStatus::RUNTIME_FAILURE)
        })
    })
    .and_then(|result| result)
    .and_then(|result| result)
}

fn lane_allows(current: ExecutionLane, required: ExecutionLane) -> bool {
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
