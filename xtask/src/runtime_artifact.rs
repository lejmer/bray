use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use crate::bundle::{
    DirectoryPublication, DirectoryPublicationError, NativeBuildOptions, NativeBuildOptionsBuilder,
    NativeBuildOptionsError,
};
use crate::workspace;
use bray_base::sha256_file;
use bray_runtime_interface::{
    BinarySymbolName, PanicAbiIdentity, ProtectedFrameAbiVersions, RuntimeAbiRole,
    RuntimeAbiVersion, RuntimeArtifactDigest, RuntimeArtifactId, RuntimeArtifactMetadata,
    RuntimeCapability, RuntimeContract, RuntimeIdentity, RuntimeRoleBinding,
    RuntimeRoleImplementation, native_runtime_role_symbol,
};
use bray_symbols::NativeLinkRequirement;
use bray_target::{NativeTarget, ObjectFormat};

const USAGE: &str = "usage: cargo xtask runtime-artifact \
    <build --output <directory> [--target <triple>] [--profile <profile>] | smoke-test>";
const METADATA_FILE_NAME: &str = "bray-runtime.brayrt";
const RUNTIME_IDENTITY: &str = "bray.runtime.reference";
const PANIC_ABI: &str = "bray.panic.unwind";

#[derive(Clone, Copy)]
enum RuntimeHostIntegration {
    Product,
    TestRunner,
}

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

    let target = options
        .native
        .target()
        .or_else(NativeTarget::current)
        .ok_or(CommandError::HostTarget)?;

    let output = options.native.target_output(target);

    build(
        target,
        &output,
        &options.profile,
        RuntimeHostIntegration::Product,
    )
}

fn smoke_test_command(mut arguments: impl Iterator<Item = String>) -> Result<(), CommandError> {
    if let Some(argument) = arguments.next() {
        return Err(CommandError::UnexpectedArgument(argument));
    }

    let target = NativeTarget::current().ok_or(CommandError::HostTarget)?;

    let directory = tempfile::Builder::new()
        .prefix("bray-runtime-artifact-smoke-")
        .tempdir()
        .map_err(CommandError::TemporaryDirectory)?;

    let output = directory.path().join(target.as_str());

    let package = build(target, &output, "release", RuntimeHostIntegration::Product)?;

    smoke_test(&package, target, directory.path())?;

    Ok(())
}

pub(crate) fn smoke_test_host() -> Result<(), String> {
    smoke_test_command(std::iter::empty()).map_err(|error| error.to_string())
}

pub(crate) fn build_for_readiness(target: NativeTarget, output: &Path) -> Result<PathBuf, String> {
    build(
        target,
        output,
        "release",
        RuntimeHostIntegration::TestRunner,
    )
    .map(|package| package.metadata)
    .map_err(|error| error.to_string())
}

fn build(
    target: NativeTarget,
    output: &Path,
    profile: &str,
    host: RuntimeHostIntegration,
) -> Result<Package, CommandError> {
    let publication = DirectoryPublication::begin(output, "bray-runtime-artifact-")
        .map_err(CommandError::Publication)?;

    build_contents(target, publication.contents(), profile, host)?;

    let output = publication.publish().map_err(CommandError::Publication)?;

    Ok(Package {
        archive: output.join(archive_file_name(target)),
        metadata: output.join(METADATA_FILE_NAME),
    })
}

fn build_contents(
    target: NativeTarget,
    output: &Path,
    profile: &str,
    host: RuntimeHostIntegration,
) -> Result<(), CommandError> {
    let root = workspace::root().map_err(CommandError::Workspace)?;

    audit_dependency_boundaries(&root)?;

    let archive_file_name = archive_file_name(target);

    let features = match host {
        RuntimeHostIntegration::Product => &[][..],
        RuntimeHostIntegration::TestRunner => &["test-host"][..],
    };

    let built = crate::native_archive::build_rust_static_library(
        &root,
        target,
        "bray-runtime",
        profile,
        archive_file_name,
        features,
    )
    .map_err(CommandError::NativeArchive)?;

    let archive = output.join(archive_file_name);
    let metadata_path = output.join(METADATA_FILE_NAME);

    fs::copy(built.archive(), &archive)
        .map_err(|error| CommandError::copy(built.archive(), &archive, error))?;

    let digest = digest_file(&archive)?;

    let metadata_value = metadata(
        target,
        archive_file_name,
        digest,
        built.native_links(),
        host,
    )?;

    let bytes = metadata_value
        .encode_json()
        .map_err(|_| CommandError::MetadataEncoding)?;

    fs::write(&metadata_path, bytes).map_err(|error| CommandError::write(&metadata_path, error))?;

    Ok(())
}

fn audit_dependency_boundaries(root: &Path) -> Result<(), CommandError> {
    crate::dependency_audit::require_no_normal_dependencies(root, "bray-runtime-abi")
        .map_err(CommandError::DependencyAudit)?;

    crate::dependency_audit::require_absent_normal_dependencies(
        root,
        "bray-runtime",
        &[
            "blake3",
            "bray-base",
            "bray-compiler-known",
            "bray-declarations",
            "bray-diagnostics",
            "bray-runtime-interface",
            "bray-source",
            "bray-symbols",
            "bray-syntax",
            "bray-target",
            "bray-test-protocol",
            "serde",
            "serde_json",
            "sha2",
            "tempfile",
        ],
    )
    .map_err(CommandError::DependencyAudit)
}

fn metadata(
    target: NativeTarget,
    archive_file_name: &str,
    digest: RuntimeArtifactDigest,
    native_links: &[NativeLinkRequirement],
    host: RuntimeHostIntegration,
) -> Result<RuntimeArtifactMetadata, CommandError> {
    let identity =
        RuntimeIdentity::try_new(RUNTIME_IDENTITY).ok_or(CommandError::MetadataContract)?;

    let artifact = RuntimeArtifactId::try_new(format!("{RUNTIME_IDENTITY}.{}", target.as_str()))
        .ok_or(CommandError::MetadataContract)?;

    let target = target.identity();

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
        runtime_role_bindings(host)?,
    )
    .map_err(|_| CommandError::MetadataContract)?;

    RuntimeArtifactMetadata::try_new(contract, archive_file_name, digest)
        .map(RuntimeArtifactMetadata::with_embedded_platform_services)
        .map(|metadata| metadata.with_native_links(native_links.iter().cloned()))
        .map_err(|_| CommandError::MetadataContract)
}

fn runtime_role_bindings(
    host: RuntimeHostIntegration,
) -> Result<Vec<RuntimeRoleBinding>, CommandError> {
    let include_test_host = matches!(host, RuntimeHostIntegration::TestRunner);

    let bindings: Vec<_> = RuntimeAbiRole::ALL
        .into_iter()
        .filter(|role| include_test_host || *role != RuntimeAbiRole::TestEntrySelection)
        .filter_map(|role| native_runtime_role_symbol(role).map(|name| (role, name)))
        .map(|(role, name)| {
            let symbol = BinarySymbolName::try_new(name).ok_or(CommandError::MetadataContract)?;

            Ok(RuntimeRoleBinding::new(
                role,
                symbol,
                RuntimeRoleImplementation::BrayRuntime,
            ))
        })
        .collect::<Result<_, _>>()?;

    Ok(bindings)
}

fn digest_file(path: &Path) -> Result<RuntimeArtifactDigest, CommandError> {
    let digest = sha256_file(path).map_err(|error| CommandError::read(path, error))?;

    Ok(RuntimeArtifactDigest::new(digest))
}

fn smoke_test(
    package: &Package,
    target: NativeTarget,
    directory: &Path,
) -> Result<(), CommandError> {
    audit_product_archive(&package.archive)?;

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

fn audit_product_archive(archive: &Path) -> Result<(), CommandError> {
    let tool =
        bray_tooling::llvm_tool_path("llvm-nm").ok_or(CommandError::NativeSymbolToolUnavailable)?;

    let output = Command::new(tool)
        .args(["--defined-only", "--extern-only"])
        .arg(archive)
        .output()
        .map_err(CommandError::NativeSymbolInspection)?;

    if !output.status.success() {
        return Err(CommandError::NativeSymbolInspectionFailed);
    }

    let test_entry_symbol = native_runtime_role_symbol(RuntimeAbiRole::TestEntrySelection)
        .ok_or(CommandError::MetadataContract)?;

    if String::from_utf8_lossy(&output.stdout).contains(test_entry_symbol) {
        return Err(CommandError::TestHostSymbolInProductArchive);
    }

    Ok(())
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

struct BuildOptions {
    native: NativeBuildOptions,
    profile: String,
}

impl BuildOptions {
    fn parse(mut arguments: impl Iterator<Item = String>) -> Result<Self, CommandError> {
        let mut native = NativeBuildOptionsBuilder::default();
        let mut profile = "release".to_owned();

        while let Some(argument) = arguments.next() {
            if native
                .parse_option(&argument, &mut arguments)
                .map_err(CommandError::BuildOptions)?
            {
                continue;
            }

            match argument.as_str() {
                "--profile" => {
                    profile = required_value(&mut arguments, "--profile")?;
                }
                _ => return Err(CommandError::UnexpectedArgument(argument)),
            }
        }

        let native = native.finish().map_err(CommandError::BuildOptions)?;

        if profile.is_empty() {
            return Err(CommandError::Usage);
        }

        Ok(Self { native, profile })
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
    BuildOptions(NativeBuildOptionsError),
    Publication(DirectoryPublicationError),
    Workspace(String),
    DependencyAudit(crate::dependency_audit::DependencyAuditError),
    NativeArchive(crate::native_archive::BuildError),
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
    NativeSymbolToolUnavailable,
    NativeSymbolInspection(std::io::Error),
    NativeSymbolInspectionFailed,
    TestHostSymbolInProductArchive,
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
            Self::BuildOptions(error) => write!(formatter, "{error}"),
            Self::Publication(error) => write!(formatter, "{error}"),
            Self::Workspace(error) => formatter.write_str(error),
            Self::DependencyAudit(error) => write!(formatter, "{error}"),
            Self::NativeArchive(error) => write!(formatter, "{error}"),
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
            Self::NativeSymbolToolUnavailable => {
                formatter.write_str("llvm-nm is unavailable for runtime artifact inspection")
            }
            Self::NativeSymbolInspection(error) => {
                write!(
                    formatter,
                    "could not inspect runtime archive symbols: {error}"
                )
            }
            Self::NativeSymbolInspectionFailed => {
                formatter.write_str("runtime archive symbol inspection failed")
            }
            Self::TestHostSymbolInProductArchive => {
                formatter.write_str("product runtime archive contains the test-host entry symbol")
            }
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
    use bray_target::NativeTarget;

    use super::{
        BuildOptions, CommandError, RuntimeHostIntegration, archive_file_name, metadata,
        runtime_role_bindings,
    };

    #[test]
    fn build_options_require_output_and_accept_an_implicit_host_target() {
        let missing = BuildOptions::parse(std::iter::empty());

        assert!(matches!(
            missing,
            Err(CommandError::BuildOptions(
                crate::bundle::NativeBuildOptionsError::MissingOutput
            ))
        ));

        let options =
            BuildOptions::parse(["--output".to_owned(), "runtime".to_owned()].into_iter())
                .unwrap_or_else(|error| panic!("host runtime build options must parse: {error}"));

        assert_eq!(options.native.target(), None);
        assert_eq!(options.native.output(), std::path::Path::new("runtime"));
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
            let archive = archive_file_name(target);
            let digest = RuntimeArtifactDigest::new([7; 32]);

            let first = metadata(
                target,
                archive,
                digest,
                &[],
                RuntimeHostIntegration::Product,
            )
            .unwrap_or_else(|error| panic!("runtime metadata must be valid: {error}"));

            let second = metadata(
                target,
                archive,
                digest,
                &[],
                RuntimeHostIntegration::Product,
            )
            .unwrap_or_else(|error| panic!("runtime metadata must be valid: {error}"));

            assert_eq!(first, second);
            assert_eq!(first.contract().target().as_str(), target.as_str());
            assert_eq!(first.archive_file_name(), archive);
            assert!(first.embeds_platform_services());

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
    fn packaged_contract_names_every_exported_runtime_role() {
        let bindings = runtime_role_bindings(RuntimeHostIntegration::Product)
            .unwrap_or_else(|error| panic!("runtime role bindings must be valid: {error}"));

        let roles: Vec<_> = bindings.iter().map(|binding| binding.role()).collect();

        assert_eq!(
            roles,
            [
                RuntimeAbiRole::RootExecution,
                RuntimeAbiRole::SynchronousRootExecution,
                RuntimeAbiRole::ForeignCallbackExecution,
                RuntimeAbiRole::RootCancellationRequest,
                RuntimeAbiRole::TaskAllocation,
                RuntimeAbiRole::TaskStart,
                RuntimeAbiRole::SuspensionRegistration,
                RuntimeAbiRole::Wake,
                RuntimeAbiRole::TaskCancellationRequest,
                RuntimeAbiRole::CurrentRunCancellationObservation,
                RuntimeAbiRole::CurrentRunCancellationPropagation,
                RuntimeAbiRole::JoinRegistration,
                RuntimeAbiRole::TerminalPublication,
                RuntimeAbiRole::RuntimeEvent,
                RuntimeAbiRole::CompatibleLaneSelection,
                RuntimeAbiRole::CleanupIncidentReporting,
                RuntimeAbiRole::MainThreadLaneStartup,
                RuntimeAbiRole::MainThreadLaneDrive,
                RuntimeAbiRole::RootTerminalObservation,
                RuntimeAbiRole::RootCompletionResolution,
                RuntimeAbiRole::PanicReporting,
                RuntimeAbiRole::EntryFailureReporting,
                RuntimeAbiRole::StructuredShutdown,
                RuntimeAbiRole::FrameCompletionMove,
                RuntimeAbiRole::PanicReportConstruction,
                RuntimeAbiRole::PanicPropagation,
                RuntimeAbiRole::AwaitedFrameComposition,
            ]
        );

        let test_bindings = runtime_role_bindings(RuntimeHostIntegration::TestRunner)
            .unwrap_or_else(|error| panic!("test runtime role bindings must be valid: {error}"));

        assert!(
            test_bindings
                .iter()
                .any(|binding| binding.role() == RuntimeAbiRole::TestEntrySelection)
        );
    }
}
