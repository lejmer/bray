use std::sync::Arc;
use std::time::Duration;

use bray_base::shared_str;

use crate::{
    AssertionFailure, CapturedStream, ExplicitTestFailure, TestCapturePolicy, TestCatalogDigest,
    TestIdentity, TestSourceAnchor,
};

/// Unique identity of one admitted native host command.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TestHostCommandId([u8; 32]);

impl TestHostCommandId {
    /// Creates an identity from its exact 256-bit value.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the exact identity bytes.
    pub const fn bytes(self) -> [u8; 32] {
        self.0
    }
}

/// A control message sent to an active native test host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TestHostControl {
    /// Requests cooperative command cancellation.
    Cancel,
}

/// Stable nonnegative duration used by the runner protocol.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TestDuration(u64);

impl TestDuration {
    /// Creates a duration from an exact nanosecond count.
    pub const fn from_nanoseconds(nanoseconds: u64) -> Self {
        Self(nanoseconds)
    }

    /// Converts a host duration when its nanosecond count is representable.
    pub fn try_from_duration(duration: Duration) -> Option<Self> {
        u64::try_from(duration.as_nanos()).ok().map(Self)
    }

    /// Returns the exact protocol nanosecond count.
    pub const fn nanoseconds(self) -> u64 {
        self.0
    }

    /// Returns the corresponding host duration.
    pub const fn duration(self) -> Duration {
        Duration::from_nanos(self.0)
    }
}

/// Timeout behavior selected for one test invocation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TestTimeoutPolicy {
    /// No per-invocation timeout is enforced.
    Unlimited,
    /// Cooperative cancellation is requested after this duration.
    Limit(TestDuration),
}

/// Source of an explicit test cancellation outcome.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TestCancellationSource {
    /// The complete test command was cancelled.
    Command,
    /// One selected invocation was cancelled independently.
    Invocation,
}

/// Stable zero-based position of one entry in a published test catalog.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TestCatalogEntryId(u32);

impl TestCatalogEntryId {
    /// Creates an entry identity from its canonical catalog position.
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the zero-based catalog position.
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Stable identity of a recoverable error type crossing a test boundary.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TestErrorTypeIdentity(Arc<str>);

impl TestErrorTypeIdentity {
    /// Creates a nonempty durable type identity.
    pub fn try_new(identity: impl Into<Arc<str>>) -> Option<Self> {
        let identity = shared_str(identity);

        if identity.is_empty() {
            return None;
        }

        Some(Self(identity))
    }

    /// Returns the durable type identity text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Stable panic category reported by one test root.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TestPanicCause {
    /// An explicit language panic.
    Message,
    /// A failed built-in assertion.
    Assertion,
    /// An explicit `std.testing.fail` call.
    ExplicitFailure,
    /// A panic caught at a runtime callback boundary.
    RuntimePanic,
    /// Storage could not be admitted.
    AllocationFailure,
}

/// Structured panic data crossing the native test-host boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestPanicReport {
    cause: TestPanicCause,
    source: Option<TestSourceAnchor>,
    message: Arc<str>,
}

impl TestPanicReport {
    /// Creates a panic report without rendering user-facing prose.
    pub fn new(
        cause: TestPanicCause,
        source: Option<TestSourceAnchor>,
        message: impl Into<Arc<str>>,
    ) -> Self {
        Self {
            cause,
            source,
            message: shared_str(message),
        }
    }

    /// Returns the stable panic category.
    pub const fn cause(&self) -> TestPanicCause {
        self.cause
    }

    /// Returns the source occurrence when one was available.
    pub const fn source(&self) -> Option<TestSourceAnchor> {
        self.source
    }

    /// Returns the retained panic payload text.
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Stable category for a test-host or runner infrastructure failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TestInfrastructureFailureKind {
    /// Runner and host protocol state was malformed or incompatible.
    Protocol,
    /// The native host could not execute an admitted invocation.
    Host,
    /// Per-invocation stream capture could not preserve its contract.
    Capture,
    /// A configured process, invocation, protocol, or capture budget was exhausted.
    ResourceExhausted,
    /// An admitted invocation produced no terminal outcome.
    MissingOutcome,
    /// Test-local cleanup could not complete normally.
    Cleanup,
    /// The host process required forced termination.
    ForcedTermination,
}

/// Structured infrastructure failure without rendered user-facing text.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TestInfrastructureFailure {
    kind: TestInfrastructureFailureKind,
    detail_code: Option<u64>,
}

impl TestInfrastructureFailure {
    /// Creates an infrastructure failure with an optional domain-specific code.
    pub const fn new(kind: TestInfrastructureFailureKind, detail_code: Option<u64>) -> Self {
        Self { kind, detail_code }
    }

    /// Returns the stable failure category.
    pub const fn kind(self) -> TestInfrastructureFailureKind {
        self.kind
    }

    /// Returns the optional domain-specific code.
    pub const fn detail_code(self) -> Option<u64> {
        self.detail_code
    }
}

/// Terminal semantic outcome of one test invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TestOutcome {
    /// The test returned normally without an error.
    Passed,
    /// The test returned a recoverable error value.
    ReturnedError {
        /// Durable identity of the concrete error type.
        error_type: TestErrorTypeIdentity,
        /// Formatted value when checked formatting support exists.
        formatted_value: Option<Arc<str>>,
    },
    /// `std.testing.fail` terminated the test.
    ExplicitFailure(ExplicitTestFailure),
    /// A built-in assertion failed.
    AssertionFailure(AssertionFailure),
    /// A panic crossed the isolated test root.
    Panicked(TestPanicReport),
    /// The invocation exceeded its configured limit and completed cancellation cleanup.
    TimedOut(TestDuration),
    /// Explicit cancellation completed normally.
    Cancelled(TestCancellationSource),
    /// The host or protocol could not complete the invocation contract.
    InfrastructureFailed(TestInfrastructureFailure),
}

/// Immutable runner instructions for one selected test invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestInvocationPlan {
    identity: TestIdentity,
    timeout: TestTimeoutPolicy,
    capture: TestCapturePolicy,
}

/// One admitted invocation command sent to a native product host.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestHostCommand {
    id: TestHostCommandId,
    catalog_digest: TestCatalogDigest,
    entry: TestCatalogEntryId,
    timeout: TestTimeoutPolicy,
    capture: TestCapturePolicy,
    error_type: Option<TestErrorTypeIdentity>,
}

impl TestHostCommand {
    /// Creates the complete bounded policy for one native host invocation.
    pub fn new(
        id: TestHostCommandId,
        catalog_digest: TestCatalogDigest,
        entry: TestCatalogEntryId,
        timeout: TestTimeoutPolicy,
        capture: TestCapturePolicy,
        error_type: Option<TestErrorTypeIdentity>,
    ) -> Self {
        Self {
            id,
            catalog_digest,
            entry,
            timeout,
            capture,
            error_type,
        }
    }

    /// Returns the unique command identity.
    pub const fn id(&self) -> TestHostCommandId {
        self.id
    }

    /// Returns the catalog digest selected by the runner.
    pub const fn catalog_digest(&self) -> TestCatalogDigest {
        self.catalog_digest
    }

    /// Returns the selected entry's canonical catalog position.
    pub const fn entry(&self) -> TestCatalogEntryId {
        self.entry
    }

    /// Returns the invocation timeout policy.
    pub const fn timeout(&self) -> TestTimeoutPolicy {
        self.timeout
    }

    /// Returns the invocation stream policy.
    pub const fn capture(&self) -> TestCapturePolicy {
        self.capture
    }

    /// Returns the checked recoverable error type for a fallible test entry.
    pub const fn error_type(&self) -> Option<&TestErrorTypeIdentity> {
        self.error_type.as_ref()
    }
}

/// Terminal native host state before the runner attaches a test identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestHostResult {
    command_id: TestHostCommandId,
    catalog_digest: TestCatalogDigest,
    outcome: TestOutcome,
    standard_output: CapturedStream,
    standard_error: CapturedStream,
}

impl TestHostResult {
    /// Creates one result after test-local cleanup has completed.
    pub const fn after_cleanup(
        command_id: TestHostCommandId,
        catalog_digest: TestCatalogDigest,
        outcome: TestOutcome,
        standard_output: CapturedStream,
        standard_error: CapturedStream,
    ) -> Self {
        Self {
            command_id,
            catalog_digest,
            outcome,
            standard_output,
            standard_error,
        }
    }

    /// Returns the command identity that produced this result.
    pub const fn command_id(&self) -> TestHostCommandId {
        self.command_id
    }

    /// Returns the catalog digest used by the host command.
    pub const fn catalog_digest(&self) -> TestCatalogDigest {
        self.catalog_digest
    }

    /// Returns the terminal semantic outcome.
    pub const fn outcome(&self) -> &TestOutcome {
        &self.outcome
    }

    /// Returns completed standard-output state.
    pub const fn standard_output(&self) -> &CapturedStream {
        &self.standard_output
    }

    /// Returns completed standard-error state.
    pub const fn standard_error(&self) -> &CapturedStream {
        &self.standard_error
    }

    /// Attaches the runner-owned stable identity.
    pub fn into_invocation_result(self, identity: TestIdentity) -> TestInvocationResult {
        TestInvocationResult::after_cleanup(
            identity,
            self.outcome,
            self.standard_output,
            self.standard_error,
        )
    }
}

impl TestInvocationPlan {
    /// Creates one invocation plan after selection and admission policy are known.
    pub const fn new(
        identity: TestIdentity,
        timeout: TestTimeoutPolicy,
        capture: TestCapturePolicy,
    ) -> Self {
        Self {
            identity,
            timeout,
            capture,
        }
    }

    /// Returns the selected test identity.
    pub const fn identity(&self) -> &TestIdentity {
        &self.identity
    }

    /// Returns the selected timeout behavior.
    pub const fn timeout(&self) -> TestTimeoutPolicy {
        self.timeout
    }

    /// Returns the selected stream behavior.
    pub const fn capture(&self) -> TestCapturePolicy {
        self.capture
    }
}

/// Completed immutable result for one invocation after test-local cleanup.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestInvocationResult {
    identity: TestIdentity,
    outcome: TestOutcome,
    standard_output: CapturedStream,
    standard_error: CapturedStream,
    duration: Option<TestDuration>,
}

impl TestInvocationResult {
    /// Creates a terminal result after the invocation and its cleanup have completed.
    pub const fn after_cleanup(
        identity: TestIdentity,
        outcome: TestOutcome,
        standard_output: CapturedStream,
        standard_error: CapturedStream,
    ) -> Self {
        Self {
            identity,
            outcome,
            standard_output,
            standard_error,
            duration: None,
        }
    }

    /// Returns a new result carrying its runner-observed execution duration.
    pub const fn with_duration(mut self, duration: TestDuration) -> Self {
        self.duration = Some(duration);

        self
    }

    /// Returns the exact selected test identity.
    pub const fn identity(&self) -> &TestIdentity {
        &self.identity
    }

    /// Returns the terminal semantic outcome.
    pub const fn outcome(&self) -> &TestOutcome {
        &self.outcome
    }

    /// Returns completed standard-output state.
    pub const fn standard_output(&self) -> &CapturedStream {
        &self.standard_output
    }

    /// Returns completed standard-error state.
    pub const fn standard_error(&self) -> &CapturedStream {
        &self.standard_error
    }

    /// Returns the runner-observed execution duration when one was measured.
    pub const fn duration(&self) -> Option<TestDuration> {
        self.duration
    }
}

#[cfg(test)]
mod tests {
    use bray_symbols::{ModulePathKey, SymbolName};

    use super::{TestDuration, TestInvocationPlan, TestTimeoutPolicy};
    use crate::test_support::product;
    use crate::{TestCaptureLimits, TestCapturePolicy, TestDeclarationPath, TestIdentity};

    #[test]
    fn invocation_plans_retain_typed_timeout_and_capture_policy() {
        let module = ModulePathKey::try_new(["example", "tests"])
            .unwrap_or_else(|| panic!("test module path must be valid"));

        let name = SymbolName::try_new("runs").unwrap_or_else(|| panic!("test name must be valid"));

        let identity = TestIdentity::new(product(), TestDeclarationPath::new(module, name));
        let timeout = TestDuration::from_nanoseconds(2_000_000);

        let plan = TestInvocationPlan::new(
            identity.clone(),
            TestTimeoutPolicy::Limit(timeout),
            TestCapturePolicy::Captured(TestCaptureLimits::new(4096, 6144)),
        );

        assert_eq!(plan.identity(), &identity);
        assert_eq!(plan.timeout(), TestTimeoutPolicy::Limit(timeout));

        assert_eq!(
            plan.capture(),
            TestCapturePolicy::Captured(TestCaptureLimits::new(4096, 6144))
        );
    }
}
