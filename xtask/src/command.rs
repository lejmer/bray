use std::process::{Command, Output};

pub(crate) fn reject_trailing_argument(
    mut arguments: impl Iterator<Item = String>,
) -> Result<(), String> {
    match arguments.next() {
        Some(argument) => Err(format!("unexpected argument: {argument}")),
        None => Ok(()),
    }
}

pub(crate) fn require_success(mut command: Command, operation: &str) -> Result<Output, String> {
    let output = command
        .output()
        .map_err(|error| format!("could not start {operation}: {error}"))?;

    if output.status.success() {
        return Ok(output);
    }

    Err(failure(operation, &output))
}

pub(crate) fn failure(operation: &str, output: &Output) -> String {
    let mut details = Vec::new();
    let standard_output = String::from_utf8_lossy(&output.stdout);
    let standard_error = String::from_utf8_lossy(&output.stderr);

    if !standard_output.trim().is_empty() {
        details.push(format!("stdout: {}", standard_output.trim()));
    }

    if !standard_error.trim().is_empty() {
        details.push(format!("stderr: {}", standard_error.trim()));
    }

    let details = if details.is_empty() {
        String::new()
    } else {
        format!(": {}", details.join("; "))
    };

    format!("{operation} failed with status {}{details}", output.status)
}
