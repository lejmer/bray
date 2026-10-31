#[cfg(any(feature = "full", test))]
use std::io::{Read, Write};
use std::process::{Command, Output};
#[cfg(feature = "full")]
use std::process::Stdio;

#[cfg(feature = "full")]
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

#[cfg(feature = "full")]
pub(crate) fn output_with_streamed_stderr(command: &mut Command) -> std::io::Result<Output> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let child_stderr = child
        .stderr
        .take()
        .ok_or_else(|| std::io::Error::other("child stderr pipe is unavailable"))?;

    let stderr_reader = std::thread::spawn(move || stream_and_capture(child_stderr));
    let output = child.wait_with_output();

    let stderr = stderr_reader
        .join()
        .map_err(|_| std::io::Error::other("child stderr reader panicked"))??;

    output.map(|mut output| {
        output.stderr = stderr;

        output
    })
}

#[cfg(feature = "full")]
fn stream_and_capture(source: impl Read) -> std::io::Result<Vec<u8>> {
    copy_and_capture(source, std::io::stderr().lock())
}

#[cfg(any(feature = "full", test))]
fn copy_and_capture(
    mut source: impl Read,
    mut destination: impl Write,
) -> std::io::Result<Vec<u8>> {
    let mut captured = Vec::new();
    let mut buffer = [0_u8; 8 * 1024];

    loop {
        let read = source.read(&mut buffer)?;

        if read == 0 {
            break;
        }

        destination.write_all(&buffer[..read])?;
        destination.flush()?;
        captured.extend_from_slice(&buffer[..read]);
    }

    Ok(captured)
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
        format!(": {}", details.join(". "))
    };

    format!("{operation} failed with status {}{details}", output.status)
}
#[cfg(feature = "full")]
const USAGE: &str = "usage: cargo xtask <compiler-known | composition | format | package-interface | performance | readiness | runtime-artifact | standard-library | style | test> ...";

#[cfg(not(feature = "full"))]
const USAGE: &str = "usage: xtask test compiler ...";

/// Runs the repository task command selected by process arguments.
pub fn run() -> std::process::ExitCode {
    let mut arguments = std::env::args().skip(1);

    match arguments.next().as_deref() {
        #[cfg(feature = "full")]
        Some("composition") => crate::composition::run(arguments),
        #[cfg(feature = "full")]
        Some("compiler-known") => crate::compiler_known::run(arguments),
        #[cfg(feature = "full")]
        Some("format") => crate::source_format::run(arguments),
        #[cfg(feature = "full")]
        Some("package-interface") => crate::package_interface::run(arguments),
        #[cfg(feature = "full")]
        Some("performance") => crate::performance::run(arguments),
        #[cfg(feature = "full")]
        Some("readiness") => crate::readiness::run(arguments),
        #[cfg(feature = "full")]
        Some("runtime-artifact") => crate::runtime_artifact::run(arguments),
        #[cfg(feature = "full")]
        Some("standard-library") => crate::standard_library::run(arguments),
        #[cfg(feature = "full")]
        Some("style") => crate::style::run(arguments),
        Some("test") => crate::testing::run(arguments),
        _ => {
            eprintln!("{USAGE}");

            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::copy_and_capture;

    #[test]
    fn streamed_child_output_is_also_retained_for_failure_diagnostics() {
        let mut streamed = Vec::new();

        let captured = copy_and_capture(Cursor::new(b"compiler progress"), &mut streamed)
            .unwrap_or_else(|error| panic!("test stream must copy: {error}"));

        assert_eq!(streamed, b"compiler progress");
        assert_eq!(captured, b"compiler progress");
    }
}
