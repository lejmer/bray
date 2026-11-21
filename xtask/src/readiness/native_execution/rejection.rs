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
        "core.memory.read<u8>(pointer)",
        "invalid memory obligations",
    )?;

    expect_rejection(
        root,
        target,
        Some(standard_library),
        THREAD_BORROW_CONFLICT_FIXTURE,
        DiagnosticKind::CheckingConflictingBorrow,
        "value = 2",
        "a write conflicting with native-thread state",
    )?;

    expect_rejection(
        root,
        target,
        Some(standard_library),
        THREAD_BORROW_ESCAPE_FIXTURE,
        DiagnosticKind::CheckingEscapingStorageDependency,
        "return thread",
        "native-thread state escaping its source scope",
    )
}

fn expect_rejection(
    root: &Path,
    target: NativeTarget,
    standard_library: Option<&Path>,
    fixture: &str,
    diagnostic: DiagnosticKind,
    site: &str,
    description: &str,
) -> Result<(), String> {
    let compiler = crate::native_toolchain::compiler_executable(root, "brayc");
    let fixture = root.join(fixture);

    let source = std::fs::read_to_string(&fixture)
        .map_err(|error| format!("could not read fixture for {description}: {error}"))?;

    let mut sites = source.match_indices(site);

    let Some((start, _)) = sites.next().filter(|_| sites.next().is_none()) else {
        return Err(format!("fixture for {description} must contain one diagnostic site"));
    };

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
        let span = &reported["primary_span"];

        reported["code"].as_u64() == Some(u64::from(diagnostic.code().raw()))
            && span["source_origin"]["file_path"].as_str()
                .is_some_and(|path| Path::new(path).file_name() == fixture.file_name())
            && span["start"].as_u64().is_some_and(|byte| byte < (start + site.len()) as u64)
            && span["end"].as_u64().is_some_and(|byte| byte > start as u64)
    }) {
        Ok(())
    } else {
        Err(crate::command::failure(
            &format!("checking {description}"),
            &output,
        ))
    }
}
