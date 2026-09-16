use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::{Mutex, MutexGuard};

use bray_base::Cancellation;
use bray_runtime_interface::ExecutableHostContract;
use bray_symbols::{PackageIdentity, ProductIdentity};
use bray_target::{CodeModel, ObjectFormat, RelocationModel, TargetArchitecture, TargetIdentity};
use bray_testing::unique_temporary_directory;

use crate::{
    DeadStripPolicy, DebugLinkPolicy, ExternalToolFailure, ExternalToolHost,
    ExternalToolInvocation, ExternalToolOutput, LinkInput, LinkInputId, LinkInputKind,
    LinkInputMode, LinkInputProvenance, LinkInputSource, LinkModel, LinkPlan, LinkPlanBuilder,
    LinkPolicy, LinkTarget, LinkedArtifact, LinkedArtifactKind, LinkedArtifactRequirement,
    LinkedProductKind, LinkerDriverIdentity, LinkerDriverKind, PlannedLinkedArtifact,
    SectionGarbageCollectionPolicy, StagingDestination, StagingDestinationId, StagingPathKey,
};

#[derive(Default)]
pub(crate) struct RecordingExternalToolHost {
    invocations: Mutex<Vec<ExternalToolInvocation>>,
    output: Option<PathBuf>,
    failure: Option<ExternalToolFailure>,
    tool_output: Option<ExternalToolOutput>,
    optimization_report: Option<Arc<[u8]>>,
    incomplete_optimization_report: Option<Arc<[u8]>>,
}

impl RecordingExternalToolHost {
    pub(crate) fn writing(output: &Path) -> Self {
        Self {
            invocations: Mutex::new(Vec::new()),
            output: Some(output.to_path_buf()),
            failure: None,
            tool_output: None,
            optimization_report: None,
            incomplete_optimization_report: None,
        }
    }

    pub(crate) fn writing_with_optimization_report(output: &Path, report: &[u8]) -> Self {
        Self {
            invocations: Mutex::new(Vec::new()),
            output: Some(output.to_path_buf()),
            failure: None,
            tool_output: None,
            optimization_report: Some(Arc::from(report)),
            incomplete_optimization_report: None,
        }
    }

    pub(crate) fn writing_with_incomplete_optimization_report(
        output: &Path,
        report: &[u8],
    ) -> Self {
        Self {
            invocations: Mutex::new(Vec::new()),
            output: Some(output.to_path_buf()),
            failure: None,
            tool_output: None,
            optimization_report: None,
            incomplete_optimization_report: Some(Arc::from(report)),
        }
    }

    pub(crate) fn failing(failure: ExternalToolFailure) -> Self {
        Self {
            invocations: Mutex::new(Vec::new()),
            output: None,
            failure: Some(failure),
            tool_output: None,
            optimization_report: None,
            incomplete_optimization_report: None,
        }
    }

    pub(crate) fn reporting(tool_output: ExternalToolOutput) -> Self {
        Self {
            invocations: Mutex::new(Vec::new()),
            output: None,
            failure: None,
            tool_output: Some(tool_output),
            optimization_report: None,
            incomplete_optimization_report: None,
        }
    }

    pub(crate) fn invocations(&self) -> MutexGuard<'_, Vec<ExternalToolInvocation>> {
        self.invocations
            .lock()
            .unwrap_or_else(|error| panic!("test invocation lock must be available: {error:?}"))
    }

    pub(crate) fn only_invocation(&self) -> ExternalToolInvocation {
        let invocations = self.invocations();

        assert_eq!(invocations.len(), 1);

        invocations[0].clone()
    }
}

impl ExternalToolHost for RecordingExternalToolHost {
    fn run(
        &self,
        invocation: &ExternalToolInvocation,
        _cancellation: &dyn Cancellation,
    ) -> Result<ExternalToolOutput, ExternalToolFailure> {
        self.invocations().push(invocation.clone());

        if let Some(failure) = &self.failure {
            // Test failures are cloned so the recording host can be reused.
            return Err(failure.clone());
        }

        if let Some(output) = &self.output {
            std::fs::write(output, b"linked")
                .unwrap_or_else(|error| panic!("test output must be written: {error:?}"));
        }

        if let Some(report) = &self.optimization_report {
            let path = optimization_report_path(invocation);

            std::fs::write(path, report).unwrap_or_else(|error| {
                panic!("test optimization report must be written: {error}")
            });
        }

        if let Some(report) = &self.incomplete_optimization_report {
            let path = crate::optimization::partial_report_path(Path::new(
                optimization_report_path(invocation),
            ));

            std::fs::write(path, report).unwrap_or_else(|error| {
                panic!("incomplete test optimization report must be written: {error}")
            });
        }

        if let Some(output) = &self.tool_output {
            // Test tool results are cloned so the recording host can be reused.
            return Ok(output.clone());
        }

        Ok(ExternalToolOutput::new(true, Some(0), [], []))
    }
}

fn optimization_report_path(invocation: &ExternalToolInvocation) -> &std::ffi::OsStr {
    invocation
        .environment()
        .iter()
        .find_map(|(name, value)| {
            (name == crate::optimization::REPORT_ENVIRONMENT).then_some(value.as_os_str())
        })
        .unwrap_or_else(|| panic!("optimization report destination must be configured"))
}

pub(crate) struct TestOutput {
    directory: PathBuf,
    path: PathBuf,
}

impl TestOutput {
    pub(crate) fn new(file_name: &str) -> Self {
        let directory = unique_temporary_directory();

        std::fs::create_dir(&directory)
            .unwrap_or_else(|error| panic!("test output directory must be created: {error:?}"));

        let path = directory.join(file_name);

        Self { directory, path }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestOutput {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_dir(&self.directory);
    }
}

pub(crate) fn link_plan_builder() -> LinkPlanBuilder {
    link_plan_builder_for(
        LinkedProductKind::Executable,
        driver(),
        crate::LinkStartupMode::PlatformCompilerDriver,
    )
}

pub(crate) fn link_plan_builder_with_driver(driver: LinkerDriverIdentity) -> LinkPlanBuilder {
    link_plan_builder_for(
        LinkedProductKind::Executable,
        driver,
        crate::LinkStartupMode::PlatformCompilerDriver,
    )
}

pub(crate) fn link_plan_builder_for(
    product_kind: LinkedProductKind,
    driver: LinkerDriverIdentity,
    startup_mode: crate::LinkStartupMode,
) -> LinkPlanBuilder {
    LinkPlanBuilder::new(
        product(),
        product_kind,
        link_target(),
        driver,
        startup_mode,
        LinkPolicy::new(
            DeadStripPolicy::Preserve,
            SectionGarbageCollectionPolicy::Preserve,
            DebugLinkPolicy::None,
            None,
        ),
    )
}

pub(crate) fn link_plan() -> LinkPlan {
    link_plan_with_driver(driver())
}

pub(crate) fn link_plan_with_driver(driver: LinkerDriverIdentity) -> LinkPlan {
    let mut builder = link_plan_builder_with_driver(driver);

    builder.push_input(link_input(0, "main.o"));

    builder.push_output(planned_output(
        0,
        LinkedArtifactKind::Executable,
        LinkedArtifactRequirement::Required,
        "application.stage",
    ));

    builder.set_entry_point(executable_host_contract().native_entry().clone());

    let Ok(plan) = builder.finish() else {
        panic!("complete test link plan must be valid");
    };

    plan
}

pub(crate) fn link_input(ordinal: u32, path: &str) -> LinkInput {
    let Ok(input) = LinkInput::try_new(
        LinkInputId::new(ordinal),
        LinkInputKind::RelocatableObject,
        LinkInputSource::file(path),
        LinkInputProvenance::Product,
        LinkInputMode::Ordinary,
    ) else {
        panic!("test link input must be valid");
    };

    input
}

pub(crate) fn planned_output(
    ordinal: u32,
    kind: LinkedArtifactKind,
    requirement: LinkedArtifactRequirement,
    path: &str,
) -> PlannedLinkedArtifact {
    PlannedLinkedArtifact::new(kind, requirement, staging_destination(ordinal, path))
}

pub(crate) fn planned_output_with_key(
    ordinal: u32,
    kind: LinkedArtifactKind,
    requirement: LinkedArtifactRequirement,
    path: &str,
    path_key: &str,
) -> PlannedLinkedArtifact {
    PlannedLinkedArtifact::new(
        kind,
        requirement,
        staging_destination_with_key(ordinal, path, path_key),
    )
}

pub(crate) fn staging_destination(ordinal: u32, path: &str) -> StagingDestination {
    staging_destination_with_key(ordinal, path, path)
}

fn staging_destination_with_key(ordinal: u32, path: &str, path_key: &str) -> StagingDestination {
    let Some(path_key) = StagingPathKey::try_new(path_key) else {
        panic!("test staging path key must be valid");
    };

    let Ok(destination) =
        StagingDestination::try_new(StagingDestinationId::new(ordinal), path, path_key)
    else {
        panic!("test staging destination must be valid");
    };

    destination
}

pub(crate) fn linked_artifact(plan: &LinkPlan) -> LinkedArtifact {
    let Some(output) = plan.outputs().first() else {
        panic!("test link plan must contain a primary output");
    };

    LinkedArtifact::new(output.kind(), output.destination().id(), NonZeroU64::MIN)
}

pub(crate) fn executable_host_contract() -> ExecutableHostContract {
    bray_testing::test_executable_host_contract_for(product(), link_target().identity().clone())
}

pub(crate) fn product() -> ProductIdentity {
    let Some(package) = PackageIdentity::try_new("example.package") else {
        panic!("test package identity must be valid");
    };

    let Some(product) = ProductIdentity::try_new(package, "application") else {
        panic!("test product identity must be valid");
    };

    product
}

pub(crate) fn target(architecture: TargetArchitecture, object_format: ObjectFormat) -> LinkTarget {
    target_with_identity(
        "test-target",
        "test-target-triple",
        architecture,
        object_format,
        LinkModel::Dynamic,
    )
}

pub(crate) fn archive_target(
    architecture: TargetArchitecture,
    object_format: ObjectFormat,
) -> LinkTarget {
    target_with_identity(
        "test-target",
        "test-target-triple",
        architecture,
        object_format,
        LinkModel::Static,
    )
}

fn target_with_identity(
    identity: &str,
    triple: &str,
    architecture: TargetArchitecture,
    object_format: ObjectFormat,
    link_model: LinkModel,
) -> LinkTarget {
    let Some(identity) = TargetIdentity::try_new(identity) else {
        panic!("test target identity must be valid");
    };

    LinkTarget::try_new(
        identity,
        triple,
        architecture,
        object_format,
        RelocationModel::PositionIndependent,
        CodeModel::Small,
        link_model,
    )
    .unwrap_or_else(|error| panic!("test link target must be valid: {error:?}"))
}

fn link_target() -> LinkTarget {
    target_with_identity(
        "linux-x86_64",
        "x86_64-unknown-linux-gnu",
        TargetArchitecture::X86_64,
        ObjectFormat::Elf,
        LinkModel::Dynamic,
    )
}

fn driver() -> LinkerDriverIdentity {
    let Some(driver) =
        LinkerDriverIdentity::try_new(LinkerDriverKind::EmbeddedLld, "lld", "1", "20")
    else {
        panic!("test linker-driver identity must be valid");
    };

    driver
}
