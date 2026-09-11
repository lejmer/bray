use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use bray_runtime_abi::{
    NativeFrameExit, NativeFrameMetadata, NativeFrameProgress, NativeFrameProgressKind,
    NativeProtectedFrame, NativeRunState, NativeRuntimeConfiguration, NativeRuntimeStatus,
};

use crate::native::frame::NativeFrameTransfer;
use crate::native::frames::{
    bray_runtime_frame_storage_admission, bray_runtime_frame_storage_release,
};
use crate::native::state::{initialize, shutdown, with_runtime};
use crate::test_support::{native_main_frame_state, with_allocation_failure};

const DEPTH: usize = 1024;
static CONTEXTS: Mutex<Vec<usize>> = Mutex::new(Vec::new());
static RUN_ID: AtomicU64 = AtomicU64::new(0);
static CALLBACK_DEPTH: AtomicUsize = AtomicUsize::new(0);
static MAX_CALLBACK_DEPTH: AtomicUsize = AtomicUsize::new(0);
static COMPLETIONS: AtomicUsize = AtomicUsize::new(0);
static RELEASES: AtomicUsize = AtomicUsize::new(0);
static COMPETITOR_AT: AtomicUsize = AtomicUsize::new(usize::MAX);

#[test]
fn deep_composition_keeps_one_run_yields_fairly_and_retries_child_transfer() {
    assert_eq!(
        initialize(NativeRuntimeConfiguration::new(2, 1)),
        NativeRuntimeStatus::SUCCESS
    );

    COMPLETIONS.store(0, Ordering::Relaxed);
    RELEASES.store(0, Ordering::Relaxed);
    CALLBACK_DEPTH.store(0, Ordering::Relaxed);
    MAX_CALLBACK_DEPTH.store(0, Ordering::Relaxed);
    COMPETITOR_AT.store(usize::MAX, Ordering::Relaxed);

    let contexts: Vec<_> = (0..DEPTH)
        .map(|_| {
            let address = bray_runtime_frame_storage_admission(Some(&metadata()));
            assert_ne!(address, 0);

            address
        })
        .collect();

    let root_context = contexts[0];
    *CONTEXTS.lock().unwrap() = contexts;

    with_runtime(|runtime| {
        let root = runtime.allocate().task().unwrap();
        let competitor = runtime.allocate().task().unwrap();

        assert_eq!(
            runtime.start(
                root,
                &mut NativeFrameTransfer::new(frame(root_context, chain, release))
            ),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            runtime.start(
                competitor,
                &mut NativeFrameTransfer::new(frame(0, compete, ignore))
            ),
            NativeRuntimeStatus::SUCCESS
        );

        runtime
            .with_started(root, |task| {
                RUN_ID.store(task.task.id().raw(), Ordering::Relaxed)
            })
            .unwrap();

        with_allocation_failure(|| {
            let mut finished = false;

            for _ in 0..DEPTH {
                assert_eq!(runtime.scheduler.task_count().unwrap(), 2);
                assert_eq!(runtime.drive_main_thread(), NativeRuntimeStatus::SUCCESS);
                let outcome = runtime.observe(root);

                if outcome.state() != NativeRunState::PENDING {
                    assert_eq!(outcome.state(), NativeRunState::COMPLETED);
                    finished = true;
                    break;
                }
            }

            assert!(finished, "the admitted activation chain must finish");
        });

        assert_eq!(
            runtime.observe(competitor).state(),
            NativeRunState::COMPLETED
        );

        assert!(COMPETITOR_AT.load(Ordering::Relaxed) < DEPTH);
        assert_eq!(COMPLETIONS.load(Ordering::Relaxed), DEPTH);
        assert_eq!(RELEASES.load(Ordering::Relaxed), DEPTH);
        assert_eq!(MAX_CALLBACK_DEPTH.load(Ordering::Relaxed), 1);
        assert_eq!(runtime.destroy_task(root), NativeRuntimeStatus::SUCCESS);

        assert_eq!(
            runtime.destroy_task(competitor),
            NativeRuntimeStatus::SUCCESS
        );
    })
    .unwrap();

    CONTEXTS.lock().unwrap().clear();
    assert_eq!(shutdown(), NativeRuntimeStatus::SUCCESS);
}

fn metadata() -> NativeFrameMetadata {
    NativeFrameMetadata::new([211; 32], 2, 1, 1, 0, 1, native_main_frame_state)
}

fn frame(
    context: usize,
    resume: extern "C-unwind" fn(usize) -> NativeFrameProgress,
    destroy: extern "C-unwind" fn(usize),
) -> NativeProtectedFrame {
    NativeProtectedFrame::new(
        context,
        metadata(),
        resume,
        resume,
        ignore,
        resolve,
        move_completion,
        destroy,
    )
}

extern "C-unwind" fn chain(context: usize) -> NativeFrameProgress {
    let depth = CALLBACK_DEPTH.fetch_add(1, Ordering::Relaxed) + 1;
    MAX_CALLBACK_DEPTH.fetch_max(depth, Ordering::Relaxed);
    let execution = crate::current_task_execution_context().unwrap();
    assert_eq!(execution.task().raw(), RUN_ID.load(Ordering::Relaxed));

    let (index, child) = {
        let contexts = CONTEXTS.lock().unwrap();

        let index = contexts
            .iter()
            .position(|address| *address == context)
            .unwrap();

        (index, contexts.get(index + 1).copied())
    };

    let progress = with_runtime(|runtime| {
        assert_eq!(runtime.scheduler.task_count().unwrap(), 2);

        if execution.state().raw() == 0 {
            if let Some(child) = child {
                assert_eq!(
                    runtime.compose_awaited(NativeFrameTransfer::new(frame(child, chain, release))),
                    NativeRuntimeStatus::SUCCESS
                );

                return NativeFrameProgress::new(NativeFrameProgressKind::SUSPENDED, 1, 0);
            }
        } else {
            assert_eq!(execution.state().raw(), 1);

            if index == 0 {
                assert_eq!(
                    runtime.resolve_awaited_terminal(|outcome| {
                        assert_eq!(outcome.state(), NativeRunState::COMPLETED);
                        assert_eq!(
                            runtime.resolve_awaited_terminal(|_| panic!(
                                "reentrant move must not execute"
                            )),
                            NativeRuntimeStatus::PENDING
                        );
                        Err(NativeRuntimeStatus::INVALID_ARGUMENT)
                    }),
                    NativeRuntimeStatus::INVALID_ARGUMENT
                );
            }

            assert_eq!(
                runtime.resolve_awaited_terminal(|outcome| {
                    assert_eq!(outcome.state(), NativeRunState::COMPLETED);
                    Ok(())
                }),
                NativeRuntimeStatus::SUCCESS
            );

            assert_eq!(
                runtime.resolve_awaited_terminal(|_| panic!("completed child must move once")),
                NativeRuntimeStatus::UNKNOWN_TASK
            );
        }

        NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0)
    })
    .unwrap();

    CALLBACK_DEPTH.fetch_sub(1, Ordering::Relaxed);

    progress
}

extern "C-unwind" fn compete(_: usize) -> NativeFrameProgress {
    COMPETITOR_AT.store(COMPLETIONS.load(Ordering::Relaxed), Ordering::Relaxed);

    NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0)
}

extern "C-unwind" fn move_completion(context: usize, _: usize) {
    if context != 0 {
        COMPLETIONS.fetch_add(1, Ordering::Relaxed);
    }
}

extern "C-unwind" fn release(context: usize) {
    RELEASES.fetch_add(1, Ordering::Relaxed);
    bray_runtime_frame_storage_release(context);
}

extern "C-unwind" fn ignore(_: usize) {}
extern "C-unwind" fn resolve(_: usize, _: NativeFrameExit) {}

static REJECT_CHILD: AtomicBool = AtomicBool::new(false);
static PARENT_ACTIVE: AtomicBool = AtomicBool::new(false);
static CHILD_CONTEXT: AtomicUsize = AtomicUsize::new(0);
static CHILD_RESUMES: AtomicUsize = AtomicUsize::new(0);
static CHILD_BROADCASTS: AtomicUsize = AtomicUsize::new(0);
static CHILD_LIFECYCLES: AtomicUsize = AtomicUsize::new(0);
static CHILD_RELEASES: AtomicUsize = AtomicUsize::new(0);

#[test]
fn rejected_and_pending_children_preserve_single_ownership() {
    for reject in [true, false] {
        assert_eq!(
            initialize(NativeRuntimeConfiguration::new(1, 1)),
            NativeRuntimeStatus::SUCCESS
        );

        REJECT_CHILD.store(reject, Ordering::Relaxed);
        PARENT_ACTIVE.store(false, Ordering::Relaxed);
        CHILD_RESUMES.store(0, Ordering::Relaxed);
        CHILD_BROADCASTS.store(0, Ordering::Relaxed);
        CHILD_LIFECYCLES.store(0, Ordering::Relaxed);
        CHILD_RELEASES.store(0, Ordering::Relaxed);
        let address = bray_runtime_frame_storage_admission(Some(&child_metadata()));
        assert_ne!(address, 0);
        CHILD_CONTEXT.store(address, Ordering::Relaxed);

        with_runtime(|runtime| {
            let root = runtime.allocate().task().unwrap();

            assert_eq!(
                runtime.start(
                    root,
                    &mut NativeFrameTransfer::new(frame(0, compose_then_exit, ignore))
                ),
                NativeRuntimeStatus::SUCCESS
            );

            runtime
                .with_started(root, |task| {
                    RUN_ID.store(task.task.id().raw(), Ordering::Relaxed)
                })
                .unwrap();

            assert_eq!(runtime.drive_main_thread(), NativeRuntimeStatus::SUCCESS);

            let expected = if reject {
                crate::RunOutcomeKind::Completed
            } else {
                crate::RunOutcomeKind::Panicked
            };

            runtime
                .with_started(root, |task| {
                    assert_eq!(
                        task.task.snapshot().unwrap().unobserved_outcome(),
                        Some(expected)
                    );
                })
                .unwrap();

            assert_eq!(CHILD_RESUMES.load(Ordering::Relaxed), 0);

            assert_eq!(
                CHILD_BROADCASTS.load(Ordering::Relaxed),
                usize::from(!reject)
            );

            assert_eq!(
                CHILD_LIFECYCLES.load(Ordering::Relaxed),
                usize::from(!reject)
            );

            assert_eq!(CHILD_RELEASES.load(Ordering::Relaxed), 1);
            assert_ne!(runtime.observe(root).state(), NativeRunState::PENDING);
            assert_eq!(runtime.destroy_task(root), NativeRuntimeStatus::SUCCESS);
            runtime.discard_cleanup_incidents();
        })
        .unwrap();

        assert_eq!(shutdown(), NativeRuntimeStatus::SUCCESS);
    }
}

fn child_metadata() -> NativeFrameMetadata {
    let state = if REJECT_CHILD.load(Ordering::Relaxed) {
        crate::test_support::native_blocking_frame_state
    } else {
        native_main_frame_state
    };

    NativeFrameMetadata::new([212; 32], 1, 1, 1, 0, 1, state)
}

struct ParentCallback;

impl Drop for ParentCallback {
    fn drop(&mut self) {
        PARENT_ACTIVE.store(false, Ordering::Relaxed);
    }
}

extern "C-unwind" fn compose_then_exit(_: usize) -> NativeFrameProgress {
    let _callback = ParentCallback;
    PARENT_ACTIVE.store(true, Ordering::Relaxed);

    assert_eq!(
        crate::current_task_execution_context()
            .unwrap()
            .task()
            .raw(),
        RUN_ID.load(Ordering::Relaxed)
    );

    let child = NativeProtectedFrame::new(
        CHILD_CONTEXT.load(Ordering::Relaxed),
        child_metadata(),
        child_resume,
        child_resume,
        child_broadcast,
        child_lifecycle,
        move_completion,
        child_release,
    );

    let status =
        with_runtime(|runtime| runtime.compose_awaited(NativeFrameTransfer::new(child))).unwrap();

    if REJECT_CHILD.load(Ordering::Relaxed) {
        assert_eq!(status, NativeRuntimeStatus::RUNTIME_FAILURE);

        NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0)
    } else {
        assert_eq!(status, NativeRuntimeStatus::SUCCESS);
        panic!("parent failed before returning its awaited suspension");
    }
}

extern "C-unwind" fn child_resume(_: usize) -> NativeFrameProgress {
    CHILD_RESUMES.fetch_add(1, Ordering::Relaxed);

    NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0)
}

extern "C-unwind" fn child_broadcast(_: usize) {
    assert!(!PARENT_ACTIVE.load(Ordering::Relaxed));
    CHILD_BROADCASTS.fetch_add(1, Ordering::Relaxed);
}

extern "C-unwind" fn child_lifecycle(_: usize, exit: NativeFrameExit) {
    assert!(!PARENT_ACTIVE.load(Ordering::Relaxed));
    assert_eq!(exit, NativeFrameExit::RUNTIME_FAILURE);
    CHILD_LIFECYCLES.fetch_add(1, Ordering::Relaxed);
}

extern "C-unwind" fn child_release(context: usize) {
    CHILD_RELEASES.fetch_add(1, Ordering::Relaxed);
    bray_runtime_frame_storage_release(context);
}

static SUPPRESSED_PRIMARY: AtomicUsize = AtomicUsize::new(0);
static SUPPRESSED_SECONDARY: AtomicUsize = AtomicUsize::new(0);
static INCIDENT_WRAPS: AtomicUsize = AtomicUsize::new(0);

#[test]
fn nested_cleanup_incident_attaches_before_enclosing_catch_consumes_report() {
    assert_eq!(
        initialize(NativeRuntimeConfiguration::new(1, 1)),
        NativeRuntimeStatus::SUCCESS
    );

    SUPPRESSED_PRIMARY.store(0, Ordering::Relaxed);
    SUPPRESSED_SECONDARY.store(0, Ordering::Relaxed);
    INCIDENT_WRAPS.store(0, Ordering::Relaxed);

    with_runtime(|runtime| {
        let root = runtime.allocate().task().unwrap();

        assert_eq!(
            runtime.start(
                root,
                &mut NativeFrameTransfer::new(frame(1, catch_nested_incident, ignore))
            ),
            NativeRuntimeStatus::SUCCESS
        );

        runtime
            .with_started(root, |task| {
                RUN_ID.store(task.task.id().raw(), Ordering::Relaxed)
            })
            .unwrap();

        assert_eq!(
            runtime.resolve_task(root).state(),
            NativeRunState::COMPLETED
        );

        assert_eq!(INCIDENT_WRAPS.load(Ordering::Relaxed), 1);
        assert_eq!(SUPPRESSED_PRIMARY.load(Ordering::Relaxed), 101);
        assert_eq!(SUPPRESSED_SECONDARY.load(Ordering::Relaxed), 202);
        assert_eq!(runtime.pending_cleanup_incidents(), 0);
        assert_eq!(runtime.destroy_task(root), NativeRuntimeStatus::SUCCESS);
    })
    .unwrap();

    assert_eq!(shutdown(), NativeRuntimeStatus::SUCCESS);
}

extern "C-unwind" fn catch_nested_incident(level: usize) -> NativeFrameProgress {
    let execution = crate::current_task_execution_context().unwrap();
    assert_eq!(execution.task().raw(), RUN_ID.load(Ordering::Relaxed));

    if level == 3 {
        let callbacks = bray_runtime_abi::NativePanicReportCallbacks::new(
            report_status,
            report_status,
            wrap_incident,
            suppress_incident,
        );

        let incident = bray_runtime_abi::NativeCleanupIncident::new(
            202,
            bray_runtime_abi::NativeTypeIdentity::new([3; 32]),
            bray_runtime_abi::NativeSourceAnchor::unavailable(),
            report_incident,
            destroy_incident,
            callbacks,
        );

        assert!(
            crate::native::incident::retain_cleanup_incident(
                crate::incident::OwnedCleanupIncident::native(incident).unwrap()
            )
            .is_ok()
        );

        return NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0);
    }

    with_runtime(|runtime| {
        if execution.state().raw() == 0 {
            assert_eq!(
                runtime.compose_awaited(NativeFrameTransfer::new(frame(
                    level + 1,
                    catch_nested_incident,
                    ignore
                ))),
                NativeRuntimeStatus::SUCCESS
            );

            return NativeFrameProgress::new(NativeFrameProgressKind::SUSPENDED, 1, 0);
        }

        assert_eq!(
            runtime.resolve_awaited_terminal(|outcome| {
                if level == 1 {
                    assert_eq!(outcome.state(), NativeRunState::PANICKED);
                    assert_eq!(outcome.payload(), 101);
                    assert_eq!(
                        SUPPRESSED_SECONDARY.load(Ordering::Relaxed),
                        202,
                        "nearest parent must attach the incident before catch consumes its report"
                    );
                } else {
                    assert_eq!(outcome.state(), NativeRunState::COMPLETED);
                }
                Ok(())
            }),
            NativeRuntimeStatus::SUCCESS
        );

        if level == 2 {
            NativeFrameProgress::new(NativeFrameProgressKind::PANICKED, 0, 101)
        } else {
            NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0)
        }
    })
    .unwrap()
}

extern "C" fn wrap_incident(incident: &bray_runtime_abi::NativeCleanupIncident) -> usize {
    INCIDENT_WRAPS.fetch_add(1, Ordering::Relaxed);

    incident.payload()
}

extern "C" fn suppress_incident(primary: usize, secondary: usize) -> usize {
    SUPPRESSED_PRIMARY.store(primary, Ordering::Relaxed);
    SUPPRESSED_SECONDARY.store(secondary, Ordering::Relaxed);

    primary
}

extern "C-unwind" fn report_status(_: usize) -> NativeRuntimeStatus {
    NativeRuntimeStatus::SUCCESS
}

extern "C-unwind" fn report_incident(
    _: &bray_runtime_abi::NativeCleanupIncident,
) -> NativeRuntimeStatus {
    NativeRuntimeStatus::SUCCESS
}

extern "C-unwind" fn destroy_incident(_: usize) -> bray_runtime_abi::NativeBrayCallOutcome {
    bray_runtime_abi::NativeBrayCallOutcome::completed()
}

#[test]
fn independently_admitted_run_retains_its_root_activation_panic() {
    extern "C-unwind" fn panicked(_: usize) -> NativeFrameProgress {
        NativeFrameProgress::new(NativeFrameProgressKind::PANICKED, 0, 42)
    }

    assert!(initialize(NativeRuntimeConfiguration::new(1, 1)).is_success());

    with_runtime(|runtime| {
        let terminal = crate::native::frame::NativeTerminalState::reserve().unwrap();

        let descriptor =
            crate::native::frame::NativeFrame::checked_descriptor(&metadata()).unwrap();

        let mut reservation = crate::native::state::NativeRunReservation::prepare(
            descriptor,
            terminal,
            crate::task::TaskAdmissionKind::Independent,
        )
        .unwrap();

        reservation.reserve(&runtime.scheduler).unwrap();

        let activation = super::NativeActivationReservation::prepare(&metadata())
            .unwrap()
            .install(frame(0, panicked, ignore));

        reservation.run().install_root(activation);
        let handle = runtime.allocate().task().unwrap();
        assert!(runtime.start_run(handle, reservation).is_success());
        let outcome = runtime.resolve_task(handle);
        assert_eq!(outcome.state(), NativeRunState::PANICKED);
        assert_eq!(outcome.payload(), 42);
        assert!(runtime.destroy_task(handle).is_success());
    })
    .unwrap();

    assert!(shutdown().is_success());
}
