use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates an external-tool operation argument.
    pub const fn external_tool_operation(kind: DiagnosticExternalToolOperation) -> Self {
        Self::new(
            DiagnosticArgName::ExternalToolOperation,
            DiagnosticArgValue::ExternalToolOperation(kind),
        )
    }
}

/// Locale-neutral external-tool operation categories.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticExternalToolOperation {
    ResponseFileWrite,
    ResponseFileRemove,
    ProcessSpawn,
    ProcessSignal,
    ProcessWait,
    ThreadSpawn,
    ThreadJoin,
    ThreadRuntimeInitialization,
    Event,
    EventPoll,
    EventRegistration,
    VirtualMemory,
    SocketAddressResolution,
    SocketAddress,
    SocketBind,
    SocketConfiguration,
    SocketConnect,
    SocketAccept,
    SocketReceive,
    SocketSend,
    StandardOutputPipe,
    StandardErrorPipe,
    StandardOutputCapture,
    StandardErrorCapture,
    StandardOutputReader,
    StandardErrorReader,
}

impl DiagnosticExternalToolOperation {
    /// Returns the stable machine key for this operation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ResponseFileWrite => "response_file_write",
            Self::ResponseFileRemove => "response_file_remove",
            Self::ProcessSpawn => "process_spawn",
            Self::ProcessSignal => "process_signal",
            Self::ProcessWait => "process_wait",
            Self::ThreadSpawn => "thread_spawn",
            Self::ThreadJoin => "thread_join",
            Self::ThreadRuntimeInitialization => "thread_runtime_initialization",
            Self::Event => "event",
            Self::EventPoll => "event_poll",
            Self::EventRegistration => "event_registration",
            Self::VirtualMemory => "virtual_memory",
            Self::SocketAddressResolution => "socket_address_resolution",
            Self::SocketAddress => "socket_address",
            Self::SocketBind => "socket_bind",
            Self::SocketConfiguration => "socket_configuration",
            Self::SocketConnect => "socket_connect",
            Self::SocketAccept => "socket_accept",
            Self::SocketReceive => "socket_receive",
            Self::SocketSend => "socket_send",
            Self::StandardOutputPipe => "standard_output_pipe",
            Self::StandardErrorPipe => "standard_error_pipe",
            Self::StandardOutputCapture => "standard_output_capture",
            Self::StandardErrorCapture => "standard_error_capture",
            Self::StandardOutputReader => "standard_output_reader",
            Self::StandardErrorReader => "standard_error_reader",
        }
    }
}
