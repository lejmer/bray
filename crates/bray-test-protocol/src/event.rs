use std::sync::Arc;

use bray_base::shared_slice;

use crate::{TestIdentity, TestInfrastructureFailure, TestOutcome, TestStreamKind};

/// Per-invocation monotonic event sequence assigned by the native host.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TestHostEventSequence(u64);

impl TestHostEventSequence {
    /// Creates a sequence from its zero-based host position.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the zero-based host position.
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Typed event emitted while a native host executes one admitted test.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TestHostEventKind {
    /// The host accepted and started the invocation.
    Started,
    /// One invocation-owned standard stream produced bytes.
    StreamChunk {
        /// Stream that produced the bytes.
        stream: TestStreamKind,
        /// Exact byte chunk in host emission order.
        bytes: Arc<[u8]>,
    },
    /// The root reached a terminal semantic outcome.
    Terminal(TestOutcome),
    /// Test-local cleanup completed and the terminal outcome can be published.
    CleanupComplete,
    /// Host infrastructure failed while owning this invocation.
    InfrastructureFailed(TestInfrastructureFailure),
}

impl TestHostEventKind {
    /// Creates a stream event from one exact byte chunk.
    pub fn stream_chunk(stream: TestStreamKind, bytes: impl IntoIterator<Item = u8>) -> Self {
        Self::StreamChunk {
            stream,
            bytes: shared_slice(bytes),
        }
    }
}

/// One test-identified event from a native product host.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestHostEvent {
    identity: TestIdentity,
    sequence: TestHostEventSequence,
    kind: TestHostEventKind,
}

impl TestHostEvent {
    /// Creates an event with an invocation-local monotonic sequence.
    pub const fn new(
        identity: TestIdentity,
        sequence: TestHostEventSequence,
        kind: TestHostEventKind,
    ) -> Self {
        Self {
            identity,
            sequence,
            kind,
        }
    }

    /// Returns the invocation that owns this event.
    pub const fn identity(&self) -> &TestIdentity {
        &self.identity
    }

    /// Returns this event's invocation-local sequence.
    pub const fn sequence(&self) -> TestHostEventSequence {
        self.sequence
    }

    /// Returns the typed event payload.
    pub const fn kind(&self) -> &TestHostEventKind {
        &self.kind
    }
}
