use std::time::Duration;

use bray_runtime_abi::NativeRuntimeStatus;

use crate::event::NativeCallback;
use crate::{RootCancellationHandle, SchedulerError, TimerRegistration};

/// Failure to retain the shared runtime or admit a run's cancellation deadline.
#[derive(Debug)]
pub enum RunTimeoutError {
    /// Native execution services could not be retained or initialized.
    Runtime(NativeRuntimeStatus),
    /// The shared scheduler could not admit the deadline and its callback.
    Deadline(SchedulerError),
}

/// Requests cancellation through the shared scheduler when the relative deadline expires.
/// The timer retains execution services until withdrawal or committed callback return.
pub fn register_run_timeout(
    duration: Duration,
    cancellation: &RootCancellationHandle,
) -> Result<TimerRegistration, RunTimeoutError> {
    let deadline = crate::scheduler::deadline_after(duration).map_err(RunTimeoutError::Deadline)?;
    let runtime = crate::native::retain_timeout_runtime().map_err(RunTimeoutError::Runtime)?;
    let scheduler = runtime.core.scheduler().clone();
    let resources = NativeCallback::timeout(cancellation.clone(), runtime).map_err(RunTimeoutError::Runtime)?;

    scheduler.register_callback(deadline, resources.retain_timer()).map_err(RunTimeoutError::Deadline)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::register_run_timeout;
    use crate::{
        RootCancellationSource, RunOutcome, current_run_cancellation_requested,
        execute_synchronous_root,
    };

    #[test]
    fn unrepresentable_timeout_preserves_target_without_starting_runtime_services() {
        let _isolation = crate::native::test_runtime_isolation();
        let root = crate::RootCancellationHandle::new(crate::CancellationContext::root());

        assert!(matches!(register_run_timeout(Duration::MAX, &root), Err(super::RunTimeoutError::Deadline(crate::SchedulerError::DeadlineOverflow))));
        assert_eq!(root.source(), None);

        assert_eq!(
            crate::native::implementation::bray_runtime_execution_shutdown(),
            bray_runtime_abi::NativeRuntimeStatus::NOT_INITIALIZED,
        );
    }

    #[test]
    fn elapsed_deadlines_request_timeout_cancellation() {
        let _isolation = crate::native::test_runtime_isolation();

        let mut timer = None;
        let mut cancellation = None;

        let mut admitted = crate::outgoing::OutgoingRecords::admit(1)
            .expect("fixture outgoing records must admit the supplied values");

        let outcome = execute_synchronous_root(
            || {
                while !current_run_cancellation_requested() {
                    std::thread::yield_now();
                }
            },
            |root| {
                cancellation = Some(root.clone());

                timer = Some(
                    register_run_timeout(Duration::ZERO, &root)
                        .unwrap_or_else(|error| panic!("test timeout must register: {error:?}")),
                );
            },
            &mut admitted,
        );

        drop(timer);
        assert!(crate::native::implementation::bray_runtime_execution_shutdown().is_success());

        assert!(matches!(outcome, RunOutcome::Completed(())));

        assert_eq!(
            cancellation.and_then(|cancellation| cancellation.source()),
            Some(RootCancellationSource::Timeout)
        );
    }

    #[test]
    fn pending_timeout_retains_services_after_runtime_entry_closes() {
        let _isolation = crate::native::test_runtime_isolation();
        let mut timer = None;
        let mut cancellation = None;
        let mut admitted = crate::outgoing::OutgoingRecords::admit(1).expect("outgoing records are admitted");
        let limit = Instant::now() + Duration::from_secs(2);

        let outcome = execute_synchronous_root(
            || {
                while !current_run_cancellation_requested() {
                    assert!(Instant::now() < limit, "retained deadline services make progress after closure");
                    std::thread::yield_now();
                }
            },
            |root| {
                cancellation = Some(root.clone());
                timer = Some(register_run_timeout(Duration::from_millis(10), &root).expect("timeout is admitted"));
                assert!(crate::native::implementation::bray_runtime_execution_shutdown().is_success());
            },
            &mut admitted,
        );

        drop(timer);
        assert!(matches!(outcome, RunOutcome::Completed(())));
        assert_eq!(cancellation.and_then(|handle| handle.source()), Some(RootCancellationSource::Timeout));
    }

    #[test]
    fn dropping_a_registration_before_its_deadline_prevents_cancellation() {
        let _isolation = crate::native::test_runtime_isolation();

        let mut timer = None;
        let mut cancellation = None;

        let mut admitted = crate::outgoing::OutgoingRecords::admit(1)
            .expect("fixture outgoing records must admit the supplied values");

        let outcome = execute_synchronous_root(
            || (),
            |root| {
                cancellation = Some(root.clone());

                timer = Some(
                    register_run_timeout(Duration::from_secs(60), &root)
                        .unwrap_or_else(|error| panic!("test timeout must register: {error:?}")),
                );
            },
            &mut admitted,
        );

        drop(timer);
        assert!(crate::native::implementation::bray_runtime_execution_shutdown().is_success());

        assert!(matches!(outcome, RunOutcome::Completed(())));

        assert_eq!(
            cancellation.and_then(|cancellation| cancellation.source()),
            None
        );
    }

    #[test]
    fn shared_scheduler_enforces_multiple_independent_deadlines() {
        let _isolation = crate::native::test_runtime_isolation();

        let mut first = None;
        let mut second = None;
        let mut registrations = Vec::new();

        let mut first_admitted = crate::outgoing::OutgoingRecords::admit(1)
            .expect("fixture outgoing records must admit the supplied values");

        let _ = execute_synchronous_root(
            || (),
            |root| {
                first = Some(root.clone());

                registrations.push(
                    register_run_timeout(Duration::ZERO, &root)
                        .unwrap_or_else(|error| panic!("first timeout must register: {error:?}")),
                );
            },
            &mut first_admitted,
        );

        let mut second_admitted = crate::outgoing::OutgoingRecords::admit(1)
            .expect("fixture outgoing records must admit the supplied values");

        let _ = execute_synchronous_root(
            || (),
            |root| {
                second = Some(root.clone());

                registrations.push(
                    register_run_timeout(Duration::ZERO, &root)
                        .unwrap_or_else(|error| panic!("second timeout must register: {error:?}")),
                );
            },
            &mut second_admitted,
        );

        wait_for_timeout(
            first
                .as_ref()
                .unwrap_or_else(|| panic!("first cancellation handle must exist")),
        );

        wait_for_timeout(
            second
                .as_ref()
                .unwrap_or_else(|| panic!("second cancellation handle must exist")),
        );

        assert_eq!(registrations.len(), 2);
        drop(registrations);
        assert!(crate::native::implementation::bray_runtime_execution_shutdown().is_success());
    }

    fn wait_for_timeout(cancellation: &crate::RootCancellationHandle) {
        let deadline = Instant::now() + Duration::from_secs(1);

        while cancellation.source().is_none() {
            assert!(
                Instant::now() < deadline,
                "timeout worker must make progress"
            );

            std::thread::yield_now();
        }
    }
}
