use std::sync::Arc;

use bray_runtime_abi::NativeTaskReference;

use crate::{CancellationObservation, TaskWakeHandle};

use super::host::{CancellationWake, CancellationWakeRegistration, CancellationWakeRegistrationError, HostContext};
use super::native::NativeCancellation;

/// Shared cancellation authority for a host run or a retained Bray task.
#[derive(Clone, Debug)]
pub struct CancellationContext {
    state: State,
}

#[derive(Clone, Debug)]
enum State {
    Host(HostContext),
    Native(Arc<NativeCancellation>),
}

impl CancellationContext {
    /// Creates an uncancelled host run.
    pub fn root() -> Self {
        Self { state: State::Host(HostContext::root()) }
    }

    pub(crate) fn native(reference: NativeTaskReference, wake: TaskWakeHandle) -> Self {
        Self { state: State::Native(Arc::new(NativeCancellation { reference, wake })) }
    }

    pub(crate) fn native_reference(&self) -> &NativeTaskReference {
        match &self.state {
            State::Native(owner) => &owner.reference,
            State::Host(_) => panic!("native task services require their retained Bray cell"),
        }
    }

    /// Creates a host child that observes its parent's cancellation requests.
    pub fn child(&self) -> Self {
        match &self.state {
            State::Host(owner) => Self { state: State::Host(owner.child()) },
            State::Native(_) => panic!("Bray tasks propagate through generated owned-task visitors"),
        }
    }

    /// Requests cooperative cancellation without completing the run.
    pub fn request(&self) -> bool {
        match &self.state {
            State::Host(owner) => owner.request(),
            State::Native(owner) => owner.request(),
        }
    }

    /// Returns whether an unshielded request can be delivered.
    pub fn is_requested(&self) -> bool {
        self.observation().observable()
    }

    pub(crate) fn recheck_dispatch(&self) -> bool {
        match &self.state {
            State::Host(owner) => owner.is_requested(),
            // Native request and outer shield release enqueue even while the
            // task runs. The scheduler already retains and coalesces that wake.
            State::Native(_) => false,
        }
    }

    /// Captures pending and currently observable cancellation state.
    pub fn observation(&self) -> CancellationObservation {
        match &self.state {
            State::Host(owner) => owner.observation(),
            State::Native(owner) => owner.observation(),
        }
    }

    /// Masks delivery while preserving the request for cleanup observation.
    pub fn shield(&self) -> CancellationShield {
        self.enter_shield();

        CancellationShield { context: self.clone() }
    }

    pub(crate) fn enter_shield(&self) {
        match &self.state {
            State::Host(owner) => owner.enter_shield(),
            State::Native(owner) => { owner.operation(2); }
        }
    }

    pub(crate) fn leave_shield(&self) {
        match &self.state {
            State::Host(owner) => owner.leave_shield(),
            State::Native(owner) => {
                if owner.operation(3) != 0 {
                    let _ = owner.wake.wake_cancellation();
                }
            }
        }
    }

    pub(crate) fn register_wake(
        &self,
        wake: impl FnOnce() -> Arc<dyn CancellationWake>,
    ) -> Result<Option<CancellationWakeRegistration>, CancellationWakeRegistrationError> {
        match &self.state {
            State::Host(owner) => owner.register_wake(wake()).map(Some),
            // The native context already retains its admitted scheduler wake.
            State::Native(_) => Ok(None),
        }
    }
}

impl Default for CancellationContext {
    fn default() -> Self { Self::root() }
}

/// Scoped suppression of ordinary cancellation delivery during cleanup.
#[derive(Debug)]
pub struct CancellationShield {
    context: CancellationContext,
}

impl Drop for CancellationShield {
    fn drop(&mut self) { self.context.leave_shield(); }
}
