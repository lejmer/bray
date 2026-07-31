use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticIoErrorKind, DiagnosticKind,
    DiagnosticNote, DiagnosticNoteKind, SeverityKind,
};
use bray_formatter::{
    FormatBytesError, FormatBytesErrorKind, FormatFileError, FormatFileErrorKind,
    FormatFileOutcome, FormatMode, format_bytes, format_file,
};

use crate::{
    TackFormatInput, TackFormatMode, TackFormatRequest,
    TackFormatService, TackServiceResult,
};

const STANDARD_INPUT_PATH: &str = "-";

pub(crate) struct BrayFormatService;

impl TackFormatService for BrayFormatService {
    fn format(&self, request: TackFormatRequest) -> TackServiceResult {
        match request.input() {
            TackFormatInput::Files(paths) => format_files(paths, request.mode()),
            TackFormatInput::StandardInput(bytes) => format_standard_input(bytes, request.mode()),
        }
    }
}

fn format_files(paths: &[PathBuf], mode: TackFormatMode) -> TackServiceResult {
    let mut diagnostics = DiagnosticBag::with_capacity(paths.len());
    let format_mode = file_mode(mode);

    for path in paths {
        match format_file(path, format_mode) {
            Ok(FormatFileOutcome::WouldChange) => {
                diagnostics.add(path_diagnostic(
                    DiagnosticId::from_index(diagnostics.len()),
                    DiagnosticKind::FormatterSourceNotFormatted,
                    path,
                ));
            }
            Ok(FormatFileOutcome::Unchanged | FormatFileOutcome::Written) => {}
            Err(error) => {
                diagnostics.add(file_error_diagnostic(
                    DiagnosticId::from_index(diagnostics.len()),
                    error,
                ));
            }
        }
    }

    service_result(diagnostics, String::new())
}

fn format_standard_input(bytes: &[u8], mode: TackFormatMode) -> TackServiceResult {
    let formatted = match format_bytes(bytes) {
        Ok(formatted) => formatted,
        Err(error) => {
            return service_result(
                DiagnosticBag::single(bytes_error_diagnostic(DiagnosticId::new(0), error)),
                String::new(),
            );
        }
    };

    if mode == TackFormatMode::Check {
        let diagnostics = if formatted.changed() {
            DiagnosticBag::single(path_diagnostic(
                DiagnosticId::new(0),
                DiagnosticKind::FormatterSourceNotFormatted,
                Path::new(STANDARD_INPUT_PATH),
            ))
        } else {
            DiagnosticBag::new()
        };

        return service_result(diagnostics, String::new());
    }

    service_result(DiagnosticBag::new(), formatted.into_text())
}

fn file_mode(mode: TackFormatMode) -> FormatMode {
    match mode {
        TackFormatMode::Check => FormatMode::Check,
        TackFormatMode::Write => FormatMode::Write,
    }
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
    let path = Path::new(STANDARD_INPUT_PATH);

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

fn service_result(diagnostics: DiagnosticBag, stdout: String) -> TackServiceResult {
    let exit_code = if diagnostics.has_errors() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    };

    TackServiceResult::new(exit_code, diagnostics, stdout, String::new())
}
