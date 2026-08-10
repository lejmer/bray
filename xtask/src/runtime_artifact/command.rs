use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::bundle::{
    DirectoryPublication, DirectoryPublicationError, NativeBuildOptions, NativeBuildOptionsBuilder,
    NativeBuildOptionsError,
};
use crate::workspace;
use super::smoke::smoke_test;
use bray_base::sha256_file;
use bray_runtime_interface::{
    BinarySymbolName, PanicAbiIdentity, ProtectedFrameAbiVersions, RuntimeAbiRole,
    RuntimeAbiVersion, RuntimeArtifactComponentMetadata, RuntimeArtifactDigest, RuntimeArtifactId,
    RuntimeArtifactMetadata, RuntimeArtifactPurpose, RuntimeCapability, RuntimeContract,
    RuntimeIdentity, RuntimeRoleBinding, RuntimeRoleImplementation, native_runtime_role_symbol,
};
use bray_symbols::NativeLinkRequirement;
use bray_target::{NativeTarget, TargetOutputKind, TargetOutputName};

const USAGE: &str = "usage: cargo xtask runtime-artifact \
    <build --output <directory> [--target <triple>] [--profile <profile>] | smoke-test>";
const METADATA_FILE_NAME: &str = "bray-runtime.brayrt";
const RUNTIME_IDENTITY: &str = "bray.runtime.reference";
const PANIC_ABI: &str = "bray.panic.unwind";
const SCHEDULER_CAPABILITIES: [RuntimeCapability; 6] = [
    RuntimeCapability::CooperativeExecution,
    RuntimeCapability::LocalLanes,
    RuntimeCapability::MigratableLanes,
    RuntimeCapability::BlockingLanes,
    RuntimeCapability::ComputeLanes,
    RuntimeCapability::MainThreadLane,
];
const SUPPORTED_CAPABILITIES: [RuntimeCapability; 10] = [
    RuntimeCapability::MemoryOperations,
    RuntimeCapability::StringOperations,
    RuntimeCapability::CharacterOperations,
    RuntimeCapability::CooperativeExecution,
    RuntimeCapability::LocalLanes,
    RuntimeCapability::MigratableLanes,
    RuntimeCapability::BlockingLanes,
    RuntimeCapability::ComputeLanes,
    RuntimeCapability::MainThreadLane,
    RuntimeCapability::Reactor,
];

pub(crate) fn run(mut arguments: impl Iterator<Item = String>) -> ExitCode {
    let result = match arguments.next().as_deref() {
        Some("build") => build_command(arguments).map(Some),
        Some("smoke-test") => smoke_test_command(arguments).map(|()| None),
        _ => Err(CommandError::Usage),
    };

    match result {
        Ok(Some(package)) => {
            println!("{}", package.metadata.display());

            for component in &package.components {
                println!("{}", component.archive.display());
            }

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

    build(target, &output, &options.profile)
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

    let package = build(target, &output, "release")?;

    smoke_test(&package, target, directory.path())?;

    Ok(())
}

pub(crate) fn smoke_test_host() -> Result<(), String> {
    smoke_test_command(std::iter::empty()).map_err(|error| error.to_string())
}

pub(crate) fn build_for_readiness(target: NativeTarget, output: &Path) -> Result<PathBuf, String> {
    build(target, output, "release")
        .map(|package| package.metadata)
        .map_err(|error| error.to_string())
}

fn build(target: NativeTarget, output: &Path, profile: &str) -> Result<Package, CommandError> {
    let publication = DirectoryPublication::begin(output, "bray-runtime-artifact-")
        .map_err(CommandError::Publication)?;

    build_contents(target, publication.contents(), profile)?;

    let output = publication.publish().map_err(CommandError::Publication)?;

    Ok(Package {
        metadata: output.join(METADATA_FILE_NAME),
        components: RuntimeArchiveKind::ALL
            .into_iter()
            .map(|kind| PackageComponent {
                kind,
                archive: output.join(archive_file_name(target, kind)),
            })
            .collect(),
    })
}

fn build_contents(target: NativeTarget, output: &Path, profile: &str) -> Result<(), CommandError> {
    let root = workspace::root().map_err(CommandError::Workspace)?;

    audit_dependency_boundaries(&root)?;

    let mut partitioner = super::partition::RuntimeArchivePartitioner::new(&root)?;
    let mut native_links = Vec::new();
    let metadata_path = output.join(METADATA_FILE_NAME);

    for kind in RuntimeArchiveKind::OWNING {
        let (crate_name, features, member_prefix) = match kind {
            RuntimeArchiveKind::Memory => {
                ("bray-runtime-builtins", &["memory"][..], "bray_runtime_builtins-")
            }
            RuntimeArchiveKind::String => {
                ("bray-runtime-builtins", &["string"][..], "bray_runtime_builtins-")
            }
            RuntimeArchiveKind::Character => (
                "bray-runtime-builtins",
                &["character"][..],
                "bray_runtime_builtins-",
            ),
            RuntimeArchiveKind::Host => {
                ("bray-runtime-adapter", &["host"][..], "bray_runtime_adapter-")
            }
            RuntimeArchiveKind::Scheduler => (
                "bray-runtime-adapter",
                &["scheduler"][..],
                "bray_runtime_adapter-",
            ),
            RuntimeArchiveKind::Cancellation => (
                "bray-runtime-adapter",
                &["cancellation"][..],
                "bray_runtime_adapter-",
            ),
            RuntimeArchiveKind::Event => {
                ("bray-runtime-adapter", &["event"][..], "bray_runtime_adapter-")
            }
            RuntimeArchiveKind::TestHost => (
                "bray-runtime-adapter",
                &["test-host"][..],
                "bray_runtime_adapter-",
            ),
            RuntimeArchiveKind::Common => unreachable!("common support is derived from owners"),
        };

        let built = crate::native_archive::build_rust_static_library(
            &root,
            target,
            crate_name,
            profile,
            features,
        )
        .map_err(CommandError::NativeArchive)?;

        partitioner.add(
            kind,
            built.archive(),
            member_prefix,
            kind == RuntimeArchiveKind::TestHost,
        )?;

        native_links.extend(built.native_links().iter().cloned());
    }

    partitioner.write(target, output)?;

    native_links.sort_by(|left, right| {
        (left.kind(), left.name()).cmp(&(right.kind(), right.name()))
    });

    native_links.dedup();

    let components = RuntimeArchiveKind::ALL
        .into_iter()
        .map(|kind| {
            let archive = output.join(archive_file_name(target, kind));

            Ok(BuiltComponent {
                kind,
                digest: digest_file(&archive)?,
                native_links: if kind == RuntimeArchiveKind::Common {
                    native_links.clone()
                } else {
                    Vec::new()
                },
                archive,
            })
        })
        .collect::<Result<Vec<_>, CommandError>>()?;

    let metadata_value = metadata(target, &components)?;

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
        "bray-runtime-builtins",
        &[
            "blake3",
            "bray-base",
            "bray-platform",
            "bray-runtime",
            "bray-runtime-interface",
            "bray-runtime-model",
            "serde",
            "serde_json",
            "sha2",
            "tempfile",
        ],
    )
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
    components: &[BuiltComponent],
) -> Result<RuntimeArtifactMetadata, CommandError> {
    let identity =
        RuntimeIdentity::try_new(RUNTIME_IDENTITY).ok_or(CommandError::MetadataContract)?;

    let artifact = RuntimeArtifactId::try_new(format!("{RUNTIME_IDENTITY}.{}", target.as_str()))
        .ok_or(CommandError::MetadataContract)?;

    let target_identity = target.identity();

    let panic_abi = PanicAbiIdentity::try_new(PANIC_ABI).ok_or(CommandError::MetadataContract)?;

    let contract = RuntimeContract::try_new(
        identity,
        artifact,
        RuntimeAbiVersion::new(1, 0),
        ProtectedFrameAbiVersions::uniform(RuntimeAbiVersion::new(1, 0)),
        target_identity,
        panic_abi,
        SUPPORTED_CAPABILITIES,
        runtime_role_bindings()?,
    )
    .map_err(|_| CommandError::MetadataContract)?;

    let mut metadata_components = Vec::new();

    for purpose in RuntimeArtifactPurpose::ALL {
        for (kind, name, capability) in [
            (
                RuntimeArchiveKind::Memory,
                "memory",
                RuntimeCapability::MemoryOperations,
            ),
            (
                RuntimeArchiveKind::String,
                "string",
                RuntimeCapability::StringOperations,
            ),
            (
                RuntimeArchiveKind::Character,
                "character",
                RuntimeCapability::CharacterOperations,
            ),
        ] {
            metadata_components.push(component_metadata(
                target,
                purpose,
                component(components, kind)?,
                name,
                [],
                [capability],
                false,
            )?);
        }

        let common_identity = component_identity(target, purpose, "common")?;

        let common = component_metadata(
            target,
            purpose,
            component(components, RuntimeArchiveKind::Common)?,
            "common",
            [],
            [],
            true,
        )?;

        metadata_components.push(common);

        for (kind, name, capabilities) in [
            (RuntimeArchiveKind::Host, "host", &[][..]),
            (
                RuntimeArchiveKind::Scheduler,
                "scheduler",
                &SCHEDULER_CAPABILITIES[..],
            ),
            (
                RuntimeArchiveKind::Cancellation,
                "cancellation",
                &[][..],
            ),
            (
                RuntimeArchiveKind::Event,
                "event",
                &[RuntimeCapability::Reactor][..],
            ),
        ] {
            let roles = contract
                .role_bindings()
                .iter()
                .map(RuntimeRoleBinding::role)
                .filter(|role| runtime_role_archive(*role) == Some(kind));

            metadata_components.push(
                component_metadata(
                    target,
                    purpose,
                    component(components, kind)?,
                    name,
                    roles,
                    capabilities.iter().copied(),
                    false,
                )?
                .with_dependencies([common_identity.clone()]),
            );
        }

        if purpose == RuntimeArtifactPurpose::TestRunner {
            metadata_components.push(
                component_metadata(
                    target,
                    purpose,
                    component(components, RuntimeArchiveKind::TestHost)?,
                    "test_host",
                    [RuntimeAbiRole::TestEntrySelection],
                    [],
                    false,
                )?
                .with_dependencies([common_identity.clone()]),
            );
        }
    }

    RuntimeArtifactMetadata::try_new(contract, metadata_components)
        .map_err(|_| CommandError::MetadataContract)
}

#[expect(
    clippy::too_many_arguments,
    reason = "runtime component publication keeps each ownership dimension explicit"
)]
fn component_metadata(
    target: NativeTarget,
    purpose: RuntimeArtifactPurpose,
    component: &BuiltComponent,
    name: &str,
    roles: impl IntoIterator<Item = RuntimeAbiRole>,
    capabilities: impl IntoIterator<Item = RuntimeCapability>,
    embeds_platform_services: bool,
) -> Result<RuntimeArtifactComponentMetadata, CommandError> {
    let identity = component_identity(target, purpose, name)?;

    let metadata = RuntimeArtifactComponentMetadata::try_new(
        identity,
        purpose,
        roles,
        capabilities,
        component
            .archive
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(CommandError::MetadataContract)?,
        component.digest,
    )
    .map_err(|_| CommandError::MetadataContract)?;

    let metadata = if embeds_platform_services {
        metadata.with_embedded_platform_services()
    } else {
        metadata
    };

    let native_links = if component.kind == RuntimeArchiveKind::Common {
        super::native_link::common_support_requirements(target, &component.native_links)
    } else {
        component.native_links.clone()
    };

    Ok(metadata.with_native_links(native_links))
}

fn component_identity(
    target: NativeTarget,
    purpose: RuntimeArtifactPurpose,
    name: &str,
) -> Result<RuntimeArtifactId, CommandError> {
    RuntimeArtifactId::try_new(format!(
        "{RUNTIME_IDENTITY}.{}.{}.{}",
        target.as_str(),
        purpose.as_str(),
        name,
    ))
    .ok_or(CommandError::MetadataContract)
}

fn runtime_role_archive(role: RuntimeAbiRole) -> Option<RuntimeArchiveKind> {
    Some(match role {
        RuntimeAbiRole::SynchronousRootExecution
        | RuntimeAbiRole::ForeignCallbackExecution
        | RuntimeAbiRole::CleanupIncidentReporting
        | RuntimeAbiRole::RootTerminalObservation
        | RuntimeAbiRole::RootCompletionResolution
        | RuntimeAbiRole::PanicReporting
        | RuntimeAbiRole::EntryFailureReporting
        | RuntimeAbiRole::StructuredShutdown
        | RuntimeAbiRole::PanicReportConstruction
        | RuntimeAbiRole::PanicPropagation => RuntimeArchiveKind::Host,
        RuntimeAbiRole::RootExecution
        | RuntimeAbiRole::TaskAllocation
        | RuntimeAbiRole::TaskStart
        | RuntimeAbiRole::SuspensionRegistration
        | RuntimeAbiRole::Wake
        | RuntimeAbiRole::JoinRegistration
        | RuntimeAbiRole::TerminalPublication
        | RuntimeAbiRole::CompatibleLaneSelection
        | RuntimeAbiRole::MainThreadLaneStartup
        | RuntimeAbiRole::MainThreadLaneDrive
        | RuntimeAbiRole::AwaitedFrameComposition
        | RuntimeAbiRole::FrameCompletionMove => RuntimeArchiveKind::Scheduler,
        RuntimeAbiRole::RootCancellationRequest
        | RuntimeAbiRole::TaskCancellationRequest
        | RuntimeAbiRole::CurrentRunCancellationObservation
        | RuntimeAbiRole::CurrentRunCancellationPropagation => RuntimeArchiveKind::Cancellation,
        RuntimeAbiRole::RuntimeEvent => RuntimeArchiveKind::Event,
        RuntimeAbiRole::TestEntrySelection => RuntimeArchiveKind::TestHost,
        _ => return None,
    })
}

fn component(
    components: &[BuiltComponent],
    kind: RuntimeArchiveKind,
) -> Result<&BuiltComponent, CommandError> {
    components
        .iter()
        .find(|component| component.kind == kind)
        .ok_or(CommandError::MetadataContract)
}

fn runtime_role_bindings() -> Result<Vec<RuntimeRoleBinding>, CommandError> {
    let bindings: Vec<_> = RuntimeAbiRole::ALL
        .into_iter()
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

pub(super) fn archive_file_name(target: NativeTarget, kind: RuntimeArchiveKind) -> String {
    let name = TargetOutputName::for_native(target.object_format(), TargetOutputKind::StaticLibrary)
        .file_name(kind.archive_stem());

    let Some(name) = name else {
        panic!("fixed runtime archive stems must form valid native output names")
    };

    name
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

pub(super) struct Package {
    metadata: PathBuf,
    pub(super) components: Vec<PackageComponent>,
}

pub(super) struct PackageComponent {
    pub(super) kind: RuntimeArchiveKind,
    pub(super) archive: PathBuf,
}

struct BuiltComponent {
    kind: RuntimeArchiveKind,
    archive: PathBuf,
    digest: RuntimeArtifactDigest,
    native_links: Vec<NativeLinkRequirement>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum RuntimeArchiveKind {
    Common,
    Memory,
    String,
    Character,
    Host,
    Scheduler,
    Cancellation,
    Event,
    TestHost,
}

impl RuntimeArchiveKind {
    const ALL: [Self; 9] = [
        Self::Common,
        Self::Memory,
        Self::String,
        Self::Character,
        Self::Host,
        Self::Scheduler,
        Self::Cancellation,
        Self::Event,
        Self::TestHost,
    ];

    const OWNING: [Self; 8] = [
        Self::Memory,
        Self::String,
        Self::Character,
        Self::Host,
        Self::Scheduler,
        Self::Cancellation,
        Self::Event,
        Self::TestHost,
    ];

    const fn archive_stem(self) -> &'static str {
        match self {
            Self::Common => "bray_runtime_common",
            Self::Memory => "bray_runtime_memory",
            Self::String => "bray_runtime_string",
            Self::Character => "bray_runtime_character",
            Self::Host => "bray_runtime_host",
            Self::Scheduler => "bray_runtime_scheduler",
            Self::Cancellation => "bray_runtime_cancellation",
            Self::Event => "bray_runtime_event",
            Self::TestHost => "bray_runtime_test_host",
        }
    }
}

#[derive(Debug)]
pub(super) enum CommandError {
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
    MetadataContract,
    MetadataEncoding,
    TemporaryDirectory(std::io::Error),
    NonUtf8Path,
    Rustc(std::io::Error),
    NativeSymbolToolUnavailable,
    NativeSymbolInspection(std::io::Error),
    NativeSymbolInspectionFailed,
    RuntimeComponentBoundary(RuntimeArchiveKind),
    RuntimePartitionTool(std::io::Error),
    RuntimePartitionToolUnavailable,
    RuntimePartitionFailed,
    RuntimePartitionMissingOwner(RuntimeArchiveKind),
    SynchronousLinkMapBoundary(String),
    HostTarget,
    SmokeLinkFailed,
    SmokeExecution(std::io::Error),
    SmokeExecutionFailed,
    CleanupReportMissing,
}

impl CommandError {
    pub(super) fn read(path: &Path, error: std::io::Error) -> Self {
        Self::Read {
            path: path.to_path_buf(),
            error,
        }
    }

    pub(super) fn write(path: &Path, error: std::io::Error) -> Self {
        Self::Write {
            path: path.to_path_buf(),
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
            Self::RuntimeComponentBoundary(kind) => {
                write!(formatter, "runtime {kind:?} archive has an invalid exported surface")
            }
            Self::RuntimePartitionTool(error) => {
                write!(formatter, "could not run runtime archive partition tool: {error}")
            }
            Self::RuntimePartitionToolUnavailable => {
                formatter.write_str("llvm-ar is unavailable for runtime archive partitioning")
            }
            Self::RuntimePartitionFailed => {
                formatter.write_str("runtime archive partitioning failed")
            }
            Self::RuntimePartitionMissingOwner(kind) => {
                write!(formatter, "runtime {kind:?} partition owns no archive members")
            }
            Self::SynchronousLinkMapBoundary(detail) => {
                write!(formatter, "synchronous runtime link map is invalid: {detail}")
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

#[cfg(test)]
mod tests {
    use bray_runtime_interface::{RuntimeAbiRole, RuntimeArtifactDigest};
    use bray_target::NativeTarget;

    use super::{
        BuildOptions, BuiltComponent, CommandError, RuntimeArchiveKind, archive_file_name,
        metadata, runtime_role_bindings,
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
            archive_file_name(NativeTarget::X86_64WindowsMsvc, RuntimeArchiveKind::Memory),
            "bray_runtime_memory.lib"
        );

        assert_eq!(
            archive_file_name(
                NativeTarget::Aarch64WindowsMsvc,
                RuntimeArchiveKind::TestHost
            ),
            "bray_runtime_test_host.lib"
        );

        assert_eq!(
            archive_file_name(
                NativeTarget::X86_64LinuxGnu,
                RuntimeArchiveKind::Host
            ),
            "libbray_runtime_host.a"
        );

        assert_eq!(
            archive_file_name(NativeTarget::Aarch64LinuxGnu, RuntimeArchiveKind::Character),
            "libbray_runtime_character.a"
        );

        assert_eq!(
            archive_file_name(
                NativeTarget::X86_64MacOs,
                RuntimeArchiveKind::Scheduler
            ),
            "libbray_runtime_scheduler.a"
        );

        assert_eq!(
            archive_file_name(
                NativeTarget::Aarch64MacOs,
                RuntimeArchiveKind::TestHost
            ),
            "libbray_runtime_test_host.a"
        );
    }

    #[test]
    fn runtime_metadata_covers_every_native_target_reproducibly() {
        for target in NativeTarget::ALL {
            let digest = RuntimeArtifactDigest::new([7; 32]);

            let components = RuntimeArchiveKind::ALL.map(|kind| BuiltComponent {
                kind,
                archive: archive_file_name(target, kind).into(),
                digest,
                native_links: Vec::new(),
            });

            let first = metadata(target, &components)
                .unwrap_or_else(|error| panic!("runtime metadata must be valid: {error}"));

            let second = metadata(target, &components)
                .unwrap_or_else(|error| panic!("runtime metadata must be valid: {error}"));

            assert_eq!(first, second);
            assert_eq!(first.contract().target().as_str(), target.as_str());
            assert_eq!(first.components().len(), 17);

            let common = first
                .components()
                .iter()
                .find(|component| component.identity().as_str().ends_with("product.common"))
                .unwrap_or_else(|| panic!("runtime metadata must contain product support"));

            let has_synchronization = common
                .native_links()
                .iter()
                .any(|requirement| requirement.name() == "synchronization");

            assert_eq!(
                has_synchronization,
                matches!(
                    target,
                    NativeTarget::X86_64WindowsMsvc | NativeTarget::Aarch64WindowsMsvc
                )
            );

            assert_eq!(
                first
                    .components()
                    .iter()
                    .filter(|component| component.embeds_platform_services())
                    .count(),
                2
            );

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
        let bindings = runtime_role_bindings()
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
                RuntimeAbiRole::TestEntrySelection,
                RuntimeAbiRole::StructuredShutdown,
                RuntimeAbiRole::FrameCompletionMove,
                RuntimeAbiRole::PanicReportConstruction,
                RuntimeAbiRole::PanicPropagation,
                RuntimeAbiRole::AwaitedFrameComposition,
            ]
        );

        let test_bindings = runtime_role_bindings()
            .unwrap_or_else(|error| panic!("test runtime role bindings must be valid: {error}"));

        assert!(
            test_bindings
                .iter()
                .any(|binding| binding.role() == RuntimeAbiRole::TestEntrySelection)
        );
    }
}
