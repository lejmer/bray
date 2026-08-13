use bray_diagnostics::{DiagnosticExternalToolFailureKind, DiagnosticExternalToolOperation};

pub(crate) const fn format_english_external_tool_operation(
    operation: DiagnosticExternalToolOperation,
) -> &'static str {
    match operation {
        DiagnosticExternalToolOperation::ResponseFileWrite => "write the response file",
        DiagnosticExternalToolOperation::ResponseFileRemove => "remove the response file",
        DiagnosticExternalToolOperation::ProcessSpawn => "start the linker process",
        DiagnosticExternalToolOperation::ProcessSignal => "signal the linker process",
        DiagnosticExternalToolOperation::ProcessWait => "wait for the linker process",
        DiagnosticExternalToolOperation::ThreadSpawn => "start a host thread",
        DiagnosticExternalToolOperation::ThreadJoin => "join a host thread",
        DiagnosticExternalToolOperation::ThreadRuntimeInitialization => {
            "initialize host thread runtime state"
        }
        DiagnosticExternalToolOperation::Event => "wait for or wake a host event",
        DiagnosticExternalToolOperation::EventPoll => "poll host events",
        DiagnosticExternalToolOperation::EventRegistration => "register a host event",
        DiagnosticExternalToolOperation::VirtualMemory => "allocate host virtual memory",
        DiagnosticExternalToolOperation::SocketAddressResolution => "resolve a socket address",
        DiagnosticExternalToolOperation::SocketAddress => "resolve a socket address",
        DiagnosticExternalToolOperation::SocketBind => "bind a socket",
        DiagnosticExternalToolOperation::SocketConfiguration => "configure a socket",
        DiagnosticExternalToolOperation::SocketConnect => "connect a socket",
        DiagnosticExternalToolOperation::SocketAccept => "accept a socket connection",
        DiagnosticExternalToolOperation::SocketReceive => "receive socket data",
        DiagnosticExternalToolOperation::SocketSend => "send socket data",
        DiagnosticExternalToolOperation::StandardOutputPipe => "open the standard-output pipe",
        DiagnosticExternalToolOperation::StandardErrorPipe => "open the standard-error pipe",
        DiagnosticExternalToolOperation::StandardOutputCapture => "read standard output",
        DiagnosticExternalToolOperation::StandardErrorCapture => "read standard error",
        DiagnosticExternalToolOperation::StandardOutputReader => "join the standard-output reader",
        DiagnosticExternalToolOperation::StandardErrorReader => "join the standard-error reader",
    }
}

pub(crate) const fn format_english_external_tool_failure(
    kind: DiagnosticExternalToolFailureKind,
) -> &'static str {
    match kind {
        DiagnosticExternalToolFailureKind::Io => "host I/O failure",
        DiagnosticExternalToolFailureKind::InvalidSize => "invalid host size",
        DiagnosticExternalToolFailureKind::ThreadIdentityExhausted => {
            "host thread identities are exhausted"
        }
        DiagnosticExternalToolFailureKind::EventGenerationExhausted => {
            "host event generations are exhausted"
        }
        DiagnosticExternalToolFailureKind::InvalidEventIdentity => "invalid host event identity",
        DiagnosticExternalToolFailureKind::RuntimeThreadAlreadyInitialized => {
            "host runtime thread is already initialized"
        }
        DiagnosticExternalToolFailureKind::SynchronizationPoisoned => {
            "host synchronization state is unavailable"
        }
        DiagnosticExternalToolFailureKind::Unsupported => "operation is unsupported on this host",
        DiagnosticExternalToolFailureKind::MissingOutputPipe => "required output pipe is missing",
        DiagnosticExternalToolFailureKind::OutputCapture => "output capture failed",
        DiagnosticExternalToolFailureKind::OutputReaderTerminated => {
            "output reader terminated unexpectedly"
        }
    }
}

pub(crate) fn format_english_external_tool_exit(
    exit: &bray_diagnostics::DiagnosticExternalToolExit,
) -> String {
    let mut rendered = exit.code().map_or_else(
        || "without a portable exit code".to_owned(),
        |code| format!("with exit code {code}"),
    );

    append_external_tool_stream(&mut rendered, "standard error", exit.standard_error());
    append_external_tool_stream(&mut rendered, "standard output", exit.standard_output());

    rendered
}

fn append_external_tool_stream(
    rendered: &mut String,
    name: &str,
    capture: &bray_diagnostics::DiagnosticExternalToolStreamCapture,
) {
    if capture.original_byte_count() == 0 {
        return;
    }

    rendered.push_str(", ");
    rendered.push_str(name);
    rendered.push_str(" (");
    rendered.push_str(&capture.captured_byte_count().to_string());
    rendered.push_str(" of ");
    rendered.push_str(&capture.original_byte_count().to_string());
    rendered.push_str(" bytes captured");

    if capture.omitted_byte_count() > 0 {
        rendered.push_str(", ");
        rendered.push_str(&capture.omitted_byte_count().to_string());
        rendered.push_str(" omitted");
    }

    if capture.is_lossy_utf8() {
        rendered.push_str(", invalid UTF-8 replaced");
    }

    rendered.push_str("):\n");
    rendered.push_str(capture.text());
}
