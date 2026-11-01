use bray_platform::{RuntimeThread, RuntimeThreadId};
use bray_runtime_abi::{
    NativeSchedulerLane, NativeSchedulerObservation, NativeSchedulerReference,
    NativeSchedulerRequirements,
};
use bray_runtime_model::{
    ExecutionLaneRequirement, ProtectedFrameAffinity, ProtectedFrameStateDescriptor,
    RuntimeCapability,
};

use super::contract::SchedulerError;
use crate::{
    ExecutionLane, ExecutionLanePlacement, ExecutionLaneSelectionError, ExecutionWorkload,
    TaskWakeCause,
};

// Every reference names a Bray-created domain retained by SchedulerData. Native
// callers lend real initialized slots/slices and never copy the opaque owner.
#[expect(
    unsafe_code,
    reason = "the Bray provider owns scheduler storage and transitions"
)]
unsafe extern "C" {
    #[link_name = "bray_runtime_scheduler_creation"]
    safe fn bray_runtime_scheduler_creation_native(
        capabilities: u32,
        main: u64,
        limit: usize,
        output: &mut NativeSchedulerReference,
    ) -> u32;
    #[link_name = "bray_runtime_scheduler_destruction"]
    safe fn bray_runtime_scheduler_destruction_native(reference: &NativeSchedulerReference);
    #[link_name = "bray_runtime_scheduler_selection"]
    safe fn bray_runtime_scheduler_selection_native(
        reference: &NativeSchedulerReference,
        requirements: &NativeSchedulerRequirements,
        output: &mut NativeSchedulerLane,
    ) -> u32;
    #[link_name = "bray_runtime_scheduler_compatibility"]
    safe fn bray_runtime_scheduler_compatibility_native(
        current: &NativeSchedulerLane,
        required: &NativeSchedulerRequirements,
        thread: u64,
    ) -> u32;
    #[link_name = "bray_runtime_scheduler_admission"]
    safe fn bray_runtime_scheduler_admission_native(
        reference: &NativeSchedulerReference,
        task: u64,
        lane: &NativeSchedulerLane,
        output: &mut u64,
    ) -> u32;
    #[link_name = "bray_runtime_scheduler_withdrawal"]
    safe fn bray_runtime_scheduler_withdrawal_native(
        reference: &NativeSchedulerReference,
        registration: u64,
    );
    #[link_name = "bray_runtime_scheduler_wake"]
    safe fn bray_runtime_scheduler_wake_native(
        reference: &NativeSchedulerReference,
        registration: u64,
        cause: u32,
        now: u64,
    ) -> u32;
    #[link_name = "bray_runtime_scheduler_acquisition"]
    safe fn bray_runtime_scheduler_acquisition_native(
        reference: &NativeSchedulerReference,
        thread: u64,
        lanes: u32,
        output: &mut NativeSchedulerObservation,
    ) -> u32;
    #[link_name = "bray_runtime_scheduler_release"]
    safe fn bray_runtime_scheduler_release_native(
        reference: &NativeSchedulerReference,
        registration: u64,
        lane: &NativeSchedulerLane,
        cancellation: u32,
        now: u64,
    ) -> u32;
    #[link_name = "bray_runtime_scheduler_snapshot"]
    safe fn bray_runtime_scheduler_snapshot_native(
        reference: &NativeSchedulerReference,
        registration: u64,
        output: &mut NativeSchedulerObservation,
    ) -> u32;
    #[link_name = "bray_runtime_scheduler_parking_registration"]
    safe fn bray_runtime_scheduler_parking_registration_native(
        reference: &NativeSchedulerReference,
    ) -> u32;
    #[link_name = "bray_runtime_scheduler_parking_withdrawal"]
    safe fn bray_runtime_scheduler_parking_withdrawal_native(
        reference: &NativeSchedulerReference,
        group: u32,
    );
    #[link_name = "bray_runtime_scheduler_parking"]
    safe fn bray_runtime_scheduler_parking_native(
        reference: &NativeSchedulerReference,
        group: u32,
        milliseconds: u32,
    );
    #[link_name = "bray_runtime_scheduler_notification"]
    safe fn bray_runtime_scheduler_notification_native(reference: &NativeSchedulerReference);
    #[link_name = "bray_runtime_scheduler_slot_release"]
    safe fn bray_runtime_scheduler_slot_release_native(
        reference: &NativeSchedulerReference,
        registration: u64,
    ) -> u32;
    #[link_name = "bray_runtime_scheduler_slot_acquisition"]
    safe fn bray_runtime_scheduler_slot_acquisition_native(
        reference: &NativeSchedulerReference,
        registration: u64,
    ) -> u32;
}

// These calls contain no source or host callbacks. Their guard cleanup belongs
// to runtime infrastructure, independently of the invoking run's cancellation
// and lexical cleanup shields. The existing scope restores that run on return.
pub(super) fn bray_runtime_scheduler_creation(
    capabilities: u32,
    main: u64,
    limit: usize,
    output: &mut NativeSchedulerReference,
) -> u32 {
    crate::context::with_independent_execution_context(|| {
        bray_runtime_scheduler_creation_native(capabilities, main, limit, output)
    })
}

pub(super) fn bray_runtime_scheduler_destruction(reference: &NativeSchedulerReference) {
    crate::context::with_independent_execution_context(|| {
        bray_runtime_scheduler_destruction_native(reference)
    })
}

pub(super) fn bray_runtime_scheduler_selection(
    reference: &NativeSchedulerReference,
    requirements: &NativeSchedulerRequirements,
    output: &mut NativeSchedulerLane,
) -> u32 {
    crate::context::with_independent_execution_context(|| {
        bray_runtime_scheduler_selection_native(reference, requirements, output)
    })
}

pub(crate) fn bray_runtime_scheduler_compatibility(
    current: &NativeSchedulerLane,
    required: &NativeSchedulerRequirements,
    thread: u64,
) -> u32 {
    crate::context::with_independent_execution_context(|| {
        bray_runtime_scheduler_compatibility_native(current, required, thread)
    })
}

pub(super) fn bray_runtime_scheduler_admission(
    reference: &NativeSchedulerReference,
    task: u64,
    lane: &NativeSchedulerLane,
    output: &mut u64,
) -> u32 {
    crate::context::with_independent_execution_context(|| {
        bray_runtime_scheduler_admission_native(reference, task, lane, output)
    })
}

pub(super) fn bray_runtime_scheduler_withdrawal(
    reference: &NativeSchedulerReference,
    registration: u64,
) {
    crate::context::with_independent_execution_context(|| {
        bray_runtime_scheduler_withdrawal_native(reference, registration)
    })
}

pub(super) fn bray_runtime_scheduler_wake(
    reference: &NativeSchedulerReference,
    registration: u64,
    cause: u32,
    now: u64,
) -> u32 {
    crate::context::with_independent_execution_context(|| {
        bray_runtime_scheduler_wake_native(reference, registration, cause, now)
    })
}

pub(super) fn bray_runtime_scheduler_acquisition(
    reference: &NativeSchedulerReference,
    thread: u64,
    lanes: u32,
    output: &mut NativeSchedulerObservation,
) -> u32 {
    crate::context::with_independent_execution_context(|| {
        bray_runtime_scheduler_acquisition_native(reference, thread, lanes, output)
    })
}

pub(super) fn bray_runtime_scheduler_release(
    reference: &NativeSchedulerReference,
    registration: u64,
    lane: &NativeSchedulerLane,
    cancellation: u32,
    now: u64,
) -> u32 {
    crate::context::with_independent_execution_context(|| {
        bray_runtime_scheduler_release_native(reference, registration, lane, cancellation, now)
    })
}

pub(super) fn bray_runtime_scheduler_snapshot(
    reference: &NativeSchedulerReference,
    registration: u64,
    output: &mut NativeSchedulerObservation,
) -> u32 {
    crate::context::with_independent_execution_context(|| {
        bray_runtime_scheduler_snapshot_native(reference, registration, output)
    })
}

pub(super) fn bray_runtime_scheduler_parking_registration(
    reference: &NativeSchedulerReference,
) -> u32 {
    crate::context::with_independent_execution_context(|| {
        bray_runtime_scheduler_parking_registration_native(reference)
    })
}

pub(super) fn bray_runtime_scheduler_parking_withdrawal(
    reference: &NativeSchedulerReference,
    group: u32,
) {
    crate::context::with_independent_execution_context(|| {
        bray_runtime_scheduler_parking_withdrawal_native(reference, group)
    })
}

pub(super) fn bray_runtime_scheduler_parking(
    reference: &NativeSchedulerReference,
    group: u32,
    milliseconds: u32,
) {
    crate::context::with_independent_execution_context(|| {
        bray_runtime_scheduler_parking_native(reference, group, milliseconds)
    })
}

pub(super) fn bray_runtime_scheduler_notification(reference: &NativeSchedulerReference) {
    crate::context::with_independent_execution_context(|| {
        bray_runtime_scheduler_notification_native(reference)
    })
}

pub(super) fn bray_runtime_scheduler_slot_release(
    reference: &NativeSchedulerReference,
    registration: u64,
) -> u32 {
    crate::context::with_independent_execution_context(|| {
        bray_runtime_scheduler_slot_release_native(reference, registration)
    })
}

pub(super) fn bray_runtime_scheduler_slot_acquisition(
    reference: &NativeSchedulerReference,
    registration: u64,
) -> u32 {
    crate::context::with_independent_execution_context(|| {
        bray_runtime_scheduler_slot_acquisition_native(reference, registration)
    })
}

#[derive(Debug)]
pub(super) struct BrayScheduler {
    pub(super) reference: NativeSchedulerReference,
}

impl BrayScheduler {
    pub(super) fn new(
        capabilities: impl IntoIterator<Item = RuntimeCapability>,
        main: RuntimeThreadId,
        limit: usize,
    ) -> Result<Self, SchedulerError> {
        let capabilities = capabilities.into_iter().fold(0, |bits, capability| {
            bits | match capability {
                RuntimeCapability::CooperativeExecution => 1,
                RuntimeCapability::LocalLanes => 2,
                RuntimeCapability::MigratableLanes => 4,
                RuntimeCapability::BlockingLanes => 8,
                RuntimeCapability::ComputeLanes => 16,
                RuntimeCapability::MainThreadLane => 32,
                RuntimeCapability::PerformanceObservation | RuntimeCapability::Reactor => 0,
            }
        });

        let mut reference = NativeSchedulerReference::default();

        let status =
            bray_runtime_scheduler_creation(capabilities, main.raw(), limit, &mut reference);

        if status != 0 {
            return Err(SchedulerError::BackingUnavailable);
        }

        Ok(Self { reference })
    }

    pub(super) fn select(
        &self,
        descriptor: &ProtectedFrameStateDescriptor,
        origin: RuntimeThreadId,
    ) -> Result<ExecutionLane, SchedulerError> {
        let requirements = NativeSchedulerRequirements {
            affinity: match descriptor.affinity() {
                ProtectedFrameAffinity::Movable => 0,
                ProtectedFrameAffinity::OriginThread => 1,
                ProtectedFrameAffinity::MainThread => 2,
            },
            workloads: descriptor
                .lane_requirements()
                .iter()
                .fold(0, |bits, requirement| {
                    bits | match requirement {
                        ExecutionLaneRequirement::Blocking => 1,
                        ExecutionLaneRequirement::Compute => 2,
                        ExecutionLaneRequirement::MainThread => 4,
                    }
                }),
            origin: origin.raw(),
        };

        let mut lane = NativeSchedulerLane::default();
        let status = bray_runtime_scheduler_selection(&self.reference, &requirements, &mut lane);

        let capability = match status {
            0 => return Ok(from_native(lane)),
            1 => RuntimeCapability::CooperativeExecution,
            2 => RuntimeCapability::LocalLanes,
            3 => RuntimeCapability::BlockingLanes,
            4 => RuntimeCapability::ComputeLanes,
            5 => RuntimeCapability::MainThreadLane,
            6 => return Err(ExecutionLaneSelectionError::ConflictingAffinity.into()),
            _ => panic!("Bray lane selection returned unknown status {status}"),
        };

        Err(ExecutionLaneSelectionError::MissingCapability(capability).into())
    }
}

impl Drop for BrayScheduler {
    fn drop(&mut self) {
        bray_runtime_scheduler_destruction(&self.reference);
    }
}

pub(crate) fn to_native(lane: ExecutionLane) -> NativeSchedulerLane {
    let (placement, thread) = match lane.placement() {
        ExecutionLanePlacement::Migratable => (0, 0),
        ExecutionLanePlacement::PinnedWorker(thread) => (1, thread.raw()),
        ExecutionLanePlacement::OriginThread(thread) => (2, thread.raw()),
        ExecutionLanePlacement::MainThread(thread) => (3, thread.raw()),
    };

    NativeSchedulerLane {
        placement,
        thread,
        workload: match lane.workload() {
            ExecutionWorkload::Cooperative => 0,
            ExecutionWorkload::Blocking => 1,
            ExecutionWorkload::Compute => 2,
            ExecutionWorkload::BlockingCompute => 3,
        },
    }
}

pub(super) fn from_native(lane: NativeSchedulerLane) -> ExecutionLane {
    let thread = || {
        RuntimeThread::from_native_identity(lane.thread)
            .expect("Bray selected a retained nonzero lane attachment")
            .id()
    };

    let placement = match lane.placement {
        0 => ExecutionLanePlacement::Migratable,
        1 => ExecutionLanePlacement::PinnedWorker(thread()),
        2 => ExecutionLanePlacement::OriginThread(thread()),
        3 => ExecutionLanePlacement::MainThread(thread()),
        placement => panic!("Bray selected unknown lane placement {placement}"),
    };

    let workload = match lane.workload {
        0 => ExecutionWorkload::Cooperative,
        1 => ExecutionWorkload::Blocking,
        2 => ExecutionWorkload::Compute,
        3 => ExecutionWorkload::BlockingCompute,
        workload => panic!("Bray selected unknown workload {workload}"),
    };

    ExecutionLane::new(placement, workload)
}

pub(super) const fn cause(cause: TaskWakeCause) -> u32 {
    match cause {
        TaskWakeCause::Explicit => 1,
        TaskWakeCause::Timer => 2,
        TaskWakeCause::Cancellation => 3,
    }
}

pub(super) fn wake_cause(cause: u32) -> Option<TaskWakeCause> {
    match cause {
        0 => None,
        1 => Some(TaskWakeCause::Explicit),
        2 => Some(TaskWakeCause::Timer),
        3 => Some(TaskWakeCause::Cancellation),
        _ => panic!("Bray returned unknown wake cause {cause}"),
    }
}

pub(super) fn lane_bit(lane: ExecutionLane, thread: RuntimeThreadId) -> u32 {
    let lane = to_native(lane);

    if lane.placement != 0 && lane.thread != thread.raw() {
        return 0;
    }

    1 << (lane.placement * 4 + lane.workload)
}
