include!("panic_report.rs");

use std::collections::BTreeMap;
use std::sync::Mutex;
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
struct StorageShape {
    identity: [u8; 32],
    bytes: usize,
    alignment: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct StorageLease {
    domain: usize,
    shape: usize,
    slot: usize,
    epoch: u64,
    address: usize,
}

#[repr(C)]
#[derive(Default)]
struct Records {
    head: usize,
    tail: usize,
    count: usize,
}

#[repr(C)]
struct ActivationDescriptor {
    storage: StorageShape,
    header: usize,
    completion: usize,
    completion_bytes: usize,
    completion_align: usize,
    states: u32,
    state: extern "C" fn(usize, u32, u32, usize) -> FrameState,
    resume: extern "C-unwind" fn(&mut FrameProgress, usize),
    cancel: extern "C-unwind" fn(&mut FrameProgress, usize),
    broadcast: extern "C-unwind" fn(&mut FrameProgress, usize),
    resolve: extern "C-unwind" fn(&mut FrameProgress, usize, FrameExit),
    move_completion: extern "C-unwind" fn(usize, usize),
    destroy: extern "C-unwind" fn(usize),
}

#[repr(C)]
struct InactiveFrame {
    lease: StorageLease,
    descriptor: *const ActivationDescriptor,
}

#[repr(transparent)]
struct InactiveFrameTransfer(usize);

#[repr(C)]
struct RunResultLayout {
    size: usize,
    alignment: usize,
    tag_size: usize,
    completed_tag: u64,
    completed_offset: usize,
    completed_size: usize,
    completed_alignment: usize,
    panicked_tag: u64,
    panicked_offset: usize,
    cancelled_tag: u64,
}

static FRAMES: Mutex<BTreeMap<usize, StorageLease>> = Mutex::new(BTreeMap::new());

#[repr(C)]
struct CleanupBundle {
    records: usize,
    descriptors: *const u8,
    count: usize,
}

unsafe extern "C" {
    safe fn bray_runtime_activation_header_layout(bytes: &mut usize, alignment: &mut usize);
    safe fn bray_runtime_cleanup_storage_admission(
        shape: &StorageShape,
        lease: &mut StorageLease,
    ) -> u32;
    safe fn bray_runtime_cleanup_storage_activation(
        lease: &StorageLease,
        shape: &StorageShape,
    ) -> usize;
    safe fn bray_runtime_cleanup_storage_release(lease: &mut StorageLease) -> u32;
    safe fn bray_runtime_activation_callback_admission(count: usize, records: &mut Records) -> u32;
    safe fn bray_runtime_activation_initialization(
        token: &InactiveFrame,
        context: usize,
        records: &mut Records,
    );
    safe fn bray_runtime_activation_publication(token: &InactiveFrame) -> u32;
    safe fn bray_runtime_task_resolution(
        task: TaskHandle,
        destination: *mut u8,
        layout: &RunResultLayout,
    ) -> Status;
    safe fn bray_runtime_task_destruction(task: TaskHandle) -> Status;
    safe fn bray_runtime_report_consumer(report: &mut PanicReport, reporting: u32) -> u32;
    safe fn bray_runtime_initialization(worker_capacity: usize, timer_capacity: usize) -> Status;
    safe fn bray_runtime_root_execution(
        frame: InactiveFrameTransfer,
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
        source_namespace_word_0: u64,
        source_namespace_word_1: u64,
        source_namespace_word_2: u64,
        source_namespace_word_3: u64,
        message_data: *const u8,
        message_length: usize,
    ) -> PanicReport;
    safe fn bray_runtime_outgoing_admission(bundle: &CleanupBundle, outcome: &mut RunOutcome);
    safe fn bray_runtime_outgoing_activation(descriptor: *const u8, lease: *mut u8) -> usize;
    safe fn bray_runtime_outgoing_retirement(
        record: usize,
        lease: *const u8,
        outcome: &mut RunOutcome,
    );
    safe fn bray_runtime_outgoing_discharge(bundle: &CleanupBundle);
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
    safe fn bray_runtime_execution_shutdown() -> Status;
    safe fn bray_runtime_structured_shutdown() -> Status;
}

extern "C" fn frame_state(_: usize, _: u32, _: u32, _: usize) -> FrameState {
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
        0,
        0,
        0,
        0,
        MESSAGE.as_ptr(),
        MESSAGE.len(),
    )
}

extern "C-unwind" fn ignore_action(context: usize) {
    let mut lease = FRAMES
        .lock()
        .expect("fixture mutex must not be poisoned")
        .remove(&context)
        .expect("frame releases exactly once");

    assert_eq!(bray_runtime_cleanup_storage_release(&mut lease), 0);
}

extern "C-unwind" fn ignore_broadcast(_: &mut FrameProgress, _: usize) {}

extern "C-unwind" fn fail_action(context: usize) {
    ignore_action(context);
    panic!("cleanup callback failure");
}

extern "C-unwind" fn ignore_resolution(_: &mut FrameProgress, _: usize, _: FrameExit) {}

extern "C-unwind" fn record_failure_resolution(_: &mut FrameProgress, _: usize, _: FrameExit) {
    FAILURE_CLEANUP.fetch_add(1, Ordering::Relaxed);
}

static COMPLETION_MOVES: AtomicUsize = AtomicUsize::new(0);

extern "C-unwind" fn move_completion(_: usize, _: usize) {
    COMPLETION_MOVES.fetch_add(1, Ordering::Relaxed);
}

extern "C-unwind" fn record_failure_action(context: usize) {
    FAILURE_CLEANUP.fetch_add(1, Ordering::Relaxed);
    ignore_action(context);
}

extern "C-unwind" fn record_failure_broadcast(_: &mut FrameProgress, _: usize) {
    FAILURE_CLEANUP.fetch_add(1, Ordering::Relaxed);
}

fn protected_frame(
    identity: u8,
    resume: extern "C-unwind" fn(&mut FrameProgress, usize),
    cancel: extern "C-unwind" fn(&mut FrameProgress, usize),
    destroy: extern "C-unwind" fn(usize),
) -> ActivationDescriptor {
    let mut bytes = 0;
    let mut alignment = 0;

    bray_runtime_activation_header_layout(&mut bytes, &mut alignment);

    ActivationDescriptor {
        storage: StorageShape {
            identity: [identity; 32],
            bytes,
            alignment,
        },
        header: 0,
        completion: bytes,
        completion_bytes: 0,
        completion_align: 1,
        states: 2,
        state: frame_state,
        resume,
        cancel,
        broadcast: ignore_broadcast,
        resolve: ignore_resolution,
        move_completion,
        destroy,
    }
}

fn admit(descriptor: &ActivationDescriptor) -> InactiveFrame {
    let mut records = Records::default();

    assert_eq!(
        bray_runtime_activation_callback_admission(4, &mut records),
        0
    );

    let mut lease = StorageLease::default();

    assert_eq!(
        bray_runtime_cleanup_storage_admission(&descriptor.storage, &mut lease),
        0
    );

    let address = bray_runtime_cleanup_storage_activation(&lease, &descriptor.storage);

    assert_ne!(address, 0);

    let token = InactiveFrame { lease, descriptor };

    assert!(FRAMES.lock().expect("fixture mutex must not be poisoned").insert(address, lease).is_none());
    bray_runtime_activation_initialization(&token, address, &mut records);
    assert_eq!(bray_runtime_activation_publication(&token), 0);

    token
}

fn start_root(descriptor: &ActivationDescriptor) -> RootHandle {
    assert!(bray_runtime_initialization(8, 8) == Status::SUCCESS);

    let frame = admit(descriptor);
    let transfer = InactiveFrameTransfer(std::ptr::from_ref(&frame).addr());

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

    let descriptor = protected_frame(7, suspend_and_wake, cancel_frame, ignore_action);
    let root = start_root(&descriptor);

    ROOT.store(root.0, Ordering::Relaxed);

    let outcome = bray_runtime_root_terminal_observation(root);

    assert!(outcome.state == RunState::COMPLETED);
    assert!(RESUMES.load(Ordering::Relaxed) == 2);
    assert!(bray_runtime_root_completion_resolution(root) == Status::SUCCESS);
    assert!(bray_runtime_execution_shutdown() == Status::SUCCESS);
    assert!(bray_runtime_structured_shutdown() == Status::SUCCESS);

    let descriptor = protected_frame(8, suspend, cancel_frame, ignore_action);
    let root = start_root(&descriptor);

    assert!(bray_runtime_main_thread_lane_drive() == Status::SUCCESS);
    assert!(bray_runtime_root_cancellation_request(root) == Status::SUCCESS);

    let outcome = bray_runtime_root_terminal_observation(root);

    assert!(outcome.state == RunState::CANCELLED);
    assert!(CANCELLATIONS.load(Ordering::Relaxed) == 1);
    assert!(bray_runtime_root_completion_resolution(root) == Status::SUCCESS);
    assert!(bray_runtime_execution_shutdown() == Status::SUCCESS);
    assert!(bray_runtime_structured_shutdown() == Status::SUCCESS);

    let descriptor = protected_frame(9, panic_frame, cancel_frame, ignore_action);
    let root = start_root(&descriptor);

    let mut outcome = bray_runtime_root_terminal_observation(root);

    assert!(outcome.state == RunState::PANICKED);
    assert_eq!(outcome.report.count, 1);
    assert!(bray_runtime_root_completion_resolution(root) == Status::SUCCESS);
    assert!(bray_runtime_panic_reporting(&mut outcome.report) == Status::SUCCESS);
    assert!(bray_runtime_panic_report_destruction(&mut panic_report()) == Status::SUCCESS);
    assert!(bray_runtime_execution_shutdown() == Status::SUCCESS);
    assert!(bray_runtime_structured_shutdown() == Status::SUCCESS);

    let mut descriptor =
        protected_frame(10, suspend_then_fail, cancel_frame, record_failure_action);

    descriptor.broadcast = record_failure_broadcast;
    descriptor.resolve = record_failure_resolution;

    let root = start_root(&descriptor);

    FAILURE_ROOT.store(root.0, Ordering::Relaxed);

    let outcome = bray_runtime_root_terminal_observation(root);

    assert!(outcome.state == RunState::RUNTIME_FAILURE);
    assert!(FAILURE_RESUMES.load(Ordering::Relaxed) == 2);
    assert!(FAILURE_CLEANUP.load(Ordering::Relaxed) == 2);
    assert!(bray_runtime_root_completion_resolution(root) == Status::SUCCESS);
    assert!(FAILURE_CLEANUP.load(Ordering::Relaxed) == 3);
    assert!(bray_runtime_execution_shutdown() == Status::SUCCESS);
    assert!(bray_runtime_structured_shutdown() == Status::SUCCESS);

    let descriptor = protected_frame(10, resume_frame, cancel_frame, fail_action);
    let root = start_root(&descriptor);

    let outcome = bray_runtime_root_terminal_observation(root);

    assert!(outcome.state == RunState::COMPLETED);
    assert!(bray_runtime_root_completion_resolution(root) == Status::SUCCESS);
    assert!(bray_runtime_cleanup_incident_reporting() == Status::SUCCESS);
    assert!(bray_runtime_execution_shutdown() == Status::SUCCESS);
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

    let moves = COMPLETION_MOVES.load(Ordering::Relaxed);
    let descriptor = protected_frame(11, resume_frame, cancel_frame, ignore_action);
    let frame = admit(&descriptor);

    assert!(bray_runtime_task_start(task, frame) == Status::SUCCESS);
    assert!(bray_runtime_main_thread_lane_drive() == Status::SUCCESS);

    let mut outcome = RunOutcome {
        state: RunState::RUNTIME_FAILURE,
        payload: 0,
        report: PanicReport::empty(),
    };

    let layout = RunResultLayout {
        size: size_of::<RunOutcome>(),
        alignment: align_of::<RunOutcome>(),
        tag_size: size_of::<RunState>(),
        completed_tag: 0,
        completed_offset: std::mem::offset_of!(RunOutcome, payload),
        completed_size: 0,
        completed_alignment: 1,
        panicked_tag: 2,
        panicked_offset: std::mem::offset_of!(RunOutcome, report),
        cancelled_tag: 1,
    };

    assert!(
        bray_runtime_task_resolution(task, std::ptr::from_mut(&mut outcome).cast(), &layout)
            == Status::SUCCESS
    );

    assert!(outcome.state == RunState::COMPLETED);
    assert_eq!(COMPLETION_MOVES.load(Ordering::Relaxed), moves + 1);
    assert!(bray_runtime_task_destruction(task) == Status::SUCCESS);
    assert_eq!(COMPLETION_MOVES.load(Ordering::Relaxed), moves + 1);
    assert!(bray_runtime_execution_shutdown() == Status::SUCCESS);
    assert!(bray_runtime_structured_shutdown() == Status::SUCCESS);
    assert!(FRAMES.lock().expect("fixture mutex must not be poisoned").is_empty());
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

    bray_runtime_outgoing_admission(
        &CleanupBundle {
            records: 2,
            descriptors: std::ptr::null(),
            count: 0,
        },
        &mut admission,
    );

    assert!(admission.state == RunState::COMPLETED);

    let records = [
        bray_runtime_outgoing_activation(std::ptr::null(), std::ptr::null_mut()),
        bray_runtime_outgoing_activation(std::ptr::null(), std::ptr::null_mut()),
    ];

    let mut reports = records.into_iter().enumerate().map(|(index, record)| {
        let mut report = PanicReport::empty();

        report.source = [1, 17, 23, 29];
        report.source_version = 31;
        report.cause = 1;
        report.primary_present = 1;
        report.message = if index == 0 { 1 } else { 10 };
        report.release_message = Some(release_report);
        report.consume = Some(bray_runtime_report_consumer);

        let mut outcome = RunOutcome {
            state: RunState::PANICKED,
            payload: 0,
            report,
        };

        bray_runtime_outgoing_retirement(record, std::ptr::null(), &mut outcome);

        outcome.report
    });

    let mut primary = reports.next().expect("fixture iterator must provide its expected item");
    let mut incident = reports.next().expect("fixture iterator must provide its expected item");

    bray_runtime_outgoing_discharge(&CleanupBundle {
        records: 2,
        descriptors: std::ptr::null(),
        count: 0,
    });

    // This valid-layout request exceeds the address space on supported 64-bit hosts.
    // It exercises actual reservation failure, not a test-only runtime switch.
    let impossible = usize::MAX / 4096;

    for _ in 0..2 {
        bray_runtime_outgoing_admission(
            &CleanupBundle {
                records: impossible,
                descriptors: std::ptr::null(),
                count: 0,
            },
            &mut admission,
        );

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
