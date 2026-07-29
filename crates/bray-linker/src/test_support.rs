use std::num::NonZeroU64;

use bray_runtime_interface::{ExecutableHostContract, RuntimeArtifactId};
use bray_symbols::{PackageIdentity, ProductIdentity};
use bray_target::{CodeModel, ObjectFormat, RelocationModel, TargetArchitecture, TargetIdentity};

use crate::{
    DeadStripPolicy, DebugLinkPolicy, LinkInput, LinkInputId, LinkInputKind, LinkInputMode,
    LinkInputProvenance, LinkInputSource, LinkModel, LinkPlan, LinkPlanBuilder, LinkPolicy,
    LinkTarget, LinkedArtifact, LinkedArtifactKind, LinkedArtifactRequirement, LinkedProductKind,
    LinkerDriverIdentity, LinkerDriverKind, PlannedLinkedArtifact, SectionGarbageCollectionPolicy,
    StagingDestination, StagingDestinationId, StagingPathKey,
};

pub(crate) fn link_plan_builder() -> LinkPlanBuilder {
    link_plan_builder_with_driver(driver())
}

pub(crate) fn link_plan_builder_with_driver(
    driver: LinkerDriverIdentity,
) -> LinkPlanBuilder {
    LinkPlanBuilder::new(
        product(),
        LinkedProductKind::Executable,
        link_target(),
        driver,
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

pub(crate) fn link_plan_with_driver(
    driver: LinkerDriverIdentity,
) -> LinkPlan {
    let mut builder = link_plan_builder_with_driver(driver);

    builder.push_input(link_input(0, "main.o"));

    builder.push_output(planned_output(
        0,
        LinkedArtifactKind::Executable,
        LinkedArtifactRequirement::Required,
        "application.stage",
    ));

    builder.set_executable_host(executable_host_contract());

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
    bray_testing::test_executable_host_contract_for(
        product(),
        link_target().identity().clone(),
    )
}

pub(crate) fn async_executable_host_contract(runtime: RuntimeArtifactId) -> ExecutableHostContract {
    bray_testing::test_async_executable_host_contract_for(
        product(),
        link_target().identity().clone(),
        runtime,
    )
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

fn link_target() -> LinkTarget {
    let Some(identity) = TargetIdentity::try_new("linux-x86_64") else {
        panic!("test target identity must be valid");
    };

    let Ok(target) = LinkTarget::try_new(
        identity,
        "x86_64-unknown-linux-gnu",
        TargetArchitecture::X86_64,
        ObjectFormat::Elf,
        RelocationModel::PositionIndependent,
        CodeModel::Small,
        LinkModel::Dynamic,
    ) else {
        panic!("test link target must be valid");
    };

    target
}

fn driver() -> LinkerDriverIdentity {
    let Some(driver) =
        LinkerDriverIdentity::try_new(LinkerDriverKind::EmbeddedLld, "lld", "1", "20")
    else {
        panic!("test linker-driver identity must be valid");
    };

    driver
}
