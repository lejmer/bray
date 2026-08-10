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
const EXECUTION_CAPABILITIES: [RuntimeCapability; 3] = [
    RuntimeCapability::CooperativeExecution,
    RuntimeCapability::LocalLanes,
    RuntimeCapability::MainThreadLane,
];
const SUPPORTED_CAPABILITIES: [RuntimeCapability; 6] = [
    RuntimeCapability::MemoryOperations,
    RuntimeCapability::StringOperations,
    RuntimeCapability::CharacterOperations,
    RuntimeCapability::CooperativeExecution,
    RuntimeCapability::LocalLanes,
    RuntimeCapability::MainThreadLane,
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

    let mut components = Vec::new();
    let metadata_path = output.join(METADATA_FILE_NAME);

    for kind in RuntimeArchiveKind::ALL {
        let archive_file_name = archive_file_name(target, kind);

        let (crate_name, features) = match kind {
            RuntimeArchiveKind::Memory => ("bray-runtime-builtins", &["memory"][..]),
            RuntimeArchiveKind::String => ("bray-runtime-builtins", &["string"][..]),
            RuntimeArchiveKind::Character => ("bray-runtime-builtins", &["character"][..]),
            RuntimeArchiveKind::ProductExecution => ("bray-runtime", &[][..]),
            RuntimeArchiveKind::TestExecution => ("bray-runtime", &["test-host"][..]),
        };

        let built = crate::native_archive::build_rust_static_library(
            &root,
            target,
            crate_name,
            profile,
            features,
        )
        .map_err(CommandError::NativeArchive)?;

        let archive = output.join(archive_file_name);

        fs::copy(built.archive(), &archive)
            .map_err(|error| CommandError::copy(built.archive(), &archive, error))?;

        components.push(BuiltComponent {
            kind,
            digest: digest_file(&archive)?,
            native_links: built.native_links().to_vec(),
            archive,
        });
    }

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

        let execution = match purpose {
            RuntimeArtifactPurpose::Product => RuntimeArchiveKind::ProductExecution,
            RuntimeArtifactPurpose::TestRunner => RuntimeArchiveKind::TestExecution,
        };

        let roles = contract
            .role_bindings()
            .iter()
            .map(RuntimeRoleBinding::role)
            .filter(|role| {
                purpose == RuntimeArtifactPurpose::TestRunner
                    || *role != RuntimeAbiRole::TestEntrySelection
            });

        metadata_components.push(component_metadata(
            target,
            purpose,
            component(components, execution)?,
            "execution",
            roles,
            EXECUTION_CAPABILITIES,
            true,
        )?);
    }

    RuntimeArtifactMetadata::try_new(contract, metadata_components)
        .map_err(|_| CommandError::MetadataContract)
}

#[expect(
    clippy::too_many_arguments,
    reason = "runtime component publication keeps each ownership dimension explicit"
)]
fn component_metadata<const C: usize>(
    target: NativeTarget,
    purpose: RuntimeArtifactPurpose,
    component: &BuiltComponent,
    name: &str,
    roles: impl IntoIterator<Item = RuntimeAbiRole>,
    capabilities: [RuntimeCapability; C],
    embeds_platform_services: bool,
) -> Result<RuntimeArtifactComponentMetadata, CommandError> {
    let identity = RuntimeArtifactId::try_new(format!(
        "{RUNTIME_IDENTITY}.{}.{}.{}",
        target.as_str(),
        purpose.as_str(),
        name,
    ))
    .ok_or(CommandError::MetadataContract)?;

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

    Ok(metadata.with_native_links(component.native_links.iter().cloned()))
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

fn archive_file_name(target: NativeTarget, kind: RuntimeArchiveKind) -> String {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RuntimeArchiveKind {
    Memory,
    String,
    Character,
    ProductExecution,
    TestExecution,
}

impl RuntimeArchiveKind {
    const ALL: [Self; 5] = [
        Self::Memory,
        Self::String,
        Self::Character,
        Self::ProductExecution,
        Self::TestExecution,
    ];

    const fn archive_stem(self) -> &'static str {
        match self {
            Self::Memory => "bray_runtime_memory",
            Self::String => "bray_runtime_string",
            Self::Character => "bray_runtime_character",
            Self::ProductExecution => "bray_runtime_product",
            Self::TestExecution => "bray_runtime_test",
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
    RuntimeComponentBoundary(RuntimeArchiveKind),
    SynchronousLinkMapBoundary,
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
            Self::RuntimeComponentBoundary(kind) => {
                write!(formatter, "runtime {kind:?} archive has an invalid exported surface")
            }
            Self::SynchronousLinkMapBoundary => {
                formatter.write_str("synchronous runtime link map has capability leakage")
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
                RuntimeArchiveKind::TestExecution
            ),
            "bray_runtime_test.lib"
        );

        assert_eq!(
            archive_file_name(
                NativeTarget::X86_64LinuxGnu,
                RuntimeArchiveKind::ProductExecution
            ),
            "libbray_runtime_product.a"
        );

        assert_eq!(
            archive_file_name(NativeTarget::Aarch64LinuxGnu, RuntimeArchiveKind::Character),
            "libbray_runtime_character.a"
        );

        assert_eq!(
            archive_file_name(
                NativeTarget::X86_64MacOs,
                RuntimeArchiveKind::ProductExecution
            ),
            "libbray_runtime_product.a"
        );

        assert_eq!(
            archive_file_name(
                NativeTarget::Aarch64MacOs,
                RuntimeArchiveKind::TestExecution
            ),
            "libbray_runtime_test.a"
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
            assert_eq!(first.components().len(), 8);

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
