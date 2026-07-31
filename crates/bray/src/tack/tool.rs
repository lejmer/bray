use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use bray_platform::{NativeProcessCommand, NativeStdio};

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
        self.arguments
            .extend(arguments.into_iter().map(Into::into));

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
    fn capture(&self, request: ToolRequest) -> Result<ToolOutput, ()>;

    fn serve(
        &self,
        request: ToolRequest,
        input: Box<dyn Read + Send>,
        output: &mut dyn Write,
    ) -> Result<ToolOutput, ()>;
}

pub(crate) struct NativeToolExecutor;

impl ToolExecutor for NativeToolExecutor {
    fn capture(&self, request: ToolRequest) -> Result<ToolOutput, ()> {
        let command = native_command(&request)?;

        let output = command
            .capture(request.input)
            .map_err(|_| ())?;

        let (status, stdout, stderr) = output.into_parts();

        Ok(ToolOutput::new(
            status.success(),
            String::from_utf8_lossy(&stdout).into_owned(),
            String::from_utf8_lossy(&stderr).into_owned(),
        ))
    }

    fn serve(
        &self,
        request: ToolRequest,
        input: Box<dyn Read + Send>,
        output: &mut dyn Write,
    ) -> Result<ToolOutput, ()> {
        let mut command = native_command(&request)?;

        command
            .stdin(NativeStdio::Piped)
            .stdout(NativeStdio::Piped)
            .stderr(NativeStdio::Piped);

        let mut child = command.spawn().map_err(|_| ())?;
        let mut child_input = child.take_stdin().ok_or(())?;
        let mut child_output = child.take_stdout().ok_or(())?;
        let mut child_error = child.take_stderr().ok_or(())?;

        // Detach this pump because the editor may keep its input open after the server exits.
        let _input_thread = std::thread::spawn(move || {
            let mut input = input;

            std::io::copy(&mut input, &mut child_input)
        });

        let error_thread = std::thread::spawn(move || {
            let mut bytes = Vec::new();

            child_error.read_to_end(&mut bytes).map(|_| bytes)
        });

        if std::io::copy(&mut child_output, output).is_err() {
            let _ = child.terminate();
            let _ = child.wait();
            let _ = error_thread.join();

            return Err(());
        }

        let status = child.wait().map_err(|_| ())?;

        let stderr = error_thread.join().map_err(|_| ())?.map_err(|_| ())?;

        Ok(ToolOutput::new(
            status.success(),
            String::new(),
            String::from_utf8_lossy(&stderr).into_owned(),
        ))
    }
}

fn native_command(request: &ToolRequest) -> Result<NativeProcessCommand, ()> {
    let program = tool_path(request.tool);
    let mut command = NativeProcessCommand::new(&program).map_err(|_| ())?;

    command.current_dir(&request.working_directory);

    for argument in &request.arguments {
        command.arg(argument);
    }

    Ok(command)
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
