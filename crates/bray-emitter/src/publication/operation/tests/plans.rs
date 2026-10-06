use std::path::{Path, PathBuf};

use crate::test_support::{interface_artifact, product_identity, target_identity};
use crate::{
    ArtifactId, ArtifactKind, ArtifactProducer, ArtifactRequirement, ArtifactRole,
    DependencyMetadataProducerId, EmissionPlan, EmissionRequest, OutputSink, OutputSinkId,
    PlannedArtifact, PlannedArtifactDestination, ProductKind, ReplacementPolicy, RequestedArtifact,
    RequestedArtifactDestination,
};

pub(super) fn memory_plan(collector: OutputSinkId) -> EmissionPlan {
    memory_artifact_plan(collector, [required_dependency_metadata_spec()])
}

pub(super) fn filesystem_plan(path: &Path, replacement: ReplacementPolicy) -> EmissionPlan {
    publication_plan(
        RequestedArtifactDestination::FilesystemDirectory(path.to_owned().into()),
        OutputSink::ManagedFilesystem {
            root: path.to_owned(),
            artifact: crate::ManagedArtifactPath::try_new("application.brayd")
                .unwrap_or_else(|| panic!("test managed path must be valid")),
            published: path.join("application.brayd"),
        },
        replacement,
    )
}

pub(super) fn filesystem_artifact_plan(
    artifacts: impl IntoIterator<Item = (TestArtifactSpec, PathBuf)>,
) -> EmissionPlan {
    filesystem_artifact_plan_for(product_identity(), artifacts)
}

pub(super) fn filesystem_artifact_plan_for(
    product: bray_symbols::ProductIdentity,
    artifacts: impl IntoIterator<Item = (TestArtifactSpec, PathBuf)>,
) -> EmissionPlan {
    filesystem_artifact_plan_with_product_and_identity(product, artifacts, None)
}

pub(super) fn filesystem_artifact_plan_with_product_and_identity(
    product: bray_symbols::ProductIdentity,
    artifacts: impl IntoIterator<Item = (TestArtifactSpec, PathBuf)>,
    identity: Option<crate::ProductBuildIdentity>,
) -> EmissionPlan {
    let artifacts: Vec<_> = artifacts.into_iter().collect();

    let package_interface = artifacts
        .iter()
        .any(|(artifact, _)| artifact.kind == ArtifactKind::PackageInterface)
        .then(interface_artifact);

    let requested = artifacts
        .iter()
        .map(|(artifact, _)| RequestedArtifact::new(artifact.kind, artifact.requirement));

    let root = artifacts
        .first()
        .and_then(|(_, path)| path.parent())
        .unwrap_or_else(|| panic!("test artifact paths must share a parent"))
        .to_owned();

    let Ok(request) = EmissionRequest::try_new(
        product,
        ProductKind::Library,
        None,
        target_identity(),
        RequestedArtifactDestination::FilesystemDirectory(root.clone().into()),
        requested,
        ReplacementPolicy::ReplaceExisting,
    ) else {
        panic!("test filesystem publication request must be valid");
    };

    let request = match identity {
        Some(identity) => request.with_build_identity(identity),
        None => request,
    };

    let product = request.product().clone();

    let planned = artifacts.into_iter().map(|(artifact, path)| {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_else(|| panic!("test artifact path must have a portable name"));

        PlannedArtifact::new(
            ArtifactId::new(product.clone(), artifact.kind, 0),
            artifact.requirement,
            artifact.role,
            artifact.producer,
            PlannedArtifactDestination::Publish(OutputSink::ManagedFilesystem {
                root: root.clone(),
                artifact: crate::ManagedArtifactPath::try_new(name)
                    .unwrap_or_else(|| panic!("test managed path must be valid")),
                published: path,
            }),
        )
    });

    let plan = EmissionPlan::new(request, None, None, planned, [], package_interface);

    plan
}

pub(super) fn publication_plan(
    destination: RequestedArtifactDestination,
    sink: OutputSink,
    replacement: ReplacementPolicy,
) -> EmissionPlan {
    let Ok(request) = EmissionRequest::try_new(
        product_identity(),
        ProductKind::Library,
        None,
        target_identity(),
        destination,
        [RequestedArtifact::new(
            ArtifactKind::DependencyMetadata,
            ArtifactRequirement::Required,
        )],
        replacement,
    ) else {
        panic!("test publication request must be valid");
    };

    let artifact = PlannedArtifact::new(
        ArtifactId::new(
            request.product().clone(),
            ArtifactKind::DependencyMetadata,
            0,
        ),
        ArtifactRequirement::Required,
        ArtifactRole::Companion,
        ArtifactProducer::DependencyMetadata(DependencyMetadataProducerId::new(0)),
        PlannedArtifactDestination::Publish(sink),
    );

    let plan = EmissionPlan::new(request, None, None, [artifact], [], None);

    plan
}

pub(super) fn memory_artifact_plan(
    collector: OutputSinkId,
    artifacts: impl IntoIterator<Item = TestArtifactSpec>,
) -> EmissionPlan {
    let artifacts: Vec<_> = artifacts.into_iter().collect();

    let package_interface = artifacts
        .iter()
        .any(|artifact| artifact.kind == ArtifactKind::PackageInterface)
        .then(interface_artifact);

    let requested = artifacts
        .iter()
        .map(|artifact| RequestedArtifact::new(artifact.kind, artifact.requirement));

    let Ok(request) = EmissionRequest::try_new(
        product_identity(),
        ProductKind::Library,
        None,
        target_identity(),
        RequestedArtifactDestination::Memory(collector.clone()),
        requested,
        ReplacementPolicy::RequireAbsent,
    ) else {
        panic!("test memory publication request must be valid");
    };

    let product = request.product().clone();

    let planned = artifacts.into_iter().map(|artifact| {
        let id = ArtifactId::new(product.clone(), artifact.kind, 0);

        PlannedArtifact::new(
            id.clone(),
            artifact.requirement,
            artifact.role,
            artifact.producer,
            PlannedArtifactDestination::Publish(OutputSink::Memory {
                collector: collector.clone(),
                artifact: id,
            }),
        )
    });

    let plan = EmissionPlan::new(request, None, None, planned, [], package_interface);

    plan
}

pub(super) fn package_interface_spec() -> TestArtifactSpec {
    TestArtifactSpec {
        kind: ArtifactKind::PackageInterface,
        requirement: ArtifactRequirement::Required,
        role: ArtifactRole::Product,
        producer: ArtifactProducer::PackageInterface,
    }
}

pub(super) fn required_dependency_metadata_spec() -> TestArtifactSpec {
    TestArtifactSpec {
        kind: ArtifactKind::DependencyMetadata,
        requirement: ArtifactRequirement::Required,
        role: ArtifactRole::Companion,
        producer: ArtifactProducer::DependencyMetadata(DependencyMetadataProducerId::new(0)),
    }
}

pub(super) struct TestArtifactSpec {
    pub(super) kind: ArtifactKind,
    pub(super) requirement: ArtifactRequirement,
    pub(super) role: ArtifactRole,
    pub(super) producer: ArtifactProducer,
}

pub(super) fn test_generation_store(root: &Path) -> PathBuf {
    crate::publication::generation::layout::product_store(root, Path::new(""), &product_identity())
}
