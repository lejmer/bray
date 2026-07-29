use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use bray_base::Cancellation;
use bray_platform::{
    NativeChildProcess, NativeExitStatus, NativePipeReader,
    NativeProcessCommand, NativeStdio,
};

use super::{
    ExternalToolFailure, ExternalToolInvocation, ExternalToolOutput,
    ExternalToolProcessBudget, ExternalToolResponseFile,
    ExternalToolResponseFileOperation, ExternalToolStream,
};

const PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Injectable compiler-host boundary for one external tool invocation.
pub trait ExternalToolHost: Send + Sync {
    /// Runs one explicit invocation and captures its output.
    fn run(
        &self,
        invocation: &ExternalToolInvocation,
        cancellation: &dyn Cancellation,
    ) -> Result<ExternalToolOutput, ExternalToolFailure>;
}

/// Native external-tool host constrained by one shared process budget.
#[derive(Clone, Debug)]
pub struct NativeExternalToolHost {
    budget: ExternalToolProcessBudget,
}

impl NativeExternalToolHost {
    /// Creates a native host using the compiler's external-tool process budget.
    pub const fn new(budget: ExternalToolProcessBudget) -> Self {
        Self { budget }
    }

    /// Returns the shared external-tool process budget.
    pub const fn budget(&self) -> &ExternalToolProcessBudget {
        &self.budget
    }

    fn run_process(
        invocation: &ExternalToolInvocation,
        cancellation: &dyn Cancellation,
    ) -> Result<ExternalToolOutput, ExternalToolFailure> {
        let mut command =
            NativeProcessCommand::new(invocation.program()).map_err(ExternalToolFailure::Process)?;

        command.env_clear();

        for (name, value) in invocation.environment() {
            command.env(name, value);
        }

        if let Some(directory) = invocation.current_directory() {
            command.current_dir(directory);
        }

        command
            .stdin(NativeStdio::Null)
            .stdout(NativeStdio::Piped)
            .stderr(NativeStdio::Piped);

        for argument in invocation.arguments() {
            command.arg(argument);
        }

        let mut child = command.spawn().map_err(ExternalToolFailure::Process)?;

        capture_output(&mut child, cancellation)
    }
}

impl ExternalToolHost for NativeExternalToolHost {
    fn run(
        &self,
        invocation: &ExternalToolInvocation,
        cancellation: &dyn Cancellation,
    ) -> Result<ExternalToolOutput, ExternalToolFailure> {
        if cancellation.is_cancelled() {
            return Err(ExternalToolFailure::Cancelled);
        }

        let _permit = self.budget.acquire(cancellation)?;

        let mut response_files =
            MaterializedResponseFiles::create(invocation.response_files(), cancellation)?;

        let process_result = if cancellation.is_cancelled() {
            Err(ExternalToolFailure::Cancelled)
        } else {
            Self::run_process(invocation, cancellation)
        };

        let cleanup_result = response_files.remove_all();

        let output = process_result?;

        cleanup_result?;

        if cancellation.is_cancelled() {
            return Err(ExternalToolFailure::Cancelled);
        }

        Ok(output)
    }
}

fn capture_output(
    child: &mut NativeChildProcess,
    cancellation: &dyn Cancellation,
) -> Result<ExternalToolOutput, ExternalToolFailure> {
    let Some(stdout) = child.take_stdout() else {
        terminate_and_reap(child)?;

        return Err(ExternalToolFailure::MissingOutputPipe(
            ExternalToolStream::StandardOutput,
        ));
    };

    let Some(stderr) = child.take_stderr() else {
        terminate_and_reap(child)?;

        return Err(ExternalToolFailure::MissingOutputPipe(
            ExternalToolStream::StandardError,
        ));
    };

    thread::scope(|scope| {
        let stdout_reader = scope.spawn(move || {
            read_output(stdout, ExternalToolStream::StandardOutput)
        });

        let stderr_reader = scope.spawn(move || {
            read_output(stderr, ExternalToolStream::StandardError)
        });

        let status = wait_for_process(child, cancellation);
        let stdout = join_output(stdout_reader, ExternalToolStream::StandardOutput);
        let stderr = join_output(stderr_reader, ExternalToolStream::StandardError);

        let status = status?;
        let stdout = stdout?;
        let stderr = stderr?;

        if cancellation.is_cancelled() {
            return Err(ExternalToolFailure::Cancelled);
        }

        Ok(ExternalToolOutput::new(
            status.success(),
            status.code(),
            stdout,
            stderr,
        ))
    })
}

fn read_output(
    mut reader: NativePipeReader,
    stream: ExternalToolStream,
) -> Result<Vec<u8>, ExternalToolFailure> {
    let mut output = Vec::new();

    reader
        .read_to_end(&mut output)
        .map_err(|error| ExternalToolFailure::OutputCapture {
            stream,
            kind: error.kind(),
        })?;

    Ok(output)
}

fn join_output(
    reader: thread::ScopedJoinHandle<'_, Result<Vec<u8>, ExternalToolFailure>>,
    stream: ExternalToolStream,
) -> Result<Vec<u8>, ExternalToolFailure> {
    reader
        .join()
        .map_err(|_| ExternalToolFailure::OutputReaderTerminated(stream))?
}

fn wait_for_process(
    child: &mut NativeChildProcess,
    cancellation: &dyn Cancellation,
) -> Result<NativeExitStatus, ExternalToolFailure> {
    loop {
        if cancellation.is_cancelled() {
            terminate_and_reap(child)?;

            return Err(ExternalToolFailure::Cancelled);
        }

        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => thread::sleep(PROCESS_POLL_INTERVAL),
            Err(error) => {
                let cleanup = terminate_and_reap(child);

                return Err(cleanup.err().unwrap_or(ExternalToolFailure::Process(error)));
            }
        }
    }
}

fn terminate_and_reap(child: &mut NativeChildProcess) -> Result<(), ExternalToolFailure> {
    let termination = child.terminate();
    let reaping = child.wait();

    match (termination, reaping) {
        (_, Ok(_)) => Ok(()),
        (Err(error), Err(_)) | (Ok(()), Err(error)) => {
            Err(ExternalToolFailure::Process(error))
        }
    }
}

struct MaterializedResponseFiles {
    paths: Vec<PathBuf>,
}

impl MaterializedResponseFiles {
    fn create(
        response_files: &[ExternalToolResponseFile],
        cancellation: &dyn Cancellation,
    ) -> Result<Self, ExternalToolFailure> {
        let mut materialized = Self { paths: Vec::new() };

        for response_file in response_files {
            if cancellation.is_cancelled() {
                return Err(ExternalToolFailure::Cancelled);
            }

            materialized.write(response_file)?;
        }

        Ok(materialized)
    }

    fn write(
        &mut self,
        response_file: &ExternalToolResponseFile,
    ) -> Result<(), ExternalToolFailure> {
        let mut file = create_response_file(response_file.path())?;

        // Cleanup must own the path independently of the immutable invocation.
        self.paths.push(response_file.path().to_path_buf());

        file.write_all(response_file.contents())
            .and_then(|_| file.flush())
            .map_err(|error| response_file_error(
                response_file.path().to_path_buf(),
                ExternalToolResponseFileOperation::Write,
                error,
            ))
    }

    fn remove_all(&mut self) -> Result<(), ExternalToolFailure> {
        let mut first_failure = None;

        for path in std::mem::take(&mut self.paths) {
            if let Err(error) = std::fs::remove_file(&path)
                && first_failure.is_none()
            {
                first_failure = Some(response_file_error(
                    path,
                    ExternalToolResponseFileOperation::Remove,
                    error,
                ));
            }
        }

        match first_failure {
            Some(failure) => Err(failure),
            None => Ok(()),
        }
    }
}

impl Drop for MaterializedResponseFiles {
    fn drop(&mut self) {
        for path in &self.paths {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn create_response_file(path: &Path) -> Result<File, ExternalToolFailure> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| {
            response_file_error(
                path.to_path_buf(),
                ExternalToolResponseFileOperation::Write,
                error,
            )
        })
}

fn response_file_error(
    path: PathBuf,
    operation: ExternalToolResponseFileOperation,
    error: std::io::Error,
) -> ExternalToolFailure {
    ExternalToolFailure::ResponseFile {
        path,
        operation,
        kind: error.kind(),
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::num::NonZeroUsize;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    use bray_testing::unique_temporary_directory;

    use super::{ExternalToolHost, NativeExternalToolHost};
    use crate::{
        ExternalToolFailure, ExternalToolInvocation,
        ExternalToolProcessBudget, ExternalToolResponseFile,
    };

    const CHILD_MARKER_VARIABLE: &str = "BRAY_EXTERNAL_TOOL_CHILD_MARKER";
    const CHILD_RESPONSE_FILE_VARIABLE: &str =
        "BRAY_EXTERNAL_TOOL_CHILD_RESPONSE_FILE";
    const CHILD_VALUE_VARIABLE: &str = "BRAY_EXTERNAL_TOOL_CHILD_VALUE";

    #[test]
    fn native_host_uses_controlled_environment_and_captures_output() {
        let invocation = child_invocation("captured", None);
        let host = native_host();

        let output = host
            .run(&invocation, &|| false)
            .unwrap_or_else(|error| panic!("test child should complete: {error:?}"));

        let stdout = String::from_utf8_lossy(output.standard_output());
        let stderr = String::from_utf8_lossy(output.standard_error());

        assert!(
            output.success(),
            "child failed with code {:?}, stdout {:?}, stderr {:?}",
            output.exit_code(),
            stdout,
            stderr
        );

        assert!(stdout.contains("value=captured"));
        assert!(stdout.contains("path=false"));
        assert!(stderr.contains("external-tool-stderr"));
    }

    #[test]
    fn native_host_materializes_and_removes_response_files() {
        let directory = unique_temporary_directory();

        std::fs::create_dir_all(&directory)
            .unwrap_or_else(|error| panic!("test directory should be created: {error:?}"));

        let response_path = directory.join("arguments.rsp");

        let response_file = ExternalToolResponseFile::try_new(
            response_path.clone(),
            Arc::<[u8]>::from(&b"first\nsecond\n"[..]),
        )
        .unwrap_or_else(|error| panic!("test response file should be valid: {error:?}"));

        let invocation = child_invocation("response", Some(response_file));
        let host = native_host();

        let output = host
            .run(&invocation, &|| false)
            .unwrap_or_else(|error| panic!("test child should complete: {error:?}"));

        assert!(
            output.success(),
            "child failed with code {:?}, stdout {:?}, stderr {:?}",
            output.exit_code(),
            String::from_utf8_lossy(output.standard_output()),
            String::from_utf8_lossy(output.standard_error())
        );

        assert!(
            String::from_utf8_lossy(output.standard_output())
                .contains("response=first\nsecond\n")
        );

        assert!(!response_path.exists());

        std::fs::remove_dir(&directory)
            .unwrap_or_else(|error| panic!("test directory should be removed: {error:?}"));
    }

    #[test]
    fn native_host_does_not_spawn_after_prior_cancellation() {
        let invocation = ExternalToolInvocation::try_new(
            "bray-test-tool-that-does-not-exist",
            [],
            [],
            None,
            [],
        )
        .unwrap_or_else(|error| {
            panic!("test invocation should be valid: {error:?}")
        });

        assert_eq!(
            native_host().run(&invocation, &|| true),
            Err(ExternalToolFailure::Cancelled)
        );
    }

    #[test]
    fn cancellation_terminates_and_reaps_native_external_tool_child() {
        let directory = unique_temporary_directory();

        std::fs::create_dir_all(&directory)
            .unwrap_or_else(|error| panic!("test directory should be created: {error:?}"));

        let marker = directory.join("started");
        let invocation = waiting_child_invocation(&marker);
        let host = native_host();
        let cancelled = Arc::new(AtomicBool::new(false));
        let started_at = Instant::now();

        let result = std::thread::scope(|scope| {
            let cancellation = Arc::clone(&cancelled);

            let running = scope.spawn(move || {
                let observe_cancellation =
                    || cancellation.load(Ordering::Acquire);

                host.run(&invocation, &observe_cancellation)
            });

            while !marker.exists() {
                assert!(
                    started_at.elapsed() < Duration::from_secs(5),
                    "test child should report startup"
                );

                std::thread::sleep(Duration::from_millis(10));
            }

            cancelled.store(true, Ordering::Release);

            running
                .join()
                .unwrap_or_else(|_| panic!("native host worker should finish"))
        });

        assert_eq!(result, Err(ExternalToolFailure::Cancelled));
        assert!(started_at.elapsed() < Duration::from_secs(5));

        std::fs::remove_file(&marker)
            .unwrap_or_else(|error| panic!("test marker should be removed: {error:?}"));

        std::fs::remove_dir(&directory)
            .unwrap_or_else(|error| panic!("test directory should be removed: {error:?}"));
    }

    #[test]
    fn external_tool_subprocess_fixture() {
        let Some(marker) = std::env::var_os(CHILD_MARKER_VARIABLE) else {
            return;
        };

        let value = std::env::var_os(CHILD_VALUE_VARIABLE)
            .unwrap_or_else(|| OsString::from("missing"));

        println!("value={}", value.to_string_lossy());
        println!("path={}", std::env::var_os("PATH").is_some());
        eprintln!("external-tool-stderr");

        if let Some(response_file) =
            std::env::var_os(CHILD_RESPONSE_FILE_VARIABLE)
        {
            let contents = std::fs::read_to_string(response_file)
                .unwrap_or_else(|error| {
                    panic!("child response file should be readable: {error:?}")
                });

            println!("response={contents}");
        }

        if !marker.is_empty() {
            std::fs::write(marker, b"started")
                .unwrap_or_else(|error| panic!("child marker should be written: {error:?}"));

            std::thread::sleep(Duration::from_secs(30));
        }
    }

    fn native_host() -> NativeExternalToolHost {
        NativeExternalToolHost::new(ExternalToolProcessBudget::new(
            NonZeroUsize::MIN,
        ))
    }

    fn child_invocation(
        value: &str,
        response_file: Option<ExternalToolResponseFile>,
    ) -> ExternalToolInvocation {
        let mut environment = vec![
            (OsString::from(CHILD_MARKER_VARIABLE), OsString::new()),
            (
                OsString::from(CHILD_VALUE_VARIABLE),
                OsString::from(value),
            ),
        ];

        if let Some(response_file) = &response_file {
            environment.push((
                OsString::from(CHILD_RESPONSE_FILE_VARIABLE),
                response_file.path().as_os_str().to_os_string(),
            ));
        }

        ExternalToolInvocation::try_new(
            std::env::current_exe()
                .unwrap_or_else(|error| panic!("test executable should be available: {error:?}")),
            [
                OsString::from("external_tool_subprocess_fixture"),
                OsString::from("--nocapture"),
            ],
            environment,
            None,
            response_file,
        )
        .unwrap_or_else(|error| panic!("test invocation should be valid: {error:?}"))
    }

    fn waiting_child_invocation(marker: &std::path::Path) -> ExternalToolInvocation {
        ExternalToolInvocation::try_new(
            std::env::current_exe()
                .unwrap_or_else(|error| panic!("test executable should be available: {error:?}")),
            [
                OsString::from("external_tool_subprocess_fixture"),
                OsString::from("--nocapture"),
            ],
            [
                (
                    OsString::from(CHILD_MARKER_VARIABLE),
                    marker.as_os_str().to_os_string(),
                ),
                (
                    OsString::from(CHILD_VALUE_VARIABLE),
                    OsString::from("waiting"),
                ),
            ],
            None,
            [],
        )
        .unwrap_or_else(|error| panic!("test invocation should be valid: {error:?}"))
    }
}
