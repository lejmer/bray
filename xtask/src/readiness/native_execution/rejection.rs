use std::path::Path;
use std::process::Command;

use bray_diagnostics::DiagnosticKind;
use bray_target::NativeTarget;

const INVALID_MEMORY_FIXTURE: &str =
    "xtask/fixtures/native-execution/memory-invalid-obligation.bray";
const THREAD_BORROW_CONFLICT_FIXTURE: &str =
    "xtask/fixtures/native-execution/native-thread-borrow-conflict.bray";
const THREAD_BORROW_ESCAPE_FIXTURE: &str =
    "xtask/fixtures/native-execution/native-thread-borrow-escape.bray";

pub(super) fn audit_rejections(
    root: &Path,
    target: NativeTarget,
    standard_library: &Path,
) -> Result<(), String> {
    expect_rejection(
        root,
        target,
        None,
        INVALID_MEMORY_FIXTURE,
        DiagnosticKind::CheckingUninitializedRawStorage,
        "invalid memory obligations",
    )?;

    expect_rejection(
        root,
        target,
        Some(standard_library),
        THREAD_BORROW_CONFLICT_FIXTURE,
        DiagnosticKind::CheckingConflictingBorrow,
        "a write conflicting with native-thread state",
    )?;

    expect_rejection(
        root,
        target,
        Some(standard_library),
        THREAD_BORROW_ESCAPE_FIXTURE,
        DiagnosticKind::CheckingEscapingStorageDependency,
        "native-thread state escaping its source scope",
    )
}

fn expect_rejection(
    root: &Path,
    target: NativeTarget,
    standard_library: Option<&Path>,
    fixture: &str,
    diagnostic: DiagnosticKind,
    description: &str,
) -> Result<(), String> {
    let compiler = crate::native_toolchain::compiler_executable(root, "brayc");
    let fixture = root.join(fixture);
    let mut command = Command::new(compiler);

    command.current_dir(root).args([
        "check",
        "--format",
        "json",
        "--product-kind",
        "executable",
        "--target",
        target.as_str(),
    ]);

    if let Some(standard_library) = standard_library {
        command.arg("--standard-library-root").arg(standard_library);
    }

    let output = command
        .arg(&fixture)
        .output()
        .map_err(|error| format!("could not check {description}: {error}"))?;

    if output.status.success() {
        return Err(format!("{description} unexpectedly passed checking"));
    }

    let report: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| crate::command::failure(&format!("reading diagnostics for {description}"), &output))?;

    let mut errors = report["diagnostics"].as_array().into_iter().flatten()
        .filter(|reported| reported["severity"] == "error").peekable();

    if errors.peek().is_some() && errors.all(|reported| {
        reported["code"].as_u64() == Some(u64::from(diagnostic.code().raw()))
            && reported["primary_span"]["source_origin"]["file_path"].as_str()
                .is_some_and(|path| Path::new(path).file_name() == fixture.file_name())
    }) {
        Ok(())
    } else {
        Err(crate::command::failure(
            &format!("checking {description}"),
            &output,
        ))
    }
}
