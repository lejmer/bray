// rust-style: allow(module-too-large, reason = "runtime artifact command parsing, construction, and metadata publication form one reproducible packaging contract")

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use super::smoke::smoke_test;
use crate::bundle::{
    DirectoryPublication, DirectoryPublicationError, NativeBuildOptions, NativeBuildOptionsBuilder,
    NativeBuildOptionsError,
};
use crate::workspace;
use bray_runtime_interface::{
    BinarySymbolName, PanicAbiIdentity, PlatformServiceRole, ProtectedFrameAbiVersions,
    RuntimeAbiRole, RuntimeAbiVersion, RuntimeArtifactComponentMetadata, RuntimeArtifactId,
    RuntimeArtifactMetadata, RuntimeArtifactPurpose, RuntimeCapability, RuntimeContract,
    RuntimeIdentity, RuntimeNativeIndexMetadata, RuntimeRoleBinding,
};
use bray_symbols::NativeLinkRequirement;
use bray_target::{NativeTarget, TargetOutputKind, TargetOutputName};

const USAGE: &str = "usage: cargo xtask runtime-artifact \
    <build --output <directory> [--target <triple>] [--profile <profile>] | smoke-test>";
pub(super) const METADATA_FILE_NAME: &str = "bray-runtime.brayrt";
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
const SUPPORTED_CAPABILITIES: [RuntimeCapability; 8] = [
    RuntimeCapability::PerformanceObservation,
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

    let mut package = build(target, &output, "release")?;

    super::smoke::add_package_dependencies(&mut package, target, directory.path())?;

    crate::progress::run("Running runtime artifact smoke tests", || {
        smoke_test(&package, target, directory.path())
    })?;

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
    DirectoryPublication::recover(output).map_err(CommandError::Publication)?;

    let root = workspace::root().map_err(CommandError::Workspace)?;

    let sources = crate::input_identity::WorkspaceSources::load(&root)
        .map_err(CommandError::InputIdentity)?;

    let standard_library_source = root.join("standard-library");

    let standard_library_input = crate::input_identity::input_digest(
        &root,
        &[target],
        crate::input_identity::Component::StandardLibrary,
        &[],
        &[&standard_library_source],
        &sources,
    )
    .map_err(CommandError::InputIdentity)?;

    let input = crate::input_identity::input_digest(
        &root,
        &[target],
        crate::input_identity::Component::Runtime,
        &[profile, &standard_library_input],
        &[],
        &sources,
    )
    .map_err(CommandError::InputIdentity)?;

    let existing_package = package(output, target);

    let expected_archives = existing_package
        .components
        .iter()
        .map(|component| component.archive.clone())
        .collect::<Vec<_>>();

    let existing_identity = super::bootstrap::cache_identity(&root, target, output, &input, &expected_archives)
        .map_err(CommandError::Bootstrap)?;

    if let Some(identity) = existing_identity
        && super::reuse::current(
            output,
            &existing_package.metadata,
            &identity,
        )
        .map_err(CommandError::InputIdentity)?
    {
        crate::progress::message("Reusing native runtime artifacts");

        return Ok(existing_package);
    }

    let publication = DirectoryPublication::begin(output, "bray-runtime-artifact-")
        .map_err(CommandError::Publication)?;

    let producer = bray_native_artifact::NativeContentDigest::from_hex(&input)
        .expect("computed runtime input identity must be a SHA-256 digest");

    build_contents(target, publication.contents(), profile, producer)?;

    let archives = package(publication.contents(), target).components.into_iter()
        .map(|component| component.archive).collect::<Vec<_>>();

    let identity = super::bootstrap::cache_identity(&root, target, publication.contents(), &input, &archives)
        .map_err(CommandError::Bootstrap)?
        .ok_or_else(|| CommandError::Bootstrap("bootstrap artifacts are missing".to_owned()))?;

    crate::input_identity::write_digest(publication.contents(), &identity)
        .map_err(CommandError::InputIdentity)?;

    let output = publication.publish().map_err(CommandError::Publication)?;

    Ok(package(&output, target))
}

fn package(output: &Path, target: NativeTarget) -> Package {
    let mut components = RuntimeArchiveKind::ALL
        .into_iter()
        .map(|kind| PackageComponent {
            kind: Some(kind),
            archive: output.join(archive_file_name(target, kind)),
        })
        .collect::<Vec<_>>();

    if let Ok(entries) = fs::read_dir(output) {
        for entry in entries.flatten() {
            let path = entry.path();

            if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.starts_with("bray_runtime_support_")
                        || name.starts_with("libbray_runtime_support_")
                })
            {
                components.push(PackageComponent {
                    kind: None,
                    archive: path,
                });
            }
        }
    }

    components.sort_by(|left, right| left.archive.cmp(&right.archive));

    Package {
        metadata: output.join(METADATA_FILE_NAME),
        components,
        native_links: Vec::new(),
    }
}

fn build_contents(
    target: NativeTarget,
    output: &Path,
    profile: &str,
    producer: bray_native_artifact::NativeContentDigest,
) -> Result<(), CommandError> {
    let root = workspace::root().map_err(CommandError::Workspace)?;

    crate::progress::run("Auditing runtime dependency boundaries", || {
        audit_dependency_boundaries(&root)?;

        super::contract::validate(&root).map_err(CommandError::RoleContract)
    })?;

    let mut partitioner = super::partition::RuntimeArchivePartitioner::new(target)?;
    let mut native_links = BTreeMap::<RuntimeArchiveKind, Vec<NativeLinkRequirement>>::new();
    let metadata_path = output.join(METADATA_FILE_NAME);

    let archive_count = RuntimeArchiveKind::OWNING.len();

    for (index, kind) in RuntimeArchiveKind::OWNING.into_iter().enumerate() {
        crate::progress::item(
            index.saturating_add(1),
            archive_count,
            &format!("Building the {kind:?} runtime archive"),
        );

        let (crate_name, features, member_prefix) = match kind {
            RuntimeArchiveKind::Host => (
                "bray-runtime-adapter",
                &["host"][..],
                "bray_runtime_adapter-",
            ),
            RuntimeArchiveKind::Callback => (
                "bray-runtime-adapter",
                &["callback"][..],
                "bray_runtime_adapter-",
            ),
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
            RuntimeArchiveKind::Event => (
                "bray-runtime-adapter",
                &["event"][..],
                "bray_runtime_adapter-",
            ),
            RuntimeArchiveKind::TestHost => (
                "bray-runtime-adapter",
                &["test-host"][..],
                "bray_runtime_adapter-",
            ),
            RuntimeArchiveKind::Observation | RuntimeArchiveKind::Bootstrap => {
                unreachable!("support archives are derived from component owners")
            }
        };

        let built = crate::native_archive::build_rust_static_library(
            &root, target, crate_name, profile, features,
        )
        .map_err(CommandError::NativeArchive)?;

        partitioner.add(
            kind,
            built.archive(),
            member_prefix,
            kind == RuntimeArchiveKind::TestHost,
        )?;

        native_links.insert(kind, built.native_links().to_vec());
    }

    let support = crate::progress::run("Partitioning runtime archives", || {
        partitioner.write(target, output)
    })?;

    crate::progress::run("Building the trusted Bray runtime archives", || {
        super::bootstrap::build_runtime_components(
            &root,
            target,
            &output.join(archive_file_name(target, RuntimeArchiveKind::Bootstrap)),
            &output.join(archive_file_name(target, RuntimeArchiveKind::Observation)),
        )
        .map_err(CommandError::Bootstrap)
    })?;

    native_links.insert(
        RuntimeArchiveKind::Bootstrap,
        crate::standard_library::native_links(&target.identity())
            .map_err(CommandError::Bootstrap)?,
    );

    let components = RuntimeArchiveKind::ALL
        .into_iter()
        .map(|kind| {
            let archive = output.join(archive_file_name(target, kind));

            BuiltComponent {
                kind,
                native_links: native_links.remove(&kind).unwrap_or_default(),
                archive,
            }
        })
        .collect::<Vec<_>>();

    let support = support
        .into_iter()
        .map(|support| BuiltSupportComponent {
            owners: support.owners,
            archive: support.archive,
        })
        .collect::<Vec<_>>();

    crate::progress::run("Validating runtime archive exports", || {
        super::archive::validate(&root, &package(output, target), target)
    })?;

    let native_indexes = crate::progress::run("Publishing runtime native indexes", || {
        super::native_index::publish(target, output, producer, &components, &support)
    })?;

    let metadata_value = metadata(
        target,
        native_indexes.into_iter().map(|index| {
            RuntimeNativeIndexMetadata::try_new(index.purpose, index.file_name, index.digest)
                .expect("published runtime native index has a valid file name")
        }),
    )
    .map_err(|_| CommandError::MetadataContract)?;

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
    indexes: impl IntoIterator<Item = RuntimeNativeIndexMetadata>,
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
        metadata_components.push(
            component_metadata(
                target,
                purpose,
                "bootstrap",
                RuntimeArchiveKind::Bootstrap.runtime_roles(),
                [],
            )?
            .with_platform_services(RuntimeArchiveKind::Bootstrap.platform_services()),
        );

        metadata_components.push(component_metadata(
            target,
            purpose,
            "observation",
            RuntimeArchiveKind::Observation.runtime_roles(),
            [RuntimeCapability::PerformanceObservation],
        )?);

        if purpose == RuntimeArtifactPurpose::Product {
            for (kind, name, capabilities) in [
                (RuntimeArchiveKind::Host, "host", &[][..]),
                (RuntimeArchiveKind::Callback, "callback", &[][..]),
                (
                    RuntimeArchiveKind::Scheduler,
                    "scheduler",
                    &SCHEDULER_CAPABILITIES[..],
                ),
                (RuntimeArchiveKind::Cancellation, "cancellation", &[][..]),
                (
                    RuntimeArchiveKind::Event,
                    "event",
                    &[RuntimeCapability::Reactor][..],
                ),
            ] {
                metadata_components.push(component_metadata(
                    target,
                    purpose,
                    name,
                    kind.runtime_roles(),
                    capabilities.iter().copied(),
                )?);
            }
        }

        if purpose == RuntimeArtifactPurpose::TestRunner {
            metadata_components.push(
                component_metadata(
                    target,
                    purpose,
                    "test_host",
                    RuntimeArchiveKind::TestHost.runtime_roles(),
                    SCHEDULER_CAPABILITIES
                        .into_iter()
                        .chain([RuntimeCapability::Reactor]),
                )?
                .with_platform_services(RuntimeArchiveKind::TestHost.platform_services()),
            );
        }
    }

    RuntimeArtifactMetadata::try_new(contract, metadata_components, indexes)
        .map_err(|_| CommandError::MetadataContract)
}

fn component_metadata(
    target: NativeTarget,
    purpose: RuntimeArtifactPurpose,
    name: &str,
    roles: impl IntoIterator<Item = RuntimeAbiRole>,
    capabilities: impl IntoIterator<Item = RuntimeCapability>,
) -> Result<RuntimeArtifactComponentMetadata, CommandError> {
    let identity = component_identity(target, purpose, name)?;

    RuntimeArtifactComponentMetadata::try_new(identity, purpose, roles, capabilities)
        .map_err(|_| CommandError::MetadataContract)
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

pub(super) fn runtime_role_archive(role: RuntimeAbiRole) -> Option<RuntimeArchiveKind> {
    use bray_runtime_interface::RuntimeRoleArtifact;

    Some(match role.artifact_owner() {
        RuntimeRoleArtifact::Compiler => return None,
        RuntimeRoleArtifact::Bootstrap => RuntimeArchiveKind::Bootstrap,
        RuntimeRoleArtifact::Observation => RuntimeArchiveKind::Observation,
        RuntimeRoleArtifact::Host => RuntimeArchiveKind::Host,
        RuntimeRoleArtifact::Callback => RuntimeArchiveKind::Callback,
        RuntimeRoleArtifact::Scheduler => RuntimeArchiveKind::Scheduler,
        RuntimeRoleArtifact::Cancellation => RuntimeArchiveKind::Cancellation,
        RuntimeRoleArtifact::Event => RuntimeArchiveKind::Event,
        RuntimeRoleArtifact::TestHost => RuntimeArchiveKind::TestHost,
    })
}

fn runtime_role_bindings() -> Result<Vec<RuntimeRoleBinding>, CommandError> {
    let bindings: Vec<_> = RuntimeAbiRole::ALL
        .into_iter()
        .filter_map(|role| role.native_symbol().map(|name| (role, name)))
        .map(|(role, name)| {
            let symbol = BinarySymbolName::try_new(name).ok_or(CommandError::MetadataContract)?;

            Ok(RuntimeRoleBinding::new(role, symbol, role.implementation()))
        })
        .collect::<Result<_, _>>()?;

    Ok(bindings)
}

pub(super) fn archive_file_name(target: NativeTarget, kind: RuntimeArchiveKind) -> String {
    let name =
        TargetOutputName::for_native(target.object_format(), TargetOutputKind::StaticLibrary)
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
    pub(super) metadata: PathBuf,
    pub(super) components: Vec<PackageComponent>,
    pub(super) native_links: Vec<NativeLinkRequirement>,
}

pub(super) struct PackageComponent {
    pub(super) kind: Option<RuntimeArchiveKind>,
    pub(super) archive: PathBuf,
}

pub(super) struct BuiltComponent {
    pub(super) kind: RuntimeArchiveKind,
    pub(super) archive: PathBuf,
    pub(super) native_links: Vec<NativeLinkRequirement>,
}

pub(super) struct BuiltSupportComponent {
    pub(super) owners: std::collections::BTreeSet<RuntimeArchiveKind>,
    pub(super) archive: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum RuntimeArchiveKind {
    Observation,
    Bootstrap,
    Host,
    Callback,
    Scheduler,
    Cancellation,
    Event,
    TestHost,
}

impl RuntimeArchiveKind {
    pub(super) const ALL: [Self; 8] = [
        Self::Observation,
        Self::Bootstrap,
        Self::Host,
        Self::Callback,
        Self::Scheduler,
        Self::Cancellation,
        Self::Event,
        Self::TestHost,
    ];

    const OWNING: [Self; 6] = [
        Self::Host,
        Self::Callback,
        Self::Scheduler,
        Self::Cancellation,
        Self::Event,
        Self::TestHost,
    ];

    pub(super) const fn archive_stem(self) -> &'static str {
        match self {
            Self::Observation => "bray_runtime_observation",
            Self::Bootstrap => "bray_runtime_bootstrap",
            Self::Host => "bray_runtime_host",
            Self::Callback => "bray_runtime_callback",
            Self::Scheduler => "bray_runtime_scheduler",
            Self::Cancellation => "bray_runtime_cancellation",
            Self::Event => "bray_runtime_event",
            Self::TestHost => "bray_runtime_test_host",
        }
    }

    pub(super) const fn component_name(self) -> &'static str {
        match self {
            Self::Observation => "observation",
            Self::Bootstrap => "bootstrap",
            Self::Host => "host",
            Self::Callback => "callback",
            Self::Scheduler => "scheduler",
            Self::Cancellation => "cancellation",
            Self::Event => "event",
            Self::TestHost => "test_host",
        }
    }

    pub(super) fn runtime_roles(self) -> impl Iterator<Item = RuntimeAbiRole> {
        RuntimeAbiRole::ALL.into_iter().filter(move |role| {
            if role.native_symbol().is_none() {
                return false;
            }

            if let Some(source) = role.source_artifact() {
                return match source {
                    bray_runtime_interface::RuntimeRoleArtifact::Bootstrap => {
                        self == Self::Bootstrap
                    }
                    bray_runtime_interface::RuntimeRoleArtifact::Observation => {
                        self == Self::Observation
                    }
                    _ => unreachable!("runtime source role has a non-source artifact"),
                };
            }

            self == Self::TestHost || runtime_role_archive(*role) == Some(self)
        })
    }

    pub(super) fn platform_services(self) -> impl Iterator<Item = PlatformServiceRole> {
        PlatformServiceRole::ALL
            .iter()
            .copied()
            .filter(move |role| match self {
                Self::TestHost => {
                    role.family() == bray_runtime_interface::PlatformServiceFamily::StandardStreams
                }
                Self::Bootstrap => role.bootstrap_declaration().is_some(),
                Self::Observation
                | Self::Host
                | Self::Callback
                | Self::Scheduler
                | Self::Cancellation
                | Self::Event => false,
            })
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
    InputIdentity(String),
    DependencyAudit(crate::dependency_audit::DependencyAuditError),
    NativeArchive(crate::native_archive::BuildError),
    Bootstrap(String),
    Read {
        path: PathBuf,
        error: std::io::Error,
    },
    Write {
        path: PathBuf,
        error: std::io::Error,
    },
    MetadataContract,
    RoleContract(String),
    MetadataEncoding,
    TemporaryDirectory(std::io::Error),
    NonUtf8Path,
    Rustc(std::io::Error),
    NativeSymbolInspection(crate::native_symbols::InspectionError),
    NativeCompilerToolUnavailable(bray_tooling::LlvmToolPathError),
    NativeCompiler(std::io::Error),
    RuntimeRoleExports {
        kind: RuntimeArchiveKind,
        error: crate::native_symbols::ExportMismatch,
    },
    RuntimeComponentBoundary {
        kind: RuntimeArchiveKind,
        forbidden: Vec<String>,
    },
    RuntimePartitionTool(std::io::Error),
    RuntimePartitionToolUnavailable(bray_tooling::LlvmToolPathError),
    RuntimePartitionFailed,
    RuntimePartitionIndex(bray_native_artifact::NativeIndexError),
    RuntimePartitionResolution(bray_native_artifact::NativeResolutionError),
    RuntimePartitionMissingOwner(RuntimeArchiveKind),
    SynchronousLinkMapBoundary(String),
    BootstrapLinkMapBoundary(String),
    HostTarget,
    SmokeLinkFailed,
    SmokeExecution {
        name: &'static str,
        error: std::io::Error,
    },
    SmokeExecutionFailed {
        name: &'static str,
        status: std::process::ExitStatus,
    },
    NativeSmokeLinkFailed(String),
    BootstrapSmokeExecution(std::io::Error),
    BootstrapSmokeExecutionFailed(std::process::ExitStatus),
    ObservationSmoke(String),
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
            Self::InputIdentity(error) => formatter.write_str(error),
            Self::DependencyAudit(error) => write!(formatter, "{error}"),
            Self::NativeArchive(error) => write!(formatter, "{error}"),
            Self::Bootstrap(error) => formatter.write_str(error),
            Self::Read { path, error } => {
                write!(formatter, "could not read {}: {error}", path.display())
            }
            Self::Write { path, error } => {
                write!(formatter, "could not write {}: {error}", path.display())
            }
            Self::RoleContract(error) => formatter.write_str(error),
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
            Self::NativeSymbolInspection(error) => {
                write!(
                    formatter,
                    "could not inspect runtime archive symbols: {error}"
                )
            }
            Self::NativeCompilerToolUnavailable(error) => {
                write!(
                    formatter,
                    "clang is unavailable for runtime artifact inspection: {error}"
                )
            }
            Self::NativeCompiler(error) => {
                write!(formatter, "could not run native compiler: {error}")
            }
            Self::RuntimeRoleExports { kind, error } => {
                write!(
                    formatter,
                    "runtime {kind:?} archive role exports do not match the catalog: {error}"
                )
            }
            Self::RuntimeComponentBoundary { kind, forbidden } => {
                write!(
                    formatter,
                    "runtime {kind:?} archive has forbidden exports [{}]",
                    forbidden.join(", ")
                )
            }
            Self::RuntimePartitionTool(error) => {
                write!(
                    formatter,
                    "could not run runtime archive partition tool: {error}"
                )
            }
            Self::RuntimePartitionToolUnavailable(error) => {
                write!(
                    formatter,
                    "llvm-ar is unavailable for runtime archive partitioning: {error}"
                )
            }
            Self::RuntimePartitionFailed => {
                formatter.write_str("runtime archive partitioning failed")
            }
            Self::RuntimePartitionIndex(error) => {
                write!(formatter, "runtime member index is invalid: {error:?}")
            }
            Self::RuntimePartitionResolution(error) => {
                write!(formatter, "runtime member selection failed: {error:?}")
            }
            Self::RuntimePartitionMissingOwner(kind) => {
                write!(
                    formatter,
                    "runtime {kind:?} partition owns no archive members"
                )
            }
            Self::SynchronousLinkMapBoundary(detail) => {
                write!(
                    formatter,
                    "synchronous runtime link map is invalid: {detail}"
                )
            }
            Self::BootstrapLinkMapBoundary(detail) => {
                write!(formatter, "bootstrap runtime link map is invalid: {detail}")
            }
            Self::HostTarget => formatter.write_str("could not determine rustc host target"),
            Self::SmokeLinkFailed => formatter.write_str("runtime artifact smoke link failed"),
            Self::SmokeExecution { name, error } => {
                write!(
                    formatter,
                    "could not run {name} runtime smoke executable: {error}"
                )
            }
            Self::SmokeExecutionFailed { name, status } => {
                write!(
                    formatter,
                    "{name} runtime smoke execution failed with {status}"
                )
            }
            Self::NativeSmokeLinkFailed(name) => {
                write!(formatter, "{name} runtime smoke link failed")
            }
            Self::BootstrapSmokeExecution(error) => {
                write!(
                    formatter,
                    "could not run bootstrap runtime smoke executable: {error}"
                )
            }
            Self::BootstrapSmokeExecutionFailed(status) => {
                write!(
                    formatter,
                    "bootstrap runtime smoke execution failed with {status}"
                )
            }
            Self::ObservationSmoke(detail) => {
                write!(formatter, "observation runtime smoke failed: {detail}")
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
    use bray_runtime_interface::{
        PlatformServiceRole, RuntimeAbiRole, RuntimeArtifactDigest, RuntimeArtifactPurpose,
        RuntimeNativeIndexMetadata,
    };
    use bray_target::NativeTarget;

    use super::{
        BuildOptions, CommandError, RuntimeArchiveKind, archive_file_name, metadata,
        runtime_role_archive, runtime_role_bindings,
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
            archive_file_name(
                NativeTarget::X86_64WindowsMsvc,
                RuntimeArchiveKind::Observation
            ),
            "bray_runtime_observation.lib"
        );

        assert_eq!(
            archive_file_name(
                NativeTarget::Aarch64WindowsMsvc,
                RuntimeArchiveKind::TestHost
            ),
            "bray_runtime_test_host.lib"
        );

        assert_eq!(
            archive_file_name(NativeTarget::X86_64LinuxGnu, RuntimeArchiveKind::Host),
            "libbray_runtime_host.a"
        );

        assert_eq!(
            archive_file_name(NativeTarget::Aarch64LinuxGnu, RuntimeArchiveKind::Callback),
            "libbray_runtime_callback.a"
        );

        assert_eq!(
            archive_file_name(
                NativeTarget::Aarch64LinuxGnu,
                RuntimeArchiveKind::Observation
            ),
            "libbray_runtime_observation.a"
        );

        assert_eq!(
            archive_file_name(NativeTarget::X86_64MacOs, RuntimeArchiveKind::Scheduler),
            "libbray_runtime_scheduler.a"
        );

        assert_eq!(
            archive_file_name(NativeTarget::Aarch64MacOs, RuntimeArchiveKind::TestHost),
            "libbray_runtime_test_host.a"
        );
    }

    #[test]
    fn runtime_metadata_covers_every_native_target_reproducibly() {
        for target in NativeTarget::ALL {
            let indexes = RuntimeArtifactPurpose::ALL.map(|purpose| {
                RuntimeNativeIndexMetadata::try_new(
                    purpose,
                    format!("{}.json", purpose.as_str()),
                    RuntimeArtifactDigest::new([7; 32]),
                )
                .expect("test index name must be valid")
            });

            let first = metadata(target, indexes.clone())
                .unwrap_or_else(|error| panic!("runtime metadata must be valid: {error}"));

            let second = metadata(target, indexes)
                .unwrap_or_else(|error| panic!("runtime metadata must be valid: {error}"));

            assert_eq!(first, second);
            assert_eq!(first.contract().target().as_str(), target.as_str());
            assert_eq!(first.components().len(), 10);

            let observation = first
                .components()
                .iter()
                .find(|component| {
                    component
                        .identity()
                        .as_str()
                        .ends_with("product.observation")
                })
                .unwrap_or_else(|| panic!("runtime metadata must contain observation support"));

            assert_eq!(
                observation.roles(),
                &[
                    RuntimeAbiRole::MemoryObservationBegin,
                    RuntimeAbiRole::MemoryAllocationObservation,
                    RuntimeAbiRole::MemoryCopyObservation,
                    RuntimeAbiRole::PerformanceIntervalBegin,
                    RuntimeAbiRole::PerformanceIntervalEnd,
                ]
            );

            let callback = first
                .components()
                .iter()
                .find(|component| component.identity().as_str().ends_with("product.callback"))
                .unwrap_or_else(|| panic!("runtime metadata must contain callback support"));

            let callback_roles = RuntimeAbiRole::ALL
                .into_iter()
                .filter(|role| {
                    role.source_declaration().is_none()
                        && runtime_role_archive(*role) == Some(RuntimeArchiveKind::Callback)
                })
                .collect::<Vec<_>>();

            assert_eq!(callback.roles(), callback_roles);

            let bootstrap = first
                .components()
                .iter()
                .find(|component| component.identity().as_str().ends_with("product.bootstrap"))
                .unwrap_or_else(|| panic!("runtime metadata must contain bootstrap support"));

            assert_eq!(
                bootstrap.roles(),
                RuntimeAbiRole::ALL
                    .into_iter()
                    .filter(|role| {
                        role.source_artifact()
                            == Some(bray_runtime_interface::RuntimeRoleArtifact::Bootstrap)
                    })
                    .collect::<Vec<_>>()
            );

            assert_eq!(
                bootstrap.platform_services(),
                RuntimeArchiveKind::Bootstrap
                    .platform_services()
                    .collect::<Vec<_>>()
            );

            let test_host = first
                .components()
                .iter()
                .find(|component| {
                    component
                        .identity()
                        .as_str()
                        .ends_with("test_runner.test_host")
                })
                .unwrap_or_else(|| panic!("runtime metadata must contain test output support"));

            assert_eq!(
                test_host.platform_services(),
                &[
                    PlatformServiceRole::StandardInputRead,
                    PlatformServiceRole::StandardInputLock,
                    PlatformServiceRole::StandardInputUnlock,
                    PlatformServiceRole::StandardOutputWrite,
                    PlatformServiceRole::StandardOutputFlush,
                    PlatformServiceRole::StandardOutputLock,
                    PlatformServiceRole::StandardOutputUnlock,
                    PlatformServiceRole::StandardErrorWrite,
                    PlatformServiceRole::StandardErrorFlush,
                    PlatformServiceRole::StandardErrorLock,
                    PlatformServiceRole::StandardErrorUnlock,
                ]
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
                RuntimeAbiRole::ReportRecordAdmission,
                RuntimeAbiRole::ReportRecordTake,
                RuntimeAbiRole::ReportRecordExchange,
                RuntimeAbiRole::ReportRecordAppend,
                RuntimeAbiRole::ReportRecordPop,
                RuntimeAbiRole::ReportSegmentMark,
                RuntimeAbiRole::ReportSegmentTake,
                RuntimeAbiRole::ReportConsumer,
                RuntimeAbiRole::RuntimeInitialization,
                RuntimeAbiRole::MemoryObservationBegin,
                RuntimeAbiRole::MemoryAllocationObservation,
                RuntimeAbiRole::MemoryCopyObservation,
                RuntimeAbiRole::PerformanceIntervalBegin,
                RuntimeAbiRole::PerformanceIntervalEnd,
                RuntimeAbiRole::RootExecution,
                RuntimeAbiRole::SynchronousRootExecution,
                RuntimeAbiRole::ForeignCallbackExecution,
                RuntimeAbiRole::NativeThreadExecution,
                RuntimeAbiRole::CurrentNativeThreadIdentity,
                RuntimeAbiRole::MainNativeThreadIdentity,
                RuntimeAbiRole::TaskEventCreation,
                RuntimeAbiRole::TaskEventSignal,
                RuntimeAbiRole::TaskEventDestruction,
                RuntimeAbiRole::ThreadAttachmentIdentity,
                RuntimeAbiRole::ThreadStaticCleanupRegistration,
                RuntimeAbiRole::ProductHostControl,
                RuntimeAbiRole::RootCancellationRequest,
                RuntimeAbiRole::TaskAllocation,
                RuntimeAbiRole::TaskStart,
                RuntimeAbiRole::SuspensionRegistration,
                RuntimeAbiRole::Wake,
                RuntimeAbiRole::TaskCancellationRequest,
                RuntimeAbiRole::CurrentRunCancellationObservation,
                RuntimeAbiRole::CurrentRunCancellationPropagation,
                RuntimeAbiRole::CleanupShieldEnter,
                RuntimeAbiRole::CleanupShieldLeave,
                RuntimeAbiRole::JoinRegistration,
                RuntimeAbiRole::TaskObservationCreation,
                RuntimeAbiRole::TaskResolution,
                RuntimeAbiRole::RuntimeEvent,
                RuntimeAbiRole::CompatibleLaneSelection,
                RuntimeAbiRole::CleanupIncidentReporting,
                RuntimeAbiRole::MainThreadLaneStartup,
                RuntimeAbiRole::MainThreadLaneDrive,
                RuntimeAbiRole::RootTerminalObservation,
                RuntimeAbiRole::RootCompletionResolution,
                RuntimeAbiRole::PanicReporting,
                RuntimeAbiRole::PanicReportDestruction,
                RuntimeAbiRole::OutgoingAdmission,
                RuntimeAbiRole::OutgoingDischarge,
                RuntimeAbiRole::OutgoingActivation,
                RuntimeAbiRole::OutgoingRetirement,
                RuntimeAbiRole::PanicReportSuppression,
                RuntimeAbiRole::EntryFailureReporting,
                RuntimeAbiRole::TestEntrySelection,
                RuntimeAbiRole::StructuredShutdown,
                RuntimeAbiRole::FrameCompletionMove,
                RuntimeAbiRole::PanicReportConstruction,
                RuntimeAbiRole::PanicPropagation,
                RuntimeAbiRole::AwaitedFrameComposition,
                RuntimeAbiRole::TaskDestruction,
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
