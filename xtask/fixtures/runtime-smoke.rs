include!("panic_report.rs");

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
struct RunOutcome {
    state: RunState,
    payload: usize,
    report: PanicReport,
}

#[repr(transparent)]
#[derive(Clone, Copy)]
struct PanicCause(u32);

impl PanicCause {
    const MESSAGE: Self = Self(0);
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
struct FrameProgress {
    kind: FrameProgressKind,
    state: u32,
    payload: usize,
    report: PanicReport,
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
    resume: extern "C-unwind" fn(&mut FrameProgress, usize),
    cancel: extern "C-unwind" fn(&mut FrameProgress, usize),
    broadcast_tasks: extern "C-unwind" fn(usize),
    resolve_lifecycle: extern "C-unwind" fn(&mut FrameProgress, usize, FrameExit),
    move_completion: extern "C-unwind" fn(usize, usize),
    destroy: extern "C-unwind" fn(usize),
}

#[repr(C)]
struct InactiveFrame {
    context: usize,
    move_before_start: extern "C" fn(usize) -> ProtectedFrame,
}

#[repr(transparent)]
struct ProtectedFrameTransfer(usize);

unsafe extern "C" {
    safe fn bray_runtime_report_consumer() -> extern "C" fn(&mut PanicReport, bool) -> u32;
    safe fn bray_runtime_initialization(worker_capacity: usize, timer_capacity: usize) -> Status;
    safe fn bray_runtime_root_execution(
        frame: ProtectedFrameTransfer,
        configuration: Configuration,
    ) -> RootStart;
    safe fn bray_runtime_root_cancellation_request(root: RootHandle) -> Status;
    safe fn bray_runtime_root_terminal_observation(root: RootHandle) -> RunOutcome;
    safe fn bray_runtime_root_completion_resolution(root: RootHandle) -> Status;
    safe fn bray_runtime_cleanup_incident_reporting() -> Status;
    safe fn bray_runtime_panic_report_construction(
        cause: PanicCause,
        source_present: u32,
        source_identity: u32,
        source_start: u32,
        source_end: u32,
        source_version: u64,
        message_data: *const u8,
        message_length: usize,
    ) -> PanicReport;
    safe fn bray_runtime_outgoing_admission(count: usize, outcome: &mut RunOutcome);
    safe fn bray_runtime_outgoing_activation() -> usize;
    safe fn bray_runtime_outgoing_retirement(record: usize, outcome: &mut RunOutcome);
    safe fn bray_runtime_outgoing_discharge(count: usize);
    safe fn bray_runtime_panic_report_suppression(
        primary: &mut PanicReport,
        incident: &mut PanicReport,
    ) -> PanicReport;
    safe fn bray_runtime_panic_reporting(report: &mut PanicReport) -> Status;
    safe fn bray_runtime_panic_report_destruction(report: &mut PanicReport) -> Status;
    safe fn bray_runtime_entry_failure_reporting(payload: usize, size: usize) -> Status;
    safe fn bray_runtime_wake(task: TaskHandle) -> Status;
    safe fn bray_runtime_main_thread_lane_startup(configuration: Configuration) -> Status;
    safe fn bray_runtime_task_allocation() -> TaskAllocation;
    safe fn bray_runtime_task_start(task: TaskHandle, frame: InactiveFrame) -> Status;
    safe fn bray_runtime_main_thread_lane_drive() -> Status;
    safe fn bray_runtime_structured_shutdown() -> Status;
}

extern "C" fn frame_state(_: usize, _: u32) -> FrameState {
    FrameState {
        affinity: FrameAffinity(2),
        lane_requirements: LaneRequirements(1 << 2),
    }
}

extern "C-unwind" fn resume_frame(destination: &mut FrameProgress, _: usize) {
    *destination = {
        FrameProgress {
            kind: FrameProgressKind(1),
            state: 0,
            payload: 17,
            report: PanicReport::empty(),
        }
    };
}

static ROOT: AtomicU64 = AtomicU64::new(0);
static RESUMES: AtomicUsize = AtomicUsize::new(0);
static CANCELLATIONS: AtomicUsize = AtomicUsize::new(0);
static FAILURE_ROOT: AtomicU64 = AtomicU64::new(0);
static FAILURE_RESUMES: AtomicUsize = AtomicUsize::new(0);
static FAILURE_CLEANUP: AtomicUsize = AtomicUsize::new(0);

extern "C-unwind" fn cancel_frame(destination: &mut FrameProgress, _: usize) {
    *destination = {
        CANCELLATIONS.fetch_add(1, Ordering::Relaxed);

        FrameProgress {
            kind: FrameProgressKind(2),
            state: 0,
            payload: 0,
            report: PanicReport::empty(),
        }
    };
}

extern "C-unwind" fn suspend_and_wake(destination: &mut FrameProgress, _: usize) {
    if RESUMES.fetch_add(1, Ordering::Relaxed) == 0 {
        assert!(bray_runtime_wake(TaskHandle(ROOT.load(Ordering::Relaxed))) == Status::SUCCESS);

        *destination = FrameProgress {
            kind: FrameProgressKind(0),
            state: 1,
            payload: 0,
            report: PanicReport::empty(),
        };

        return;
    }

    resume_frame(destination, 0)
}

extern "C-unwind" fn suspend(destination: &mut FrameProgress, _: usize) {
    *destination = {
        FrameProgress {
            kind: FrameProgressKind(0),
            state: 1,
            payload: 0,
            report: PanicReport::empty(),
        }
    };
}

extern "C-unwind" fn suspend_then_fail(destination: &mut FrameProgress, _: usize) {
    *destination = (|| {
        if FAILURE_RESUMES.fetch_add(1, Ordering::Relaxed) == 0 {
            assert!(
                bray_runtime_wake(TaskHandle(FAILURE_ROOT.load(Ordering::Relaxed)))
                    == Status::SUCCESS
            );

            return FrameProgress {
                kind: FrameProgressKind(0),
                state: 1,
                payload: 0,
                report: PanicReport::empty(),
            };
        }

        FrameProgress {
            kind: FrameProgressKind(4),
            state: 0,
            payload: 0,
            report: PanicReport::empty(),
        }
    })();
}

extern "C-unwind" fn panic_frame(destination: &mut FrameProgress, _: usize) {
    *destination = FrameProgress {
        kind: FrameProgressKind(3),
        state: 0,
        payload: 0,
        report: panic_report(),
    };

    panic!("callback unwinds after publishing its Bray report");
}

fn panic_report() -> PanicReport {
    const MESSAGE: &[u8] = b"runtime smoke panic";

    bray_runtime_panic_report_construction(
        PanicCause::MESSAGE,
        1,
        0,
        0,
        1,
        0,
        MESSAGE.as_ptr(),
        MESSAGE.len(),
    )
}

extern "C-unwind" fn ignore_action(_: usize) {}

extern "C-unwind" fn fail_action(_: usize) {
    panic!("cleanup callback failure");
}

extern "C-unwind" fn ignore_resolution(_: &mut FrameProgress, _: usize, _: FrameExit) {}

extern "C-unwind" fn record_failure_resolution(_: &mut FrameProgress, _: usize, _: FrameExit) {
    FAILURE_CLEANUP.fetch_add(1, Ordering::Relaxed);
}

extern "C-unwind" fn ignore_completion_move(_: usize, _: usize) {}

extern "C" fn move_before_start(context: usize) -> ProtectedFrame {
    unsafe {
        // The inactive-frame transfer gives this callback sole ownership of the descriptor.
        *Box::from_raw(context as *mut ProtectedFrame)
    }
}

extern "C-unwind" fn record_failure_action(_: usize) {
    FAILURE_CLEANUP.fetch_add(1, Ordering::Relaxed);
}

fn protected_frame(
    identity: u8,
    resume: extern "C-unwind" fn(&mut FrameProgress, usize),
    cancel: extern "C-unwind" fn(&mut FrameProgress, usize),
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
    assert!(bray_runtime_initialization(8, 8) == Status::SUCCESS);

    let transfer = ProtectedFrameTransfer(&frame as *const ProtectedFrame as usize);

    let start = bray_runtime_root_execution(
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
    admitted_reports_survive_failed_reservation();

    let root = start_root(protected_frame(
        7,
        suspend_and_wake,
        cancel_frame,
        ignore_action,
    ));

    ROOT.store(root.0, Ordering::Relaxed);

    let outcome = bray_runtime_root_terminal_observation(root);

    assert!(outcome.state == RunState::COMPLETED);
    assert!(RESUMES.load(Ordering::Relaxed) == 2);
    assert!(bray_runtime_root_completion_resolution(root) == Status::SUCCESS);
    assert!(bray_runtime_structured_shutdown() == Status::SUCCESS);

    let root = start_root(protected_frame(8, suspend, cancel_frame, ignore_action));

    assert!(bray_runtime_main_thread_lane_drive() == Status::SUCCESS);
    assert!(bray_runtime_root_cancellation_request(root) == Status::SUCCESS);

    let outcome = bray_runtime_root_terminal_observation(root);

    assert!(outcome.state == RunState::CANCELLED);
    assert!(CANCELLATIONS.load(Ordering::Relaxed) == 1);
    assert!(bray_runtime_root_completion_resolution(root) == Status::SUCCESS);
    assert!(bray_runtime_structured_shutdown() == Status::SUCCESS);

    let root = start_root(protected_frame(9, panic_frame, cancel_frame, ignore_action));

    let mut outcome = bray_runtime_root_terminal_observation(root);

    assert!(outcome.state == RunState::PANICKED);
    assert_eq!(outcome.report.count, 1);
    assert!(bray_runtime_root_completion_resolution(root) == Status::SUCCESS);
    assert!(bray_runtime_panic_reporting(&mut outcome.report) == Status::SUCCESS);
    assert!(bray_runtime_panic_report_destruction(&mut panic_report()) == Status::SUCCESS);
    assert!(bray_runtime_structured_shutdown() == Status::SUCCESS);

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

    let outcome = bray_runtime_root_terminal_observation(root);

    assert!(outcome.state == RunState::RUNTIME_FAILURE);
    assert!(FAILURE_RESUMES.load(Ordering::Relaxed) == 2);
    assert!(FAILURE_CLEANUP.load(Ordering::Relaxed) == 3);
    assert!(bray_runtime_root_completion_resolution(root) == Status::SUCCESS);
    assert!(bray_runtime_structured_shutdown() == Status::SUCCESS);

    let root = start_root(protected_frame(10, resume_frame, cancel_frame, fail_action));

    let outcome = bray_runtime_root_terminal_observation(root);

    assert!(outcome.state == RunState::COMPLETED);
    assert!(bray_runtime_cleanup_incident_reporting() == Status::SUCCESS);
    assert!(bray_runtime_root_completion_resolution(root) == Status::SUCCESS);
    assert!(bray_runtime_structured_shutdown() == Status::SUCCESS);

    let failure = 42_i32;

    assert!(
        bray_runtime_entry_failure_reporting((&raw const failure).addr(), size_of::<i32>(),)
            == Status::SUCCESS
    );

    assert!(bray_runtime_initialization(8, 8) == Status::SUCCESS);

    assert!(
        bray_runtime_main_thread_lane_startup(Configuration {
            task_capacity: 8,
            timer_capacity: 8,
        }) == Status::SUCCESS
    );

    let allocation = bray_runtime_task_allocation();

    assert!(allocation.status == Status::SUCCESS);

    let task = TaskHandle(allocation.task);

    let frame = InactiveFrame {
        context: Box::into_raw(Box::new(protected_frame(
            11,
            resume_frame,
            cancel_frame,
            ignore_action,
        ))) as usize,
        move_before_start,
    };

    assert!(bray_runtime_task_start(task, frame) == Status::SUCCESS);
    assert!(bray_runtime_main_thread_lane_drive() == Status::SUCCESS);
    assert!(bray_runtime_structured_shutdown() == Status::SUCCESS);
}

static REPORT_RELEASES: AtomicUsize = AtomicUsize::new(0);

extern "C" fn release_report(id: usize, _: usize, outcome: &mut RunOutcome) {
    assert!(outcome.state == RunState::COMPLETED);

    let previous = REPORT_RELEASES.fetch_add(id, Ordering::Relaxed);
    assert_eq!(previous, if id == 1 { 0 } else { 1 });
}

fn admitted_reports_survive_failed_reservation() {
    let mut admission = RunOutcome {
        state: RunState::COMPLETED,
        payload: 0,
        report: PanicReport::empty(),
    };

    bray_runtime_outgoing_admission(2, &mut admission);
    assert!(admission.state == RunState::COMPLETED);

    let records = [
        bray_runtime_outgoing_activation(),
        bray_runtime_outgoing_activation(),
    ];

    let mut reports = records.into_iter().enumerate().map(|(index, record)| {
        let mut report = PanicReport::empty();
        report.source = [1, 17, 23, 29];
        report.source_version = 31;
        report.cause = 1;
        report.message = if index == 0 { 1 } else { 10 };
        report.release_message = Some(release_report);
        report.consume = Some(bray_runtime_report_consumer());

        let mut outcome = RunOutcome {
            state: RunState::PANICKED,
            payload: 0,
            report,
        };

        bray_runtime_outgoing_retirement(record, &mut outcome);

        outcome.report
    });

    let mut primary = reports.next().unwrap();
    let mut incident = reports.next().unwrap();
    bray_runtime_outgoing_discharge(2);

    // This valid-layout request exceeds the address space on supported 64-bit hosts.
    // It exercises actual reservation failure, not a test-only runtime switch.
    let impossible = usize::MAX / 4096;

    for _ in 0..2 {
        bray_runtime_outgoing_admission(impossible, &mut admission);
        assert!(admission.state == RunState::PANICKED);
        assert_eq!(admission.report.cause, 4);
        assert!(bray_runtime_panic_report_destruction(&mut admission.report) == Status::SUCCESS);
        admission.state = RunState::COMPLETED;
    }

    let mut report = bray_runtime_panic_report_suppression(&mut primary, &mut incident);
    assert_eq!(report.source, [1, 17, 23, 29]);
    assert_eq!(report.source_version, 31);
    assert_eq!(report.count, 1);
    assert_eq!(REPORT_RELEASES.load(Ordering::Relaxed), 0);
    assert!(bray_runtime_panic_report_destruction(&mut report) == Status::SUCCESS);
    assert!(bray_runtime_panic_report_destruction(&mut report) == Status::SUCCESS);
    assert_eq!(REPORT_RELEASES.load(Ordering::Relaxed), 11);
}
