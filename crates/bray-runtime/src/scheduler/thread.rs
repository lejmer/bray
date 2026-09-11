use std::sync::{Arc, Weak};

use bray_platform::RuntimeThreadId;
use bray_runtime_model::RuntimeCapability;

use crate::{ExecutionLane, ExecutionLanePlacement, ExecutionWorkload};

use super::contract::SchedulerError;
use super::engine::{Scheduler, SchedulerData};

/// Keeps admitted origin-thread queue headers alive for one attached runtime thread.
pub(crate) struct ThreadLaneRegistration {
    scheduler: Weak<SchedulerData>,
    lanes: [ExecutionLane; 3],
    count: usize,
}

impl Scheduler {
    /// Reserves supported local queues before a runtime thread starts executing frames.
    pub(crate) fn register_thread_lanes(
        &self,
        thread: RuntimeThreadId,
    ) -> Result<ThreadLaneRegistration, SchedulerError> {
        let mut registration = ThreadLaneRegistration {
            scheduler: Arc::downgrade(&self.data),
            lanes: [ExecutionLane::new(
                ExecutionLanePlacement::OriginThread(thread),
                ExecutionWorkload::Cooperative,
            ); 3],
            count: 0,
        };

        if !self
            .data
            .capabilities
            .contains(&RuntimeCapability::LocalLanes)
        {
            return Ok(registration);
        }

        for (capability, workload) in [
            (
                RuntimeCapability::CooperativeExecution,
                ExecutionWorkload::Cooperative,
            ),
            (
                RuntimeCapability::BlockingLanes,
                ExecutionWorkload::Blocking,
            ),
            (RuntimeCapability::ComputeLanes, ExecutionWorkload::Compute),
        ] {
            if self.data.capabilities.contains(&capability) {
                registration.lanes[registration.count] =
                    ExecutionLane::new(ExecutionLanePlacement::OriginThread(thread), workload);

                registration.count += 1;
            }
        }

        let count = registration.count;
        registration.count = 0;
        let mut state = self.lock_state()?;

        let missing = registration.lanes[..count]
            .iter()
            .filter(|lane| !state.queues.contains_key(lane))
            .count();

        let additional = state
            .cleanup_lanes
            .checked_add(missing)
            .ok_or(SchedulerError::ReadyQueueCapacityReached)?;

        // Worker headers must not consume capacity promised to already admitted cleanup roots.
        crate::allocation::reserve_map_entries(&mut state.queues, additional)?;
        state.reserve_ready_queues(&registration.lanes[..count])?;
        registration.count = count;

        Ok(registration)
    }
}

impl Drop for ThreadLaneRegistration {
    fn drop(&mut self) {
        if self.count == 0 {
            return;
        }

        let Some(scheduler) = self.scheduler.upgrade() else {
            return;
        };

        let mut state = scheduler
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        state.release_ready_queues(&self.lanes[..self.count]);
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use bray_platform::{RuntimeThreadId, RuntimeThreadScope};
    use bray_runtime_model::{
        ExecutionLaneRequirement, ProtectedAsyncFrameId, ProtectedFrameAffinity,
        ProtectedFrameStateDescriptor, ProtectedFrameStateId, RuntimeCapability,
    };

    use crate::test_support::{TestFrame, register_task, with_allocation_failure};
    use crate::{
        ExecutionLane, ExecutionLanePlacement, ExecutionWorkload, FrameExecutionState, Scheduler,
        SchedulerError, SchedulerLimits, TaskControlBlock,
    };

    fn child_execution(
        origin: RuntimeThreadId,
        requirements: impl IntoIterator<Item = ExecutionLaneRequirement>,
        affinity: ProtectedFrameAffinity,
    ) -> FrameExecutionState {
        FrameExecutionState::new(
            ProtectedAsyncFrameId::new([93; 32]),
            ProtectedFrameStateDescriptor::new(
                ProtectedFrameStateId::new(42),
                requirements,
                [],
                [],
                affinity,
            ),
        )
        .with_origin(origin)
    }

    #[test]
    fn origin_headers_are_required_and_queued_runs_retain_them_after_thread_release() {
        let runtime = RuntimeThreadScope::enter().unwrap();
        let root_origin = runtime.runtime().id();

        let origin = std::thread::spawn(|| RuntimeThreadScope::enter().unwrap().runtime().id())
            .join()
            .unwrap();

        let scheduler = Scheduler::new(
            [
                RuntimeCapability::CooperativeExecution,
                RuntimeCapability::MigratableLanes,
                RuntimeCapability::LocalLanes,
                RuntimeCapability::BlockingLanes,
                RuntimeCapability::MainThreadLane,
            ],
            root_origin,
            SchedulerLimits::new(NonZeroUsize::new(1).unwrap(), NonZeroUsize::new(1).unwrap()),
        );

        let task = TaskControlBlock::start(TestFrame::completing(1)).unwrap();
        let registration = register_task(&scheduler, &task, root_origin);
        let migratable = registration.lane(ProtectedFrameStateId::new(0)).unwrap();

        let local = ExecutionLane::new(
            ExecutionLanePlacement::OriginThread(origin),
            ExecutionWorkload::Cooperative,
        );

        let child = child_execution(origin, [], ProtectedFrameAffinity::OriginThread);
        let wake = registration.wake_handle();
        wake.wake().unwrap();
        let ready = scheduler.take_ready(migratable).unwrap().unwrap();

        assert!(
            matches!(ready.suspend(child.clone()), Err(SchedulerError::MissingReadyQueue(lane)) if lane == local)
        );

        assert!(!scheduler.retains_thread(origin).unwrap());
        assert!(scheduler.take_ready(local).unwrap().is_none());
        wake.wake().unwrap();
        let ready = scheduler.take_ready(migratable).unwrap().unwrap();
        assert_eq!(ready.execution_state().frame(), task.descriptor().frame());

        let headers = scheduler.register_thread_lanes(origin).unwrap();
        assert_eq!(registration.execution_lane(&child).unwrap(), local);

        for rejected in [
            child_execution(
                origin,
                [ExecutionLaneRequirement::Blocking],
                ProtectedFrameAffinity::OriginThread,
            ),
            child_execution(root_origin, [], ProtectedFrameAffinity::MainThread),
        ] {
            assert!(matches!(
                registration.execution_lane(&rejected),
                Err(SchedulerError::MissingReadyQueue(_))
            ));
        }

        wake.wake().unwrap();
        with_allocation_failure(|| ready.suspend(child.clone())).unwrap();
        assert!(scheduler.retains_thread(origin).unwrap());
        assert!(!scheduler.retains_thread(root_origin).unwrap());
        with_allocation_failure(|| drop(headers));
        assert!(scheduler.lock_state().unwrap().queues.contains_key(&local));
        let ready = scheduler.take_ready(local).unwrap().unwrap();
        assert_eq!(ready.execution_state(), &child);
        with_allocation_failure(|| ready.suspend(child.clone())).unwrap();
        wake.wake().unwrap();
        let ready = scheduler.take_ready(local).unwrap().unwrap();
        assert_eq!(ready.execution_state(), &child);

        {
            let execution = ready.execution_state().clone();

            ready.complete(execution)
        }
        .unwrap();

        assert!(!scheduler.retains_thread(origin).unwrap());
        with_allocation_failure(|| drop(registration));
        assert!(!scheduler.lock_state().unwrap().queues.contains_key(&local));
    }
}
