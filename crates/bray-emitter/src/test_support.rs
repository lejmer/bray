use bray_codegen::{
    ArtifactDigest, ArtifactDigestAlgorithm, AssemblySyntaxKind, BackendArtifactId,
    BackendArtifactKind, BackendArtifactRequest, BackendArtifactRequestEntry,
    BackendArtifactRequirement, BackendCapabilities, BackendIdentity, BackendSerializationOptions,
    BackendTargetPlatform, CodegenUnit, DebugInformationMode, DebugInformationOutputMode,
    LinkableArtifactKind, LinkableArtifactRequirement,
};
use bray_runtime_interface::{
    BinarySymbolName, ExecutableHostContract, ExecutableHostContractBuilder, RootExecution,
    RuntimeAbiRole, RuntimeAbiVersion, RuntimeRoleBinding, RuntimeRoleImplementation,
};
use bray_symbols::PackageIdentity;
use bray_target::test_support::test_target_machine;
use bray_target::{
    ObjectFormat, TargetArchitecture, TargetIdentity, TargetOutputDescription, TargetOutputKind,
    TargetOutputName,
};
use bray_testing::test_mir_unit;

use crate::{
    ArtifactId, ArtifactKind, ArtifactProducer, ArtifactRequirement, ArtifactRole, EmissionPlan,
    EmissionRequest, EmittedArtifact, LinkerProducerId, OutputSink, PlannedArtifact,
    PlannedArtifactDestination, ProductIdentity, ProductKind, ReplacementPolicy, RequestedArtifact,
    RequestedArtifactDestination,
};

pub(crate) fn product_identity() -> ProductIdentity {
    let Some(package) = PackageIdentity::try_new("example.package") else {
        panic!("test package identity must be valid");
    };

    let Some(product) = ProductIdentity::try_new(package, "application") else {
        panic!("test product identity must be valid");
    };

    product
}

pub(crate) fn executable_host_contract() -> ExecutableHostContract {
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
    .map(runtime_role_binding);

    let mut builder = ExecutableHostContractBuilder::new(
        product_identity(),
        entry,
        RootExecution::Synchronous,
        RuntimeAbiVersion::new(1, 0),
    );

    for role in roles {
        builder.push_role_binding(role);
    }

    let Ok(host) = builder.finish() else {
        panic!("test executable host contract must be valid");
    };

    host
}

pub(crate) fn interface_artifact() -> bray_package_interface::InterfaceArtifact {
    let product = product_identity();

    bray_package_interface::test_support::interface_artifact_for(
        product.package().clone(),
        product.name(),
    )
}

pub(crate) fn target_identity() -> TargetIdentity {
    let Some(target) = TargetIdentity::try_new("x86_64-unknown-linux-gnu") else {
        panic!("test target identity must be valid");
    };

    target
}

pub(crate) fn target_output_description() -> TargetOutputDescription {
    let names = [
        output_name(TargetOutputKind::Assembly, "", ".s"),
        output_name(TargetOutputKind::BackendIr, "", ".ll"),
        output_name(TargetOutputKind::BackendBitcode, "", ".bc"),
        output_name(TargetOutputKind::RelocatableObject, "", ".o"),
        output_name(TargetOutputKind::ExecutableModule, "", ".wasm"),
        output_name(TargetOutputKind::DebugCompanion, "", ".debug"),
        output_name(TargetOutputKind::PackageInterface, "", ".brayi"),
        output_name(TargetOutputKind::DependencyMetadata, "", ".brayd"),
        output_name(TargetOutputKind::Executable, "", ""),
        output_name(TargetOutputKind::StaticLibrary, "lib", ".a"),
        output_name(TargetOutputKind::SharedLibrary, "lib", ".so"),
        output_name(TargetOutputKind::LinkedCompanion, "", ".companion"),
    ];

    target_output_description_from(names)
}

pub(crate) fn target_output_description_from(
    names: impl IntoIterator<Item = TargetOutputName>,
) -> TargetOutputDescription {
    let Ok(description) =
        TargetOutputDescription::try_new(target_identity(), test_target_machine(), names)
    else {
        panic!("test target output description must be valid");
    };

    description
}

pub(crate) fn backend_identity() -> BackendIdentity {
    let Some(backend) = BackendIdentity::try_new("llvm", "bray-1", "llvm-22") else {
        panic!("test backend identity must be valid");
    };

    backend
}

pub(crate) fn backend_capabilities() -> BackendCapabilities {
    BackendCapabilities::new(
        [BackendTargetPlatform::new(
            TargetArchitecture::X86_64,
            ObjectFormat::Elf,
        )],
        [
            BackendArtifactKind::RelocatableObject,
            BackendArtifactKind::Assembly,
            BackendArtifactKind::BackendIr,
            BackendArtifactKind::BackendBitcode,
            BackendArtifactKind::ExecutableModule,
            BackendArtifactKind::DebugCompanion,
        ],
        [
            DebugInformationMode::None,
            DebugInformationMode::LineTables,
            DebugInformationMode::Full,
        ],
        [
            AssemblySyntaxKind::TargetDefault,
            AssemblySyntaxKind::Intel,
            AssemblySyntaxKind::Att,
        ],
    )
}

pub(crate) fn codegen_unit_key(seed: u32) -> bray_codegen::CodegenUnitKey {
    let Ok(unit) = CodegenUnit::try_new(seed, [test_mir_unit(seed)]) else {
        panic!("test codegen unit must be valid");
    };

    unit.key().clone()
}

pub(crate) fn emission_request(
    artifacts: impl IntoIterator<Item = RequestedArtifact>,
) -> EmissionRequest {
    emission_request_for(
        ProductKind::Executable,
        RequestedArtifactDestination::FilesystemDirectory("out".into()),
        artifacts,
    )
}

pub(crate) fn emission_request_for(
    product_kind: ProductKind,
    destination: RequestedArtifactDestination,
    artifacts: impl IntoIterator<Item = RequestedArtifact>,
) -> EmissionRequest {
    let Ok(request) = EmissionRequest::try_new(
        product_identity(),
        product_kind,
        matches!(product_kind, ProductKind::Executable | ProductKind::Test)
            .then(executable_host_contract),
        target_identity(),
        destination,
        artifacts,
        ReplacementPolicy::RequireAbsent,
    ) else {
        panic!("test emission request must be valid");
    };

    request
}

fn runtime_role_binding(role: RuntimeAbiRole) -> RuntimeRoleBinding {
    let Some(symbol_name) = BinarySymbolName::try_new(format!("role_{role:?}")) else {
        panic!("test runtime role symbol name must be valid");
    };

    RuntimeRoleBinding::new(
        role,
        symbol_name,
        RuntimeRoleImplementation::CompilerLowering,
    )
}

pub(crate) fn emission_plan() -> EmissionPlan {
    let request = emission_request_for(
        ProductKind::Library,
        RequestedArtifactDestination::FilesystemDirectory("out".into()),
        [RequestedArtifact::new(
            ArtifactKind::PackageInterface,
            ArtifactRequirement::Required,
        )],
    );

    let artifact = PlannedArtifact::new(
        ArtifactId::new(request.product().clone(), ArtifactKind::PackageInterface, 0),
        ArtifactRequirement::Required,
        ArtifactRole::Product,
        ArtifactProducer::PackageInterface,
        PlannedArtifactDestination::Publish(OutputSink::Filesystem("application.brayi".into())),
    );

    let Ok(plan) = EmissionPlan::try_new(request, None, [artifact], [], Some(interface_artifact()))
    else {
        panic!("test emission plan must be valid");
    };

    plan
}

pub(crate) fn linked_artifact(
    kind: ArtifactKind,
    role: ArtifactRole,
    path: &str,
    ordinal: u32,
) -> PlannedArtifact {
    PlannedArtifact::new(
        ArtifactId::new(product_identity(), kind, 0),
        ArtifactRequirement::Required,
        role,
        ArtifactProducer::Linker(LinkerProducerId::new(ordinal)),
        PlannedArtifactDestination::Publish(OutputSink::Filesystem(path.into())),
    )
}

pub(crate) fn backend_artifact_plan_parts() -> (
    EmissionRequest,
    BackendIdentity,
    PlannedArtifact,
    BackendArtifactRequest,
) {
    let request = emission_request([RequestedArtifact::new(
        ArtifactKind::RelocatableObject,
        ArtifactRequirement::Required,
    )]);

    let backend = backend_identity();

    let backend_artifact = BackendArtifactId::new(
        codegen_unit_key(1),
        BackendArtifactKind::RelocatableObject,
        0,
    );

    let backend_entry = BackendArtifactRequestEntry::new(
        backend_artifact.clone(),
        BackendArtifactRequirement::Required,
    );

    let serialization = BackendSerializationOptions::new(AssemblySyntaxKind::TargetDefault, false);

    let Ok(backend_request) = BackendArtifactRequest::try_new(
        backend_artifact.unit().clone(),
        [backend_entry],
        DebugInformationOutputMode::Omit,
        Some(LinkableArtifactRequirement::new(
            LinkableArtifactKind::RelocatableObject,
            BackendArtifactRequirement::Required,
        )),
        serialization,
    ) else {
        panic!("test backend request must be valid");
    };

    let artifact = PlannedArtifact::new(
        ArtifactId::new(
            request.product().clone(),
            ArtifactKind::RelocatableObject,
            0,
        ),
        ArtifactRequirement::Required,
        ArtifactRole::Inspection,
        ArtifactProducer::Backend {
            artifact: backend_artifact,
            backend: backend.clone(),
        },
        PlannedArtifactDestination::Publish(OutputSink::Filesystem("application.o".into())),
    );

    (request, backend, artifact, backend_request)
}

pub(crate) fn emitted_artifact(plan: &EmissionPlan) -> EmittedArtifact {
    let Some(planned) = plan.published_artifacts().next() else {
        panic!("test plan must publish one artifact");
    };

    let PlannedArtifactDestination::Publish(sink) = planned.destination() else {
        panic!("test artifact must have an external sink");
    };

    EmittedArtifact::new(
        planned.id().clone(),
        sink.clone(),
        planned.producer().clone(),
        planned.role(),
        4,
        artifact_digest(),
    )
}

fn artifact_digest() -> ArtifactDigest {
    let Some(digest) = ArtifactDigest::try_new(ArtifactDigestAlgorithm::Blake3, [0_u8; 32]) else {
        panic!("test artifact digest must be valid");
    };

    digest
}

pub(crate) fn output_name(kind: TargetOutputKind, prefix: &str, suffix: &str) -> TargetOutputName {
    let Ok(name) = TargetOutputName::try_new(kind, prefix, suffix) else {
        panic!("test target output name must be valid");
    };

    name
}
