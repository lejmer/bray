use std::ffi::OsStr;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{
    Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Stdio,
};

use crate::{PlatformError, PlatformErrorKind, PlatformOperation};

/// Native child-process stream policy.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NativeStdio {
    /// Inherit the corresponding host stream.
    Inherit,
    /// Attach the host null device.
    Null,
    /// Create an owned parent-child pipe.
    Piped,
}

impl NativeStdio {
    fn into_stdio(self) -> Stdio {
        match self {
            Self::Inherit => Stdio::inherit(),
            Self::Null => Stdio::null(),
            Self::Piped => Stdio::piped(),
        }
    }
}

/// Owned child-process construction request.
#[derive(Debug)]
pub struct NativeProcessCommand {
    command: Command,
    stdin: NativeStdio,
    stdout: NativeStdio,
    stderr: NativeStdio,
}

impl NativeProcessCommand {
    /// Creates a command for a nonempty native program path.
    pub fn new(program: impl AsRef<OsStr>) -> Result<Self, PlatformError> {
        if program.as_ref().is_empty() {
            return Err(PlatformError::new(
                PlatformOperation::ProcessSpawn,
                PlatformErrorKind::Io(std::io::ErrorKind::InvalidInput),
            ));
        }

        Ok(Self {
            command: Command::new(program),
            stdin: NativeStdio::Inherit,
            stdout: NativeStdio::Inherit,
            stderr: NativeStdio::Inherit,
        })
    }

    /// Appends one argument without shell interpretation.
    pub fn arg(&mut self, argument: impl AsRef<OsStr>) -> &mut Self {
        self.command.arg(argument);

        self
    }

    /// Sets one child environment variable.
    pub fn env(
        &mut self,
        name: impl AsRef<OsStr>,
        value: impl AsRef<OsStr>,
    ) -> &mut Self {
        self.command.env(name, value);

        self
    }

    /// Removes inherited environment variables from the child environment.
    pub fn env_clear(&mut self) -> &mut Self {
        self.command.env_clear();

        self
    }

    /// Sets the child working directory.
    pub fn current_dir(&mut self, directory: impl AsRef<Path>) -> &mut Self {
        self.command.current_dir(directory);

        self
    }

    /// Selects the child standard-input policy.
    pub const fn stdin(&mut self, policy: NativeStdio) -> &mut Self {
        self.stdin = policy;

        self
    }

    /// Selects the child standard-output policy.
    pub const fn stdout(&mut self, policy: NativeStdio) -> &mut Self {
        self.stdout = policy;

        self
    }

    /// Selects the child standard-error policy.
    pub const fn stderr(&mut self, policy: NativeStdio) -> &mut Self {
        self.stderr = policy;

        self
    }

    /// Creates the child and transfers stream ownership to the returned handle.
    pub fn spawn(mut self) -> Result<NativeChildProcess, PlatformError> {
        self.command.stdin(self.stdin.into_stdio());
        self.command.stdout(self.stdout.into_stdio());
        self.command.stderr(self.stderr.into_stdio());

        let child = self.command.spawn().map_err(|error| {
            PlatformError::from_io(PlatformOperation::ProcessSpawn, &error)
        })?;

        Ok(NativeChildProcess { child })
    }
}

/// Owned native child-process handle.
///
/// Dropping this handle closes parent-owned handles but does not terminate the child. Runtime and
/// standard-library owners must explicitly wait, terminate, or deliberately detach it.
#[derive(Debug)]
pub struct NativeChildProcess {
    child: Child,
}

impl NativeChildProcess {
    /// Returns the host process identity.
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    /// Takes the parent writer connected to piped child input.
    pub fn take_stdin(&mut self) -> Option<NativePipeWriter> {
        self.child.stdin.take().map(NativePipeWriter)
    }

    /// Takes the parent reader connected to piped child output.
    pub fn take_stdout(&mut self) -> Option<NativePipeReader> {
        self.child.stdout.take().map(NativePipeReader::Stdout)
    }

    /// Takes the parent reader connected to piped child error output.
    pub fn take_stderr(&mut self) -> Option<NativePipeReader> {
        self.child.stderr.take().map(NativePipeReader::Stderr)
    }

    /// Requests forceful host termination.
    pub fn terminate(&mut self) -> Result<(), PlatformError> {
        self.child.kill().map_err(|error| {
            PlatformError::from_io(PlatformOperation::ProcessSignal, &error)
        })
    }

    /// Reaps the child after it terminates.
    pub fn wait(&mut self) -> Result<NativeExitStatus, PlatformError> {
        self.child
            .wait()
            .map(NativeExitStatus)
            .map_err(|error| {
                PlatformError::from_io(PlatformOperation::ProcessWait, &error)
            })
    }

    /// Observes termination without blocking and reaps a completed child.
    pub fn try_wait(&mut self) -> Result<Option<NativeExitStatus>, PlatformError> {
        self.child
            .try_wait()
            .map(|status| status.map(NativeExitStatus))
            .map_err(|error| {
                PlatformError::from_io(PlatformOperation::ProcessWait, &error)
            })
    }
}

/// Native child exit status without host-authored text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeExitStatus(ExitStatus);

impl NativeExitStatus {
    /// Returns whether the child reported successful termination.
    pub fn success(self) -> bool {
        self.0.success()
    }

    /// Returns the portable numeric exit code when available.
    pub fn code(self) -> Option<i32> {
        self.0.code()
    }
}

/// Owned parent writer connected to child standard input.
#[derive(Debug)]
pub struct NativePipeWriter(ChildStdin);

impl Write for NativePipeWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.0.write(buffer)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

/// Owned parent reader connected to child output.
#[derive(Debug)]
pub enum NativePipeReader {
    /// Child standard output.
    Stdout(ChildStdout),
    /// Child standard error.
    Stderr(ChildStderr),
}

impl Read for NativePipeReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Stdout(reader) => reader.read(buffer),
            Self::Stderr(reader) => reader.read(buffer),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::{NativeProcessCommand, NativeStdio};

    #[test]
    fn child_processes_transfer_piped_output_and_are_reaped() {
        let mut command = if cfg!(target_os = "windows") {
            let mut command = NativeProcessCommand::new("cmd")
                .unwrap_or_else(|error| panic!("test command must be valid: {error:?}"));

            command.arg("/C").arg("echo bray");

            command
        } else {
            let mut command = NativeProcessCommand::new("printf")
                .unwrap_or_else(|error| panic!("test command must be valid: {error:?}"));

            command.arg("bray");

            command
        };

        command.stdout(NativeStdio::Piped);

        let mut child = command
            .spawn()
            .unwrap_or_else(|error| panic!("test child must spawn: {error:?}"));

        let Some(mut output) = child.take_stdout() else {
            panic!("piped output must be owned by the parent");
        };

        let mut bytes = Vec::new();

        output
            .read_to_end(&mut bytes)
            .unwrap_or_else(|error| panic!("test output must be readable: {error:?}"));

        let status = child
            .wait()
            .unwrap_or_else(|error| panic!("test child must be reaped: {error:?}"));

        assert!(status.success());
        assert!(String::from_utf8_lossy(&bytes).contains("bray"));
    }
}
