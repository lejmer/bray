use crate::DiagnosticIoErrorKind;

/// Specific failure in a declared source generator's closed execution contract.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticProjectGeneratorFailure {
    /// A generator or generated output reference does not exist.
    UnknownReference {
        /// Exact reference.
        name: String,
    },
    /// A tool is absent or is not an executable product for the declared target.
    ToolNotExecutable {
        /// Exact package/product/target selection.
        tool: String,
    },
    /// The tool target does not match the configured execution host.
    ToolTargetMismatch {
        /// Declared target.
        target: String,
        /// Compiler host target, absent when the host has no supported native target.
        host_target: Option<String>,
    },
    /// An environment name or value cannot be passed to a process.
    InvalidEnvironment {
        /// Environment variable name.
        name: String,
    },
    /// Declared outputs overlap or occur more than once.
    OutputCollision {
        /// Conflicting output identity.
        path: String,
    },
    /// An output does not have a portable relative `.bray` identity.
    InvalidOutput {
        /// Supplied output identity.
        path: String,
    },
    /// An executor wrote an output outside the declared inventory.
    UndeclaredOutput {
        /// Supplied output identity.
        path: String,
    },
    /// An executor did not produce a required output.
    MissingOutput {
        /// Declared output identity.
        path: String,
    },
    /// A generated source is not UTF-8.
    NonUtf8Output {
        /// Declared output identity.
        path: String,
    },
    /// An input is a link, directory, or other non-regular file.
    InputNotRegular {
        /// Declared input identity.
        path: String,
    },
    /// Reading a declared input failed.
    InputIo {
        /// Declared input identity.
        path: String,
        /// Exact host I/O cause.
        kind: DiagnosticIoErrorKind,
    },
    /// Product, host-tool, and generated-source dependencies form a cycle.
    DependencyCycle,
    /// The caller has not supplied a sandbox executor.
    ExecutorUnavailable,
    /// The host did not supply the exact declared tool artifact.
    ToolUnavailable {
        /// Exact package/product/target selection.
        tool: String,
    },
    /// The operation was cancelled before publication.
    Cancelled,
    /// The executor cannot enforce the required sandbox policy.
    SandboxUnavailable,
    /// The sandbox rejected an attempted access.
    AccessDenied {
        /// Logical resource rejected by the sandbox.
        resource: String,
    },
    /// The generator process failed.
    ExecutionFailed {
        /// Exit code, or absence if terminated by the host.
        code: Option<i32>,
    },
    /// The sandbox executor encountered an I/O failure.
    ExecutorIo {
        /// Exact host I/O cause.
        kind: DiagnosticIoErrorKind,
    },
}
