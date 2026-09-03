/// Reference-runtime component owning an execution role.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RuntimeRoleArtifact {
    /// Compiler-generated operation without a separately linked symbol.
    Compiler,
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
