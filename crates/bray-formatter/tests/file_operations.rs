use std::num::NonZeroUsize;
use std::path::Path;

use bray_formatter::{
    FormatFileError, FormatFileErrorKind, FormatFileOutcome, FormatMode, FormatterConfiguration,
    format_file, format_files,
};
use bray_testing::TemporaryFile;

#[test]
fn check_mode_reports_changes_without_writing() {
    let file = TemporaryFile::write("main.bray", b"module app;func main(){return;}");
    let original = read(file.path());

    let outcome = match format_test_file(file.path(), FormatMode::Check) {
        Ok(outcome) => outcome,
        Err(error) => panic!("check formatting should succeed: {error:?}"),
    };

    assert_eq!(outcome, FormatFileOutcome::WouldChange);
    assert!(outcome.changed());
    assert_eq!(read(file.path()), original);
}

#[test]
fn write_mode_updates_changed_files_and_leaves_formatted_files_stable() {
    let file = TemporaryFile::write("main.bray", b"module app;func main(){return;}");

    let first = match format_test_file(file.path(), FormatMode::Write) {
        Ok(outcome) => outcome,
        Err(error) => panic!("write formatting should succeed: {error:?}"),
    };

    let formatted_bytes = read(file.path());

    let second = match format_test_file(file.path(), FormatMode::Write) {
        Ok(outcome) => outcome,
        Err(error) => panic!("repeat formatting should succeed: {error:?}"),
    };

    assert_eq!(first, FormatFileOutcome::Written);
    assert_eq!(second, FormatFileOutcome::Unchanged);
    assert_eq!(read(file.path()), formatted_bytes);
}

#[test]
fn write_mode_preserves_utf8_byte_order_mark_and_crlf() {
    let file = TemporaryFile::write(
        "main.bray",
        b"\xef\xbb\xbfmodule app;\r\nfunc main(){return;}\r\n",
    );

    let outcome = match format_test_file(file.path(), FormatMode::Write) {
        Ok(outcome) => outcome,
        Err(error) => panic!("BOM source formatting should succeed: {error:?}"),
    };

    let bytes = read(file.path());
    let source = String::from_utf8_lossy(&bytes[3..]);

    assert_eq!(outcome, FormatFileOutcome::Written);
    assert!(bytes.starts_with(b"\xef\xbb\xbf"));
    assert!(source.contains("\r\n"));
    assert!(!source.replace("\r\n", "").contains('\n'));
}

#[test]
fn concurrent_source_sets_return_mixed_results_in_input_order() {
    let changed = TemporaryFile::write("changed.bray", b"module changed;");
    let invalid = TemporaryFile::write("invalid.bray", &[b'm', 0xff, b'x']);
    let unchanged = TemporaryFile::write("unchanged.bray", b"module unchanged;\n");

    let paths = vec![
        changed.path().to_path_buf(),
        invalid.path().to_path_buf(),
        unchanged.path().to_path_buf(),
    ];

    let results = format_files(
        &paths,
        FormatMode::Check,
        &FormatterConfiguration::default(),
        NonZeroUsize::new(3).unwrap_or_else(|| unreachable!()),
    );

    assert!(matches!(
        results.first(),
        Some(Ok(FormatFileOutcome::WouldChange))
    ));

    let Some(Err(error)) = results.get(1) else {
        panic!("invalid source must retain the second result position");
    };

    assert_eq!(error.kind(), FormatFileErrorKind::InvalidUtf8);
    assert_eq!(error.path(), invalid.path());

    assert!(matches!(
        results.get(2),
        Some(Ok(FormatFileOutcome::Unchanged))
    ));
}

#[test]
fn invalid_utf8_returns_a_typed_failure_without_writing() {
    let file = TemporaryFile::write("main.bray", &[b'm', 0xff, b'x']);
    let original = read(file.path());

    let error = match format_test_file(file.path(), FormatMode::Write) {
        Ok(outcome) => panic!("invalid UTF-8 should fail, got {outcome:?}"),
        Err(error) => error,
    };

    assert_eq!(error.kind(), FormatFileErrorKind::InvalidUtf8);
    assert_eq!(error.path(), file.path());
    assert_eq!(error.invalid_utf8_at(), Some(1));
    assert_eq!(read(file.path()), original);
}

#[cfg(windows)]
#[test]
fn failed_atomic_replacement_preserves_original_source() {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_SHARE_READ: u32 = 1;

    let file = TemporaryFile::write("main.bray", b"module app;func main(){return;}");
    let original = read(file.path());

    let lock = match OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(file.path())
    {
        Ok(lock) => lock,
        Err(error) => panic!("test source must be locked against replacement: {error:?}"),
    };

    let error = match format_test_file(file.path(), FormatMode::Write) {
        Ok(outcome) => panic!("locked source replacement should fail, got {outcome:?}"),
        Err(error) => error,
    };

    assert_eq!(error.kind(), FormatFileErrorKind::Write);
    assert_eq!(read(file.path()), original);

    drop(lock);
}

fn read(path: &std::path::Path) -> Vec<u8> {
    match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => panic!("temporary source should be readable: {error:?}"),
    }
}

fn format_test_file(path: &Path, mode: FormatMode) -> Result<FormatFileOutcome, FormatFileError> {
    format_file(path, mode, &FormatterConfiguration::default())
}
