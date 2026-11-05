use std::time::Duration;

use bray_runtime_abi::NativeRuntimeStatus;

use crate::event::NativeCallback;
use crate::{RootCancellationHandle, SchedulerError, TimerRegistration};

#[expect(unsafe_code, reason = "timeout operations borrow the complete retained Source execution owner")]
unsafe extern "C" {
    safe fn bray_runtime_execution_timeout_preparation(
        owner: &crate::native::RetainedRuntime,
        failure: &mut bray_runtime_abi::NativeWorkerFailure,
    ) -> NativeRuntimeStatus;
    safe fn bray_runtime_execution_timeout_registration(
        owner: &crate::native::RetainedRuntime,
        deadline: u64,
        callback: &bray_runtime_abi::NativeEventCallback,
        identity: &mut u64,
    ) -> u32;
    safe fn bray_runtime_execution_timeout_withdrawal(
        owner: &crate::native::RetainedRuntime,
        identity: u64,
    ) -> u32;
}

/// Failure to retain the shared runtime or admit a run's cancellation deadline.
#[derive(Debug)]
pub enum RunTimeoutError {
    /// Native execution services could not be retained or initialized.
    Runtime(NativeRuntimeStatus),
    /// The demanded progress worker could not establish its target thread or attachment.
    Worker(bray_runtime_abi::NativeWorkerFailure),
    /// Native cancellation callback backing could not be admitted.
    Callback(NativeRuntimeStatus),
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
    let runtime = crate::native::retain_runtime().map_err(RunTimeoutError::Runtime)?;
    let resources = NativeCallback::timeout(cancellation.clone(), runtime)
        .map_err(RunTimeoutError::Callback)?;
    let descriptor = resources.retain_timer();
    let owner = NativeCallback::resources(descriptor.context);
    let crate::event::Resources::Timeout { runtime, .. } = owner.as_ref() else {
        unreachable!("timeout admission retains its execution owner");
    };

    let registered = crate::native::with_cleanup_runtime(runtime, || {
        let mut failure = bray_runtime_abi::NativeWorkerFailure::default();
        let status = bray_runtime_execution_timeout_preparation(runtime, &mut failure);

        if failure.stage != 0 {
            return Err(RunTimeoutError::Worker(failure));
        }

        if !status.is_success() {
            return Err(RunTimeoutError::Runtime(status));
        }

        let mut identity = 0;
        let status = bray_runtime_execution_timeout_registration(runtime, deadline, &descriptor, &mut identity);

        crate::scheduler::deadline_registration_result(status, identity).map_err(RunTimeoutError::Deadline)
    });

    match registered {
        Ok((Ok(identity), status)) => {
            let registration = TimerRegistration::execution(identity, owner);

            if status.is_success() {
                Ok(registration)
            } else {
                drop(registration);

                Err(RunTimeoutError::Runtime(status))
            }
        }
        rejected => {
            let released = (descriptor.call)(descriptor.context, 0);

            assert!(released.is_success(), "rejected timeout releases its callback descriptor once");

            match rejected {
                Ok((Err(error), status)) => {
                    if !status.is_success() {
                        crate::native::record_cleanup_failure(1);
                    }

                    Err(error)
                }
                Err(status) => Err(RunTimeoutError::Runtime(status)),
                Ok((Ok(_), _)) => unreachable!("successful registration transfers the descriptor"),
            }
        }
    }
}

pub(crate) fn withdraw(runtime: &crate::native::RetainedRuntime, identity: u64) {
    // This resource owner remains live through descriptor release. Withdrawal
    // needs no new driver or attachment and cannot fail admission during Drop.
    let removed = bray_runtime_execution_timeout_withdrawal(runtime, identity);

    assert!(removed <= 1, "Source withdraws a deadline or observes its committed delivery");
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

        assert!(matches!(
            register_run_timeout(Duration::MAX, &root),
            Err(super::RunTimeoutError::Deadline(
                crate::SchedulerError::DeadlineOverflow
            ))
        ));

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

        let mut admitted =
            crate::outgoing::OutgoingRecords::admit(1).expect("outgoing records are admitted");

        let limit = Instant::now() + Duration::from_secs(2);

        let outcome = execute_synchronous_root(
            || {
                while !current_run_cancellation_requested() {
                    assert!(
                        Instant::now() < limit,
                        "retained deadline services make progress after closure"
                    );

                    std::thread::yield_now();
                }
            },
            |root| {
                cancellation = Some(root.clone());

                timer = Some(
                    register_run_timeout(Duration::from_millis(10), &root)
                        .expect("timeout is admitted"),
                );

                assert!(
                    crate::native::implementation::bray_runtime_execution_shutdown().is_success()
                );
            },
            &mut admitted,
        );

        drop(timer);
        assert!(matches!(outcome, RunOutcome::Completed(())));

        assert_eq!(
            cancellation.and_then(|handle| handle.source()),
            Some(RootCancellationSource::Timeout)
        );
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
