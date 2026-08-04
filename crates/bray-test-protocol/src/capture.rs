use std::sync::Arc;

use bray_base::shared_slice;

/// Standard stream owned by one test invocation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TestStreamKind {
    /// Standard output.
    StandardOutput,
    /// Standard error.
    StandardError,
}

/// Capture behavior selected for one test invocation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TestCapturePolicy {
    /// Retain a bounded prefix of each standard stream.
    Captured(TestCaptureLimits),
    /// Allow test code to use the host process streams directly.
    Inherited,
    /// Discard test output.
    Discarded,
}

/// Byte budgets applied to one captured test invocation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TestCaptureLimits {
    per_stream_byte_limit: u64,
    invocation_byte_limit: u64,
}

impl TestCaptureLimits {
    /// Creates explicit per-stream and invocation-wide capture limits.
    pub const fn new(per_stream_byte_limit: u64, invocation_byte_limit: u64) -> Self {
        Self {
            per_stream_byte_limit,
            invocation_byte_limit,
        }
    }

    /// Returns the maximum retained byte count for either stream.
    pub const fn per_stream_byte_limit(self) -> u64 {
        self.per_stream_byte_limit
    }

    /// Returns the maximum retained byte count across both streams.
    pub const fn invocation_byte_limit(self) -> u64 {
        self.invocation_byte_limit
    }
}

/// Capture behavior recorded for one completed stream.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CapturedStreamPolicy {
    /// Output was retained by a dedicated invocation sink.
    Captured,
    /// Output used the host process stream directly.
    Inherited,
    /// Output was discarded.
    Discarded,
}

/// Stable category for a stream failure observed during one invocation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TestStreamFailureKind {
    /// The selected capture or protocol byte budget was exhausted.
    ResourceExhausted,
    /// The invocation-owned stream sink was unavailable.
    SinkUnavailable,
    /// The underlying inherited stream rejected an operation.
    Platform,
}

/// Structured failure retained independently of captured bytes.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TestStreamFailure {
    kind: TestStreamFailureKind,
    platform_code: Option<i64>,
}

impl TestStreamFailure {
    /// Creates a stream failure with an optional platform-specific code.
    pub const fn new(kind: TestStreamFailureKind, platform_code: Option<i64>) -> Self {
        Self {
            kind,
            platform_code,
        }
    }

    /// Returns the stable failure category.
    pub const fn kind(self) -> TestStreamFailureKind {
        self.kind
    }

    /// Returns the platform-specific code when one was available.
    pub const fn platform_code(self) -> Option<i64> {
        self.platform_code
    }
}

/// Completed output state for one invocation-owned standard stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapturedStream {
    policy: CapturedStreamPolicy,
    bytes: Arc<[u8]>,
    discarded_byte_count: u64,
    failure: Option<TestStreamFailure>,
}

impl CapturedStream {
    /// Creates a captured stream from its retained prefix and discarded suffix count.
    pub fn captured(
        bytes: impl IntoIterator<Item = u8>,
        discarded_byte_count: u64,
        failure: Option<TestStreamFailure>,
    ) -> Self {
        Self {
            policy: CapturedStreamPolicy::Captured,
            bytes: shared_slice(bytes),
            discarded_byte_count,
            failure,
        }
    }

    /// Creates a stream that used the host process handle directly.
    pub fn inherited(failure: Option<TestStreamFailure>) -> Self {
        Self::without_bytes(CapturedStreamPolicy::Inherited, failure)
    }

    /// Creates a stream whose bytes were deliberately discarded.
    pub fn discarded() -> Self {
        Self::without_bytes(CapturedStreamPolicy::Discarded, None)
    }

    fn without_bytes(policy: CapturedStreamPolicy, failure: Option<TestStreamFailure>) -> Self {
        Self {
            policy,
            bytes: Arc::from([]),
            discarded_byte_count: 0,
            failure,
        }
    }

    /// Returns how output from this stream was handled.
    pub const fn policy(&self) -> CapturedStreamPolicy {
        self.policy
    }

    /// Returns the retained output prefix.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns whether output exceeded the selected capture limit.
    pub const fn truncated(&self) -> bool {
        self.discarded_byte_count != 0
    }

    /// Returns the known number of bytes discarded after the retained prefix.
    pub const fn discarded_byte_count(&self) -> u64 {
        self.discarded_byte_count
    }

    /// Returns a stream failure independently of any retained output.
    pub const fn failure(&self) -> Option<TestStreamFailure> {
        self.failure
    }
}

#[cfg(test)]
mod tests {
    use super::{CapturedStream, CapturedStreamPolicy};

    #[test]
    fn captured_streams_retain_prefix_and_truncation_independently() {
        let stream = CapturedStream::captured(b"prefix".iter().copied(), 9, None);

        assert_eq!(stream.policy(), CapturedStreamPolicy::Captured);
        assert_eq!(stream.bytes(), b"prefix");
        assert!(stream.truncated());
        assert_eq!(stream.discarded_byte_count(), 9);
        assert_eq!(stream.failure(), None);
    }
}
