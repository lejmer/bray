use std::num::NonZeroU64;

use bray_runtime_interface::{
    BinarySymbolName, ExecutableHostContract, ExecutableHostContractBuilder, PanicAbiIdentity,
    ProtectedFrameAbiVersions, RootExecution, RuntimeAbiRole, RuntimeAbiVersion,
    RuntimeArtifactId, RuntimeCapability, RuntimeContract, RuntimeIdentity, RuntimeRequirements,
    RuntimeRoleBinding, RuntimeRoleImplementation,
};
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
    LinkPlanBuilder::new(
        product(),
        LinkedProductKind::Executable,
        link_target(),
        driver(),
        LinkPolicy::new(
            DeadStripPolicy::Preserve,
            SectionGarbageCollectionPolicy::Preserve,
            DebugLinkPolicy::None,
            None,
        ),
    )
}

pub(crate) fn link_plan() -> LinkPlan {
    let mut builder = link_plan_builder();

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
    host_contract(RootExecution::Synchronous, None)
}

pub(crate) fn async_executable_host_contract(runtime: RuntimeArtifactId) -> ExecutableHostContract {
    host_contract(
        RootExecution::Asynchronous {
            frame: bray_runtime_interface::ProtectedAsyncFrameId::new([11; 32]),
        },
        Some(runtime),
    )
}

fn host_contract(
    root: RootExecution,
    runtime: Option<RuntimeArtifactId>,
) -> ExecutableHostContract {
    let Some(entry) = BinarySymbolName::try_new("_bray_host_start") else {
        panic!("test host entry symbol name must be valid");
    };

    let roles = [
        RuntimeAbiRole::RootExecution,
        RuntimeAbiRole::RootCancellationRequest,
        RuntimeAbiRole::CleanupIncidentReporting,
        RuntimeAbiRole::RootTerminalObservation,
        RuntimeAbiRole::StructuredShutdown,
    ]
    .into_iter()
    .map(compiler_role_binding);

    let requirements = runtime_requirements(runtime.is_some());

    let mut builder = ExecutableHostContractBuilder::new(product(), entry, root, requirements);

    for role in roles {
        builder.push_role_binding(role);
    }

    if let Some(runtime) = runtime {
        builder.select_runtime(runtime_contract(runtime));
    }

    let Ok(host) = builder.finish() else {
        panic!("test executable host contract must be valid");
    };

    host
}

fn runtime_requirements(is_async: bool) -> RuntimeRequirements {
    let Some(panic_abi) = PanicAbiIdentity::try_new("bray.panic.test") else {
        panic!("test panic ABI identity must be valid");
    };

    let runtime = is_async.then(runtime_identity);

    let frame_abi = is_async.then(|| {
        ProtectedFrameAbiVersions::uniform(RuntimeAbiVersion::new(1, 0))
    });

    let roles = is_async
        .then_some([
            RuntimeAbiRole::MainThreadLaneStartup,
            RuntimeAbiRole::MainThreadLaneDrive,
        ])
        .into_iter()
        .flatten();

    let capabilities = is_async
        .then_some([
            RuntimeCapability::CooperativeExecution,
            RuntimeCapability::MainThreadLane,
        ])
        .into_iter()
        .flatten();

    RuntimeRequirements::new(
        runtime,
        RuntimeAbiVersion::new(1, 0),
        frame_abi,
        link_target().identity().clone(),
        panic_abi,
        roles,
        capabilities,
        [],
    )
}

fn runtime_contract(artifact: RuntimeArtifactId) -> RuntimeContract {
    let Some(panic_abi) = PanicAbiIdentity::try_new("bray.panic.test") else {
        panic!("test panic ABI identity must be valid");
    };

    RuntimeContract::try_new(
        runtime_identity(),
        artifact,
        RuntimeAbiVersion::new(1, 0),
        ProtectedFrameAbiVersions::uniform(RuntimeAbiVersion::new(1, 0)),
        link_target().identity().clone(),
        panic_abi,
        [
            RuntimeCapability::CooperativeExecution,
            RuntimeCapability::MainThreadLane,
        ],
        [
            runtime_role_binding(RuntimeAbiRole::MainThreadLaneStartup),
            runtime_role_binding(RuntimeAbiRole::MainThreadLaneDrive),
        ],
    )
    .unwrap_or_else(|error| panic!("test runtime contract must be valid: {error:?}"))
}

fn runtime_identity() -> RuntimeIdentity {
    RuntimeIdentity::try_new("bray.runtime.test")
        .unwrap_or_else(|| panic!("test runtime identity must be valid"))
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

fn runtime_role_binding(role: RuntimeAbiRole) -> RuntimeRoleBinding {
    role_binding(role, RuntimeRoleImplementation::BrayRuntime)
}

fn compiler_role_binding(role: RuntimeAbiRole) -> RuntimeRoleBinding {
    role_binding(role, RuntimeRoleImplementation::CompilerLowering)
}

fn role_binding(
    role: RuntimeAbiRole,
    implementation: RuntimeRoleImplementation,
) -> RuntimeRoleBinding {
    let Some(symbol_name) = BinarySymbolName::try_new(format!("role_{role:?}")) else {
        panic!("test runtime role symbol name must be valid");
    };

    RuntimeRoleBinding::new(role, symbol_name, implementation)
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
