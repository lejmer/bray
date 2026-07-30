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
    move_completion: extern "C" fn(usize, usize),
    destroy: extern "C" fn(usize),
}

unsafe extern "C" {
    safe fn bray_runtime_main_thread_lane_startup_v1(configuration: Configuration) -> Status;
    safe fn bray_runtime_task_allocation_v1() -> TaskAllocation;
    safe fn bray_runtime_task_start_v1(
        task: TaskHandle,
        frame: ProtectedFrame,
    ) -> Status;
    safe fn bray_runtime_main_thread_lane_drive_v1() -> Status;
    safe fn bray_runtime_structured_shutdown_v1() -> Status;
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

extern "C" fn ignore_completion_move(_: usize, _: usize) {}

fn main() {
    assert!(
        bray_runtime_main_thread_lane_startup_v1(Configuration {
            task_capacity: 8,
            timer_capacity: 8,
        }) == Status::SUCCESS
    );

    let allocation = bray_runtime_task_allocation_v1();

    assert!(allocation.status == Status::SUCCESS);

    let task = TaskHandle(allocation.task);

    let frame = ProtectedFrame {
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
        move_completion: ignore_completion_move,
        destroy: ignore_action,
    };

    assert!(bray_runtime_task_start_v1(task, frame) == Status::SUCCESS);
    assert!(bray_runtime_main_thread_lane_drive_v1() == Status::SUCCESS);
    assert!(bray_runtime_structured_shutdown_v1() == Status::SUCCESS);
}
