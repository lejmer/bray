use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(crate) fn is_explicit_program_path(path: &Path) -> bool {
    path.components().count() >= 2
}

/// Driver-owned deterministic response file supplied to an external tool.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalToolResponseFile {
    path: PathBuf,
    contents: Arc<[u8]>,
}

impl ExternalToolResponseFile {
    /// Creates a response file when its exact path is nonempty.
    pub fn try_new(
        path: impl Into<PathBuf>,
        contents: impl Into<Arc<[u8]>>,
    ) -> Result<Self, ExternalToolResponseFileBuildError> {
        let path = path.into();

        if path.as_os_str().is_empty() {
            return Err(ExternalToolResponseFileBuildError::EmptyPath);
        }

        Ok(Self {
            path,
            contents: contents.into(),
        })
    }

    /// Returns the exact path where the compiler host materializes the response file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the exact driver-encoded response-file contents.
    pub fn contents(&self) -> &[u8] {
        &self.contents
    }
}

/// A contract violation that prevents response-file construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalToolResponseFileBuildError {
    /// The response-file path is empty.
    EmptyPath,
}

/// Immutable external-tool invocation without shell interpretation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalToolInvocation {
    program: PathBuf,
    arguments: Arc<[OsString]>,
    environment: Arc<[(OsString, OsString)]>,
    current_directory: Option<PathBuf>,
    response_files: Arc<[ExternalToolResponseFile]>,
}

impl ExternalToolInvocation {
    /// Validates and creates one compiler-host external-tool invocation.
    pub fn try_new(
        program: impl Into<PathBuf>,
        arguments: impl IntoIterator<Item = OsString>,
        environment: impl IntoIterator<Item = (OsString, OsString)>,
        current_directory: Option<PathBuf>,
        response_files: impl IntoIterator<Item = ExternalToolResponseFile>,
    ) -> Result<Self, ExternalToolInvocationBuildError> {
        let program = program.into();

        if program.as_os_str().is_empty() {
            return Err(ExternalToolInvocationBuildError::EmptyProgram);
        }

        if current_directory
            .as_ref()
            .is_some_and(|directory| directory.as_os_str().is_empty())
        {
            return Err(ExternalToolInvocationBuildError::EmptyCurrentDirectory);
        }

        let environment = canonical_environment(environment)?;
        let response_files = canonical_response_files(response_files)?;

        Ok(Self {
            program,
            arguments: arguments.into_iter().collect(),
            environment,
            current_directory,
            response_files,
        })
    }

    /// Returns the exact native executable path.
    pub fn program(&self) -> &Path {
        &self.program
    }

    /// Returns the explicit argument vector in driver-defined order.
    pub fn arguments(&self) -> &[OsString] {
        &self.arguments
    }

    /// Returns the complete child environment in canonical variable-name order.
    pub fn environment(&self) -> &[(OsString, OsString)] {
        &self.environment
    }

    /// Returns the exact child working directory when one is required.
    pub fn current_directory(&self) -> Option<&Path> {
        self.current_directory.as_deref()
    }

    /// Returns response files in canonical path order.
    pub fn response_files(&self) -> &[ExternalToolResponseFile] {
        &self.response_files
    }
}

fn canonical_environment(
    environment: impl IntoIterator<Item = (OsString, OsString)>,
) -> Result<Arc<[(OsString, OsString)]>, ExternalToolInvocationBuildError> {
    let mut environment: Vec<_> = environment.into_iter().collect();

    if environment.iter().any(|(name, _)| name.is_empty()) {
        return Err(ExternalToolInvocationBuildError::EmptyEnvironmentVariableName);
    }

    environment.sort_unstable_by(|left, right| left.0.cmp(&right.0));

    if environment.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(ExternalToolInvocationBuildError::DuplicateEnvironmentVariableName);
    }

    Ok(environment.into())
}

fn canonical_response_files(
    response_files: impl IntoIterator<Item = ExternalToolResponseFile>,
) -> Result<Arc<[ExternalToolResponseFile]>, ExternalToolInvocationBuildError> {
    let mut response_files: Vec<_> = response_files.into_iter().collect();

    response_files.sort_unstable_by(|left, right| left.path.cmp(&right.path));

    if response_files
        .windows(2)
        .any(|pair| pair[0].path == pair[1].path)
    {
        return Err(ExternalToolInvocationBuildError::DuplicateResponseFile);
    }

    Ok(response_files.into())
}

/// A contract violation that prevents external-tool invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalToolInvocationBuildError {
    /// The executable path is empty.
    EmptyProgram,
    /// The selected working directory is empty.
    EmptyCurrentDirectory,
    /// An environment variable name is empty.
    EmptyEnvironmentVariableName,
    /// An environment variable name occurs more than once.
    DuplicateEnvironmentVariableName,
    /// More than one response file uses the same path.
    DuplicateResponseFile,
}

/// Captured result of one completed external tool.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ExternalToolOutput {
    success: bool,
    exit_code: Option<i32>,
    standard_output: Arc<[u8]>,
    standard_error: Arc<[u8]>,
}

impl ExternalToolOutput {
    /// Creates one completed external-tool result.
    pub fn new(
        success: bool,
        exit_code: Option<i32>,
        standard_output: impl Into<Arc<[u8]>>,
        standard_error: impl Into<Arc<[u8]>>,
    ) -> Self {
        Self {
            success,
            exit_code,
            standard_output: standard_output.into(),
            standard_error: standard_error.into(),
        }
    }

    /// Returns whether the process reported successful termination.
    pub const fn success(&self) -> bool {
        self.success
    }

    /// Returns the portable numeric exit code when available.
    pub const fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }

    /// Returns captured standard output without interpreting tool-authored text.
    pub fn standard_output(&self) -> &[u8] {
        &self.standard_output
    }

    /// Returns captured standard error without interpreting tool-authored text.
    pub fn standard_error(&self) -> &[u8] {
        &self.standard_error
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::{OsStr, OsString};
    use std::sync::Arc;

    use super::{
        ExternalToolInvocation, ExternalToolInvocationBuildError, ExternalToolResponseFile,
    };

    #[test]
    fn invocations_canonicalize_environment_and_response_files() {
        let invocation = ExternalToolInvocation::try_new(
            "linker",
            [OsString::from("first"), OsString::from("second")],
            [
                (OsString::from("Z_VAR"), OsString::from("last")),
                (OsString::from("A_VAR"), OsString::from("first")),
            ],
            None,
            [response_file("z.rsp"), response_file("a.rsp")],
        )
        .unwrap_or_else(|error| panic!("test invocation should be valid: {error:?}"));

        assert_eq!(
            invocation.arguments(),
            [OsStr::new("first"), OsStr::new("second")]
        );

        assert_eq!(
            invocation
                .environment()
                .iter()
                .map(|(name, _)| name.as_os_str())
                .collect::<Vec<_>>(),
            [OsStr::new("A_VAR"), OsStr::new("Z_VAR")]
        );

        assert_eq!(
            invocation
                .response_files()
                .iter()
                .map(|file| file.path())
                .collect::<Vec<_>>(),
            [std::path::Path::new("a.rsp"), std::path::Path::new("z.rsp")]
        );
    }

    #[test]
    fn invocations_reject_ambiguous_host_inputs() {
        assert_eq!(
            ExternalToolInvocation::try_new("", [], [], None, [],),
            Err(ExternalToolInvocationBuildError::EmptyProgram)
        );

        assert_eq!(
            ExternalToolInvocation::try_new(
                "linker",
                [],
                [
                    (OsString::from("PATH"), OsString::from("first")),
                    (OsString::from("PATH"), OsString::from("second")),
                ],
                None,
                [],
            ),
            Err(ExternalToolInvocationBuildError::DuplicateEnvironmentVariableName)
        );

        assert_eq!(
            ExternalToolInvocation::try_new(
                "linker",
                [],
                [],
                None,
                [response_file("same.rsp"), response_file("same.rsp")],
            ),
            Err(ExternalToolInvocationBuildError::DuplicateResponseFile)
        );
    }

    fn response_file(path: &str) -> ExternalToolResponseFile {
        ExternalToolResponseFile::try_new(path, Arc::<[u8]>::from(&b"contents"[..]))
            .unwrap_or_else(|error| panic!("test response file should be valid: {error:?}"))
    }
}
