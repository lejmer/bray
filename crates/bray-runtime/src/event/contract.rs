/// Monotonic change generation published by one runtime event.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RuntimeEventGeneration(pub(super) u64);

impl RuntimeEventGeneration {
    /// Returns the process-local numeric generation.
    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// Failure to observe or mutate one runtime event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeEventError {
    /// Event generations cannot advance without losing ordering.
    GenerationExhausted,
    /// Wait storage could not be reserved.
    AllocationFailed,
    /// The reserved wait record still belongs to an outstanding registration.
    AlreadyRegistered,
}

/// Infallible notification used when a runtime event changes or closes.
pub trait RuntimeEventWake: Send + Sync + 'static {
    /// Makes the registered observer runnable.
    fn wake(&self);
}

impl<F> RuntimeEventWake for F
where
    F: Fn() + Send + Sync + 'static,
{
    fn wake(&self) {
        self();
    }
}
