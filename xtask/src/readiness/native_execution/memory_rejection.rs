use std::path::Path;
use std::process::Command;

use bray_target::NativeTarget;

use super::core::output_contains;

const INVALID_MEMORY_FIXTURE: &str =
    "xtask/fixtures/native-execution/memory-invalid-obligation.bray";

pub(super) fn audit_memory_rejection(root: &Path, target: NativeTarget) -> Result<(), String> {
    let compiler = crate::native_toolchain::compiler_executable(root, "brayc");
    let fixture = root.join(INVALID_MEMORY_FIXTURE);
    let mut command = Command::new(compiler);

    command.current_dir(root).args([
        "check",
        "--product-kind",
        "executable",
        "--target",
        target.as_str(),
    ]);

    let output = command
        .arg(&fixture)
        .output()
        .map_err(|error| format!("could not check invalid memory fixture: {error}"))?;

    if output.status.success() {
        return Err("invalid memory obligations unexpectedly passed checking".to_owned());
    }

    if output_contains(&output, "E7087") {
        Ok(())
    } else {
        Err(crate::command::failure(
            "checking invalid memory obligations",
            &output,
        ))
    }
}
