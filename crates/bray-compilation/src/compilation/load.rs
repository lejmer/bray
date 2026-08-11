use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticNote,
    DiagnosticNoteKind, DiagnosticProjectCommandFailure, DiagnosticSourceInput,
    DiagnosticSourceInputOrigin, SeverityKind,
};
use bray_source::{
    SourceInput, SourceInputKind, SourceLoadError, SourceUtf8Error, TextSizeOverflow,
};
use bray_symbols::PackageIdentity;

use crate::PackageSourceAuthority;

/// Error returned when loading durable compilation state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CompilationLoadError {
    /// The compiler cannot assign another compact diagnostic ID.
    TooManyDiagnostics {
        /// Number of diagnostics already assigned.
        count: usize,
    },
}

impl std::fmt::Display for CompilationLoadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooManyDiagnostics { count } => {
                write!(
                    formatter,
                    "cannot assign a diagnostic after {count} diagnostics"
                )
            }
        }
    }
}

impl std::error::Error for CompilationLoadError {}

impl CompilationLoadError {
    /// Converts this terminal load failure into a locale-neutral diagnostic.
    pub fn diagnostic(self) -> Diagnostic {
        let failure = match self {
            Self::TooManyDiagnostics { count } => match u64::try_from(count) {
                Ok(count) => {
                    DiagnosticProjectCommandFailure::CompilationDiagnosticCapacityExceeded { count }
                }
                Err(_) => {
                    DiagnosticProjectCommandFailure::CompilationDiagnosticCountUnrepresentable
                }
            },
        };

        failure.diagnostic(DiagnosticId::new(0))
    }
}

pub(super) fn next_diagnostic_id(
    diagnostics: &DiagnosticBag,
) -> Result<DiagnosticId, CompilationLoadError> {
    let raw = match u32::try_from(diagnostics.len()) {
        Ok(raw) => raw,
        Err(_) => {
            return Err(CompilationLoadError::TooManyDiagnostics {
                count: diagnostics.len(),
            });
        }
    };

    Ok(DiagnosticId::new(raw))
}

pub(super) fn missing_source_input_diagnostic(id: DiagnosticId) -> Diagnostic {
    Diagnostic::new(
        id,
        DiagnosticKind::RequestMissingSourceInput,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::source_count(0))
    .with_note(DiagnosticNote::new(DiagnosticNoteKind::SourceInputRequired))
}

pub(super) fn duplicate_source_input_diagnostic(
    id: DiagnosticId,
    context: SourceInputDiagnosticContext,
) -> Diagnostic {
    with_source_input_context_args(
        Diagnostic::new(
            id,
            DiagnosticKind::RequestDuplicateSourceInput,
            SeverityKind::Error,
        ),
        context,
    )
}

pub(super) fn package_source_authority_diagnostic(
    id: DiagnosticId,
    package: &PackageIdentity,
    authority: PackageSourceAuthority,
) -> Diagnostic {
    let kind = match authority {
        PackageSourceAuthority::Ordinary => DiagnosticKind::RequestReservedPackageIdentity,
        PackageSourceAuthority::StandardLibrary => {
            DiagnosticKind::RequestStandardLibraryPackageIdentityRequired
        }
    };

    Diagnostic::new(id, kind, SeverityKind::Error)
        .with_arg(DiagnosticArg::referenced_name(package.as_str()))
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct SourceInputDiagnosticContext {
    input_index: u64,
    kind: SourceInputKind,
    origin: DiagnosticSourceInputOrigin,
}

impl SourceInputDiagnosticContext {
    pub(super) fn from_input(input_index: u64, input: &SourceInput) -> Self {
        let origin = match input.kind() {
            SourceInputKind::File => match input.file_path() {
                Some(path) => DiagnosticSourceInputOrigin::File(path.to_path_buf()),
                None => DiagnosticSourceInputOrigin::Missing,
            },
            SourceInputKind::VirtualText => match input.virtual_name() {
                Some(name) => DiagnosticSourceInputOrigin::Name(name.to_owned()),
                None => DiagnosticSourceInputOrigin::Missing,
            },
            SourceInputKind::GeneratedText => match input.generated_name() {
                Some(name) => DiagnosticSourceInputOrigin::Name(name.to_owned()),
                None => DiagnosticSourceInputOrigin::Missing,
            },
            SourceInputKind::LspOpenDocument => match input.lsp_uri() {
                Some(uri) => DiagnosticSourceInputOrigin::Uri(uri.to_owned()),
                None => DiagnosticSourceInputOrigin::Missing,
            },
        };

        Self {
            input_index,
            kind: input.kind(),
            origin,
        }
    }

    fn into_diagnostic_input(self) -> DiagnosticSourceInput {
        DiagnosticSourceInput::new(self.input_index, self.kind, self.origin)
    }
}

pub(super) fn source_load_diagnostic(
    id: DiagnosticId,
    context: SourceInputDiagnosticContext,
    error: SourceLoadError,
) -> Diagnostic {
    let diagnostic = match error {
        SourceLoadError::TooManySources { count } => too_many_sources_diagnostic(id, count),
        SourceLoadError::InvalidUtf8(error) => invalid_utf8_diagnostic(id, error),
        SourceLoadError::TextTooLarge(error) => text_too_large_diagnostic(id, error),
    };

    with_source_input_context_args(diagnostic, context)
}

fn with_source_input_context_args(
    diagnostic: Diagnostic,
    context: SourceInputDiagnosticContext,
) -> Diagnostic {
    diagnostic.with_arg(DiagnosticArg::source_input(context.into_diagnostic_input()))
}

fn too_many_sources_diagnostic(id: DiagnosticId, count: u64) -> Diagnostic {
    Diagnostic::new(id, DiagnosticKind::SourceTooManyInputs, SeverityKind::Error)
        .with_arg(DiagnosticArg::source_count(count))
        .with_note(DiagnosticNote::new(DiagnosticNoteKind::SourceIdsAreCompact))
}

fn text_too_large_diagnostic(id: DiagnosticId, error: TextSizeOverflow) -> Diagnostic {
    let mut diagnostic =
        Diagnostic::new(id, DiagnosticKind::SourceTextTooLarge, SeverityKind::Error).with_note(
            DiagnosticNote::new(DiagnosticNoteKind::SourceTextOffsetsAreCompact),
        );

    if let Some(byte_count) = DiagnosticArg::byte_count(error.bytes()) {
        diagnostic = diagnostic.with_arg(byte_count);
    }

    diagnostic
}

fn invalid_utf8_diagnostic(id: DiagnosticId, error: SourceUtf8Error) -> Diagnostic {
    let mut diagnostic =
        Diagnostic::new(id, DiagnosticKind::SourceInvalidUtf8, SeverityKind::Error)
            .with_arg(DiagnosticArg::text_offset(error.valid_up_to()))
            .with_note(DiagnosticNote::new(DiagnosticNoteKind::SourceMustBeUtf8));

    if let Some(byte_count) = invalid_utf8_byte_count(error) {
        diagnostic = diagnostic.with_arg(byte_count);
    }

    diagnostic
}

fn invalid_utf8_byte_count(error: SourceUtf8Error) -> Option<DiagnosticArg> {
    let len = error.invalid_sequence_len()?;

    DiagnosticArg::byte_count(len)
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticId, DiagnosticKind,
        DiagnosticNote, DiagnosticNoteKind, DiagnosticSourceInput, DiagnosticSourceInputOrigin,
    };
    use bray_source::{
        SourceInputKind, SourceLoadError, SourceUtf8Error, TextSize, TextSizeOverflow,
    };

    use super::{SourceInputDiagnosticContext, source_load_diagnostic};

    #[test]
    fn source_load_errors_convert_to_source_diagnostics() {
        let too_many = source_load_diagnostic(
            DiagnosticId::new(7),
            SourceInputDiagnosticContext {
                input_index: 9,
                kind: SourceInputKind::VirtualText,
                origin: DiagnosticSourceInputOrigin::Name(String::from("buffer")),
            },
            SourceLoadError::TooManySources { count: 10 },
        );

        assert_eq!(too_many.kind(), DiagnosticKind::SourceTooManyInputs);

        assert_eq!(
            too_many.args(),
            &[
                DiagnosticArg::new(
                    DiagnosticArgName::SourceCount,
                    DiagnosticArgValue::SourceCount(10)
                ),
                DiagnosticArg::new(
                    DiagnosticArgName::SourceInput,
                    DiagnosticArgValue::SourceInput(DiagnosticSourceInput::new(
                        9,
                        SourceInputKind::VirtualText,
                        DiagnosticSourceInputOrigin::Name(String::from("buffer"))
                    ))
                )
            ]
        );

        assert_eq!(
            too_many.notes(),
            &[DiagnosticNote::new(DiagnosticNoteKind::SourceIdsAreCompact)]
        );

        let too_large = source_load_diagnostic(
            DiagnosticId::new(8),
            SourceInputDiagnosticContext {
                input_index: 3,
                kind: SourceInputKind::LspOpenDocument,
                origin: DiagnosticSourceInputOrigin::Uri(String::from("file:///main.bray")),
            },
            SourceLoadError::TextTooLarge(TextSizeOverflow::new(usize::MAX)),
        );

        assert_eq!(too_large.kind(), DiagnosticKind::SourceTextTooLarge);

        assert_eq!(
            too_large.notes(),
            &[DiagnosticNote::new(
                DiagnosticNoteKind::SourceTextOffsetsAreCompact
            )]
        );

        assert!(too_large.args().contains(&DiagnosticArg::source_input(
            DiagnosticSourceInput::new(
                3,
                SourceInputKind::LspOpenDocument,
                DiagnosticSourceInputOrigin::Uri(String::from("file:///main.bray"))
            )
        )));

        if let Some(byte_count) = DiagnosticArg::byte_count(usize::MAX) {
            assert!(too_large.args().contains(&byte_count));
        }

        let invalid_utf8 = source_load_diagnostic(
            DiagnosticId::new(9),
            SourceInputDiagnosticContext {
                input_index: 4,
                kind: SourceInputKind::File,
                origin: DiagnosticSourceInputOrigin::File("bad.bray".into()),
            },
            SourceLoadError::InvalidUtf8(SourceUtf8Error::new(TextSize::new(2), Some(1))),
        );

        assert_eq!(invalid_utf8.kind(), DiagnosticKind::SourceInvalidUtf8);

        assert!(invalid_utf8.args().contains(&DiagnosticArg::new(
            DiagnosticArgName::TextOffset,
            DiagnosticArgValue::TextOffset(TextSize::new(2))
        )));

        for diagnostic in [&too_many, &too_large, &invalid_utf8] {
            bray_testing::assert_goal_state_diagnostic(diagnostic);
        }

        bray_testing::assert_goal_state_diagnostic_kind(
            &bray_diagnostics::DiagnosticBag::single(too_many),
            DiagnosticKind::SourceTooManyInputs,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &bray_diagnostics::DiagnosticBag::single(too_large),
            DiagnosticKind::SourceTextTooLarge,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            &bray_diagnostics::DiagnosticBag::single(invalid_utf8),
            DiagnosticKind::SourceInvalidUtf8,
        );
    }
}
