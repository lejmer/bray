use std::env;
use std::path::{Path, PathBuf};

use bray_diagnostics::DiagnosticLlvmToolRole;

/// Locates one executable in the configured or bundled LLVM toolchain.
pub fn llvm_tool_path(tool: DiagnosticLlvmToolRole) -> Result<PathBuf, LlvmToolPathError> {
    let name = tool.executable_name();

    let executable_name = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    };

    let configured = env::var_os(bray_codegen_llvm::LLVM_PREFIX_ENVIRONMENT_VARIABLE)
        .map(PathBuf::from)
        .or_else(|| bray_codegen_llvm::COMPILED_LLVM_PREFIX.map(PathBuf::from))
        .map(|prefix| prefix.join("bin").join(&executable_name));

    if let Some(path) = configured.as_deref() {
        if probe_tool_candidate(tool, path)? {
            return Ok(path.to_path_buf());
        }
    }

    let executable = env::current_exe().map_err(|error| LlvmToolPathError::CurrentExecutable {
        tool,
        error: error.kind(),
    })?;

    executable
        .ancestors()
        .map(|ancestor| {
            ancestor
                .join("toolchains")
                .join("llvm")
                .join("active")
                .join("bin")
                .join(&executable_name)
        })
        .find_map(|path| match probe_tool_candidate(tool, &path) {
            Ok(true) => Some(Ok(path)),
            Ok(false) => None,
            Err(error) => Some(Err(error)),
        })
        .unwrap_or_else(|| Err(LlvmToolPathError::Unavailable { tool, configured }))
}

/// Exact failure while resolving one executable from the LLVM installation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LlvmToolPathError {
    /// The host could not resolve the running executable used to locate bundled tools.
    CurrentExecutable {
        /// Exact requested LLVM tool role.
        tool: DiagnosticLlvmToolRole,
        /// Stable host I/O failure category.
        error: std::io::ErrorKind,
    },
    /// Neither the configured installation nor the bundled installation contains the tool.
    Unavailable {
        /// Exact requested LLVM tool role.
        tool: DiagnosticLlvmToolRole,
        /// Configured candidate path when an LLVM prefix was selected.
        configured: Option<PathBuf>,
    },
    /// The host could not inspect a concrete candidate path.
    CandidateInspection {
        /// Exact requested LLVM tool role.
        tool: DiagnosticLlvmToolRole,
        /// Exact candidate path inspected by the compiler.
        path: PathBuf,
        /// Stable host I/O failure category.
        error: std::io::ErrorKind,
    },
    /// A concrete candidate is not a regular executable file.
    InvalidCandidate {
        /// Exact requested LLVM tool role.
        tool: DiagnosticLlvmToolRole,
        /// Exact candidate path that violated the tool contract.
        path: PathBuf,
    },
}

fn probe_tool_candidate(
    tool: DiagnosticLlvmToolRole,
    path: &Path,
) -> Result<bool, LlvmToolPathError> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(LlvmToolPathError::CandidateInspection {
                tool,
                path: path.to_path_buf(),
                error: error.kind(),
            });
        }
    };

    if !metadata.is_file() || !host_file_is_executable(&metadata) {
        return Err(LlvmToolPathError::InvalidCandidate {
            tool,
            path: path.to_path_buf(),
        });
    }

    Ok(true)
}

#[cfg(unix)]
fn host_file_is_executable(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt as _;

    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
const fn host_file_is_executable(_metadata: &std::fs::Metadata) -> bool {
    true
}

impl std::fmt::Display for LlvmToolPathError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CurrentExecutable { tool, error } => write!(
                formatter,
                "could not locate the bundled {} from the current executable: {error}",
                tool.executable_name()
            ),
            Self::Unavailable { tool, configured } => {
                if let Some(path) = configured {
                    write!(
                        formatter,
                        "{} is absent at {} and from the bundled toolchain",
                        tool.executable_name(),
                        path.display()
                    )
                } else {
                    write!(
                        formatter,
                        "{} is absent from the bundled toolchain",
                        tool.executable_name()
                    )
                }
            }
            Self::CandidateInspection { tool, path, error } => write!(
                formatter,
                "could not inspect {} candidate {}: {error}",
                tool.executable_name(),
                path.display()
            ),
            Self::InvalidCandidate { tool, path } => write!(
                formatter,
                "{} candidate {} is not a regular executable file",
                tool.executable_name(),
                path.display()
            ),
        }
    }
}

impl std::error::Error for LlvmToolPathError {}
