use std::path::{Path, PathBuf};
#[cfg(feature = "compiler")]
use std::{env, ffi::OsString, num::NonZeroUsize, sync::Arc};

#[cfg(feature = "compiler")]
use bray_codegen::{
    BackendSelectionError, CodeGenerator, CodeGeneratorRegistry, CodeGeneratorRegistryBuildError,
    CodegenConfiguration, CodegenFailure, codegen_failure_diagnostic,
};
#[cfg(feature = "compiler")]
use bray_codegen_llvm::LlvmCodeGenerator;
use bray_compilation::{
    Compilation, CompilationLoadError, CompilationRequest, PackageInterfaceExportRequest,
    SelectedTarget,
};
#[cfg(feature = "compiler")]
use bray_diagnostics::{Diagnostic, DiagnosticArg, DiagnosticId, DiagnosticKind, SeverityKind};
#[cfg(feature = "compiler")]
use bray_diagnostics::{
    DiagnosticHostEnvironmentVariable, DiagnosticInvocationBuildFailure, DiagnosticIoErrorKind,
    DiagnosticLinkerCapabilityBuildFailure, DiagnosticLlvmToolRole,
    DiagnosticNativeLinkerBuildFailure, DiagnosticProjectCommandFailure,
    DiagnosticProjectOperation, DiagnosticUnsupportedEmissionReason,
};
#[cfg(feature = "compiler")]
use bray_linker::{
    ExternalToolHost, ExternalToolInvocationBuildError, ExternalToolProcessBudget, Linker,
    LinkerBuildError, LinkerDriver, LinkerDriverCapabilitiesBuildError, LinkerDriverIdentity,
    LinkerDriverKind, LinkerTargetIdentity, LlvmArchiveDriver, LlvmArchiveDriverBuildError,
    NativeExternalToolHost, SystemLinkerConfiguration, SystemLinkerConfigurationBuildError,
    SystemLinkerDriver, SystemLinkerDriverBuildError, SystemLinkerFamily,
};
use bray_package_interface::{
    InterfaceLanguageRevision, InterfaceProductIdentity, InterfaceProductKind,
    PackageInterfaceIdentity,
};
use bray_project::ProjectGraph;
use bray_symbols::{PackageVersion, ProductIdentity};
use bray_target::TargetIdentity;
#[cfg(feature = "compiler")]
use bray_target::{NativeTarget, ObjectFormat};

/// Loads a compilation without a code-generation backend.
pub fn load_compilation(request: CompilationRequest) -> Result<Compilation, CompilationLoadError> {
    Compilation::load(request)
}

/// Loads a compilation configured with the LLVM code-generation backend.
#[cfg(feature = "compiler")]
pub fn load_llvm_compilation(
    request: CompilationRequest,
) -> Result<Compilation, LlvmCompilationLoadError> {
    let generator =
        LlvmCodeGenerator::try_new().map_err(LlvmCompilationLoadError::BackendInitialization)?;

    let identity = generator.identity().clone();

    let registry = CodeGeneratorRegistry::try_new([Arc::new(generator) as Arc<dyn CodeGenerator>])
        .map_err(LlvmCompilationLoadError::Registry)?;

    let codegen = CodegenConfiguration::try_new(registry, identity)
        .map_err(LlvmCompilationLoadError::BackendSelection)?;

    Compilation::load_with_codegen(request, codegen).map_err(LlvmCompilationLoadError::Compilation)
}

/// Exact failure while constructing an LLVM-backed compilation.
#[cfg(feature = "compiler")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LlvmCompilationLoadError {
    /// The LLVM backend could not initialize in the current process.
    BackendInitialization(CodegenFailure),
    /// The backend registry rejected the LLVM backend identity.
    Registry(CodeGeneratorRegistryBuildError),
    /// The LLVM backend could not be selected from the constructed registry.
    BackendSelection(BackendSelectionError),
    /// Durable compilation state could not be loaded after backend construction.
    Compilation(CompilationLoadError),
}

#[cfg(feature = "compiler")]
impl std::fmt::Display for LlvmCompilationLoadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BackendInitialization(failure) => {
                write!(formatter, "LLVM backend initialization failed: {failure:?}")
            }
            Self::Registry(CodeGeneratorRegistryBuildError::DuplicateIdentity(identity)) => {
                write!(
                    formatter,
                    "LLVM backend identity {} is registered more than once",
                    identity.name()
                )
            }
            Self::BackendSelection(BackendSelectionError::Unavailable(identity)) => write!(
                formatter,
                "LLVM backend {} is unavailable after registry construction",
                identity.name()
            ),
            Self::BackendSelection(BackendSelectionError::NotSelected(identity)) => write!(
                formatter,
                "LLVM backend {} was not selected after registry construction",
                identity.name()
            ),
            Self::Compilation(error) => write!(formatter, "{error}"),
        }
    }
}

#[cfg(feature = "compiler")]
impl std::error::Error for LlvmCompilationLoadError {}

#[cfg(feature = "compiler")]
impl LlvmCompilationLoadError {
    /// Converts this construction failure into a locale-neutral terminal diagnostic.
    pub fn diagnostic(self, target: &str) -> Diagnostic {
        match self {
            Self::BackendInitialization(failure) => {
                codegen_initialization_diagnostic(failure, target)
            }
            Self::Registry(CodeGeneratorRegistryBuildError::DuplicateIdentity(identity)) => {
                codegen_configuration_diagnostic(identity.name(), target)
            }
            Self::BackendSelection(error) => {
                let identity = match error {
                    BackendSelectionError::Unavailable(identity)
                    | BackendSelectionError::NotSelected(identity) => identity,
                };

                codegen_configuration_diagnostic(identity.name(), target)
            }
            Self::Compilation(error) => error.diagnostic(),
        }
    }
}

#[cfg(feature = "compiler")]
fn codegen_initialization_diagnostic(failure: CodegenFailure, target: &str) -> Diagnostic {
    codegen_failure_diagnostic("llvm", target, &failure)
}

#[cfg(feature = "compiler")]
fn codegen_configuration_diagnostic(backend: &str, target: &str) -> Diagnostic {
    codegen_diagnostic(DiagnosticKind::CodegenInvalidConfiguration, backend, target)
}

#[cfg(feature = "compiler")]
fn codegen_diagnostic(kind: DiagnosticKind, backend: &str, target: &str) -> Diagnostic {
    Diagnostic::new(DiagnosticId::new(0), kind, SeverityKind::Error)
        .with_arg(DiagnosticArg::referenced_name(backend))
        .with_arg(DiagnosticArg::target_triple(target))
}

/// Resolves a compiler target from one exact project-selected target identity.
pub fn selected_target(identity: &TargetIdentity) -> Option<SelectedTarget> {
    SelectedTarget::for_identity(identity)
}

/// Creates the package-interface export request for a library product.
pub fn package_interface_export_request(
    product: ProductIdentity,
    version: &PackageVersion,
) -> PackageInterfaceExportRequest {
    let interface_product = InterfaceProductIdentity::try_new(product.name())
        .unwrap_or_else(|| panic!("the product identity must be valid"));

    // Export metadata shares the immutable Arc-backed package version.
    let identity = PackageInterfaceIdentity::try_new(
        product.package().clone(),
        version.clone(),
        interface_product,
        InterfaceProductKind::Library,
        "public-v1",
    )
    .unwrap_or_else(|| panic!("the public-surface identity must be valid"));

    PackageInterfaceExportRequest::new(identity, InterfaceLanguageRevision::new(0))
}

/// Returns one product's deterministic output directory.
pub fn project_output_directory(
    graph: &ProjectGraph,
    workspace_root: &Path,
    product: &ProductIdentity,
    target_name: &str,
) -> PathBuf {
    graph
        .output_root()
        .beneath(workspace_root)
        .join(target_name)
        .join(product.package().as_str())
        .join(product.name())
}

/// Returns the package-interface path for one product and selected target.
pub fn project_interface_path(
    graph: &ProjectGraph,
    workspace_root: &Path,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> Option<PathBuf> {
    let target_name = graph
        .targets()
        .iter()
        .find(|candidate| candidate.identity() == target)
        .map(bray_project::ProjectTarget::name)?;

    Some(
        project_output_directory(graph, workspace_root, product, target_name)
            .join(format!("{}.brayi", product.name())),
    )
}

/// Creates the linker composition for one native target and optional linker-map destination.
#[cfg(feature = "compiler")]
pub fn native_linker(
    target: NativeTarget,
    map_output: Option<bray_linker::SystemLinkerMapOutput>,
) -> Result<Linker, NativeLinkerBuildError> {
    let archive =
        llvm_tool_path(DiagnosticLlvmToolRole::Archiver).map_err(NativeLinkerBuildError::Tool)?;

    let host = Arc::new(NativeExternalToolHost::new(ExternalToolProcessBudget::new(
        NonZeroUsize::MIN,
    )));

    let archive_identity =
        LinkerDriverIdentity::try_new(LinkerDriverKind::Archiver, "llvm-ar", "1", "22")
            .ok_or(NativeLinkerBuildError::ArchiveIdentity)?;

    let archive = LlvmArchiveDriver::try_new(
        archive_identity,
        archive,
        [],
        None,
        Arc::clone(&host) as Arc<dyn ExternalToolHost>,
    )
    .map_err(NativeLinkerBuildError::ArchiveDriver)?;

    let mut drivers = vec![Arc::new(archive) as Arc<dyn LinkerDriver>];

    if let Some((family, program, environment)) = system_linker_configuration(target)? {
        let system_identity = LinkerDriverIdentity::try_new(
            LinkerDriverKind::System,
            system_linker_name(family),
            "1",
            "1",
        )
        .ok_or(NativeLinkerBuildError::SystemIdentity)?;

        let linker_target = LinkerTargetIdentity::try_new(
            target.identity(),
            target.as_str(),
            target.architecture(),
            target.object_format(),
        )
        .ok_or(NativeLinkerBuildError::TargetIdentity)?;

        let mut configuration =
            SystemLinkerConfiguration::try_new(family, linker_target, program, environment, None)
                .map_err(NativeLinkerBuildError::SystemConfiguration)?;

        if let Some(output) = map_output {
            configuration = configuration.with_map_output(output);
        }

        let system = SystemLinkerDriver::try_new(
            system_identity,
            configuration,
            Arc::clone(&host) as Arc<dyn ExternalToolHost>,
        )
        .map_err(NativeLinkerBuildError::SystemDriver)?;

        drivers.push(Arc::new(system) as Arc<dyn LinkerDriver>);
    }

    Linker::try_new(drivers).map_err(NativeLinkerBuildError::Linker)
}

/// Exact failure while composing the native linker and archiver available to the compiler host.
#[cfg(feature = "compiler")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeLinkerBuildError {
    /// A required LLVM tool could not be resolved from the configured or bundled installation.
    Tool(LlvmToolPathError),
    /// The compiler-owned archiver identity violated the linker identity contract.
    ArchiveIdentity,
    /// The LLVM archive driver rejected its compiler-owned configuration.
    ArchiveDriver(LlvmArchiveDriverBuildError),
    /// The compiler-owned system-linker identity violated the linker identity contract.
    SystemIdentity,
    /// The selected target could not form a linker target identity.
    TargetIdentity,
    /// The selected system-linker command violated its configuration contract.
    SystemConfiguration(SystemLinkerConfigurationBuildError),
    /// The system-linker driver rejected its compiler-owned configuration.
    SystemDriver(SystemLinkerDriverBuildError),
    /// The complete compiler-host driver registry is invalid.
    Linker(LinkerBuildError),
    /// A required host environment variable is absent.
    MissingEnvironment(DiagnosticHostEnvironmentVariable),
}

#[cfg(feature = "compiler")]
impl NativeLinkerBuildError {
    /// Converts this host-toolchain composition failure into a terminal structured diagnostic.
    pub fn diagnostic(self, target: &str) -> Diagnostic {
        match self {
            Self::Tool(LlvmToolPathError::CurrentExecutable { error, .. }) => {
                project_command_diagnostic(DiagnosticProjectCommandFailure::CurrentExecutable {
                    operation: DiagnosticProjectOperation::ToolchainExecutable,
                    error: DiagnosticIoErrorKind::from(error),
                })
            }
            Self::Tool(LlvmToolPathError::Unavailable { tool, configured }) => {
                unsupported_emission_diagnostic(
                    target,
                    DiagnosticUnsupportedEmissionReason::ToolUnavailable { tool, configured },
                )
            }
            Self::Tool(LlvmToolPathError::CandidateInspection { tool, path, error }) => {
                unsupported_emission_diagnostic(
                    target,
                    DiagnosticUnsupportedEmissionReason::ToolInspectionFailed {
                        tool,
                        path,
                        error: DiagnosticIoErrorKind::from(error),
                    },
                )
            }
            Self::Tool(LlvmToolPathError::InvalidCandidate { tool, path }) => {
                unsupported_emission_diagnostic(
                    target,
                    DiagnosticUnsupportedEmissionReason::InvalidToolFile { tool, path },
                )
            }
            Self::MissingEnvironment(variable) => unsupported_emission_diagnostic(
                target,
                DiagnosticUnsupportedEmissionReason::MissingHostEnvironment(variable),
            ),
            Self::ArchiveIdentity => native_linker_defect_diagnostic(
                target,
                DiagnosticNativeLinkerBuildFailure::ArchiveIdentity,
            ),
            Self::ArchiveDriver(error) => {
                native_linker_defect_diagnostic(target, archive_driver_build_failure(error))
            }
            Self::SystemIdentity => native_linker_defect_diagnostic(
                target,
                DiagnosticNativeLinkerBuildFailure::SystemIdentity,
            ),
            Self::TargetIdentity => native_linker_defect_diagnostic(
                target,
                DiagnosticNativeLinkerBuildFailure::TargetIdentity,
            ),
            Self::SystemConfiguration(error) => {
                native_linker_defect_diagnostic(target, system_configuration_build_failure(error))
            }
            Self::SystemDriver(error) => {
                native_linker_defect_diagnostic(target, system_driver_build_failure(error))
            }
            Self::Linker(LinkerBuildError::DuplicateDriver) => native_linker_defect_diagnostic(
                target,
                DiagnosticNativeLinkerBuildFailure::DuplicateDriver,
            ),
        }
    }
}

#[cfg(feature = "compiler")]
fn native_linker_defect_diagnostic(
    target: &str,
    failure: DiagnosticNativeLinkerBuildFailure,
) -> Diagnostic {
    project_command_diagnostic(DiagnosticProjectCommandFailure::NativeLinkerConstruction {
        target: target.to_owned(),
        failure,
    })
}

#[cfg(feature = "compiler")]
const fn archive_driver_build_failure(
    error: LlvmArchiveDriverBuildError,
) -> DiagnosticNativeLinkerBuildFailure {
    match error {
        LlvmArchiveDriverBuildError::DriverKindMismatch => {
            DiagnosticNativeLinkerBuildFailure::ArchiveDriverKindMismatch
        }
        LlvmArchiveDriverBuildError::Capabilities(error) => {
            DiagnosticNativeLinkerBuildFailure::ArchiveCapabilities(
                linker_capability_build_failure(error),
            )
        }
        LlvmArchiveDriverBuildError::ProgramPathNotExplicit => {
            DiagnosticNativeLinkerBuildFailure::ArchiveProgramPathNotExplicit
        }
        LlvmArchiveDriverBuildError::Invocation(error) => {
            DiagnosticNativeLinkerBuildFailure::ArchiveInvocation(invocation_build_failure(error))
        }
    }
}

#[cfg(feature = "compiler")]
const fn system_configuration_build_failure(
    error: SystemLinkerConfigurationBuildError,
) -> DiagnosticNativeLinkerBuildFailure {
    match error {
        SystemLinkerConfigurationBuildError::ProgramPathNotExplicit => {
            DiagnosticNativeLinkerBuildFailure::SystemProgramPathNotExplicit
        }
        SystemLinkerConfigurationBuildError::Invocation(error) => {
            DiagnosticNativeLinkerBuildFailure::SystemInvocation(invocation_build_failure(error))
        }
    }
}

#[cfg(feature = "compiler")]
const fn system_driver_build_failure(
    error: SystemLinkerDriverBuildError,
) -> DiagnosticNativeLinkerBuildFailure {
    match error {
        SystemLinkerDriverBuildError::DriverKindMismatch => {
            DiagnosticNativeLinkerBuildFailure::SystemDriverKindMismatch
        }
        SystemLinkerDriverBuildError::Capabilities(error) => {
            DiagnosticNativeLinkerBuildFailure::SystemCapabilities(linker_capability_build_failure(
                error,
            ))
        }
    }
}

#[cfg(feature = "compiler")]
const fn linker_capability_build_failure(
    failure: LinkerDriverCapabilitiesBuildError,
) -> DiagnosticLinkerCapabilityBuildFailure {
    match failure {
        LinkerDriverCapabilitiesBuildError::DriverKindMismatch => {
            DiagnosticLinkerCapabilityBuildFailure::DriverKindMismatch
        }
        LinkerDriverCapabilitiesBuildError::UnsupportedTarget => {
            DiagnosticLinkerCapabilityBuildFailure::UnsupportedTarget
        }
        LinkerDriverCapabilitiesBuildError::MissingTargets => {
            DiagnosticLinkerCapabilityBuildFailure::MissingTargets
        }
        LinkerDriverCapabilitiesBuildError::DuplicateTarget => {
            DiagnosticLinkerCapabilityBuildFailure::DuplicateTarget
        }
    }
}

#[cfg(feature = "compiler")]
const fn invocation_build_failure(
    failure: ExternalToolInvocationBuildError,
) -> DiagnosticInvocationBuildFailure {
    match failure {
        ExternalToolInvocationBuildError::EmptyProgram => {
            DiagnosticInvocationBuildFailure::EmptyProgram
        }
        ExternalToolInvocationBuildError::EmptyCurrentDirectory => {
            DiagnosticInvocationBuildFailure::EmptyCurrentDirectory
        }
        ExternalToolInvocationBuildError::EmptyEnvironmentVariableName => {
            DiagnosticInvocationBuildFailure::EmptyEnvironmentVariableName
        }
        ExternalToolInvocationBuildError::DuplicateEnvironmentVariableName => {
            DiagnosticInvocationBuildFailure::DuplicateEnvironmentVariableName
        }
        ExternalToolInvocationBuildError::DuplicateResponseFile => {
            DiagnosticInvocationBuildFailure::DuplicateResponseFile
        }
    }
}

#[cfg(feature = "compiler")]
fn unsupported_emission_diagnostic(
    target: &str,
    reason: DiagnosticUnsupportedEmissionReason,
) -> Diagnostic {
    Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::RequestUnsupportedProductEmission,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::target_triple(target))
    .with_arg(DiagnosticArg::unsupported_emission_reason(reason))
}

#[cfg(feature = "compiler")]
fn project_command_diagnostic(failure: DiagnosticProjectCommandFailure) -> Diagnostic {
    failure.diagnostic(DiagnosticId::new(0))
}

#[cfg(feature = "compiler")]
fn system_linker_configuration(
    target: NativeTarget,
) -> Result<Option<(SystemLinkerFamily, PathBuf, Vec<(OsString, OsString)>)>, NativeLinkerBuildError>
{
    if NativeTarget::current().is_none_or(|host| host.architecture() != target.architecture()) {
        return Ok(None);
    }

    match target.object_format() {
        ObjectFormat::Elf if cfg!(windows) => {
            let root = env::var_os("SystemRoot").map(PathBuf::from).ok_or(
                NativeLinkerBuildError::MissingEnvironment(
                    DiagnosticHostEnvironmentVariable::WindowsSystemRoot,
                ),
            )?;

            Ok(Some((
                SystemLinkerFamily::WslGnuCompiler,
                root.join("System32").join("wsl.exe"),
                selected_environment(&[
                    "SystemRoot",
                    "USERPROFILE",
                    "LOCALAPPDATA",
                    "WSLENV",
                    "PATH",
                    "TEMP",
                    "TMP",
                ]),
            )))
        }
        ObjectFormat::Elf if cfg!(target_os = "linux") => Ok(Some((
            SystemLinkerFamily::GnuCompiler,
            PathBuf::from("/usr/bin/cc"),
            Vec::new(),
        ))),
        ObjectFormat::Coff if cfg!(windows) => Ok(Some((
            SystemLinkerFamily::MicrosoftCompiler,
            llvm_tool_path(DiagnosticLlvmToolRole::CompilerDriver)
                .map_err(NativeLinkerBuildError::Tool)?,
            selected_environment(&[
                "SystemRoot",
                "USERPROFILE",
                "LOCALAPPDATA",
                "PATH",
                "LIB",
                "INCLUDE",
                "TEMP",
                "TMP",
            ]),
        ))),
        ObjectFormat::MachO if cfg!(target_os = "macos") => Ok(Some((
            SystemLinkerFamily::AppleCompiler,
            PathBuf::from("/usr/bin/clang"),
            selected_environment(&["HOME", "PATH", "SDKROOT", "DEVELOPER_DIR", "TMPDIR"]),
        ))),
        ObjectFormat::Coff
        | ObjectFormat::Elf
        | ObjectFormat::MachO
        | ObjectFormat::WebAssembly
        | ObjectFormat::Xcoff => Ok(None),
    }
}

#[cfg(feature = "compiler")]
fn selected_environment(names: &[&str]) -> Vec<(OsString, OsString)> {
    names
        .iter()
        .filter_map(|name| env::var_os(name).map(|value| ((*name).into(), value)))
        .collect()
}

#[cfg(feature = "compiler")]
const fn system_linker_name(family: SystemLinkerFamily) -> &'static str {
    match family {
        SystemLinkerFamily::GnuCompiler => "gnu-compiler",
        SystemLinkerFamily::WslGnuCompiler => "wsl-gnu-compiler",
        SystemLinkerFamily::MicrosoftCompiler => "microsoft-compiler",
        SystemLinkerFamily::AppleCompiler => "apple-compiler",
        SystemLinkerFamily::Gnu | SystemLinkerFamily::Microsoft | SystemLinkerFamily::Apple => {
            "system-linker"
        }
    }
}

#[cfg(feature = "compiler")]
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
#[cfg(feature = "compiler")]
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

#[cfg(feature = "compiler")]
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

#[cfg(all(feature = "compiler", unix))]
fn host_file_is_executable(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt as _;

    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(all(feature = "compiler", not(unix)))]
const fn host_file_is_executable(_metadata: &std::fs::Metadata) -> bool {
    true
}

#[cfg(feature = "compiler")]
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

#[cfg(feature = "compiler")]
impl std::error::Error for LlvmToolPathError {}

#[cfg(all(test, feature = "compiler"))]
mod tests {
    use std::path::PathBuf;

    use bray_diagnostics::{
        DiagnosticHostEnvironmentVariable, DiagnosticKind, DiagnosticLlvmToolRole,
    };
    use bray_messages::DiagnosticRenderer;
    use bray_testing::assert_goal_state_diagnostic_kind;

    use super::{LlvmToolPathError, NativeLinkerBuildError};

    #[test]
    fn native_tool_availability_failures_preserve_each_exact_reason() {
        let failures = [
            (
                NativeLinkerBuildError::Tool(LlvmToolPathError::Unavailable {
                    tool: DiagnosticLlvmToolRole::Archiver,
                    configured: Some(PathBuf::from("toolchain/bin/llvm-ar")),
                }),
                "LLVM archiver",
            ),
            (
                NativeLinkerBuildError::Tool(LlvmToolPathError::CandidateInspection {
                    tool: DiagnosticLlvmToolRole::CompilerDriver,
                    path: PathBuf::from("toolchain/bin/clang"),
                    error: std::io::ErrorKind::PermissionDenied,
                }),
                "permission denied",
            ),
            (
                NativeLinkerBuildError::Tool(LlvmToolPathError::InvalidCandidate {
                    tool: DiagnosticLlvmToolRole::SymbolInspector,
                    path: PathBuf::from("toolchain/bin/llvm-nm"),
                }),
                "regular executable file",
            ),
            (
                NativeLinkerBuildError::MissingEnvironment(
                    DiagnosticHostEnvironmentVariable::WindowsSystemRoot,
                ),
                "SystemRoot",
            ),
        ];

        for (failure, expected) in failures {
            let diagnostic = failure.diagnostic("x86_64-pc-windows-msvc");
            let bag = bray_diagnostics::DiagnosticBag::single(diagnostic.clone());

            assert_goal_state_diagnostic_kind(
                &bag,
                DiagnosticKind::RequestUnsupportedProductEmission,
            );

            let rendered = DiagnosticRenderer::english().render(&diagnostic);
            assert!(rendered.message().contains(expected));
        }
    }
}
