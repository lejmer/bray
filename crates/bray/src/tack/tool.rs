use std::ffi::OsString;
use std::hash::Hasher;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use bray_base::StableDigestHasher;
use bray_platform::{
    NativeChildProcess, NativeProcessCommand, NativeStdio, PlatformError, PlatformErrorKind,
    PlatformOperation,
};

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
    exit_code: Option<i32>,
    subject: Option<String>,
    stdout: String,
    stderr: String,
}

impl ToolOutput {
    #[cfg(test)]
    pub(crate) fn new(success: bool, stdout: String, stderr: String) -> Self {
        Self {
            success,
            exit_code: None,
            subject: None,
            stdout,
            stderr,
        }
    }

    pub(crate) fn from_process(
        success: bool,
        exit_code: Option<i32>,
        stdout: String,
        stderr: String,
    ) -> Self {
        Self {
            success,
            exit_code,
            subject: None,
            stdout,
            stderr,
        }
    }

    pub(crate) fn with_subject(mut self, subject: impl Into<String>) -> Self {
        self.subject = Some(subject.into());

        self
    }

    pub(crate) const fn success(&self) -> bool {
        self.success
    }

    pub(crate) fn into_parts(self) -> (bool, Option<i32>, Option<String>, String, String) {
        (
            self.success,
            self.exit_code,
            self.subject,
            self.stdout,
            self.stderr,
        )
    }
}

pub(crate) trait ToolExecutor {
    fn identity(
        &self,
        tool: Tool,
        _working_directory: &Path,
    ) -> Result<[u8; 32], ToolIdentityError> {
        let mut identity = StableDigestHasher::new();
        identity.write(tool.executable_name().as_bytes());

        Ok(identity.finalize())
    }

    fn capture(&self, request: ToolRequest) -> Result<ToolOutput, ToolExecutionError>;

    fn serve(
        &self,
        request: ToolRequest,
        input: Box<dyn Read + Send>,
        output: &mut dyn Write,
    ) -> Result<ToolOutput, ToolExecutionError>;
}

#[derive(Debug)]
pub(crate) struct ToolIdentityError {
    pub(crate) path: PathBuf,
    pub(crate) error: std::io::Error,
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
    fn identity(
        &self,
        tool: Tool,
        working_directory: &Path,
    ) -> Result<[u8; 32], ToolIdentityError> {
        let path = resolved_tool_path(tool, working_directory)?;

        bray_emitter::path_digest(&path).map_err(|error| {
            let (path, error) = error.into_parts();

            ToolIdentityError { path, error }
        })
    }

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

        Ok(ToolOutput::from_process(
            status.success(),
            status.code(),
            stdout,
            stderr,
        ))
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

        Ok(ToolOutput::from_process(
            status.success(),
            status.code(),
            String::new(),
            stderr,
        ))
    }
}

fn native_command(
    request: &ToolRequest,
) -> Result<(NativeProcessCommand, PathBuf), ToolExecutionError> {
    let program =
        resolved_tool_path(request.tool, &request.working_directory).map_err(|error| {
            ToolExecutionError::Platform {
                program: error.path,
                error: PlatformError::new(
                    PlatformOperation::ProcessSpawn,
                    PlatformErrorKind::Io(error.error.kind()),
                ),
            }
        })?;

    let mut command =
        NativeProcessCommand::new(&program).map_err(|error| ToolExecutionError::Platform {
            program: program.clone(),
            error,
        })?;

    command.current_dir(&request.working_directory);

    for argument in &request.arguments {
        command.arg(argument);
    }

    Ok((command, program))
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

fn resolved_tool_path(tool: Tool, working_directory: &Path) -> Result<PathBuf, ToolIdentityError> {
    let selected = PathBuf::from(tool_path(tool));
    let search = std::env::var_os("PATH").unwrap_or_default();

    resolved_tool_path_from_selection(selected, &search, working_directory)
}

fn resolved_tool_path_from_selection(
    selected: PathBuf,
    search: &std::ffi::OsStr,
    working_directory: &Path,
) -> Result<PathBuf, ToolIdentityError> {
    let executable = if cfg!(windows) && selected.extension().is_none() {
        selected.with_extension("exe")
    } else {
        selected.clone()
    };

    if executable.is_absolute() || selected.components().count() > 1 {
        let selected_path = if executable.is_absolute() {
            executable
        } else {
            working_directory.join(executable)
        };

        if !selected_path.is_file() {
            return Err(ToolIdentityError {
                path: selected,
                error: std::io::ErrorKind::NotFound.into(),
            });
        }

        return std::fs::canonicalize(&selected_path).map_err(|error| ToolIdentityError {
            path: selected,
            error,
        });
    }

    let file_name = executable;

    let current_directory = cfg!(windows).then(|| working_directory.join(&file_name));

    let resolved = current_directory
        .into_iter()
        .chain(std::env::split_paths(search).map(|directory| {
            let directory = if directory.is_absolute() {
                directory
            } else {
                working_directory.join(directory)
            };

            directory.join(&file_name)
        }))
        .find(|candidate| is_executable_file(candidate));

    let Some(resolved) = resolved else {
        return Err(ToolIdentityError {
            path: file_name,
            error: std::io::ErrorKind::NotFound.into(),
        });
    };

    std::fs::canonicalize(&resolved).map_err(|error| ToolIdentityError {
        path: resolved,
        error,
    })
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };

    if !metadata.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        metadata.permissions().mode() & 0o111 != 0
    }

    #[cfg(not(unix))]
    {
        true
    }
}

fn executable_file_name(name: &str) -> OsString {
    if cfg!(windows) {
        OsString::from(format!("{name}.exe"))
    } else {
        OsString::from(name)
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::path::{Path, PathBuf};

    use super::{executable_file_name, resolved_tool_path_from_selection};

    #[test]
    fn explicit_missing_tool_path_is_preserved_in_identity_failure() {
        let selected = PathBuf::from("missing").join("custom-brayc");

        let error = resolved_tool_path_from_selection(
            selected.clone(),
            OsStr::new(""),
            Path::new("workspace"),
        )
        .expect_err("missing explicit compiler path must fail");

        assert_eq!(error.path, selected);
        assert_eq!(error.error.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn relative_tool_path_is_resolved_from_the_request_working_directory() {
        let workspace = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("temporary workspace should exist: {error}"));

        let selected = PathBuf::from("tools").join(executable_file_name("brayc"));
        let compiler = workspace.path().join(&selected);

        std::fs::create_dir_all(
            compiler
                .parent()
                .unwrap_or_else(|| panic!("compiler fixture should have a parent")),
        )
        .unwrap_or_else(|error| panic!("compiler fixture directory should exist: {error}"));

        std::fs::write(&compiler, b"compiler")
            .unwrap_or_else(|error| panic!("compiler fixture should exist: {error}"));

        let resolved =
            resolved_tool_path_from_selection(selected, OsStr::new(""), workspace.path())
                .unwrap_or_else(|error| {
                    panic!("workspace-relative compiler should resolve: {error:?}")
                });

        let expected = std::fs::canonicalize(compiler)
            .unwrap_or_else(|error| panic!("compiler fixture should canonicalize: {error}"));

        assert_eq!(resolved, expected);
    }

    #[test]
    fn bare_tool_name_uses_the_host_search_order() {
        let workspace = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("temporary workspace should exist: {error}"));

        let search = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("temporary search directory should exist: {error}"));

        let file_name = executable_file_name("brayc");
        let workspace_compiler = workspace.path().join(&file_name);
        let search_compiler = search.path().join(&file_name);

        std::fs::write(&workspace_compiler, b"workspace compiler")
            .unwrap_or_else(|error| panic!("workspace compiler fixture should exist: {error}"));

        std::fs::write(&search_compiler, b"search compiler")
            .unwrap_or_else(|error| panic!("search compiler fixture should exist: {error}"));

        make_executable(&workspace_compiler);
        make_executable(&search_compiler);

        let resolved = resolved_tool_path_from_selection(
            PathBuf::from("brayc"),
            search.path().as_os_str(),
            workspace.path(),
        )
        .unwrap_or_else(|error| panic!("bare compiler name should resolve: {error:?}"));

        let selected = if cfg!(windows) {
            workspace_compiler
        } else {
            search_compiler
        };

        let expected = std::fs::canonicalize(selected)
            .unwrap_or_else(|error| panic!("compiler fixture should canonicalize: {error}"));

        assert_eq!(resolved, expected);
    }

    #[cfg(unix)]
    #[test]
    fn bare_tool_name_skips_nonexecutable_search_candidates() {
        let first = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("first search directory should exist: {error}"));

        let second = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("second search directory should exist: {error}"));

        let first_compiler = first.path().join("brayc");
        let second_compiler = second.path().join("brayc");

        std::fs::write(&first_compiler, b"nonexecutable compiler")
            .unwrap_or_else(|error| panic!("first compiler fixture should exist: {error}"));

        std::fs::write(&second_compiler, b"executable compiler")
            .unwrap_or_else(|error| panic!("second compiler fixture should exist: {error}"));

        make_executable(&second_compiler);

        let search = std::env::join_paths([first.path(), second.path()])
            .unwrap_or_else(|error| panic!("search path should join: {error}"));

        let resolved = resolved_tool_path_from_selection(
            PathBuf::from("brayc"),
            &search,
            Path::new("workspace"),
        )
        .unwrap_or_else(|error| panic!("executable compiler should resolve: {error:?}"));

        let expected = std::fs::canonicalize(second_compiler)
            .unwrap_or_else(|error| panic!("compiler fixture should canonicalize: {error}"));

        assert_eq!(resolved, expected);
    }

    fn make_executable(path: &Path) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let permissions = std::fs::Permissions::from_mode(0o755);

            std::fs::set_permissions(path, permissions)
                .unwrap_or_else(|error| panic!("compiler fixture should be executable: {error}"));
        }

        #[cfg(not(unix))]
        let _ = path;
    }
}
