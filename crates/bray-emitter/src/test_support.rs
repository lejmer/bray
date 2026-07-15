use bray_codegen::{
    ArtifactDigest, ArtifactDigestAlgorithm, AssemblySyntaxKind, BackendArtifactId,
    BackendArtifactKind, BackendArtifactRequest, BackendArtifactRequestEntry,
    BackendArtifactRequirement, BackendIdentity, BackendSerializationOptions, CodegenUnit,
    DebugInformationOutputMode, LinkableArtifactRequirement,
};
use bray_symbols::PackageIdentity;
use bray_target::TargetIdentity;
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

pub(crate) fn target_identity() -> TargetIdentity {
    let Some(target) = TargetIdentity::try_new("x86_64-unknown-linux-gnu") else {
        panic!("test target identity must be valid");
    };

    target
}

pub(crate) fn backend_identity() -> BackendIdentity {
    let Some(backend) = BackendIdentity::try_new("llvm", "bray-1", "llvm-22") else {
        panic!("test backend identity must be valid");
    };

    backend
}

pub(crate) fn emission_request(
    artifacts: impl IntoIterator<Item = RequestedArtifact>,
) -> EmissionRequest {
    let Ok(request) = EmissionRequest::try_new(
        product_identity(),
        ProductKind::Executable,
        target_identity(),
        RequestedArtifactDestination::FilesystemDirectory("out".into()),
        artifacts,
        ReplacementPolicy::RequireAbsent,
    ) else {
        panic!("test emission request must be valid");
    };

    request
}

pub(crate) fn emission_plan() -> EmissionPlan {
    let request = emission_request([RequestedArtifact::new(
        ArtifactKind::PackageInterface,
        ArtifactRequirement::Required,
    )]);

    let artifact = PlannedArtifact::new(
        ArtifactId::new(request.product().clone(), ArtifactKind::PackageInterface, 0),
        ArtifactRequirement::Required,
        ArtifactRole::Product,
        ArtifactProducer::PackageInterface,
        PlannedArtifactDestination::Publish(OutputSink::Filesystem("application.brayi".into())),
    );

    let Ok(plan) = EmissionPlan::try_new(request, None, [artifact], []) else {
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

    let Ok(unit) = CodegenUnit::try_new(1, [test_mir_unit(1)]) else {
        panic!("test codegen unit must be valid");
    };

    let backend_artifact = BackendArtifactId::new(
        unit.key().clone(),
        BackendArtifactKind::RelocatableObject,
        0,
    );

    let backend_entry = BackendArtifactRequestEntry::new(
        backend_artifact.clone(),
        BackendArtifactRequirement::Required,
    );

    let serialization = BackendSerializationOptions::new(AssemblySyntaxKind::TargetDefault, false);

    let Ok(backend_request) = BackendArtifactRequest::try_new(
        unit.key().clone(),
        [backend_entry],
        DebugInformationOutputMode::Omit,
        LinkableArtifactRequirement::RelocatableObject,
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
