use std::fmt;
use std::io;

/// Native mechanism whose operation failed.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PlatformOperation {
    /// Create a native thread.
    ThreadSpawn,
    /// Join a native thread.
    ThreadJoin,
    /// Initialize runtime state for a native thread.
    ThreadRuntimeInitialization,
    /// Wait for or wake an event.
    Event,
    /// Poll registered native events.
    EventPoll,
    /// Acquire anonymous virtual memory.
    VirtualMemory,
    /// Create a child process.
    ProcessSpawn,
    /// Signal a child process.
    ProcessSignal,
    /// Observe or reap a child process.
    ProcessWait,
    /// Resolve a socket address.
    SocketAddressResolution,
    /// Bind a native socket.
    SocketBind,
    /// Connect a native socket.
    SocketConnect,
}

impl PlatformOperation {
    /// Returns the stable mechanism name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ThreadSpawn => "thread_spawn",
            Self::ThreadJoin => "thread_join",
            Self::ThreadRuntimeInitialization => "thread_runtime_initialization",
            Self::Event => "event",
            Self::EventPoll => "event_poll",
            Self::VirtualMemory => "virtual_memory",
            Self::ProcessSpawn => "process_spawn",
            Self::ProcessSignal => "process_signal",
            Self::ProcessWait => "process_wait",
            Self::SocketAddressResolution => "socket_address_resolution",
            Self::SocketBind => "socket_bind",
            Self::SocketConnect => "socket_connect",
        }
    }
}

/// Stable failure category independent of host error text.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PlatformErrorKind {
    /// The host operation returned an I/O error.
    Io(io::ErrorKind),
    /// A requested size was zero or not representable.
    InvalidSize,
    /// Process-local runtime thread identities were exhausted.
    ThreadIdentityExhausted,
    /// A native event's monotonic wake generation was exhausted.
    EventGenerationExhausted,
    /// Runtime state was already installed on the current thread.
    RuntimeThreadAlreadyInitialized,
    /// Shared synchronization state was poisoned by a panic.
    SynchronizationPoisoned,
    /// The requested mechanism is unavailable on this host.
    Unsupported,
}

/// Typed native-mechanism failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlatformError {
    operation: PlatformOperation,
    kind: PlatformErrorKind,
}

impl PlatformError {
    /// Creates a failure for one native operation.
    pub const fn new(operation: PlatformOperation, kind: PlatformErrorKind) -> Self {
        Self { operation, kind }
    }

    /// Converts a host I/O error without retaining host-authored text.
    pub fn from_io(operation: PlatformOperation, error: &io::Error) -> Self {
        Self::new(operation, PlatformErrorKind::Io(error.kind()))
    }

    /// Returns the failed native operation.
    pub const fn operation(self) -> PlatformOperation {
        self.operation
    }

    /// Returns the stable failure category.
    pub const fn kind(self) -> PlatformErrorKind {
        self.kind
    }
}

impl fmt::Display for PlatformError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {:?}", self.operation.as_str(), self.kind)
    }
}

impl std::error::Error for PlatformError {}
