use bray_runtime_abi::{NativeEventReference, NativeRuntimeStatus};

use super::{NativeCallback, native};
use crate::CancellationContext;
use crate::context::with_independent_execution_context as independent;

/// An internal retained borrow acquired at the external event boundary.
pub(crate) struct Event {
    reference: NativeEventReference,
}

impl Event {
    pub(crate) fn acquire(runtime: u64, identity: usize) -> Option<Self> {
        let mut reference = NativeEventReference::default();
        let status = independent(|| native::acquisition(runtime, identity, &mut reference));

        (status == 0).then(|| Self { reference })
    }

    pub(crate) fn observation(&self) -> (u64, bool) {
        let mut generation = 0;
        let closed = independent(|| native::observation(&self.reference, &mut generation));

        (generation, closed != 0)
    }

    pub(crate) fn register(
        &self,
        observed: u64,
        target: CancellationContext,
        wake: &NativeCallback,
    ) -> Result<EventRegistration, NativeRuntimeStatus> {
        let descriptor = wake.retain();

        let status = independent(|| {
            native::registration(
                &self.reference,
                observed,
                target.native_reference(),
                &descriptor,
            )
        });

        let registration = EventRegistration { target };

        if !status.is_success() {
            return Err(status);
        }

        Ok(registration)
    }
}

impl Drop for Event {
    fn drop(&mut self) {
        independent(|| native::release(&self.reference));
    }
}

/// Withdrawal authority borrowing the task's coadmitted Source wait slot.
pub(crate) struct EventRegistration {
    target: CancellationContext,
}

impl EventRegistration {
    pub(crate) fn is_ready(&self) -> bool {
        independent(|| native::readiness(self.target.native_reference())) != 0
    }
}

impl Drop for EventRegistration {
    fn drop(&mut self) {
        independent(|| native::withdrawal(self.target.native_reference()));
    }
}
