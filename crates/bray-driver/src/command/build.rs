use std::path::{Path, PathBuf};
use std::sync::Arc;

use bray_base::NonEmptySharedStr;
use bray_runtime_interface::RuntimeCapability;
use bray_target::TargetOutputKind;
use clap::{Args, ValueEnum};

use crate::command::DriverCommand;

/// Code generation backend selected for a build.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DriverBackend {
    /// The production LLVM backend.
    Llvm,
}

/// Product runtime selected at the build boundary.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DriverRuntimeSelection {
    /// A specific conforming runtime artifact metadata document.
    Artifact(PathBuf),
    /// A runtime profile configured by the compiler host.
    Profile(DriverRuntimeProfile),
}

/// Non-empty configured runtime profile name.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DriverRuntimeProfile(NonEmptySharedStr);

impl DriverRuntimeProfile {
    /// Creates a configured profile name unless the supplied name is empty.
    pub fn try_new(name: impl Into<Arc<str>>) -> Option<Self> {
        NonEmptySharedStr::try_new(name).map(Self)
    }

    /// Returns the configured profile name.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Optional backend inspection artifact requested beside a product.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DriverInspectionArtifact {
    /// Human-readable target assembly.
    Assembly,
    /// Human-readable backend low-level IR.
    BackendIr,
    /// Backend-owned binary IR or bitcode.
    BackendBitcode,
    /// Relocatable native object.
    RelocatableObject,
}

impl DriverInspectionArtifact {
    pub(crate) const fn artifact_kind(self) -> bray_emitter::ArtifactKind {
        match self {
            Self::Assembly => bray_emitter::ArtifactKind::Assembly,
            Self::BackendIr => bray_emitter::ArtifactKind::BackendIr,
            Self::BackendBitcode => bray_emitter::ArtifactKind::BackendBitcode,
            Self::RelocatableObject => bray_emitter::ArtifactKind::RelocatableObject,
        }
    }
}

/// Typed package-product configuration selected by `brayc build`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DriverProductConfiguration {
    backend: DriverBackend,
    build: bray_compilation::BuildConfiguration,
    runtime: Option<DriverRuntimeSelection>,
    required_capabilities: Vec<RuntimeCapability>,
    output: PathBuf,
    test_catalog: Option<PathBuf>,
    artifacts: Vec<TargetOutputKind>,
    inspections: Vec<DriverInspectionArtifact>,
}

impl DriverProductConfiguration {
    /// Creates product configuration and canonicalizes repeated artifact selections.
    pub fn new(
        backend: DriverBackend,
        build: bray_compilation::BuildConfiguration,
        runtime: Option<DriverRuntimeSelection>,
        mut required_capabilities: Vec<RuntimeCapability>,
        output: PathBuf,
        test_catalog: Option<PathBuf>,
        mut artifacts: Vec<TargetOutputKind>,
        mut inspections: Vec<DriverInspectionArtifact>,
    ) -> Self {
        required_capabilities.sort_unstable();
        required_capabilities.dedup();
        artifacts.sort_unstable();
        artifacts.dedup();
        inspections.sort_unstable();
        inspections.dedup();

        Self {
            backend,
            build,
            runtime,
            required_capabilities,
            output,
            test_catalog,
            artifacts,
            inspections,
        }
    }

    /// Returns the selected code generation backend.
    pub const fn backend(&self) -> DriverBackend {
        self.backend
    }

    /// Returns the selected native build configuration.
    pub const fn build(&self) -> bray_compilation::BuildConfiguration {
        self.build
    }

    /// Returns the selected runtime artifact or configured profile.
    pub const fn runtime(&self) -> Option<&DriverRuntimeSelection> {
        self.runtime.as_ref()
    }

    /// Returns required execution capabilities in canonical order.
    pub fn required_capabilities(&self) -> &[RuntimeCapability] {
        &self.required_capabilities
    }

    /// Returns the product output directory.
    pub fn output(&self) -> &Path {
        &self.output
    }

    /// Returns the requested test-catalog publication path, when applicable.
    pub fn test_catalog(&self) -> Option<&Path> {
        self.test_catalog.as_deref()
    }

    /// Returns required artifacts in canonical order.
    pub fn artifacts(&self) -> &[TargetOutputKind] {
        &self.artifacts
    }

    /// Returns optional inspection artifacts in canonical order.
    pub fn inspections(&self) -> &[DriverInspectionArtifact] {
        &self.inspections
    }
}

#[derive(Args, Debug)]
pub(crate) struct CliBuildCommand {
    #[arg(long, value_enum, default_value = "llvm")]
    backend: CliBackend,
    #[arg(long)]
    release: bool,
    #[arg(
        long = "runtime-artifact",
        value_name = "METADATA",
        conflicts_with = "runtime_profile"
    )]
    runtime_artifact: Option<PathBuf>,
    #[arg(
        long = "runtime-profile",
        value_name = "PROFILE",
        value_parser = clap::builder::NonEmptyStringValueParser::new(),
        conflicts_with = "runtime_artifact"
    )]
    runtime_profile: Option<String>,
    #[arg(long = "require-capability", value_enum, value_name = "CAPABILITY")]
    required_capabilities: Vec<CliRuntimeCapability>,
    #[arg(long, value_name = "DIRECTORY")]
    output: PathBuf,
    #[arg(long = "test-catalog", value_name = "PATH")]
    test_catalog: Option<PathBuf>,
    #[arg(long = "artifact", value_enum, value_name = "ARTIFACT")]
    artifacts: Vec<CliArtifact>,
    #[arg(long = "inspect", value_enum, value_name = "ARTIFACT")]
    inspections: Vec<CliInspectionArtifact>,
    #[arg(value_name = "FILE", num_args = 0..)]
    files: Vec<PathBuf>,
}

impl CliBuildCommand {
    pub(crate) fn into_driver_command(self) -> DriverCommand {
        let runtime = self
            .runtime_artifact
            .map(DriverRuntimeSelection::Artifact)
            .or_else(|| {
                self.runtime_profile.map(|profile| {
                    let profile = DriverRuntimeProfile::try_new(profile)
                        .unwrap_or_else(|| panic!("clap must reject empty runtime profile names"));

                    DriverRuntimeSelection::Profile(profile)
                })
            });

        let configuration = DriverProductConfiguration::new(
            self.backend.into(),
            if self.release {
                bray_compilation::BuildConfiguration::Release
            } else {
                bray_compilation::BuildConfiguration::Development
            },
            runtime,
            self.required_capabilities
                .into_iter()
                .map(RuntimeCapability::from)
                .collect(),
            self.output,
            self.test_catalog,
            self.artifacts
                .into_iter()
                .map(TargetOutputKind::from)
                .collect(),
            self.inspections
                .into_iter()
                .map(DriverInspectionArtifact::from)
                .collect(),
        );

        DriverCommand::build(configuration, self.files)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum CliArtifact {
    Assembly,
    BackendIr,
    BackendBitcode,
    RelocatableObject,
    ExecutableModule,
    DebugCompanion,
    PackageInterface,
    PackageImplementation,
    DependencyMetadata,
    Executable,
    StaticLibrary,
    SharedLibrary,
    LinkedCompanion,
}

impl From<CliArtifact> for TargetOutputKind {
    fn from(artifact: CliArtifact) -> Self {
        match artifact {
            CliArtifact::Assembly => Self::Assembly,
            CliArtifact::BackendIr => Self::BackendIr,
            CliArtifact::BackendBitcode => Self::BackendBitcode,
            CliArtifact::RelocatableObject => Self::RelocatableObject,
            CliArtifact::ExecutableModule => Self::ExecutableModule,
            CliArtifact::DebugCompanion => Self::DebugCompanion,
            CliArtifact::PackageInterface => Self::PackageInterface,
            CliArtifact::PackageImplementation => Self::PackageImplementation,
            CliArtifact::DependencyMetadata => Self::DependencyMetadata,
            CliArtifact::Executable => Self::Executable,
            CliArtifact::StaticLibrary => Self::StaticLibrary,
            CliArtifact::SharedLibrary => Self::SharedLibrary,
            CliArtifact::LinkedCompanion => Self::LinkedCompanion,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum CliBackend {
    Llvm,
}

impl From<CliBackend> for DriverBackend {
    fn from(backend: CliBackend) -> Self {
        match backend {
            CliBackend::Llvm => Self::Llvm,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum CliRuntimeCapability {
    MemoryOperations,
    StringOperations,
    CharacterOperations,
    CooperativeExecution,
    LocalLanes,
    MigratableLanes,
    BlockingLanes,
    ComputeLanes,
    MainThreadLane,
    Reactor,
}

impl From<CliRuntimeCapability> for RuntimeCapability {
    fn from(capability: CliRuntimeCapability) -> Self {
        match capability {
            CliRuntimeCapability::MemoryOperations => Self::MemoryOperations,
            CliRuntimeCapability::StringOperations => Self::StringOperations,
            CliRuntimeCapability::CharacterOperations => Self::CharacterOperations,
            CliRuntimeCapability::CooperativeExecution => Self::CooperativeExecution,
            CliRuntimeCapability::LocalLanes => Self::LocalLanes,
            CliRuntimeCapability::MigratableLanes => Self::MigratableLanes,
            CliRuntimeCapability::BlockingLanes => Self::BlockingLanes,
            CliRuntimeCapability::ComputeLanes => Self::ComputeLanes,
            CliRuntimeCapability::MainThreadLane => Self::MainThreadLane,
            CliRuntimeCapability::Reactor => Self::Reactor,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum CliInspectionArtifact {
    Assembly,
    BackendIr,
    BackendBitcode,
    RelocatableObject,
}

impl From<CliInspectionArtifact> for DriverInspectionArtifact {
    fn from(artifact: CliInspectionArtifact) -> Self {
        match artifact {
            CliInspectionArtifact::Assembly => Self::Assembly,
            CliInspectionArtifact::BackendIr => Self::BackendIr,
            CliInspectionArtifact::BackendBitcode => Self::BackendBitcode,
            CliInspectionArtifact::RelocatableObject => Self::RelocatableObject,
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_runtime_interface::RuntimeCapability;
    use bray_target::TargetOutputKind;

    use super::{
        DriverBackend, DriverInspectionArtifact, DriverProductConfiguration, DriverRuntimeProfile,
    };

    #[test]
    fn runtime_profile_names_are_non_empty() {
        assert_eq!(DriverRuntimeProfile::try_new(""), None);

        let profile = DriverRuntimeProfile::try_new("native")
            .unwrap_or_else(|| panic!("non-empty runtime profile must be valid"));

        assert_eq!(profile.as_str(), "native");
    }

    #[test]
    fn product_configuration_canonicalizes_repeated_selections() {
        let configuration = DriverProductConfiguration::new(
            DriverBackend::Llvm,
            bray_compilation::BuildConfiguration::Development,
            None,
            [
                RuntimeCapability::Reactor,
                RuntimeCapability::CooperativeExecution,
                RuntimeCapability::Reactor,
            ]
            .into(),
            "out".into(),
            None,
            [
                TargetOutputKind::StaticLibrary,
                TargetOutputKind::StaticLibrary,
            ]
            .into(),
            [
                DriverInspectionArtifact::BackendIr,
                DriverInspectionArtifact::BackendIr,
            ]
            .into(),
        );

        assert_eq!(
            configuration.required_capabilities(),
            &[
                RuntimeCapability::CooperativeExecution,
                RuntimeCapability::Reactor,
            ]
        );

        assert_eq!(
            configuration.artifacts(),
            &[TargetOutputKind::StaticLibrary]
        );

        assert_eq!(
            configuration.inspections(),
            &[DriverInspectionArtifact::BackendIr]
        );
    }
}
