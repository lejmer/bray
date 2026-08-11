use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use bray_platform::{NativeChildProcess, NativeProcessCommand, NativeStdio, PlatformError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Tool {
    Compiler,
    Formatter,
    LanguageServer,
}

impl Tool {
    const fn environment_variable(self) -> &'static str {
        match self {
            Self::Compiler => "BRAYC",
            Self::Formatter => "BRAYFMT",
            Self::LanguageServer => "BRAY_LSP",
        }
    }

    const fn executable_name(self) -> &'static str {
        match self {
            Self::Compiler => "brayc",
            Self::Formatter => "brayfmt",
            Self::LanguageServer => "bray-lsp",
        }
    }
}

#[derive(Debug)]
pub(crate) struct ToolRequest {
    tool: Tool,
    arguments: Vec<OsString>,
    working_directory: PathBuf,
    input: Option<Vec<u8>>,
}

impl ToolRequest {
    pub(crate) fn new(tool: Tool, working_directory: impl Into<PathBuf>) -> Self {
        Self {
            tool,
            arguments: Vec::new(),
            working_directory: working_directory.into(),
            input: None,
        }
    }

    pub(crate) fn arg(&mut self, argument: impl Into<OsString>) -> &mut Self {
        self.arguments.push(argument.into());

        self
    }

    pub(crate) fn args(
        &mut self,
        arguments: impl IntoIterator<Item = impl Into<OsString>>,
    ) -> &mut Self {
        self.arguments.extend(arguments.into_iter().map(Into::into));

        self
    }

    pub(crate) fn input(&mut self, input: Vec<u8>) -> &mut Self {
        self.input = Some(input);

        self
    }

    #[cfg(test)]
    pub(crate) const fn tool(&self) -> Tool {
        self.tool
    }

    #[cfg(test)]
    pub(crate) fn arguments(&self) -> &[OsString] {
        &self.arguments
    }

    #[cfg(test)]
    pub(crate) fn working_directory(&self) -> &Path {
        &self.working_directory
    }

    #[cfg(test)]
    pub(crate) fn input_bytes(&self) -> Option<&[u8]> {
        self.input.as_deref()
    }
}

#[derive(Debug)]
pub(crate) struct ToolOutput {
    success: bool,
    stdout: String,
    stderr: String,
}

impl ToolOutput {
    pub(crate) fn new(success: bool, stdout: String, stderr: String) -> Self {
        Self {
            success,
            stdout,
            stderr,
        }
    }

    pub(crate) const fn success(&self) -> bool {
        self.success
    }

    pub(crate) fn into_parts(self) -> (bool, String, String) {
        (self.success, self.stdout, self.stderr)
    }
}

pub(crate) trait ToolExecutor {
    fn capture(&self, request: ToolRequest) -> Result<ToolOutput, ToolExecutionError>;

    fn serve(
        &self,
        request: ToolRequest,
        input: Box<dyn Read + Send>,
        output: &mut dyn Write,
    ) -> Result<ToolOutput, ToolExecutionError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ToolStream {
    StandardInput,
    StandardOutput,
    StandardError,
}

#[derive(Debug)]
pub(crate) enum ToolExecutionError {
    Platform {
        program: PathBuf,
        error: PlatformError,
    },
    MissingStream(ToolStream),
    StreamIo {
        stream: ToolStream,
        error: std::io::ErrorKind,
    },
    InvalidUtf8(ToolStream),
    StreamThreadPanicked(ToolStream),
}

pub(crate) struct NativeToolExecutor;

impl ToolExecutor for NativeToolExecutor {
    fn capture(&self, request: ToolRequest) -> Result<ToolOutput, ToolExecutionError> {
        let (command, program) = native_command(&request)?;

        let output = command
            .capture(request.input)
            .map_err(|error| ToolExecutionError::Platform { program, error })?;

        let (status, stdout, stderr) = output.into_parts();

        let stdout = String::from_utf8(stdout)
            .map_err(|_| ToolExecutionError::InvalidUtf8(ToolStream::StandardOutput))?;

        let stderr = String::from_utf8(stderr)
            .map_err(|_| ToolExecutionError::InvalidUtf8(ToolStream::StandardError))?;

        Ok(ToolOutput::new(status.success(), stdout, stderr))
    }

    fn serve(
        &self,
        request: ToolRequest,
        input: Box<dyn Read + Send>,
        output: &mut dyn Write,
    ) -> Result<ToolOutput, ToolExecutionError> {
        let (mut command, program) = native_command(&request)?;

        command
            .stdin(NativeStdio::Piped)
            .stdout(NativeStdio::Piped)
            .stderr(NativeStdio::Piped);

        let mut child = command
            .spawn()
            .map_err(|error| ToolExecutionError::Platform {
                program: program.clone(),
                error,
            })?;

        let Some(mut child_input) = child.take_stdin() else {
            cleanup_child(&mut child);

            return Err(ToolExecutionError::MissingStream(ToolStream::StandardInput));
        };

        let Some(mut child_output) = child.take_stdout() else {
            cleanup_child(&mut child);

            return Err(ToolExecutionError::MissingStream(
                ToolStream::StandardOutput,
            ));
        };

        let Some(mut child_error) = child.take_stderr() else {
            cleanup_child(&mut child);

            return Err(ToolExecutionError::MissingStream(ToolStream::StandardError));
        };

        // The editor can keep input open after the server exits, so the child owns this detached
        // pump and its eventual I/O result cannot delay server completion.
        let _input_thread = std::thread::spawn(move || {
            let mut input = input;

            std::io::copy(&mut input, &mut child_input)
        });

        let error_thread = std::thread::spawn(move || {
            let mut bytes = Vec::new();

            child_error.read_to_end(&mut bytes).map(|_| bytes)
        });

        if let Err(error) = std::io::copy(&mut child_output, output) {
            let _ = child.terminate();
            let _ = child.wait();
            let _ = error_thread.join();

            return Err(ToolExecutionError::StreamIo {
                stream: ToolStream::StandardOutput,
                error: error.kind(),
            });
        }

        let status = child
            .wait()
            .map_err(|error| ToolExecutionError::Platform { program, error })?;

        let stderr = error_thread
            .join()
            .map_err(|_| ToolExecutionError::StreamThreadPanicked(ToolStream::StandardError))?
            .map_err(|error| ToolExecutionError::StreamIo {
                stream: ToolStream::StandardError,
                error: error.kind(),
            })?;

        let stderr = String::from_utf8(stderr)
            .map_err(|_| ToolExecutionError::InvalidUtf8(ToolStream::StandardError))?;

        Ok(ToolOutput::new(status.success(), String::new(), stderr))
    }
}

fn native_command(
    request: &ToolRequest,
) -> Result<(NativeProcessCommand, PathBuf), ToolExecutionError> {
    let program = tool_path(request.tool);

    let mut command =
        NativeProcessCommand::new(&program).map_err(|error| ToolExecutionError::Platform {
            program: PathBuf::from(&program),
            error,
        })?;

    command.current_dir(&request.working_directory);

    for argument in &request.arguments {
        command.arg(argument);
    }

    Ok((command, PathBuf::from(program)))
}

fn cleanup_child(child: &mut NativeChildProcess) {
    let _ = child.terminate();
    let _ = child.wait();
}

fn tool_path(tool: Tool) -> OsString {
    if let Some(path) = std::env::var_os(tool.environment_variable()) {
        return path;
    }

    let sibling = std::env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().map(Path::to_path_buf))
        .map(|directory| directory.join(executable_file_name(tool.executable_name())));

    match sibling {
        Some(path) if path.is_file() => path.into_os_string(),
        _ => OsString::from(tool.executable_name()),
    }
}

fn executable_file_name(name: &str) -> OsString {
    if cfg!(windows) {
        OsString::from(format!("{name}.exe"))
    } else {
        OsString::from(name)
    }
}
