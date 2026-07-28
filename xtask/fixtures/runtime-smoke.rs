#[repr(transparent)]
#[derive(Clone, Copy, Eq, PartialEq)]
struct Status(u32);

impl Status {
    const SUCCESS: Self = Self(0);
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Configuration {
    task_capacity: usize,
    timer_capacity: usize,
}

#[repr(transparent)]
#[derive(Clone, Copy, Eq, PartialEq)]
struct RunState(u32);

impl RunState {
    const COMPLETED: Self = Self(0);
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RunOutcome {
    state: RunState,
    payload: usize,
}

#[repr(transparent)]
#[derive(Clone, Copy)]
struct TaskHandle(u64);

#[repr(C)]
#[derive(Clone, Copy)]
struct TaskAllocation {
    status: Status,
    task: u64,
}

#[repr(transparent)]
#[derive(Clone, Copy)]
struct FrameAffinity(u32);

#[repr(transparent)]
#[derive(Clone, Copy)]
struct LaneRequirements(u32);

#[repr(C)]
#[derive(Clone, Copy)]
struct FrameState {
    affinity: FrameAffinity,
    lane_requirements: LaneRequirements,
}

#[repr(transparent)]
#[derive(Clone, Copy)]
struct FrameProgressKind(u32);

#[repr(C)]
#[derive(Clone, Copy)]
struct FrameProgress {
    kind: FrameProgressKind,
    state: u32,
    payload: usize,
}

#[repr(transparent)]
#[derive(Clone, Copy)]
struct FrameExit(u32);

#[repr(C)]
#[derive(Clone, Copy)]
struct ProtectedFrame {
    context: usize,
    identity: [u8; 32],
    state_count: u32,
    size: usize,
    alignment: usize,
    completion_size: usize,
    completion_alignment: usize,
    state: extern "C" fn(usize, u32) -> FrameState,
    resume: extern "C" fn(usize, u8) -> FrameProgress,
    broadcast_tasks: extern "C" fn(usize),
    resolve_lifecycle: extern "C" fn(usize, FrameExit),
    destroy: extern "C" fn(usize),
}

unsafe extern "C" {
    safe fn bray_runtime_main_thread_lane_startup_v1(configuration: Configuration) -> Status;
    safe fn bray_runtime_root_execution_v1(
        callback: extern "C" fn(usize) -> RunOutcome,
        context: usize,
    ) -> RunOutcome;
    safe fn bray_runtime_task_allocation_v1(frame: ProtectedFrame) -> TaskAllocation;
    safe fn bray_runtime_task_start_v1(task: TaskHandle) -> Status;
    safe fn bray_runtime_main_thread_lane_drive_v1() -> Status;
    safe fn bray_runtime_join_registration_v1(
        task: TaskHandle,
        callback: extern "C" fn(usize),
        context: usize,
    ) -> RunOutcome;
    safe fn bray_runtime_structured_shutdown_v1() -> Status;
}

extern "C" fn root(context: usize) -> RunOutcome {
    RunOutcome {
        state: RunState::COMPLETED,
        payload: context + 1,
    }
}

extern "C" fn frame_state(_: usize, _: u32) -> FrameState {
    FrameState {
        affinity: FrameAffinity(2),
        lane_requirements: LaneRequirements(1 << 2),
    }
}

extern "C" fn resume_frame(_: usize, _: u8) -> FrameProgress {
    FrameProgress {
        kind: FrameProgressKind(1),
        state: 0,
        payload: 17,
    }
}

extern "C" fn ignore_action(_: usize) {}

extern "C" fn ignore_resolution(_: usize, _: FrameExit) {}

extern "C" fn ignore_wake(_: usize) {}

fn main() {
    assert!(
        bray_runtime_main_thread_lane_startup_v1(Configuration {
            task_capacity: 8,
            timer_capacity: 8,
        }) == Status::SUCCESS
    );

    let outcome = bray_runtime_root_execution_v1(root, 41);

    assert!(outcome.state == RunState::COMPLETED);
    assert!(outcome.payload == 42);

    let allocation = bray_runtime_task_allocation_v1(ProtectedFrame {
        context: 0,
        identity: [7; 32],
        state_count: 1,
        size: 8,
        alignment: 8,
        completion_size: 8,
        completion_alignment: 8,
        state: frame_state,
        resume: resume_frame,
        broadcast_tasks: ignore_action,
        resolve_lifecycle: ignore_resolution,
        destroy: ignore_action,
    });

    assert!(allocation.status == Status::SUCCESS);

    let task = TaskHandle(allocation.task);

    assert!(bray_runtime_task_start_v1(task) == Status::SUCCESS);
    assert!(bray_runtime_main_thread_lane_drive_v1() == Status::SUCCESS);

    let task_outcome = bray_runtime_join_registration_v1(task, ignore_wake, 0);

    assert!(task_outcome.state == RunState::COMPLETED);
    assert!(task_outcome.payload == 17);
    assert!(bray_runtime_structured_shutdown_v1() == Status::SUCCESS);
}
