use std::path::{Path, PathBuf};
use std::sync::Arc;

use bray_base::NonEmptySharedStr;
use bray_emitter::ManagedOutputDirectory;
use bray_emitter::ProductBuildIdentity;
use bray_linker::SystemLinkerMapOutput;
use bray_messages::command_help as help;
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
    output_root: PathBuf,
    managed_output_directory: Option<ManagedOutputDirectory>,
    linker_map_output: Option<SystemLinkerMapOutput>,
    test_catalog: bool,
    build_identity: Option<ProductBuildIdentity>,
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
        output_root: PathBuf,
        test_catalog: bool,
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
            output_root,
            managed_output_directory: None,
            linker_map_output: None,
            test_catalog,
            build_identity: None,
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

    /// Selects a stable public directory beneath the output root.
    pub fn with_managed_output_directory(mut self, directory: ManagedOutputDirectory) -> Self {
        self.managed_output_directory = Some(directory);

        self
    }

    /// Returns the root that owns public output and private compiler state.
    pub fn output_root(&self) -> &Path {
        &self.output_root
    }

    /// Returns the stable root-relative public output directory, when configured by a host.
    pub const fn managed_output_directory(&self) -> Option<&ManagedOutputDirectory> {
        self.managed_output_directory.as_ref()
    }

    /// Selects an explicit linker-map destination for the linked product.
    pub fn with_linker_map_output(mut self, output: SystemLinkerMapOutput) -> Self {
        self.linker_map_output = Some(output);

        self
    }

    /// Returns the explicitly requested linker-map destination, when present.
    pub const fn linker_map_output(&self) -> Option<&SystemLinkerMapOutput> {
        self.linker_map_output.as_ref()
    }

    /// Returns whether the native test-host catalog is part of the product publication.
    pub const fn publishes_test_catalog(&self) -> bool {
        self.test_catalog
    }

    /// Records the complete identity chain required by retained no-build consumers.
    pub fn with_build_identity(mut self, identity: ProductBuildIdentity) -> Self {
        self.build_identity = Some(identity);

        self
    }

    /// Returns the retained product identity chain supplied by the host.
    pub const fn build_identity(&self) -> Option<&ProductBuildIdentity> {
        self.build_identity.as_ref()
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
    #[arg(long, value_enum, default_value = "llvm", help = help::BACKEND)]
    backend: CliBackend,
    #[arg(long, help = help::RELEASE)]
    release: bool,
    #[arg(
        long = "runtime-artifact",
        value_name = "METADATA",
        conflicts_with = "runtime_profile",
        help = help::RUNTIME_ARTIFACT
    )]
    runtime_artifact: Option<PathBuf>,
    #[arg(
        long = "runtime-profile",
        value_name = "PROFILE",
        value_parser = clap::builder::NonEmptyStringValueParser::new(),
        conflicts_with = "runtime_artifact",
        help = help::RUNTIME_PROFILE
    )]
    runtime_profile: Option<String>,
    #[arg(
        long = "require-capability",
        value_enum,
        value_name = "CAPABILITY",
        help = help::RUNTIME_CAPABILITY
    )]
    required_capabilities: Vec<CliRuntimeCapability>,
    #[arg(long, value_name = "DIRECTORY", help = help::BUILD_OUTPUT)]
    output: PathBuf,
    #[arg(
        long,
        value_name = "RELATIVE-DIRECTORY",
        value_parser = parse_managed_output_directory,
        hide = true
    )]
    managed_output_directory: Option<ManagedOutputDirectory>,
    #[arg(
        long = "linker-map-output",
        value_name = "FILE",
        value_parser = parse_linker_map_output,
        hide = true
    )]
    linker_map_output: Option<SystemLinkerMapOutput>,
    #[arg(long = "test-catalog", help = help::TEST_CATALOG)]
    test_catalog: bool,
    #[arg(long = "build-identity", value_parser = parse_build_identity, hide = true)]
    build_identity: Option<ProductBuildIdentity>,
    #[arg(long = "artifact", value_enum, value_name = "ARTIFACT", help = help::ARTIFACT)]
    artifacts: Vec<CliArtifact>,
    #[arg(
        long = "inspect",
        value_enum,
        value_name = "ARTIFACT",
        help = help::INSPECTION_ARTIFACT
    )]
    inspections: Vec<CliInspectionArtifact>,
    #[arg(value_name = "FILE", num_args = 0.., help = help::SOURCE_FILE)]
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

        let mut configuration = DriverProductConfiguration::new(
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

        if let Some(directory) = self.managed_output_directory {
            configuration = configuration.with_managed_output_directory(directory);
        }

        if let Some(output) = self.linker_map_output {
            configuration = configuration.with_linker_map_output(output);
        }

        if let Some(identity) = self.build_identity {
            configuration = configuration.with_build_identity(identity);
        }

        DriverCommand::build(configuration, self.files)
    }
}

fn parse_managed_output_directory(value: &str) -> Result<ManagedOutputDirectory, String> {
    ManagedOutputDirectory::try_new(value)
        .ok_or_else(|| help::MANAGED_OUTPUT_DIRECTORY_INVALID.to_owned())
}

fn parse_linker_map_output(value: &str) -> Result<SystemLinkerMapOutput, String> {
    SystemLinkerMapOutput::try_new(value).ok_or_else(|| help::LINKER_MAP_OUTPUT_INVALID.to_owned())
}

fn parse_build_identity(value: &str) -> Result<ProductBuildIdentity, String> {
    serde_json::from_str(value).map_err(|error| error.to_string())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum CliArtifact {
    #[value(help = help::ARTIFACT_ASSEMBLY)]
    Assembly,
    #[value(help = help::ARTIFACT_BACKEND_IR)]
    BackendIr,
    #[value(help = help::ARTIFACT_BACKEND_BITCODE)]
    BackendBitcode,
    #[value(help = help::ARTIFACT_OBJECT)]
    RelocatableObject,
    #[value(help = help::ARTIFACT_EXECUTABLE_MODULE)]
    ExecutableModule,
    #[value(help = help::ARTIFACT_DEBUG_COMPANION)]
    DebugCompanion,
    #[value(help = help::ARTIFACT_PACKAGE_INTERFACE)]
    PackageInterface,
    #[value(help = help::ARTIFACT_PACKAGE_IMPLEMENTATION)]
    PackageImplementation,
    #[value(help = help::ARTIFACT_DEPENDENCY_METADATA)]
    DependencyMetadata,
    #[value(help = help::ARTIFACT_EXECUTABLE)]
    Executable,
    #[value(help = help::ARTIFACT_STATIC_LIBRARY)]
    StaticLibrary,
    #[value(help = help::ARTIFACT_SHARED_LIBRARY)]
    SharedLibrary,
    #[value(help = help::ARTIFACT_LINKED_COMPANION)]
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
    #[value(help = help::BACKEND_LLVM)]
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
    #[value(help = help::CAPABILITY_PERFORMANCE_OBSERVATION)]
    PerformanceObservation,
    #[value(help = help::CAPABILITY_COOPERATIVE)]
    CooperativeExecution,
    #[value(help = help::CAPABILITY_LOCAL_LANES)]
    LocalLanes,
    #[value(help = help::CAPABILITY_MIGRATABLE_LANES)]
    MigratableLanes,
    #[value(help = help::CAPABILITY_BLOCKING_LANES)]
    BlockingLanes,
    #[value(help = help::CAPABILITY_COMPUTE_LANES)]
    ComputeLanes,
    #[value(help = help::CAPABILITY_MAIN_THREAD)]
    MainThreadLane,
    #[value(help = help::CAPABILITY_REACTOR)]
    Reactor,
}

impl From<CliRuntimeCapability> for RuntimeCapability {
    fn from(capability: CliRuntimeCapability) -> Self {
        match capability {
            CliRuntimeCapability::PerformanceObservation => Self::PerformanceObservation,
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
    #[value(help = help::ARTIFACT_ASSEMBLY)]
    Assembly,
    #[value(help = help::ARTIFACT_BACKEND_IR)]
    BackendIr,
    #[value(help = help::ARTIFACT_BACKEND_BITCODE)]
    BackendBitcode,
    #[value(help = help::ARTIFACT_OBJECT)]
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
            false,
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
