use bray_formatter::{
    FormatFileErrorKind, FormatFileOutcome, FormatMode, format_file,
};
use bray_testing::TemporaryFile;

#[test]
fn check_mode_reports_changes_without_writing() {
    let file = TemporaryFile::write("main.bray", b"module app;func main(){return;}");
    let original = read(file.path());

    let outcome = match format_file(file.path(), FormatMode::Check) {
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

    let first = match format_file(file.path(), FormatMode::Write) {
        Ok(outcome) => outcome,
        Err(error) => panic!("write formatting should succeed: {error:?}"),
    };

    let formatted_bytes = read(file.path());

    let second = match format_file(file.path(), FormatMode::Write) {
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

    let outcome = match format_file(file.path(), FormatMode::Write) {
        Ok(outcome) => outcome,
        Err(error) => panic!("BOM source formatting should succeed: {error:?}"),
    };

    let bytes = read(file.path());
    let source = String::from_utf8_lossy(&bytes[3..]);

    assert_eq!(outcome, FormatFileOutcome::Written);
    assert!(bytes.starts_with(b"\xef\xbb\xbf"));
    assert!(source.contains("\r\n"));
    assert_eq!(source.replace("\r\n", "").contains('\n'), false);
}

#[test]
fn invalid_utf8_returns_a_typed_failure_without_writing() {
    let file = TemporaryFile::write("main.bray", &[b'm', 0xff, b'x']);
    let original = read(file.path());

    let error = match format_file(file.path(), FormatMode::Write) {
        Ok(outcome) => panic!("invalid UTF-8 should fail, got {outcome:?}"),
        Err(error) => error,
    };

    assert_eq!(error.kind(), FormatFileErrorKind::InvalidUtf8);
    assert_eq!(error.path(), file.path());
    assert_eq!(error.invalid_utf8_at(), Some(1));
    assert_eq!(read(file.path()), original);
}

fn read(path: &std::path::Path) -> Vec<u8> {
    match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => panic!("temporary source should be readable: {error:?}"),
    }
}
