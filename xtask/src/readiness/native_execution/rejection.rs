use std::path::Path;
use std::process::Command;

use bray_target::NativeTarget;

use super::core::output_contains;

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
        "E7087",
        "invalid memory obligations",
    )?;

    expect_rejection(
        root,
        target,
        Some(standard_library),
        THREAD_BORROW_CONFLICT_FIXTURE,
        "E7036",
        "a write conflicting with native-thread state",
    )?;

    expect_rejection(
        root,
        target,
        Some(standard_library),
        THREAD_BORROW_ESCAPE_FIXTURE,
        "E7113",
        "native-thread state escaping its source scope",
    )
}

fn expect_rejection(
    root: &Path,
    target: NativeTarget,
    standard_library: Option<&Path>,
    fixture: &str,
    diagnostic: &str,
    description: &str,
) -> Result<(), String> {
    let compiler = crate::native_toolchain::compiler_executable(root, "brayc");
    let fixture = root.join(fixture);
    let mut command = Command::new(compiler);

    command.current_dir(root).args([
        "check",
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

    if output_contains(&output, diagnostic) {
        Ok(())
    } else {
        Err(crate::command::failure(
            &format!("checking {description}"),
            &output,
        ))
    }
}
