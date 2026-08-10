use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

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

#[repr(transparent)]
#[derive(Clone, Copy)]
struct RootHandle(u64);

#[repr(C)]
#[derive(Clone, Copy)]
struct RootStart {
    status: Status,
    root: u64,
}

#[repr(transparent)]
#[derive(Clone, Copy, Eq, PartialEq)]
struct RunState(u32);

impl RunState {
    const COMPLETED: Self = Self(0);
    const CANCELLED: Self = Self(1);
    const PANICKED: Self = Self(2);
    const RUNTIME_FAILURE: Self = Self(3);
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RunOutcome {
    state: RunState,
    payload: usize,
}

#[repr(transparent)]
#[derive(Clone, Copy)]
struct PanicCause(u32);

impl PanicCause {
    const MESSAGE: Self = Self(0);
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SourceAnchor {
    present: u32,
    source: u32,
    start: u32,
    end: u32,
    version: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct StringView {
    data: *const u8,
    length: usize,
}

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
    resume: extern "C-unwind" fn(usize) -> FrameProgress,
    cancel: extern "C-unwind" fn(usize) -> FrameProgress,
    broadcast_tasks: extern "C-unwind" fn(usize),
    resolve_lifecycle: extern "C-unwind" fn(usize, FrameExit),
    move_completion: extern "C-unwind" fn(usize, usize),
    destroy: extern "C-unwind" fn(usize),
}

#[repr(transparent)]
struct ProtectedFrameTransfer(usize);

unsafe extern "C" {
    safe fn bray_runtime_root_execution_v1(
        frame: ProtectedFrameTransfer,
        configuration: Configuration,
    ) -> RootStart;
    safe fn bray_runtime_root_cancellation_request_v1(root: RootHandle) -> Status;
    safe fn bray_runtime_root_terminal_observation_v1(root: RootHandle) -> RunOutcome;
    safe fn bray_runtime_root_completion_resolution_v1(root: RootHandle) -> Status;
    safe fn bray_runtime_cleanup_incident_reporting_v1() -> Status;
    safe fn bray_runtime_panic_report_construction_v1(
        cause: PanicCause,
        source: SourceAnchor,
        message: StringView,
    ) -> usize;
    safe fn bray_runtime_panic_reporting_v1(payload: usize) -> Status;
    safe fn bray_runtime_entry_failure_reporting_v1(payload: usize, size: usize) -> Status;
    safe fn bray_runtime_wake_v1(task: TaskHandle, state: u32) -> Status;
    safe fn bray_runtime_main_thread_lane_startup_v1(configuration: Configuration) -> Status;
    safe fn bray_runtime_task_allocation_v1() -> TaskAllocation;
    safe fn bray_runtime_task_start_v1(
        task: TaskHandle,
        frame: ProtectedFrameTransfer,
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

extern "C-unwind" fn resume_frame(_: usize) -> FrameProgress {
    FrameProgress {
        kind: FrameProgressKind(1),
        state: 0,
        payload: 17,
    }
}

static ROOT: AtomicU64 = AtomicU64::new(0);
static RESUMES: AtomicUsize = AtomicUsize::new(0);
static CANCELLATIONS: AtomicUsize = AtomicUsize::new(0);
static FAILURE_ROOT: AtomicU64 = AtomicU64::new(0);
static FAILURE_RESUMES: AtomicUsize = AtomicUsize::new(0);
static FAILURE_CLEANUP: AtomicUsize = AtomicUsize::new(0);

extern "C-unwind" fn cancel_frame(_: usize) -> FrameProgress {
    CANCELLATIONS.fetch_add(1, Ordering::Relaxed);

    FrameProgress {
        kind: FrameProgressKind(2),
        state: 0,
        payload: 0,
    }
}

extern "C-unwind" fn suspend_and_wake(_: usize) -> FrameProgress {
    if RESUMES.fetch_add(1, Ordering::Relaxed) == 0 {
        assert!(
            bray_runtime_wake_v1(TaskHandle(ROOT.load(Ordering::Relaxed)), 1)
                == Status::SUCCESS
        );

        return FrameProgress {
            kind: FrameProgressKind(0),
            state: 1,
            payload: 0,
        };
    }

    resume_frame(0)
}

extern "C-unwind" fn suspend(_: usize) -> FrameProgress {
    FrameProgress {
        kind: FrameProgressKind(0),
        state: 1,
        payload: 0,
    }
}

extern "C-unwind" fn suspend_then_fail(_: usize) -> FrameProgress {
    if FAILURE_RESUMES.fetch_add(1, Ordering::Relaxed) == 0 {
        assert!(
            bray_runtime_wake_v1(
                TaskHandle(FAILURE_ROOT.load(Ordering::Relaxed)),
                1,
            ) == Status::SUCCESS
        );

        return FrameProgress {
            kind: FrameProgressKind(0),
            state: 1,
            payload: 0,
        };
    }

    FrameProgress {
        kind: FrameProgressKind(4),
        state: 0,
        payload: 0,
    }
}

extern "C-unwind" fn panic_frame(_: usize) -> FrameProgress {
    const MESSAGE: &[u8] = b"runtime smoke panic";

    FrameProgress {
        kind: FrameProgressKind(3),
        state: 0,
        payload: bray_runtime_panic_report_construction_v1(
            PanicCause::MESSAGE,
            SourceAnchor {
                present: 1,
                source: 0,
                start: 0,
                end: 1,
                version: 0,
            },
            StringView {
                data: MESSAGE.as_ptr(),
                length: MESSAGE.len(),
            },
        ),
    }
}

extern "C-unwind" fn ignore_action(_: usize) {}

extern "C-unwind" fn fail_action(_: usize) {
    panic!("cleanup callback failure");
}

extern "C-unwind" fn ignore_resolution(_: usize, _: FrameExit) {}

extern "C-unwind" fn record_failure_resolution(_: usize, _: FrameExit) {
    FAILURE_CLEANUP.fetch_add(1, Ordering::Relaxed);
}

extern "C-unwind" fn ignore_completion_move(_: usize, _: usize) {}

extern "C-unwind" fn record_failure_action(_: usize) {
    FAILURE_CLEANUP.fetch_add(1, Ordering::Relaxed);
}

fn protected_frame(
    identity: u8,
    resume: extern "C-unwind" fn(usize) -> FrameProgress,
    cancel: extern "C-unwind" fn(usize) -> FrameProgress,
    action: extern "C-unwind" fn(usize),
) -> ProtectedFrame {
    ProtectedFrame {
        context: 0,
        identity: [identity; 32],
        state_count: 2,
        size: 8,
        alignment: 8,
        completion_size: 8,
        completion_alignment: 8,
        state: frame_state,
        resume,
        cancel,
        broadcast_tasks: action,
        resolve_lifecycle: ignore_resolution,
        move_completion: ignore_completion_move,
        destroy: action,
    }
}

fn start_root(frame: ProtectedFrame) -> RootHandle {
    let transfer = ProtectedFrameTransfer(&frame as *const ProtectedFrame as usize);

    let start = bray_runtime_root_execution_v1(
        transfer,
        Configuration {
            task_capacity: 8,
            timer_capacity: 8,
        },
    );

    assert!(start.status == Status::SUCCESS);

    RootHandle(start.root)
}

fn main() {
    let root = start_root(protected_frame(
        7,
        suspend_and_wake,
        cancel_frame,
        ignore_action,
    ));

    ROOT.store(root.0, Ordering::Relaxed);

    let outcome = bray_runtime_root_terminal_observation_v1(root);

    assert!(outcome.state == RunState::COMPLETED);
    assert!(RESUMES.load(Ordering::Relaxed) == 2);
    assert!(bray_runtime_root_completion_resolution_v1(root) == Status::SUCCESS);
    assert!(bray_runtime_structured_shutdown_v1() == Status::SUCCESS);

    let root = start_root(protected_frame(8, suspend, cancel_frame, ignore_action));

    assert!(bray_runtime_main_thread_lane_drive_v1() == Status::SUCCESS);
    assert!(bray_runtime_root_cancellation_request_v1(root) == Status::SUCCESS);

    let outcome = bray_runtime_root_terminal_observation_v1(root);

    assert!(outcome.state == RunState::CANCELLED);
    assert!(CANCELLATIONS.load(Ordering::Relaxed) == 1);
    assert!(bray_runtime_root_completion_resolution_v1(root) == Status::SUCCESS);
    assert!(bray_runtime_structured_shutdown_v1() == Status::SUCCESS);

    let root = start_root(protected_frame(
        9,
        panic_frame,
        cancel_frame,
        ignore_action,
    ));

    let outcome = bray_runtime_root_terminal_observation_v1(root);

    assert!(outcome.state == RunState::PANICKED);
    assert!(bray_runtime_panic_reporting_v1(outcome.payload) == Status::SUCCESS);
    assert!(bray_runtime_root_completion_resolution_v1(root) == Status::SUCCESS);
    assert!(bray_runtime_structured_shutdown_v1() == Status::SUCCESS);

    let root = start_root(ProtectedFrame {
        context: 0,
        identity: [10; 32],
        state_count: 2,
        size: 8,
        alignment: 8,
        completion_size: 8,
        completion_alignment: 8,
        state: frame_state,
        resume: suspend_then_fail,
        cancel: cancel_frame,
        broadcast_tasks: record_failure_action,
        resolve_lifecycle: record_failure_resolution,
        move_completion: ignore_completion_move,
        destroy: record_failure_action,
    });

    FAILURE_ROOT.store(root.0, Ordering::Relaxed);

    let outcome = bray_runtime_root_terminal_observation_v1(root);

    assert!(outcome.state == RunState::RUNTIME_FAILURE);
    assert!(FAILURE_RESUMES.load(Ordering::Relaxed) == 2);
    assert!(FAILURE_CLEANUP.load(Ordering::Relaxed) == 3);
    assert!(bray_runtime_root_completion_resolution_v1(root) == Status::SUCCESS);
    assert!(bray_runtime_structured_shutdown_v1() == Status::SUCCESS);

    let root = start_root(protected_frame(10, resume_frame, cancel_frame, fail_action));

    let outcome = bray_runtime_root_terminal_observation_v1(root);

    assert!(outcome.state == RunState::COMPLETED);
    assert!(bray_runtime_cleanup_incident_reporting_v1() == Status::SUCCESS);
    assert!(bray_runtime_root_completion_resolution_v1(root) == Status::SUCCESS);
    assert!(bray_runtime_structured_shutdown_v1() == Status::SUCCESS);

    let failure = 42_i32;

    assert!(
        bray_runtime_entry_failure_reporting_v1(
            (&raw const failure).addr(),
            size_of::<i32>(),
        ) == Status::SUCCESS
    );

    assert!(
        bray_runtime_main_thread_lane_startup_v1(Configuration {
            task_capacity: 8,
            timer_capacity: 8,
        }) == Status::SUCCESS
    );

    let allocation = bray_runtime_task_allocation_v1();

    assert!(allocation.status == Status::SUCCESS);

    let task = TaskHandle(allocation.task);
    let frame = protected_frame(11, resume_frame, cancel_frame, ignore_action);
    let transfer = ProtectedFrameTransfer(&frame as *const ProtectedFrame as usize);

    assert!(bray_runtime_task_start_v1(task, transfer) == Status::SUCCESS);
    assert!(bray_runtime_main_thread_lane_drive_v1() == Status::SUCCESS);
    assert!(bray_runtime_structured_shutdown_v1() == Status::SUCCESS);
}
