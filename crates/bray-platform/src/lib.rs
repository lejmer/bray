//! Safe native mechanisms used by trusted runtime and standard-library layers.

#![forbid(unsafe_code)]

mod clock;
mod contract;
mod error;
mod event;
mod memory;
mod network;
mod process;
mod thread;

pub use clock::{MonotonicClock, MonotonicDeadline, MonotonicInstant};
pub use contract::{HostPlatform, PlatformCapability, PlatformContract};
pub use error::{PlatformError, PlatformErrorKind, PlatformOperation};
pub use event::{
    NativeEvent, NativeEventPoller, NativeEventRegistration, NativeEventWakeHandle,
    NativePollEvent, NativeWaitOutcome, WakeObservation,
};
pub use memory::{NativeMemoryKind, NativeMemoryRegion, NativeReadOnlyMemory};
pub use network::{
    bind_tcp_listener, bind_udp_socket, connect_tcp, resolve_socket_addresses,
};
pub use process::{
    NativeChildProcess, NativeExitStatus, NativePipeReader, NativePipeWriter,
    NativeProcessCommand, NativeStdio,
};
pub use thread::{
    NativeThread, NativeThreadOutcome, RuntimeThread, RuntimeThreadId,
    RuntimeThreadScope, current_runtime_thread,
};
