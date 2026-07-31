use std::path::{Path, PathBuf};

use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticIoErrorKind, DiagnosticKind,
    DiagnosticNote, DiagnosticNoteKind, SeverityKind,
};
use bray_formatter::{
    FormatBytesError, FormatBytesErrorKind, FormatFileError, FormatFileErrorKind,
    FormatFileOutcome, FormatMode, format_bytes, format_file,
};

pub(crate) fn format_files(paths: &[PathBuf], mode: FormatMode) -> DiagnosticBag {
    let mut diagnostics = DiagnosticBag::with_capacity(paths.len());

    for path in paths {
        match format_file(path, mode) {
            Ok(FormatFileOutcome::WouldChange) => diagnostics.add(path_diagnostic(
                DiagnosticId::from_index(diagnostics.len()),
                DiagnosticKind::FormatterSourceNotFormatted,
                path,
            )),
            Ok(FormatFileOutcome::Unchanged | FormatFileOutcome::Written) => {}
            Err(error) => diagnostics.add(file_error_diagnostic(
                DiagnosticId::from_index(diagnostics.len()),
                error,
            )),
        }
    }

    diagnostics
}

pub(crate) fn format_standard_input(
    bytes: &[u8],
    mode: FormatMode,
) -> Result<String, DiagnosticBag> {
    let formatted = format_bytes(bytes).map_err(|error| {
        DiagnosticBag::single(bytes_error_diagnostic(DiagnosticId::new(0), error))
    })?;

    if mode == FormatMode::Check {
        return if formatted.changed() {
            Err(DiagnosticBag::single(path_diagnostic(
                DiagnosticId::new(0),
                DiagnosticKind::FormatterSourceNotFormatted,
                Path::new("-"),
            )))
        } else {
            Ok(String::new())
        };
    }

    Ok(formatted.into_text())
}

fn file_error_diagnostic(id: DiagnosticId, error: FormatFileError) -> Diagnostic {
    match error.kind() {
        FormatFileErrorKind::Read => Diagnostic::new(
            id,
            DiagnosticKind::SourceFileReadFailed,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::file_path(error.path()))
        .with_arg(DiagnosticArg::io_error_kind(file_io_kind(&error)))
        .with_note(DiagnosticNote::new(
            DiagnosticNoteKind::SourceFileMustBeReadable,
        )),
        FormatFileErrorKind::InvalidUtf8 => {
            path_diagnostic(id, DiagnosticKind::FormatterSourceInvalidUtf8, error.path())
                .with_note(DiagnosticNote::new(DiagnosticNoteKind::SourceMustBeUtf8))
        }
        FormatFileErrorKind::SourceTooLarge => {
            size_diagnostic(id, error.path(), error.byte_count())
        }
        FormatFileErrorKind::Write => {
            path_diagnostic(id, DiagnosticKind::FormatterSourceWriteFailed, error.path())
                .with_arg(DiagnosticArg::io_error_kind(file_io_kind(&error)))
        }
    }
}

fn file_io_kind(error: &FormatFileError) -> DiagnosticIoErrorKind {
    error
        .io_error_kind()
        .map(DiagnosticIoErrorKind::from)
        .unwrap_or(DiagnosticIoErrorKind::Other)
}

fn bytes_error_diagnostic(id: DiagnosticId, error: FormatBytesError) -> Diagnostic {
    let path = Path::new("-");

    match error.kind() {
        FormatBytesErrorKind::InvalidUtf8 => {
            path_diagnostic(id, DiagnosticKind::FormatterSourceInvalidUtf8, path)
                .with_note(DiagnosticNote::new(DiagnosticNoteKind::SourceMustBeUtf8))
        }
        FormatBytesErrorKind::SourceTooLarge => size_diagnostic(id, path, Some(error.byte_count())),
    }
}

fn size_diagnostic(id: DiagnosticId, path: &Path, byte_count: Option<usize>) -> Diagnostic {
    let mut diagnostic = path_diagnostic(id, DiagnosticKind::FormatterSourceTooLarge, path)
        .with_note(DiagnosticNote::new(
            DiagnosticNoteKind::SourceTextOffsetsAreCompact,
        ));

    if let Some(byte_count) = byte_count.and_then(DiagnosticArg::byte_count) {
        diagnostic = diagnostic.with_arg(byte_count);
    }

    diagnostic
}

fn path_diagnostic(id: DiagnosticId, kind: DiagnosticKind, path: &Path) -> Diagnostic {
    Diagnostic::new(id, kind, SeverityKind::Error).with_arg(DiagnosticArg::file_path(path))
}
