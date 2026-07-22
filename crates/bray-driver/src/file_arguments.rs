use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use bray_compilation::{CompilationOptions, CompilationRequest};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticIoErrorKind, DiagnosticKind,
    DiagnosticNote, DiagnosticNoteKind, SeverityKind,
};
use bray_source::{SourceIdentity, SourceInput, SourceVersion};
use bray_symbols::PackageIdentity;

const FILE_ARGUMENT_SOURCE_VERSION: SourceVersion = SourceVersion::new(0);

/// Error returned when converting file arguments into compiler source inputs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DriverSourceInputError {
    /// A file argument index cannot fit in a stable source identity.
    SourceIdentityOverflow {
        /// Zero-based file argument index.
        input_index: usize,
    },
    /// A file argument could not be read as bytes.
    ReadFile {
        /// Zero-based file argument index.
        input_index: usize,
        /// File path that could not be read.
        path: PathBuf,
        /// Structured I/O error category reported by the host.
        kind: ErrorKind,
    },
}

impl DriverSourceInputError {
    /// Returns the zero-based file argument index associated with this error.
    pub const fn input_index(&self) -> usize {
        match self {
            Self::SourceIdentityOverflow { input_index } => *input_index,
            Self::ReadFile { input_index, .. } => *input_index,
        }
    }

    /// Converts this user-facing source-input error into a diagnostic.
    pub fn into_diagnostic(self, id: DiagnosticId) -> Diagnostic {
        match self {
            Self::SourceIdentityOverflow { input_index } => {
                let mut diagnostic = Diagnostic::new(
                    id,
                    DiagnosticKind::RequestInvalidSourceInput,
                    SeverityKind::Error,
                )
                .with_note(DiagnosticNote::new(
                    DiagnosticNoteKind::SourceInputNeedsStableIdentity,
                ));

                if let Some(input_index) = DiagnosticArg::input_index(input_index) {
                    diagnostic = diagnostic.with_arg(input_index);
                }

                diagnostic
            }
            Self::ReadFile {
                input_index,
                path,
                kind,
            } => {
                let mut diagnostic = Diagnostic::new(
                    id,
                    DiagnosticKind::SourceFileReadFailed,
                    SeverityKind::Error,
                )
                .with_arg(DiagnosticArg::file_path(path))
                .with_arg(DiagnosticArg::io_error_kind(DiagnosticIoErrorKind::from(
                    kind,
                )))
                .with_note(DiagnosticNote::new(
                    DiagnosticNoteKind::SourceFileMustBeReadable,
                ));

                if let Some(input_index) = DiagnosticArg::input_index(input_index) {
                    diagnostic = diagnostic.with_arg(input_index);
                }

                diagnostic
            }
        }
    }

    /// Converts this user-facing source-input error into a diagnostic bag.
    pub fn into_diagnostic_bag(self) -> DiagnosticBag {
        DiagnosticBag::single(self.into_diagnostic(DiagnosticId::new(0)))
    }
}

/// Builds a compilation request for `package_identity` from file arguments read at the driver boundary.
///
/// Source identities are assigned deterministically by file argument order,
/// starting at zero. Each file input receives source version zero.
pub fn compilation_request_from_file_arguments<I, P>(
    package_identity: PackageIdentity,
    file_arguments: I,
    options: CompilationOptions,
) -> Result<CompilationRequest, DiagnosticBag>
where
    I: IntoIterator<Item = P>,
    P: Into<PathBuf>,
{
    let sources = source_inputs_from_file_arguments(file_arguments)
        .map_err(|error| error.into_diagnostic_bag())?;

    Ok(CompilationRequest::with_options(
        package_identity,
        sources,
        options,
    ))
}

/// Reads file arguments as bytes and converts them into source inputs.
///
/// Source identities are assigned deterministically by file argument order,
/// starting at zero. The resulting inputs are validated when compilation loads
/// their source text.
pub fn source_inputs_from_file_arguments<I, P>(
    file_arguments: I,
) -> Result<Vec<SourceInput>, DriverSourceInputError>
where
    I: IntoIterator<Item = P>,
    P: Into<PathBuf>,
{
    let mut sources = Vec::new();

    for (input_index, file_argument) in file_arguments.into_iter().enumerate() {
        let path = file_argument.into();

        sources.push(source_input_from_file_argument(input_index, path)?);
    }

    Ok(sources)
}

fn source_input_from_file_argument(
    input_index: usize,
    path: PathBuf,
) -> Result<SourceInput, DriverSourceInputError> {
    let identity = source_identity_for_input_index(input_index)?;

    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return Err(DriverSourceInputError::ReadFile {
                input_index,
                path,
                kind: error.kind(),
            });
        }
    };

    Ok(SourceInput::file_bytes(
        identity,
        path,
        FILE_ARGUMENT_SOURCE_VERSION,
        bytes,
    ))
}

fn source_identity_for_input_index(
    input_index: usize,
) -> Result<SourceIdentity, DriverSourceInputError> {
    let raw = match u32::try_from(input_index) {
        Ok(raw) => raw,
        Err(_) => return Err(DriverSourceInputError::SourceIdentityOverflow { input_index }),
    };

    Ok(SourceIdentity::new(raw))
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;

    use bray_compilation::{Compilation, CompilationOptions, WorkerBudget};
    use bray_diagnostics::{
        DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticIoErrorKind,
        DiagnosticKind, DiagnosticNote, DiagnosticNoteKind,
    };
    use bray_source::{SourceId, SourceIdentity, SourceVersion};

    use super::{
        DriverSourceInputError, compilation_request_from_file_arguments,
        source_identity_for_input_index, source_inputs_from_file_arguments,
    };
    use crate::test_support::{TemporaryFile, package_identity, unique_temporary_directory};

    #[test]
    fn file_arguments_are_loaded_as_raw_file_source_inputs() {
        let first_file = TemporaryFile::write("first.bray", &[0xef, 0xbb, 0xbf, b'm', b'o', b'd']);
        let second_file = TemporaryFile::write("second.bray", &[0xff, b'x']);

        let inputs = match source_inputs_from_file_arguments([
            first_file.path().to_path_buf(),
            second_file.path().to_path_buf(),
        ]) {
            Ok(inputs) => inputs,
            Err(error) => panic!("file source inputs should load: {error:?}"),
        };

        let [first_input, second_input] = inputs.as_slice() else {
            panic!("test should load exactly two source inputs: {inputs:?}");
        };

        assert_eq!(first_input.identity(), SourceIdentity::new(0));
        assert_eq!(second_input.identity(), SourceIdentity::new(1));
        assert_eq!(first_input.version(), SourceVersion::new(0));
        assert_eq!(second_input.version(), SourceVersion::new(0));
        assert_eq!(first_input.file_path(), Some(first_file.path()));
        assert_eq!(second_input.file_path(), Some(second_file.path()));
        assert_eq!(first_input.text(), None);
        assert_eq!(second_input.text(), None);
        assert_eq!(first_input.bytes(), &[0xef, 0xbb, 0xbf, b'm', b'o', b'd']);
        assert_eq!(second_input.bytes(), &[0xff, b'x']);
    }

    #[test]
    fn file_arguments_build_compilation_requests_with_options() {
        let file = TemporaryFile::write("main.bray", b"module main\n");

        let options = CompilationOptions::new(
            WorkerBudget::serial(),
            bray_compilation::SelectedTarget::baseline(),
        );

        let request = match compilation_request_from_file_arguments(
            package_identity(),
            [file.path().to_path_buf()],
            options.clone(),
        ) {
            Ok(request) => request,
            Err(error) => panic!("compilation request should build: {error:?}"),
        };

        assert_eq!(request.options(), &options);
        assert_eq!(request.sources().len(), 1);

        let compilation = match Compilation::load(request) {
            Ok(compilation) => compilation,
            Err(error) => panic!("compilation should load driver file input: {error:?}"),
        };

        assert_eq!(compilation.source_count(), 1);

        assert_eq!(
            compilation.source_text(SourceId::new(0)),
            Some("module main\n")
        );
    }

    #[test]
    fn file_argument_read_errors_report_input_index_path_and_kind() {
        let missing_path = unique_temporary_directory().join("missing.bray");

        let error = match source_inputs_from_file_arguments([missing_path.clone()]) {
            Ok(inputs) => panic!("missing file should fail to load: {inputs:?}"),
            Err(error) => error,
        };

        assert_eq!(
            error,
            DriverSourceInputError::ReadFile {
                input_index: 0,
                path: missing_path,
                kind: ErrorKind::NotFound
            }
        );

        assert_eq!(error.input_index(), 0);
    }

    #[test]
    fn file_argument_read_errors_convert_to_diagnostics() {
        let missing_path = unique_temporary_directory().join("missing.bray");

        let diagnostics = match compilation_request_from_file_arguments(
            package_identity(),
            [missing_path.clone()],
            CompilationOptions::new(
                WorkerBudget::serial(),
                bray_compilation::SelectedTarget::baseline(),
            ),
        ) {
            Ok(request) => panic!("missing file should fail to build a request: {request:?}"),
            Err(diagnostics) => diagnostics,
        };

        let diagnostic = match diagnostics.diagnostics() {
            [diagnostic] => diagnostic,
            diagnostics => panic!("expected one diagnostic: {diagnostics:?}"),
        };

        assert_eq!(diagnostic.kind(), DiagnosticKind::SourceFileReadFailed);

        assert_eq!(
            diagnostic.args(),
            &[
                DiagnosticArg::new(
                    DiagnosticArgName::FilePath,
                    DiagnosticArgValue::FilePath(missing_path)
                ),
                DiagnosticArg::new(
                    DiagnosticArgName::IoErrorKind,
                    DiagnosticArgValue::IoErrorKind(DiagnosticIoErrorKind::NotFound)
                ),
                DiagnosticArg::new(
                    DiagnosticArgName::InputIndex,
                    DiagnosticArgValue::InputIndex(0)
                )
            ]
        );

        assert_eq!(
            diagnostic.notes(),
            &[DiagnosticNote::new(
                DiagnosticNoteKind::SourceFileMustBeReadable
            )]
        );
    }

    #[test]
    fn file_argument_identity_overflow_is_reported() {
        let overflow_index = match usize::try_from(u64::from(u32::MAX) + 1) {
            Ok(overflow_index) => overflow_index,
            Err(_) => return,
        };

        assert_eq!(
            source_identity_for_input_index(overflow_index),
            Err(DriverSourceInputError::SourceIdentityOverflow {
                input_index: overflow_index
            })
        );

        let diagnostic = DriverSourceInputError::SourceIdentityOverflow {
            input_index: overflow_index,
        }
        .into_diagnostic(bray_diagnostics::DiagnosticId::new(0));

        assert_eq!(diagnostic.kind(), DiagnosticKind::RequestInvalidSourceInput);

        assert_eq!(
            diagnostic.notes(),
            &[DiagnosticNote::new(
                DiagnosticNoteKind::SourceInputNeedsStableIdentity
            )]
        );
    }
}
