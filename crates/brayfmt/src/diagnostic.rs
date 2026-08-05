use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticIoErrorKind, DiagnosticKind,
    DiagnosticNote, DiagnosticNoteKind, SeverityKind,
};
use bray_formatter::{
    FormatBytesError, FormatBytesErrorKind, FormatFileError, FormatFileErrorKind,
    FormatFileOutcome, FormatMode, FormatterConfiguration, format_bytes,
    format_files as format_source_files,
};

use crate::configuration::ConfigurationError;

pub(crate) fn format_files(
    paths: &[PathBuf],
    mode: FormatMode,
    configuration: &FormatterConfiguration,
    worker_count: NonZeroUsize,
) -> DiagnosticBag {
    let mut diagnostics = DiagnosticBag::with_capacity(paths.len());

    for (path, result) in paths.iter().zip(format_source_files(
        paths,
        mode,
        configuration,
        worker_count,
    )) {
        match result {
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
    configuration: &FormatterConfiguration,
) -> Result<String, DiagnosticBag> {
    let formatted = format_bytes(bytes, configuration).map_err(|error| {
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

pub(crate) fn configuration_error_diagnostic(error: ConfigurationError) -> Diagnostic {
    let id = DiagnosticId::new(0);

    match error {
        ConfigurationError::Read {
            path,
            io_error_kind,
        } => Diagnostic::new(
            id,
            DiagnosticKind::FormatterConfigurationReadFailed,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::file_path(path))
        .with_arg(DiagnosticArg::io_error_kind(DiagnosticIoErrorKind::from(
            io_error_kind,
        ))),
        ConfigurationError::Malformed { path } => {
            path_diagnostic(id, DiagnosticKind::FormatterConfigurationMalformed, &path)
        }
        ConfigurationError::UnknownRule { path, rule_name } => {
            path_diagnostic(id, DiagnosticKind::FormatterConfigurationUnknownRule, &path)
                .with_arg(DiagnosticArg::referenced_name(rule_name))
        }
        ConfigurationError::InvalidMaximumLineWidth {
            path,
            maximum_line_width,
        } => path_diagnostic(
            id,
            DiagnosticKind::FormatterConfigurationInvalidMaximumWidth,
            &path,
        )
        .with_arg(DiagnosticArg::actual_count(maximum_line_width)),
    }
}

fn file_error_diagnostic(id: DiagnosticId, error: FormatFileError) -> Diagnostic {
    let kind = file_error_diagnostic_kind(error.kind());

    match error.kind() {
        FormatFileErrorKind::Read => Diagnostic::new(id, kind, SeverityKind::Error)
            .with_arg(DiagnosticArg::file_path(error.path()))
            .with_arg(DiagnosticArg::io_error_kind(file_io_kind(&error)))
            .with_note(DiagnosticNote::new(
                DiagnosticNoteKind::SourceFileMustBeReadable,
            )),
        FormatFileErrorKind::InvalidUtf8 => path_diagnostic(id, kind, error.path())
            .with_note(DiagnosticNote::new(DiagnosticNoteKind::SourceMustBeUtf8)),
        FormatFileErrorKind::SourceTooLarge => {
            size_diagnostic(id, kind, error.path(), error.byte_count())
        }
        FormatFileErrorKind::Write => path_diagnostic(id, kind, error.path())
            .with_arg(DiagnosticArg::io_error_kind(file_io_kind(&error))),
    }
}

const fn file_error_diagnostic_kind(kind: FormatFileErrorKind) -> DiagnosticKind {
    match kind {
        FormatFileErrorKind::Read => DiagnosticKind::SourceFileReadFailed,
        FormatFileErrorKind::InvalidUtf8 => DiagnosticKind::FormatterSourceInvalidUtf8,
        FormatFileErrorKind::SourceTooLarge => DiagnosticKind::FormatterSourceTooLarge,
        FormatFileErrorKind::Write => DiagnosticKind::FormatterSourceWriteFailed,
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
    let kind = bytes_error_diagnostic_kind(error.kind());

    match error.kind() {
        FormatBytesErrorKind::InvalidUtf8 => path_diagnostic(id, kind, path)
            .with_note(DiagnosticNote::new(DiagnosticNoteKind::SourceMustBeUtf8)),
        FormatBytesErrorKind::SourceTooLarge => {
            size_diagnostic(id, kind, path, Some(error.byte_count()))
        }
    }
}

const fn bytes_error_diagnostic_kind(kind: FormatBytesErrorKind) -> DiagnosticKind {
    match kind {
        FormatBytesErrorKind::InvalidUtf8 => DiagnosticKind::FormatterSourceInvalidUtf8,
        FormatBytesErrorKind::SourceTooLarge => DiagnosticKind::FormatterSourceTooLarge,
    }
}

fn size_diagnostic(
    id: DiagnosticId,
    kind: DiagnosticKind,
    path: &Path,
    byte_count: Option<usize>,
) -> Diagnostic {
    let mut diagnostic = path_diagnostic(id, kind, path).with_note(DiagnosticNote::new(
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

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_formatter::{FormatBytesErrorKind, FormatFileErrorKind};

    use super::{bytes_error_diagnostic_kind, file_error_diagnostic_kind};

    #[test]
    fn formatter_failures_preserve_their_diagnostic_categories() {
        assert_eq!(
            bytes_error_diagnostic_kind(FormatBytesErrorKind::InvalidUtf8),
            DiagnosticKind::FormatterSourceInvalidUtf8
        );

        assert_eq!(
            bytes_error_diagnostic_kind(FormatBytesErrorKind::SourceTooLarge),
            DiagnosticKind::FormatterSourceTooLarge
        );

        assert_eq!(
            file_error_diagnostic_kind(FormatFileErrorKind::Write),
            DiagnosticKind::FormatterSourceWriteFailed
        );
    }
}
