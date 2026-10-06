use std::path::Path;

use bray_codegen::{ArtifactContent, ArtifactDigest};

use crate::{
    ArtifactContribution, ArtifactKind, ArtifactProducer, EmissionOutcome, EmissionPlan,
    EmissionStatus,
};

pub(super) fn contribution(
    plan: &EmissionPlan,
    bytes: &[u8],
    digest: Option<ArtifactDigest>,
) -> ArtifactContribution {
    contribution_for(plan, ArtifactKind::DependencyMetadata, bytes, digest, None)
}

pub(super) fn contribution_for(
    plan: &EmissionPlan,
    kind: ArtifactKind,
    bytes: &[u8],
    digest: Option<ArtifactDigest>,
    producer: Option<ArtifactProducer>,
) -> ArtifactContribution {
    let Some(planned) = plan
        .published_artifacts()
        .find(|artifact| artifact.id().kind() == kind)
    else {
        panic!("test plan must publish the requested artifact kind");
    };

    let Ok(content) = ArtifactContent::try_memory(bytes.to_vec()) else {
        panic!("test artifact content must be valid");
    };

    let producer = producer.unwrap_or_else(|| planned.producer().clone());

    ArtifactContribution::new(planned.id().clone(), producer, content, digest)
}

pub(super) fn assert_complete_artifact(outcome: &EmissionOutcome, expected: &[u8]) {
    let artifacts = outcome.artifacts();

    let Ok(expected_len) = u64::try_from(expected.len()) else {
        panic!("test artifact length must fit the publication contract");
    };

    assert!(
        matches!(outcome.status(), EmissionStatus::Complete),
        "unexpected emission outcome: {outcome:#?}"
    );

    assert!(outcome.diagnostics().is_empty());

    assert_eq!(artifacts.artifacts().len(), 1);
    assert_eq!(artifacts.artifacts()[0].byte_len(), expected_len);

    assert_eq!(
        artifacts.artifacts()[0].digest().bytes(),
        blake3::hash(expected).as_bytes()
    );
}

pub(super) fn file_bytes(path: &Path) -> Vec<u8> {
    let Ok(bytes) = std::fs::read(path) else {
        panic!("published test artifact must be readable");
    };

    bytes
}

pub(super) fn never_cancelled() -> bool {
    false
}

pub(super) fn always_cancelled() -> bool {
    true
}
