/// Reference-runtime component owning an execution role.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RuntimeRoleArtifact {
    /// Compiler-generated operation without a separately linked symbol.
    Compiler,
    /// Trusted Bray bootstrap component.
    Bootstrap,
    /// Trusted Bray performance-observation component.
    Observation,
    /// Product host and panic handling.
    Host,
    /// Foreign and native-thread entry.
    Callback,
    /// Task execution and scheduling.
    Scheduler,
    /// Run cancellation.
    Cancellation,
    /// Runtime events.
    Event,
    /// Native test selection and execution.
    TestHost,
}
