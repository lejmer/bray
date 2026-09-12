use std::ffi::OsString;
use std::path::Path;

use bray_diagnostics::{
    DiagnosticDocumentParseKind, DiagnosticIoErrorKind, DiagnosticProjectCommandFailure,
    DiagnosticProjectOperation,
};

/// Process argument selecting a compiler request file. It must be the only argument pair.
pub const COMPILER_REQUEST_ARGUMENT: &str = "--compiler-request";

/// Writes ordered compiler arguments without converting native paths to Unicode.
pub fn write_compiler_request(
    path: &Path,
    arguments: &[OsString],
) -> Result<(), DiagnosticProjectCommandFailure> {
    let bytes = serde_json::to_vec(arguments).map_err(|error| {
        document_failure(path, DiagnosticDocumentParseKind::Serialization, error)
    })?;

    std::fs::write(path, bytes).map_err(|error| io_failure(path, error))
}

/// Reads compiler arguments, preserving their order and native path encoding.
pub fn read_compiler_request(
    path: &Path,
) -> Result<Vec<OsString>, DiagnosticProjectCommandFailure> {
    let bytes = std::fs::read(path).map_err(|error| io_failure(path, error))?;

    serde_json::from_slice(&bytes)
        .map_err(|error: serde_json::Error| document_failure(path, error.classify().into(), error))
}

fn io_failure(path: &Path, error: std::io::Error) -> DiagnosticProjectCommandFailure {
    DiagnosticProjectCommandFailure::Io {
        operation: DiagnosticProjectOperation::CompilerRequest,
        path: path.to_path_buf(),
        error: DiagnosticIoErrorKind::from(error.kind()),
    }
}

fn document_failure(
    path: &Path,
    problem: DiagnosticDocumentParseKind,
    error: serde_json::Error,
) -> DiagnosticProjectCommandFailure {
    DiagnosticProjectCommandFailure::Document {
        operation: DiagnosticProjectOperation::CompilerRequest,
        path: Some(path.to_path_buf()),
        problem,
        detail: Some(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::{read_compiler_request, write_compiler_request};

    #[test]
    fn large_requests_preserve_order_and_native_arguments() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("request.json");

        let mut arguments = vec![
            OsString::from("a long path with spaces/".repeat(2000)),
            OsString::from("\"quoted\""),
            OsString::new(),
        ];

        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStringExt;
            arguments.push(OsString::from_wide(&[0xD800, 0x61]));
        }

        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            arguments.push(OsString::from_vec(vec![0xFF, 0x61]));
        }

        write_compiler_request(&path, &arguments).unwrap();
        assert_eq!(read_compiler_request(&path).unwrap(), arguments);
    }

    #[test]
    fn malformed_requests_preserve_document_causes() {
        use bray_diagnostics::{DiagnosticDocumentParseKind, DiagnosticProjectCommandFailure};

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("request.json");

        for (bytes, expected) in [
            ("[", DiagnosticDocumentParseKind::UnexpectedEnd),
            ("{}", DiagnosticDocumentParseKind::Schema),
        ] {
            std::fs::write(&path, bytes).unwrap();

            let DiagnosticProjectCommandFailure::Document {
                path: actual_path,
                problem,
                detail,
                ..
            } = read_compiler_request(&path).unwrap_err()
            else {
                panic!("expected document failure");
            };

            assert_eq!((actual_path, problem), (Some(path.clone()), expected));
            assert!(detail.is_some());
        }
    }
}
