use std::path::PathBuf;

use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticExternalToolOperation, DiagnosticId,
    DiagnosticIoErrorKind, DiagnosticKind, DiagnosticProjectCommandFailure,
    DiagnosticProjectOperation, DiagnosticProjectProcessFailure, DiagnosticProjectSelectionProblem,
    SeverityKind,
};
use bray_platform::{PlatformError, PlatformErrorKind, PlatformOperation};

pub(crate) fn selection_diagnostics(problem: DiagnosticProjectSelectionProblem) -> DiagnosticBag {
    DiagnosticBag::single(
        Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::ProjectCommandSelectionInvalid,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::project_selection_problem(problem)),
    )
}

pub(crate) fn operation_diagnostics(failure: DiagnosticProjectCommandFailure) -> DiagnosticBag {
    DiagnosticBag::single(failure.diagnostic(DiagnosticId::new(0)))
}

pub(crate) fn process_failure(
    operation: DiagnosticProjectOperation,
    program: impl Into<PathBuf>,
    error: PlatformError,
) -> DiagnosticProjectCommandFailure {
    let failure = match error.kind() {
        PlatformErrorKind::Io(error) => {
            DiagnosticProjectProcessFailure::Io(DiagnosticIoErrorKind::from(error))
        }
        PlatformErrorKind::InvalidSize => DiagnosticProjectProcessFailure::InvalidSize,
        PlatformErrorKind::ThreadIdentityExhausted => {
            DiagnosticProjectProcessFailure::ThreadIdentityExhausted
        }
        PlatformErrorKind::EventGenerationExhausted => {
            DiagnosticProjectProcessFailure::EventGenerationExhausted
        }
        PlatformErrorKind::InvalidEventIdentity => {
            DiagnosticProjectProcessFailure::InvalidEventIdentity
        }
        PlatformErrorKind::RuntimeThreadAlreadyInitialized => {
            DiagnosticProjectProcessFailure::RuntimeThreadAlreadyInitialized
        }
        PlatformErrorKind::SynchronizationPoisoned => {
            DiagnosticProjectProcessFailure::SynchronizationPoisoned
        }
        PlatformErrorKind::Unsupported => DiagnosticProjectProcessFailure::Unsupported,
    };

    DiagnosticProjectCommandFailure::Process {
        operation,
        program: program.into(),
        native_operation: platform_operation(error.operation()),
        failure,
    }
}

const fn platform_operation(operation: PlatformOperation) -> DiagnosticExternalToolOperation {
    match operation {
        PlatformOperation::ThreadSpawn => DiagnosticExternalToolOperation::ThreadSpawn,
        PlatformOperation::ThreadJoin => DiagnosticExternalToolOperation::ThreadJoin,
        PlatformOperation::ThreadRuntimeInitialization => {
            DiagnosticExternalToolOperation::ThreadRuntimeInitialization
        }
        PlatformOperation::Event => DiagnosticExternalToolOperation::Event,
        PlatformOperation::EventPoll => DiagnosticExternalToolOperation::EventPoll,
        PlatformOperation::EventRegistration => DiagnosticExternalToolOperation::EventRegistration,
        PlatformOperation::VirtualMemory => DiagnosticExternalToolOperation::VirtualMemory,
        PlatformOperation::ProcessSpawn => DiagnosticExternalToolOperation::ProcessSpawn,
        PlatformOperation::ProcessSignal => DiagnosticExternalToolOperation::ProcessSignal,
        PlatformOperation::ProcessWait => DiagnosticExternalToolOperation::ProcessWait,
        PlatformOperation::SocketAddressResolution => {
            DiagnosticExternalToolOperation::SocketAddressResolution
        }
        PlatformOperation::SocketAddress => DiagnosticExternalToolOperation::SocketAddress,
        PlatformOperation::SocketBind => DiagnosticExternalToolOperation::SocketBind,
        PlatformOperation::SocketConfiguration => {
            DiagnosticExternalToolOperation::SocketConfiguration
        }
        PlatformOperation::SocketConnect => DiagnosticExternalToolOperation::SocketConnect,
        PlatformOperation::SocketAccept => DiagnosticExternalToolOperation::SocketAccept,
        PlatformOperation::SocketReceive => DiagnosticExternalToolOperation::SocketReceive,
        PlatformOperation::SocketSend => DiagnosticExternalToolOperation::SocketSend,
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        DiagnosticKind, DiagnosticProjectCommandFailure, DiagnosticProjectOperation,
        DiagnosticProjectSelectionProblem,
    };

    use super::{operation_diagnostics, selection_diagnostics};

    #[test]
    fn project_command_failures_keep_structured_operation_categories() {
        let selection = selection_diagnostics(
            DiagnosticProjectSelectionProblem::InvalidPackageIdentity("package".to_owned()),
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &selection,
            DiagnosticKind::ProjectCommandSelectionInvalid,
        );

        let defect = operation_diagnostics(DiagnosticProjectCommandFailure::Invariant(
            DiagnosticProjectOperation::GitClone,
        ));

        bray_testing::assert_goal_state_diagnostic_kind(
            &defect,
            DiagnosticKind::ProjectCompilerDefect,
        );
    }
}
