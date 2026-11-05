use std::panic::{AssertUnwindSafe, catch_unwind};

#[cfg(test)]
use std::panic::panic_any;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use crate::context::with_run_cancellation_context;
use crate::{CancellationContext, RunOutcome};

/// Product-host authority to request cancellation of the executable root run.
#[derive(Clone, Debug)]
pub struct RootCancellationHandle {
    cancellation: CancellationContext,
    source: Arc<AtomicU8>,
}

impl RootCancellationHandle {
    pub(crate) fn new(cancellation: CancellationContext) -> Self {
        Self {
            cancellation,
            source: Arc::new(AtomicU8::new(0)),
        }
    }

    /// Requests cooperative cancellation of the executable root.
    pub fn request(&self) -> bool {
        self.request_with_source(RootCancellationSource::Explicit)
    }

    /// Requests cooperative cancellation because the host deadline elapsed.
    pub fn request_timeout(&self) -> bool {
        self.request_with_source(RootCancellationSource::Timeout)
    }

    /// Returns the first host request source when cancellation was requested.
    pub fn source(&self) -> Option<RootCancellationSource> {
        RootCancellationSource::from_code(self.source.load(Ordering::Acquire))
    }

    fn request_with_source(&self, source: RootCancellationSource) -> bool {
        let _ = self
            .source
            .compare_exchange(0, source.code(), Ordering::AcqRel, Ordering::Acquire);

        self.cancellation.request()
    }
}

/// First host action that requested root cancellation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RootCancellationSource {
    /// An explicit host or runner request.
    Explicit,
    /// The configured root deadline elapsed.
    Timeout,
}

impl RootCancellationSource {
    const fn code(self) -> u8 {
        match self {
            Self::Explicit => 1,
            Self::Timeout => 2,
        }
    }

    const fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::Explicit),
            2 => Some(Self::Timeout),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct PropagatedCancellation;

#[cfg(test)]
pub(crate) fn propagate_current_run_cancellation() -> ! {
    panic_any(PropagatedCancellation)
}

pub(crate) fn is_propagated_cancellation(payload: &(dyn std::any::Any + Send)) -> bool {
    payload.is::<PropagatedCancellation>()
}

pub(crate) fn execute_synchronous_root<T>(
    root: impl FnOnce() -> T,
    on_started: impl FnOnce(RootCancellationHandle),
    outgoing: &mut crate::outgoing::OutgoingRecords,
) -> RunOutcome<T> {
    let cancellation = CancellationContext::root();

    on_started(RootCancellationHandle::new(cancellation.clone()));

    with_run_cancellation_context(cancellation, || contain_callback(root, outgoing))
}

pub(crate) fn contain_callback<T>(
    callback: impl FnOnce() -> T,
    outgoing: &mut crate::outgoing::OutgoingRecords,
) -> RunOutcome<T> {
    match catch_unwind(AssertUnwindSafe(callback)) {
        Ok(value) => RunOutcome::Completed(value),
        Err(payload) if is_propagated_cancellation(payload.as_ref()) => RunOutcome::Cancelled,
        Err(payload) => RunOutcome::Panicked(crate::RuntimePanic::from_payload(payload, outgoing)),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        RootCancellationSource, execute_synchronous_root,
        propagate_current_run_cancellation,
    };
    use crate::{RunOutcome, current_run_cancellation_observable};

    #[test]
    fn synchronous_roots_execute_directly_and_capture_panics() {
        let _isolation = crate::native::test_runtime_isolation();

        let mut admitted = crate::outgoing::OutgoingRecords::admit(1)
            .expect("fixture outgoing records must admit the supplied values");

        assert!(matches!(
            execute_synchronous_root(|| 17, |_| {}, &mut admitted),
            RunOutcome::Completed(17)
        ));

        let panicked =
            execute_synchronous_root(|| -> i32 { panic!("root panic") }, |_| {}, &mut admitted);

        assert!(matches!(panicked, RunOutcome::Panicked(_)));
    }

    #[test]
    fn synchronous_roots_expose_host_cancellation_to_run_operations() {
        let _isolation = crate::native::test_runtime_isolation();

        let mut admitted = crate::outgoing::OutgoingRecords::admit(1)
            .expect("fixture outgoing records must admit the supplied values");

        let outcome = execute_synchronous_root(
            current_run_cancellation_observable,
            |root| {
                assert!(root.request());
            },
            &mut admitted,
        );

        assert!(matches!(outcome, RunOutcome::Completed(true)));
    }

    #[test]
    fn synchronous_cancellation_entry_reaches_the_root_outcome() {
        let _isolation = crate::native::test_runtime_isolation();

        let mut admitted = crate::outgoing::OutgoingRecords::admit(1)
            .expect("fixture outgoing records must admit the supplied values");

        let outcome =
            execute_synchronous_root(propagate_current_run_cancellation, |_| {}, &mut admitted);

        assert!(matches!(outcome, RunOutcome::Cancelled));
    }

    #[test]
    fn first_root_cancellation_source_is_stable() {
        let _isolation = crate::native::test_runtime_isolation();

        let mut cancellation = None;

        let mut admitted = crate::outgoing::OutgoingRecords::admit(1)
            .expect("fixture outgoing records must admit the supplied values");

        let outcome = execute_synchronous_root(
            || (),
            |root| {
                assert!(root.request_timeout());
                assert!(!root.request());

                cancellation = Some(root);
            },
            &mut admitted,
        );

        assert!(matches!(outcome, RunOutcome::Completed(())));

        assert_eq!(
            cancellation.and_then(|cancellation| cancellation.source()),
            Some(RootCancellationSource::Timeout)
        );
    }

}
