//! Safe native mechanisms used by compiler-host and trusted runtime layers.

#![forbid(unsafe_code)]

mod clock;
mod contract;
mod entropy;
mod error;
mod event;
mod memory;
mod network;
mod output;
mod process;
mod thread;

pub use clock::{
    MonotonicClock, MonotonicDeadline, MonotonicInstant, MonotonicReading, WallClock,
    WallClockTimestamp,
};
pub use contract::{HostPlatform, PlatformCapability, PlatformContract};
pub use entropy::{SystemEntropy, SystemEntropyError};
pub use error::{PlatformError, PlatformErrorKind, PlatformOperation};
pub use event::{
    NativeEvent, NativeEventPoller, NativeEventRegistration, NativeEventWakeHandle,
    NativePollEvent, NativePollInterest, NativePollReady, NativeWaitOutcome, WakeObservation,
};
pub use memory::{NativeMemoryKind, NativeMemoryRegion, NativeReadOnlyMemory};
pub use network::{
    NativeTcpListener, NativeTcpStream, NativeUdpSocket, bind_tcp_listener, bind_udp_socket,
    connect_tcp, resolve_socket_addresses,
};
pub use output::{
    CapturedRunStream, RunOutputContext, RunOutputStream, current_run_output_context,
    flush_current_run_output, with_optional_run_output_context, with_run_output_context,
    write_current_run_output,
};
pub use process::{
    NativeChildProcess, NativeExitStatus, NativePipeReader, NativePipeWriter, NativeProcessCommand,
    NativeProcessOutput, NativeStdio,
};
pub use thread::{
    NativeThread, NativeThreadOutcome, RuntimeThread, RuntimeThreadEntry, RuntimeThreadId,
    RuntimeThreadScope, current_runtime_thread,
};
