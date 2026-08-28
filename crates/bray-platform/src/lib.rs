//! Safe native mechanisms used by compiler-host and trusted runtime layers.

#![forbid(unsafe_code)]

#[cfg(feature = "clock")]
mod clock;
#[cfg(feature = "contract")]
mod contract;
#[cfg(feature = "entropy")]
mod entropy;
mod error;
#[cfg(feature = "event")]
mod event;
#[cfg(feature = "memory")]
mod memory;
#[cfg(feature = "network")]
mod network;
#[cfg(feature = "test-output")]
mod output;
#[cfg(feature = "process")]
mod process;
#[cfg(feature = "thread")]
mod thread;

#[cfg(feature = "clock")]
pub use clock::{
    MonotonicClock, MonotonicDeadline, MonotonicInstant, WallClock, WallClockTimestamp,
};
#[cfg(feature = "contract")]
pub use contract::{HostPlatform, PlatformCapability, PlatformContract};
#[cfg(feature = "entropy")]
pub use entropy::{SystemEntropy, SystemEntropyError};
pub use error::{PlatformError, PlatformErrorKind, PlatformOperation};
#[cfg(feature = "event")]
pub use event::{
    NativeEvent, NativeEventPoller, NativeEventRegistration, NativeEventWakeHandle,
    NativePollEvent, NativePollInterest, NativePollReady, NativeWaitOutcome, WakeObservation,
};
#[cfg(feature = "memory")]
pub use memory::{NativeMemoryKind, NativeMemoryRegion, NativeReadOnlyMemory};
#[cfg(feature = "network")]
pub use network::{
    NativeTcpListener, NativeTcpStream, NativeUdpSocket, bind_tcp_listener, bind_udp_socket,
    connect_tcp, resolve_socket_addresses,
};
#[cfg(feature = "test-output")]
pub use output::{
    CapturedRunStream, RunOutputContext, RunOutputOperation, RunOutputOperationError,
    RunOutputStream, begin_current_run_output_operation, current_run_output_context,
    with_optional_run_output_context, with_run_output_context,
};
#[cfg(feature = "process")]
pub use process::{
    NativeChildProcess, NativeExitStatus, NativePipeReader, NativePipeWriter, NativeProcessCommand,
    NativeProcessOutput, NativeStdio,
};
#[cfg(feature = "thread")]
pub use thread::{
    NativeThread, NativeThreadOutcome, RuntimeThread, RuntimeThreadEntry, RuntimeThreadId,
    RuntimeThreadScope, current_runtime_thread, main_runtime_thread,
    mark_current_runtime_thread_as_main, register_runtime_thread_exit_callback,
};
