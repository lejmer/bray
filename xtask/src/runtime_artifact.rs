use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Output};

use crate::{digest, workspace};
use bray_base::NonEmptySharedStr;
use bray_runtime_interface::{
    AWAITED_FRAME_COMPOSITION_SYMBOL, BinarySymbolName, CLEANUP_INCIDENT_REPORTING_SYMBOL,
    COMPATIBLE_LANE_SELECTION_SYMBOL, CURRENT_RUN_CANCELLATION_OBSERVATION_SYMBOL,
    ENTRY_FAILURE_REPORTING_SYMBOL, FRAME_COMPLETION_MOVE_SYMBOL, JOIN_REGISTRATION_SYMBOL,
    MAIN_THREAD_LANE_DRIVE_SYMBOL, MAIN_THREAD_LANE_STARTUP_SYMBOL, PANIC_PROPAGATION_SYMBOL,
    PANIC_REPORT_CONSTRUCTION_SYMBOL, PANIC_REPORTING_SYMBOL, PanicAbiIdentity,
    ProtectedFrameAbiVersions, ROOT_CANCELLATION_REQUEST_SYMBOL, ROOT_COMPLETION_RESOLUTION_SYMBOL,
    ROOT_EXECUTION_SYMBOL, ROOT_TERMINAL_OBSERVATION_SYMBOL, RUNTIME_EVENT_SYMBOL, RuntimeAbiRole,
    RuntimeAbiVersion, RuntimeArtifactDigest, RuntimeArtifactId, RuntimeArtifactMetadata,
    RuntimeCapability, RuntimeContract, RuntimeIdentity, RuntimeRoleBinding,
    RuntimeRoleImplementation, STRUCTURED_SHUTDOWN_SYMBOL, SUSPENSION_REGISTRATION_SYMBOL,
    SYNCHRONOUS_ROOT_EXECUTION_SYMBOL, TASK_ALLOCATION_SYMBOL, TASK_CANCELLATION_REQUEST_SYMBOL,
    TASK_START_SYMBOL, TERMINAL_PUBLICATION_SYMBOL, WAKE_SYMBOL,
};
use bray_symbols::{NativeLinkKind, NativeLinkRequirement};
use bray_target::{NativeTarget, ObjectFormat, TargetIdentity};

const USAGE: &str = "usage: cargo xtask runtime-artifact \
    <build --target <triple> --output <directory> [--profile <profile>] | smoke-test>";
const METADATA_FILE_NAME: &str = "bray-runtime.brayrt";
const RUNTIME_IDENTITY: &str = "bray.runtime.reference";
const PANIC_ABI: &str = "bray.panic.unwind";

pub(crate) fn run(mut arguments: impl Iterator<Item = String>) -> ExitCode {
    let result = match arguments.next().as_deref() {
        Some("build") => build_command(arguments).map(Some),
        Some("smoke-test") => smoke_test_command(arguments).map(|()| None),
        _ => Err(CommandError::Usage),
    };

    match result {
        Ok(Some(package)) => {
            println!("{}", package.metadata.display());
            println!("{}", package.archive.display());

            ExitCode::SUCCESS
        }
        Ok(None) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");

            ExitCode::FAILURE
        }
    }
}

fn build_command(arguments: impl Iterator<Item = String>) -> Result<Package, CommandError> {
    let options = BuildOptions::parse(arguments)?;

    build(&options)
}

fn smoke_test_command(mut arguments: impl Iterator<Item = String>) -> Result<(), CommandError> {
    if let Some(argument) = arguments.next() {
        return Err(CommandError::UnexpectedArgument(argument));
    }

    let target = host_target()?;

    let directory = tempfile::Builder::new()
        .prefix("bray-runtime-artifact-smoke-")
        .tempdir()
        .map_err(CommandError::TemporaryDirectory)?;

    let options = BuildOptions {
        target,
        output: directory.path().to_path_buf(),
        profile: "release".to_owned(),
    };

    let package = build(&options)?;

    smoke_test(&package, options.target, directory.path())?;

    Ok(())
}

pub(crate) fn smoke_test_host() -> Result<(), String> {
    smoke_test_command(std::iter::empty()).map_err(|error| error.to_string())
}

pub(crate) fn build_for_readiness(target: NativeTarget, output: &Path) -> Result<PathBuf, String> {
    let options = BuildOptions {
        target,
        output: output.to_path_buf(),
        profile: "release".to_owned(),
    };

    build(&options)
        .map(|package| package.metadata)
        .map_err(|error| error.to_string())
}

fn build(options: &BuildOptions) -> Result<Package, CommandError> {
    let root = workspace::root().map_err(CommandError::Workspace)?;
    let target_directory = root.join("target");

    let mut command = Command::new("cargo");

    command.current_dir(&root).args([
        "rustc",
        "--color",
        "never",
        "--package",
        "bray-runtime",
        "--target",
        options.target.as_str(),
        "--profile",
        &options.profile,
        "--target-dir",
    ]);

    command.arg(&target_directory);
    command.args(["--", "--print", "native-static-libs"]);
    configure_cross_c_toolchain(&mut command, &root, options.target);

    let output = command.output().map_err(CommandError::Cargo)?;

    if !output.status.success() {
        return Err(CommandError::BuildFailed);
    }

    let native_links = native_link_requirements(&output)?;
    let archive_file_name = archive_file_name(options.target);

    let source = target_directory
        .join(options.target.as_str())
        .join(profile_directory(&options.profile))
        .join(archive_file_name);

    let archive = options.output.join(archive_file_name);
    let metadata_path = options.output.join(METADATA_FILE_NAME);

    fs::create_dir_all(&options.output)
        .map_err(|error| CommandError::write(&options.output, error))?;

    fs::copy(&source, &archive).map_err(|error| CommandError::copy(&source, &archive, error))?;

    let digest = digest_file(&archive)?;
    let metadata_value = metadata(options, archive_file_name, digest, &native_links)?;

    let bytes = metadata_value
        .encode_json()
        .map_err(|_| CommandError::MetadataEncoding)?;

    fs::write(&metadata_path, bytes).map_err(|error| CommandError::write(&metadata_path, error))?;

    Ok(Package {
        archive,
        metadata: metadata_path,
    })
}

fn configure_cross_c_toolchain(command: &mut Command, root: &Path, target: NativeTarget) {
    if !cfg!(windows) || target != NativeTarget::X86_64LinuxGnu {
        return;
    }

    command
        .env(
            "CC_x86_64_unknown_linux_gnu",
            crate::llvm::tool_path(root, "clang"),
        )
        .env(
            "AR_x86_64_unknown_linux_gnu",
            crate::llvm::tool_path(root, "llvm-ar"),
        );
}

fn metadata(
    options: &BuildOptions,
    archive_file_name: &str,
    digest: RuntimeArtifactDigest,
    native_links: &[NativeLinkRequirement],
) -> Result<RuntimeArtifactMetadata, CommandError> {
    let identity =
        RuntimeIdentity::try_new(RUNTIME_IDENTITY).ok_or(CommandError::MetadataContract)?;

    let artifact =
        RuntimeArtifactId::try_new(format!("{RUNTIME_IDENTITY}.{}", options.target.as_str()))
            .ok_or(CommandError::MetadataContract)?;

    let target = options.target.identity();

    let panic_abi = PanicAbiIdentity::try_new(PANIC_ABI).ok_or(CommandError::MetadataContract)?;

    let contract = RuntimeContract::try_new(
        identity,
        artifact,
        RuntimeAbiVersion::new(1, 0),
        ProtectedFrameAbiVersions::uniform(RuntimeAbiVersion::new(1, 0)),
        target,
        panic_abi,
        [
            RuntimeCapability::CooperativeExecution,
            RuntimeCapability::LocalLanes,
            RuntimeCapability::MainThreadLane,
        ],
        runtime_role_bindings()?,
    )
    .map_err(|_| CommandError::MetadataContract)?;

    RuntimeArtifactMetadata::try_new(contract, archive_file_name, digest)
        .map(|metadata| metadata.with_native_links(native_links.iter().cloned()))
        .map_err(|_| CommandError::MetadataContract)
}

fn native_link_requirements(output: &Output) -> Result<Vec<NativeLinkRequirement>, CommandError> {
    let standard_output = String::from_utf8_lossy(&output.stdout);
    let standard_error = String::from_utf8_lossy(&output.stderr);

    let Some(arguments) = standard_output
        .lines()
        .chain(standard_error.lines())
        .find_map(|line| line.trim().strip_prefix("note: native-static-libs:"))
    else {
        return Err(CommandError::MissingNativeLinks);
    };

    parse_native_link_arguments(arguments.split_whitespace())
}

fn parse_native_link_arguments<'a>(
    mut arguments: impl Iterator<Item = &'a str>,
) -> Result<Vec<NativeLinkRequirement>, CommandError> {
    let mut requirements = Vec::new();

    while let Some(argument) = arguments.next() {
        let (name, kind) = if argument == "-framework" {
            (
                arguments.next().ok_or(CommandError::InvalidNativeLink)?,
                NativeLinkKind::Framework,
            )
        } else if let Some(name) = argument.strip_prefix("-l") {
            (name, NativeLinkKind::System)
        } else if let Some(name) = argument.strip_prefix("/defaultlib:") {
            (name, NativeLinkKind::System)
        } else if let Some(name) = argument.strip_suffix(".lib") {
            (name, NativeLinkKind::System)
        } else {
            return Err(CommandError::InvalidNativeLink);
        };

        let name = NonEmptySharedStr::try_new(name).ok_or(CommandError::InvalidNativeLink)?;

        requirements.push(NativeLinkRequirement::new(name, kind));
    }

    Ok(requirements)
}

fn runtime_role_bindings() -> Result<Vec<RuntimeRoleBinding>, CommandError> {
    [
        (RuntimeAbiRole::RootExecution, ROOT_EXECUTION_SYMBOL),
        (
            RuntimeAbiRole::SynchronousRootExecution,
            SYNCHRONOUS_ROOT_EXECUTION_SYMBOL,
        ),
        (
            RuntimeAbiRole::RootCancellationRequest,
            ROOT_CANCELLATION_REQUEST_SYMBOL,
        ),
        (
            RuntimeAbiRole::CleanupIncidentReporting,
            CLEANUP_INCIDENT_REPORTING_SYMBOL,
        ),
        (
            RuntimeAbiRole::RootTerminalObservation,
            ROOT_TERMINAL_OBSERVATION_SYMBOL,
        ),
        (
            RuntimeAbiRole::RootCompletionResolution,
            ROOT_COMPLETION_RESOLUTION_SYMBOL,
        ),
        (RuntimeAbiRole::PanicReporting, PANIC_REPORTING_SYMBOL),
        (
            RuntimeAbiRole::PanicReportConstruction,
            PANIC_REPORT_CONSTRUCTION_SYMBOL,
        ),
        (RuntimeAbiRole::PanicPropagation, PANIC_PROPAGATION_SYMBOL),
        (
            RuntimeAbiRole::EntryFailureReporting,
            ENTRY_FAILURE_REPORTING_SYMBOL,
        ),
        (RuntimeAbiRole::TaskAllocation, TASK_ALLOCATION_SYMBOL),
        (RuntimeAbiRole::TaskStart, TASK_START_SYMBOL),
        (
            RuntimeAbiRole::AwaitedFrameComposition,
            AWAITED_FRAME_COMPOSITION_SYMBOL,
        ),
        (
            RuntimeAbiRole::FrameCompletionMove,
            FRAME_COMPLETION_MOVE_SYMBOL,
        ),
        (
            RuntimeAbiRole::SuspensionRegistration,
            SUSPENSION_REGISTRATION_SYMBOL,
        ),
        (RuntimeAbiRole::Wake, WAKE_SYMBOL),
        (
            RuntimeAbiRole::TaskCancellationRequest,
            TASK_CANCELLATION_REQUEST_SYMBOL,
        ),
        (
            RuntimeAbiRole::CurrentRunCancellationObservation,
            CURRENT_RUN_CANCELLATION_OBSERVATION_SYMBOL,
        ),
        (RuntimeAbiRole::JoinRegistration, JOIN_REGISTRATION_SYMBOL),
        (
            RuntimeAbiRole::TerminalPublication,
            TERMINAL_PUBLICATION_SYMBOL,
        ),
        (RuntimeAbiRole::RuntimeEvent, RUNTIME_EVENT_SYMBOL),
        (
            RuntimeAbiRole::CompatibleLaneSelection,
            COMPATIBLE_LANE_SELECTION_SYMBOL,
        ),
        (
            RuntimeAbiRole::MainThreadLaneStartup,
            MAIN_THREAD_LANE_STARTUP_SYMBOL,
        ),
        (
            RuntimeAbiRole::MainThreadLaneDrive,
            MAIN_THREAD_LANE_DRIVE_SYMBOL,
        ),
        (
            RuntimeAbiRole::StructuredShutdown,
            STRUCTURED_SHUTDOWN_SYMBOL,
        ),
    ]
    .into_iter()
    .map(|(role, name)| {
        let symbol = BinarySymbolName::try_new(name).ok_or(CommandError::MetadataContract)?;

        Ok(RuntimeRoleBinding::new(
            role,
            symbol,
            RuntimeRoleImplementation::BrayRuntime,
        ))
    })
    .collect()
}

fn digest_file(path: &Path) -> Result<RuntimeArtifactDigest, CommandError> {
    let digest = digest::sha256(path).map_err(|error| CommandError::read(path, error))?;

    Ok(RuntimeArtifactDigest::new(digest))
}

fn smoke_test(
    package: &Package,
    target: NativeTarget,
    directory: &Path,
) -> Result<(), CommandError> {
    let source = directory.join("runtime-smoke.rs");

    let executable = directory.join(if cfg!(windows) {
        "runtime-smoke.exe"
    } else {
        "runtime-smoke"
    });

    fs::write(&source, SMOKE_SOURCE).map_err(|error| CommandError::write(&source, error))?;

    let archive = package.archive.to_str().ok_or(CommandError::NonUtf8Path)?;

    let status = Command::new("rustc")
        .args([
            "--edition",
            "2024",
            "--target",
            target.as_str(),
            "-C",
            &format!("link-arg={archive}"),
            "-o",
        ])
        .arg(&executable)
        .arg(&source)
        .status()
        .map_err(CommandError::Rustc)?;

    if !status.success() {
        return Err(CommandError::SmokeLinkFailed);
    }

    let output = Command::new(&executable)
        .output()
        .map_err(CommandError::SmokeExecution)?;

    if !output.status.success() {
        return Err(CommandError::SmokeExecutionFailed);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);

    if !stderr.contains("cleanup_incident ordinal=0 ") {
        return Err(CommandError::CleanupReportMissing);
    }

    Ok(())
}

fn host_target() -> Result<NativeTarget, CommandError> {
    let output = Command::new("rustc")
        .arg("-vV")
        .output()
        .map_err(CommandError::Rustc)?;

    if !output.status.success() {
        return Err(CommandError::HostTarget);
    }

    let output = String::from_utf8(output.stdout).map_err(|_| CommandError::HostTarget)?;

    let identity = output
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .and_then(TargetIdentity::try_new)
        .ok_or(CommandError::HostTarget)?;

    NativeTarget::for_identity(&identity).ok_or(CommandError::HostTarget)
}

const fn archive_file_name(target: NativeTarget) -> &'static str {
    match target.object_format() {
        ObjectFormat::Coff => "bray_runtime.lib",
        ObjectFormat::Elf | ObjectFormat::MachO => "libbray_runtime.a",
        ObjectFormat::WebAssembly | ObjectFormat::Xcoff => {
            panic!("native runtime target must use COFF, ELF, or Mach-O")
        }
    }
}

fn profile_directory(profile: &str) -> &str {
    if profile == "dev" { "debug" } else { profile }
}

struct BuildOptions {
    target: NativeTarget,
    output: PathBuf,
    profile: String,
}

impl BuildOptions {
    fn parse(mut arguments: impl Iterator<Item = String>) -> Result<Self, CommandError> {
        let mut target = None;
        let mut output = None;
        let mut profile = "release".to_owned();

        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--target" => target = Some(required_value(&mut arguments, "--target")?),
                "--output" => {
                    output = Some(PathBuf::from(required_value(&mut arguments, "--output")?));
                }
                "--profile" => {
                    profile = required_value(&mut arguments, "--profile")?;
                }
                _ => return Err(CommandError::UnexpectedArgument(argument)),
            }
        }

        let target = target
            .and_then(TargetIdentity::try_new)
            .and_then(|identity| NativeTarget::for_identity(&identity))
            .ok_or(CommandError::Usage)?;

        let output = output.ok_or(CommandError::Usage)?;

        if output.as_os_str().is_empty() || profile.is_empty() {
            return Err(CommandError::Usage);
        }

        Ok(Self {
            target,
            output,
            profile,
        })
    }
}

struct Package {
    archive: PathBuf,
    metadata: PathBuf,
}

#[derive(Debug)]
enum CommandError {
    Usage,
    UnexpectedArgument(String),
    MissingValue(&'static str),
    Workspace(String),
    Cargo(std::io::Error),
    BuildFailed,
    MissingNativeLinks,
    InvalidNativeLink,
    Read {
        path: PathBuf,
        error: std::io::Error,
    },
    Write {
        path: PathBuf,
        error: std::io::Error,
    },
    Copy {
        source: PathBuf,
        destination: PathBuf,
        error: std::io::Error,
    },
    MetadataContract,
    MetadataEncoding,
    TemporaryDirectory(std::io::Error),
    NonUtf8Path,
    Rustc(std::io::Error),
    HostTarget,
    SmokeLinkFailed,
    SmokeExecution(std::io::Error),
    SmokeExecutionFailed,
    CleanupReportMissing,
}

impl CommandError {
    fn read(path: &Path, error: std::io::Error) -> Self {
        Self::Read {
            path: path.to_path_buf(),
            error,
        }
    }

    fn write(path: &Path, error: std::io::Error) -> Self {
        Self::Write {
            path: path.to_path_buf(),
            error,
        }
    }

    fn copy(source: &Path, destination: &Path, error: std::io::Error) -> Self {
        Self::Copy {
            source: source.to_path_buf(),
            destination: destination.to_path_buf(),
            error,
        }
    }
}

impl fmt::Display for CommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage => formatter.write_str(USAGE),
            Self::UnexpectedArgument(argument) => {
                write!(
                    formatter,
                    "unexpected runtime-artifact argument: {argument}"
                )
            }
            Self::MissingValue(option) => {
                write!(formatter, "{option} requires a value")
            }
            Self::Workspace(error) => formatter.write_str(error),
            Self::Cargo(error) => write!(formatter, "could not run Cargo: {error}"),
            Self::BuildFailed => formatter.write_str("runtime artifact build failed"),
            Self::MissingNativeLinks => {
                formatter.write_str("rustc did not report runtime native link requirements")
            }
            Self::InvalidNativeLink => {
                formatter.write_str("rustc reported an unsupported runtime native link argument")
            }
            Self::Read { path, error } => {
                write!(formatter, "could not read {}: {error}", path.display())
            }
            Self::Write { path, error } => {
                write!(formatter, "could not write {}: {error}", path.display())
            }
            Self::Copy {
                source,
                destination,
                error,
            } => write!(
                formatter,
                "could not copy {} to {}: {error}",
                source.display(),
                destination.display()
            ),
            Self::MetadataContract => {
                formatter.write_str("runtime artifact metadata contract is invalid")
            }
            Self::MetadataEncoding => {
                formatter.write_str("runtime artifact metadata could not be encoded")
            }
            Self::TemporaryDirectory(error) => {
                write!(formatter, "could not create smoke-test directory: {error}")
            }
            Self::NonUtf8Path => formatter.write_str("runtime archive path is not valid UTF-8"),
            Self::Rustc(error) => write!(formatter, "could not run rustc: {error}"),
            Self::HostTarget => formatter.write_str("could not determine rustc host target"),
            Self::SmokeLinkFailed => formatter.write_str("runtime artifact smoke link failed"),
            Self::SmokeExecution(error) => {
                write!(formatter, "could not run runtime smoke executable: {error}")
            }
            Self::SmokeExecutionFailed => {
                formatter.write_str("runtime artifact smoke execution failed")
            }
            Self::CleanupReportMissing => {
                formatter.write_str("runtime smoke did not report its cleanup incident")
            }
        }
    }
}

fn required_value(
    arguments: &mut impl Iterator<Item = String>,
    option: &'static str,
) -> Result<String, CommandError> {
    arguments.next().ok_or(CommandError::MissingValue(option))
}

const SMOKE_SOURCE: &str = include_str!("../fixtures/runtime-smoke.rs");

#[cfg(test)]
mod tests {
    use bray_runtime_interface::{RuntimeAbiRole, RuntimeArtifactDigest};
    use bray_symbols::NativeLinkKind;
    use bray_target::NativeTarget;

    use super::{
        BuildOptions, CommandError, archive_file_name, metadata, parse_native_link_arguments,
        runtime_role_bindings,
    };

    #[test]
    fn build_options_require_target_and_output() {
        let result =
            BuildOptions::parse(["--target".to_owned(), "x86_64-test".to_owned()].into_iter());

        assert!(matches!(result, Err(CommandError::Usage)));
    }

    #[test]
    fn archive_names_follow_native_target_conventions() {
        assert_eq!(
            archive_file_name(NativeTarget::X86_64WindowsMsvc),
            "bray_runtime.lib"
        );

        assert_eq!(
            archive_file_name(NativeTarget::Aarch64WindowsMsvc),
            "bray_runtime.lib"
        );

        assert_eq!(
            archive_file_name(NativeTarget::X86_64LinuxGnu),
            "libbray_runtime.a"
        );

        assert_eq!(
            archive_file_name(NativeTarget::Aarch64LinuxGnu),
            "libbray_runtime.a"
        );

        assert_eq!(
            archive_file_name(NativeTarget::X86_64MacOs),
            "libbray_runtime.a"
        );

        assert_eq!(
            archive_file_name(NativeTarget::Aarch64MacOs),
            "libbray_runtime.a"
        );
    }

    #[test]
    fn runtime_metadata_covers_every_native_target_reproducibly() {
        for target in NativeTarget::ALL {
            let options = BuildOptions {
                target,
                output: "out".into(),
                profile: "release".to_owned(),
            };

            let archive = archive_file_name(target);
            let digest = RuntimeArtifactDigest::new([7; 32]);

            let first = metadata(&options, archive, digest, &[])
                .unwrap_or_else(|error| panic!("runtime metadata must be valid: {error}"));

            let second = metadata(&options, archive, digest, &[])
                .unwrap_or_else(|error| panic!("runtime metadata must be valid: {error}"));

            assert_eq!(first, second);
            assert_eq!(first.contract().target().as_str(), target.as_str());
            assert_eq!(first.archive_file_name(), archive);

            assert_eq!(
                first
                    .encode_json()
                    .unwrap_or_else(|_| panic!("runtime metadata must encode")),
                second
                    .encode_json()
                    .unwrap_or_else(|_| panic!("runtime metadata must encode"))
            );
        }
    }

    #[test]
    fn rustc_native_link_arguments_preserve_platform_requirements_in_order() {
        let cases = [
            (
                "-lgcc_s -lutil -lrt -lpthread -lm -ldl -lc",
                vec![
                    ("gcc_s", NativeLinkKind::System),
                    ("util", NativeLinkKind::System),
                    ("rt", NativeLinkKind::System),
                    ("pthread", NativeLinkKind::System),
                    ("m", NativeLinkKind::System),
                    ("dl", NativeLinkKind::System),
                    ("c", NativeLinkKind::System),
                ],
            ),
            (
                "kernel32.lib ntdll.lib userenv.lib /defaultlib:msvcrt",
                vec![
                    ("kernel32", NativeLinkKind::System),
                    ("ntdll", NativeLinkKind::System),
                    ("userenv", NativeLinkKind::System),
                    ("msvcrt", NativeLinkKind::System),
                ],
            ),
            (
                "-framework Security -framework CoreFoundation -lSystem",
                vec![
                    ("Security", NativeLinkKind::Framework),
                    ("CoreFoundation", NativeLinkKind::Framework),
                    ("System", NativeLinkKind::System),
                ],
            ),
        ];

        for (arguments, expected) in cases {
            let requirements = parse_native_link_arguments(arguments.split_whitespace())
                .unwrap_or_else(|error| panic!("native links must parse: {error}"));

            assert_eq!(
                requirements
                    .iter()
                    .map(|requirement| (requirement.name(), requirement.kind()))
                    .collect::<Vec<_>>(),
                expected
            );
        }
    }

    #[test]
    fn packaged_contract_names_every_exported_runtime_role() {
        let bindings = runtime_role_bindings()
            .unwrap_or_else(|error| panic!("runtime role bindings must be valid: {error}"));

        let roles: Vec<_> = bindings.iter().map(|binding| binding.role()).collect();

        assert_eq!(
            roles,
            [
                RuntimeAbiRole::RootExecution,
                RuntimeAbiRole::SynchronousRootExecution,
                RuntimeAbiRole::RootCancellationRequest,
                RuntimeAbiRole::CleanupIncidentReporting,
                RuntimeAbiRole::RootTerminalObservation,
                RuntimeAbiRole::RootCompletionResolution,
                RuntimeAbiRole::PanicReporting,
                RuntimeAbiRole::PanicReportConstruction,
                RuntimeAbiRole::PanicPropagation,
                RuntimeAbiRole::EntryFailureReporting,
                RuntimeAbiRole::TaskAllocation,
                RuntimeAbiRole::TaskStart,
                RuntimeAbiRole::AwaitedFrameComposition,
                RuntimeAbiRole::FrameCompletionMove,
                RuntimeAbiRole::SuspensionRegistration,
                RuntimeAbiRole::Wake,
                RuntimeAbiRole::TaskCancellationRequest,
                RuntimeAbiRole::CurrentRunCancellationObservation,
                RuntimeAbiRole::JoinRegistration,
                RuntimeAbiRole::TerminalPublication,
                RuntimeAbiRole::RuntimeEvent,
                RuntimeAbiRole::CompatibleLaneSelection,
                RuntimeAbiRole::MainThreadLaneStartup,
                RuntimeAbiRole::MainThreadLaneDrive,
                RuntimeAbiRole::StructuredShutdown,
            ]
        );
    }
}
