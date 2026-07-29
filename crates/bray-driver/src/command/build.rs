use std::path::{Path, PathBuf};

use bray_runtime_interface::RuntimeCapability;
use bray_symbols::ProductKind;
use clap::{Args, ValueEnum};

use crate::command::DriverCommand;

/// Code generation backend selected for a build.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DriverBackend {
    /// The production LLVM backend.
    Llvm,
}

/// Compilation target selected for a build.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DriverTarget {
    /// Bray's deterministic x86-64 Linux baseline.
    X86_64UnknownLinuxGnu,
}

impl DriverTarget {
    /// Returns the compiler target represented by this driver selection.
    pub fn selected_target(self) -> bray_compilation::SelectedTarget {
        match self {
            Self::X86_64UnknownLinuxGnu => bray_compilation::SelectedTarget::baseline(),
        }
    }
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
pub struct DriverRuntimeProfile(String);

impl DriverRuntimeProfile {
    /// Creates a configured profile name unless the supplied name is empty.
    pub fn try_new(name: impl Into<String>) -> Option<Self> {
        let name = name.into();

        (!name.is_empty()).then_some(Self(name))
    }

    /// Returns the configured profile name.
    pub fn as_str(&self) -> &str {
        &self.0
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
    product_kind: ProductKind,
    target: DriverTarget,
    backend: DriverBackend,
    runtime: Option<DriverRuntimeSelection>,
    required_capabilities: Vec<RuntimeCapability>,
    output: PathBuf,
    inspections: Vec<DriverInspectionArtifact>,
}

impl DriverProductConfiguration {
    /// Creates product configuration and canonicalizes repeated capabilities and inspections.
    pub fn new(
        product_kind: ProductKind,
        target: DriverTarget,
        backend: DriverBackend,
        runtime: Option<DriverRuntimeSelection>,
        mut required_capabilities: Vec<RuntimeCapability>,
        output: PathBuf,
        mut inspections: Vec<DriverInspectionArtifact>,
    ) -> Self {
        required_capabilities.sort_unstable();
        required_capabilities.dedup();
        inspections.sort_unstable();
        inspections.dedup();

        Self {
            product_kind,
            target,
            backend,
            runtime,
            required_capabilities,
            output,
            inspections,
        }
    }

    /// Returns the selected language product kind.
    pub const fn product_kind(&self) -> ProductKind {
        self.product_kind
    }

    /// Returns the selected compilation target.
    pub const fn target(&self) -> DriverTarget {
        self.target
    }

    /// Returns the selected code generation backend.
    pub const fn backend(&self) -> DriverBackend {
        self.backend
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

    /// Returns optional inspection artifacts in canonical order.
    pub fn inspections(&self) -> &[DriverInspectionArtifact] {
        &self.inspections
    }

    pub(crate) const fn product_name(&self) -> &'static str {
        match self.product_kind {
            ProductKind::Library => "library",
            ProductKind::Executable => "application",
            ProductKind::Test => "tests",
        }
    }
}

#[derive(Args, Debug)]
pub(crate) struct CliBuildCommand {
    #[arg(long = "product-kind", value_enum, default_value = "library")]
    product_kind: CliProductKind,
    #[arg(
        long,
        value_enum,
        default_value = "x86_64-unknown-linux-gnu",
        value_name = "TRIPLE"
    )]
    target: CliTarget,
    #[arg(long, value_enum, default_value = "llvm")]
    backend: CliBackend,
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
            self.product_kind.into(),
            self.target.into(),
            self.backend.into(),
            runtime,
            self.required_capabilities
                .into_iter()
                .map(RuntimeCapability::from)
                .collect(),
            self.output,
            self.inspections
                .into_iter()
                .map(DriverInspectionArtifact::from)
                .collect(),
        );

        DriverCommand::build(configuration, self.files)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum CliProductKind {
    Library,
    Executable,
    Test,
}

impl From<CliProductKind> for ProductKind {
    fn from(kind: CliProductKind) -> Self {
        match kind {
            CliProductKind::Library => Self::Library,
            CliProductKind::Executable => Self::Executable,
            CliProductKind::Test => Self::Test,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum CliTarget {
    #[value(name = "x86_64-unknown-linux-gnu")]
    X86_64UnknownLinuxGnu,
}

impl From<CliTarget> for DriverTarget {
    fn from(target: CliTarget) -> Self {
        match target {
            CliTarget::X86_64UnknownLinuxGnu => Self::X86_64UnknownLinuxGnu,
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
    use bray_symbols::ProductKind;

    use super::{
        DriverBackend, DriverInspectionArtifact, DriverProductConfiguration,
        DriverRuntimeProfile, DriverTarget,
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
            ProductKind::Library,
            DriverTarget::X86_64UnknownLinuxGnu,
            DriverBackend::Llvm,
            None,
            [
                RuntimeCapability::Reactor,
                RuntimeCapability::CooperativeExecution,
                RuntimeCapability::Reactor,
            ]
            .into(),
            "out".into(),
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
            configuration.inspections(),
            &[DriverInspectionArtifact::BackendIr]
        );
    }
}
