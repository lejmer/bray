use std::path::Path;
use std::process::Command;

const EXECUTABLE_ENVIRONMENT: &str = "BRAY_PERFORMANCE_EXECUTABLE";

pub(super) fn workload_command(
    executable: &Path,
    child_executable: &Path,
    working_directory: &Path,
) -> Command {
    let mut command = Command::new(executable);

    command
        .current_dir(working_directory)
        .env(EXECUTABLE_ENVIRONMENT, child_executable);

    command
}
