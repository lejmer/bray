use std::cell::Cell;
use std::io::Write;
use std::sync::Arc;

use bray_platform::{RuntimeThreadId, RuntimeThreadScope};
use bray_runtime_abi::{
    NativeExecutionLane, NativeExecutionLaneResult, NativeRunOutcome, NativeRunState,
    NativeRuntimeStatus, NativeTaskHandle,
};

use crate::{
    CleanupIncidentProducer, ExecutionLane, ExecutionLanePlacement, ExecutionWorkload, RunOutcome,
};

use super::super::frame::NativeTerminalState;
use super::core::{
    CLEANUP_RUNTIME, CURRENT_NATIVE_TASK, NativeRuntime, RetainedRuntime, with_runtime,
};

pub(in crate::native) fn scheduler_status(error: crate::SchedulerError) -> NativeRuntimeStatus {
    match error {
        crate::SchedulerError::AdmissionAllocation(_) => NativeRuntimeStatus::ALLOCATION_FAILURE,
        _ => NativeRuntimeStatus::RUNTIME_FAILURE,
    }
}

pub(crate) fn thread_attachment_status(error: bray_platform::PlatformError) -> NativeRuntimeStatus {
    match error.kind() {
        bray_platform::PlatformErrorKind::Io(std::io::ErrorKind::OutOfMemory) => {
            NativeRuntimeStatus::ALLOCATION_FAILURE
        }
        _ => NativeRuntimeStatus::RUNTIME_FAILURE,
    }
}

pub(in crate::native) fn current_thread_lanes(
    thread: RuntimeThreadId,
    main_thread_lane: bool,
    cleanup_workloads: bool,
) -> impl Iterator<Item = ExecutionLane> + Clone {
    let placements = [
        main_thread_lane.then_some(ExecutionLanePlacement::MainThread(thread)),
        Some(ExecutionLanePlacement::OriginThread(thread)),
        Some(ExecutionLanePlacement::PinnedWorker(thread)),
        cleanup_workloads.then_some(ExecutionLanePlacement::Migratable),
    ];

    let workloads = if cleanup_workloads {
        &[
            ExecutionWorkload::Cooperative,
            ExecutionWorkload::Blocking,
            ExecutionWorkload::Compute,
        ][..]
    } else {
        &[ExecutionWorkload::Cooperative][..]
    };

    placements.into_iter().flatten().flat_map(move |placement| {
        workloads
            .iter()
            .copied()
            .map(move |workload| ExecutionLane::new(placement, workload))
    })
}

pub(in crate::native) fn with_cleanup_runtime<T>(
    retained: &RetainedRuntime,
    callback: impl FnOnce() -> T,
) -> Result<T, NativeRuntimeStatus> {
    if with_runtime(|runtime| Arc::ptr_eq(&runtime.core, &retained.core)).unwrap_or(false) {
        return crate::product::with_execution_factory(
            super::super::product_execution::retain_current_execution,
            || with_runtime(|runtime| runtime.with_cleanup_driving(callback)),
        );
    }

    let thread = RuntimeThreadScope::enter_or_reuse().map_err(thread_attachment_status)?;

    let main_thread_lane = retained.core.scheduler.main_thread() == Some(thread.runtime().id());

    let runtime = NativeRuntime {
        thread,
        main_thread_lane,
        cleanup_workloads: Cell::new(true),
        worker: None,
        core: Arc::clone(&retained.core),
        #[cfg(test)]
        _test_isolation: None,
    };

    Ok(CLEANUP_RUNTIME.set(&runtime, || {
        crate::product::with_execution_factory(
            super::super::product_execution::retain_current_execution,
            callback,
        )
    }))
}

pub(in crate::native) struct CleanupWorkloadScope<'a> {
    pub(in crate::native) runtime: &'a NativeRuntime,
    pub(in crate::native) previous: bool,
}

impl Drop for CleanupWorkloadScope<'_> {
    fn drop(&mut self) {
        self.runtime.cleanup_workloads.set(self.previous);
    }
}

pub(in crate::native) fn current_native_task() -> Option<NativeTaskHandle> {
    CURRENT_NATIVE_TASK.with(Cell::get)
}

pub(super) fn with_native_task<T>(task: NativeTaskHandle, operation: impl FnOnce() -> T) -> T {
    let previous = CURRENT_NATIVE_TASK.with(|current| current.replace(Some(task)));
    let _binding = NativeTaskBindingScope(previous);

    operation()
}

struct NativeTaskBindingScope(Option<NativeTaskHandle>);

impl Drop for NativeTaskBindingScope {
    fn drop(&mut self) {
        CURRENT_NATIVE_TASK.with(|current| current.set(self.0));
    }
}

pub(in crate::native) fn write_cleanup_incident_report(
    writer: &mut dyn Write,
    incident: &crate::CleanupIncident,
) -> std::io::Result<()> {
    write!(
        writer,
        "cleanup_incident ordinal={} producer=",
        incident.ordinal()
    )?;

    match incident.producer() {
        CleanupIncidentProducer::SynchronousRoot => write!(writer, "synchronous_root")?,
        CleanupIncidentProducer::Task(task) => write!(writer, "task:{}", task.raw())?,
    }

    match incident.origin() {
        crate::CleanupIncidentOrigin::SynchronousRoot => writeln!(writer),
        crate::CleanupIncidentOrigin::ProtectedFrame { frame, state } => {
            write!(writer, " frame=")?;

            for byte in frame.digest() {
                write!(writer, "{byte:02x}")?;
            }

            writeln!(writer, " state={}", state.raw())
        }
    }
}

pub(in crate::native) fn task_outcome(
    outcome: RunOutcome<usize>,
    terminal: &NativeTerminalState,
) -> Result<NativeRunOutcome, crate::RuntimePanic> {
    let outcome = match outcome {
        RunOutcome::Completed(payload) => NativeRunOutcome::new(NativeRunState::COMPLETED, payload),
        RunOutcome::Cancelled => NativeRunOutcome::new(NativeRunState::CANCELLED, 0),
        RunOutcome::Panicked(mut panic) => {
            let Some(payload) = terminal.take_panic_payload() else {
                return Err(panic);
            };

            for incident in panic.take_suppressed() {
                terminal
                    .record_cleanup_incident(crate::incident::OwnedCleanupIncident::host(incident));
            }

            NativeRunOutcome::new(NativeRunState::PANICKED, payload)
        }
    };

    Ok(terminal.resolve_cleanup_outcome(outcome))
}

pub(in crate::native) fn lane_result(lane: ExecutionLane) -> NativeExecutionLaneResult {
    let lane = match (lane.placement(), lane.workload()) {
        (_, ExecutionWorkload::Blocking) => NativeExecutionLane::BLOCKING,
        (_, ExecutionWorkload::Compute) => NativeExecutionLane::COMPUTE,
        (ExecutionLanePlacement::MainThread(_), _) => NativeExecutionLane::MAIN_THREAD,
        (ExecutionLanePlacement::OriginThread(_), _) => NativeExecutionLane::ORIGIN_THREAD,
        (ExecutionLanePlacement::Migratable | ExecutionLanePlacement::PinnedWorker(_), _) => {
            NativeExecutionLane::COOPERATIVE
        }
    };

    NativeExecutionLaneResult::success(lane)
}

pub(in crate::native) fn runtime_failure(status: NativeRuntimeStatus) -> NativeRunOutcome {
    NativeRunOutcome::new(NativeRunState::RUNTIME_FAILURE, status.code() as usize)
}

#[cfg(test)]
mod tests {
    use super::with_cleanup_runtime;
    use super::{current_native_task, with_native_task, write_cleanup_incident_report};
    use crate::native::state::{initialize, retain_runtime, shutdown, with_runtime};
    use crate::test_support::with_allocation_failure;
    use crate::{CleanupIncidentOrigin, CleanupIncidentProducer, CleanupReportSink};
    use bray_runtime_abi::{NativeRuntimeConfiguration, NativeRuntimeStatus};
    use std::sync::Arc;

    #[test]
    fn retention_on_another_thread_preserves_domain_main_authority() {
        let _isolation = super::super::test_runtime_isolation();

        // Keep the Main attachment alive after its base execution binding is shut down.
        let main = bray_platform::RuntimeThreadScope::enter().unwrap();
        assert!(initialize(NativeRuntimeConfiguration::new(4, 1)).is_success());
        let retained = retain_runtime().unwrap();
        let shared = retained.clone();

        let from_other = std::thread::spawn(move || {
            with_cleanup_runtime(&shared, || {
                with_runtime(|runtime| assert!(!runtime.main_thread_lane)).unwrap();

                retain_runtime().unwrap()
            })
            .unwrap()
        })
        .join()
        .unwrap();

        assert!(shutdown().is_success());

        with_cleanup_runtime(&from_other, || {
            with_runtime(|runtime| {
                assert!(runtime.main_thread_lane);
                assert_eq!(runtime.thread.runtime().id(), main.runtime().id());
            })
            .unwrap();
        })
        .unwrap();

        retained.release();
        from_other.release();

        // Reusing a physical Main attachment cannot grant authority to a no-Main domain.
        let isolated = super::super::admit_cleanup_runtime().unwrap();

        with_cleanup_runtime(&isolated, || {
            with_runtime(|runtime| assert!(!runtime.main_thread_lane)).unwrap();
        })
        .unwrap();

        isolated.release();
    }

    #[test]
    fn cleanup_lane_search_preserves_placement_and_workload_priority() {
        use crate::{ExecutionLanePlacement, ExecutionWorkload};

        let scope = bray_platform::RuntimeThreadScope::enter().unwrap();
        let thread = scope.runtime().id();

        let all = super::current_thread_lanes(thread, true, true)
            .map(|lane| (lane.placement(), lane.workload()))
            .collect::<Vec<_>>();

        assert_eq!(
            all,
            [
                (
                    ExecutionLanePlacement::MainThread(thread),
                    ExecutionWorkload::Cooperative
                ),
                (
                    ExecutionLanePlacement::MainThread(thread),
                    ExecutionWorkload::Blocking
                ),
                (
                    ExecutionLanePlacement::MainThread(thread),
                    ExecutionWorkload::Compute
                ),
                (
                    ExecutionLanePlacement::OriginThread(thread),
                    ExecutionWorkload::Cooperative
                ),
                (
                    ExecutionLanePlacement::OriginThread(thread),
                    ExecutionWorkload::Blocking
                ),
                (
                    ExecutionLanePlacement::OriginThread(thread),
                    ExecutionWorkload::Compute
                ),
                (
                    ExecutionLanePlacement::PinnedWorker(thread),
                    ExecutionWorkload::Cooperative
                ),
                (
                    ExecutionLanePlacement::PinnedWorker(thread),
                    ExecutionWorkload::Blocking
                ),
                (
                    ExecutionLanePlacement::PinnedWorker(thread),
                    ExecutionWorkload::Compute
                ),
                (
                    ExecutionLanePlacement::Migratable,
                    ExecutionWorkload::Cooperative
                ),
                (
                    ExecutionLanePlacement::Migratable,
                    ExecutionWorkload::Blocking
                ),
                (
                    ExecutionLanePlacement::Migratable,
                    ExecutionWorkload::Compute
                ),
            ]
        );

        for main in [false, true] {
            for cleanup in [false, true] {
                let lanes = super::current_thread_lanes(thread, main, cleanup);

                let expected = all.iter().copied().filter(|(placement, workload)| {
                    (main || !matches!(placement, ExecutionLanePlacement::MainThread(_)))
                        && (cleanup
                            || (*placement != ExecutionLanePlacement::Migratable
                                && *workload == ExecutionWorkload::Cooperative))
                });

                assert!(
                    lanes
                        .clone()
                        .map(|lane| (lane.placement(), lane.workload()))
                        .eq(expected)
                );

                assert!(lanes.clone().eq(lanes));
            }
        }
    }

    #[test]
    fn nested_native_task_bindings_restore_the_parent_even_after_unwind() {
        let parent = bray_runtime_abi::NativeTaskHandle::new(1).unwrap();
        let child = bray_runtime_abi::NativeTaskHandle::new(2).unwrap();

        assert_eq!(current_native_task(), None);

        with_native_task(parent, || {
            assert_eq!(current_native_task(), Some(parent));
            with_native_task(child, || assert_eq!(current_native_task(), Some(child)));
            assert_eq!(current_native_task(), Some(parent));

            let failure = std::panic::catch_unwind(|| {
                with_native_task(child, || {
                    assert_eq!(current_native_task(), Some(child));

                    panic!("nested native task failed");
                });
            });

            assert!(failure.is_err());
            assert_eq!(current_native_task(), Some(parent));
        });

        assert_eq!(current_native_task(), None);
    }

    #[test]
    fn cleanup_incident_reports_are_stable_and_observable() {
        let reports = CleanupReportSink::new();

        let origin = CleanupIncidentOrigin::new(
            bray_runtime_model::ProtectedAsyncFrameId::new([5; 32]),
            bray_runtime_model::ProtectedFrameStateId::new(7),
        );

        reports.transfer(
            CleanupIncidentProducer::SynchronousRoot,
            origin,
            "cleanup failed",
        );

        let mut output = Vec::new();

        reports.drain(|incident| {
            write_cleanup_incident_report(&mut output, &incident)
                .unwrap_or_else(|error| panic!("test report must write: {error}"));
        });

        let expected = format!(
            "cleanup_incident ordinal=0 producer=synchronous_root frame={} state=7\n",
            "05".repeat(32)
        );

        assert_eq!(String::from_utf8(output), Ok(expected));
    }

    #[test]
    fn synchronous_incident_reports_do_not_invent_a_frame_or_state() {
        let reports = CleanupReportSink::new();

        reports.transfer(
            CleanupIncidentProducer::SynchronousRoot,
            CleanupIncidentOrigin::SynchronousRoot,
            "cleanup failed",
        );

        let mut output = Vec::new();

        reports.drain(|incident| write_cleanup_incident_report(&mut output, &incident).unwrap());

        assert_eq!(
            String::from_utf8(output).unwrap(),
            "cleanup_incident ordinal=0 producer=synchronous_root\n"
        );
    }

    #[test]
    fn borrowed_cleanup_binding_preserves_runtime_owners_and_rejects_root_shutdown() {
        assert!(initialize(NativeRuntimeConfiguration::new(4, 1)).is_success());
        let retained = retain_runtime().unwrap();
        assert!(shutdown().is_success());
        assert!(initialize(NativeRuntimeConfiguration::new(4, 1)).is_success());
        let previous = retain_runtime().unwrap();

        let owners = retained
            .core
            .owners
            .load(std::sync::atomic::Ordering::Acquire);

        with_cleanup_runtime(&retained, || {
            assert_eq!(
                initialize(NativeRuntimeConfiguration::new(4, 1)),
                NativeRuntimeStatus::ALREADY_INITIALIZED
            );

            assert_eq!(shutdown(), NativeRuntimeStatus::INVALID_ARGUMENT);
            let selected = retain_runtime().unwrap();
            assert!(Arc::ptr_eq(&selected.core, &retained.core));
            selected.release();

            assert_eq!(
                retained
                    .core
                    .owners
                    .load(std::sync::atomic::Ordering::Acquire),
                owners
            );

            with_cleanup_runtime(&previous, || {
                with_runtime(|runtime| assert!(Arc::ptr_eq(&runtime.core, &previous.core)))
                    .unwrap();
            })
            .unwrap();

            with_runtime(|runtime| assert!(Arc::ptr_eq(&runtime.core, &retained.core))).unwrap();
        })
        .unwrap();

        with_runtime(|runtime| assert!(Arc::ptr_eq(&runtime.core, &previous.core))).unwrap();
        previous.release();
        retained.release();
        assert!(shutdown().is_success());
    }

    #[test]
    fn retained_binding_on_an_attached_thread_needs_no_allocation_and_restores_after_unwind() {
        assert!(initialize(NativeRuntimeConfiguration::new(4, 1)).is_success());
        let retained = retain_runtime().unwrap();
        assert!(shutdown().is_success());
        assert!(initialize(NativeRuntimeConfiguration::new(4, 1)).is_success());
        let previous = with_runtime(|runtime| Arc::clone(&runtime.core)).unwrap();
        let thread = bray_platform::current_runtime_thread().unwrap().id();

        let result = with_allocation_failure(|| {
            with_cleanup_runtime(&retained, || {
                with_runtime(|runtime| {
                    assert!(Arc::ptr_eq(&runtime.core, &retained.core));
                    assert_eq!(runtime.thread.runtime().id(), thread);
                    assert!(runtime.cleanup_workloads.get());
                })
                .unwrap();

                with_cleanup_runtime(&retained, || 42).unwrap()
            })
        });

        assert_eq!(result, Ok(42));

        let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = with_cleanup_runtime(&retained, || panic!("restore cleanup binding"));
        }));

        assert!(unwind.is_err());

        with_runtime(|runtime| {
            assert!(Arc::ptr_eq(&runtime.core, &previous));
            assert!(!runtime.cleanup_workloads.get());
            assert_eq!(runtime.thread.runtime().id(), thread);
        })
        .unwrap();

        retained.release();
        assert!(shutdown().is_success());
    }
}
