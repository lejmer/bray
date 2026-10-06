use bray_codegen::{ArtifactDigest, ArtifactDigestAlgorithm};
use bray_diagnostics::{
    Diagnostic, DiagnosticArgName, DiagnosticArgValue, DiagnosticBag, DiagnosticId, DiagnosticKind,
    SeverityKind,
};
use bray_testing::assert_goal_state_diagnostic_kind;

use super::captured_sinks::CapturingResolver;
use super::fixtures::{assert_complete_artifact, contribution, contribution_for, never_cancelled};
use super::plans::{
    TestArtifactSpec, filesystem_artifact_plan, filesystem_plan, memory_artifact_plan, memory_plan,
    package_interface_spec, required_dependency_metadata_spec, test_generation_store,
};

use crate::{
    ArtifactKind, ArtifactProducer, ArtifactPublisher, ArtifactRequirement, ArtifactRole,
    DependencyMetadataProducerId, EmissionFailure, EmissionStatus, LinkerProducerId, OutputSinkId,
    ReplacementPolicy,
};

#[test]
fn corrupted_existing_generation_preserves_the_digest_failure() {
    let Ok(output) = tempfile::tempdir() else {
        panic!("test output directory must be created");
    };

    let plan = filesystem_plan(output.path(), ReplacementPolicy::ReplaceExisting);

    let first = ArtifactPublisher::new(&never_cancelled)
        .publish(&plan, [contribution(&plan, b"stable", None)]);

    assert_complete_artifact(&first, b"stable");

    let generation = first
        .generation()
        .unwrap_or_else(|| panic!("managed generation must exist"));

    let artifact = &first.artifacts().artifacts()[0];

    let path = generation
        .artifact_path(artifact.id())
        .unwrap_or_else(|| panic!("managed artifact path must resolve"));

    std::fs::write(path, b"broken")
        .unwrap_or_else(|error| panic!("test generation must be corrupted: {error}"));

    let outcome = ArtifactPublisher::new(&never_cancelled)
        .publish(&plan, [contribution(&plan, b"stable", None)]);

    assert!(matches!(
        outcome.status(),
        EmissionStatus::Failed(EmissionFailure::Publication(_))
    ));

    assert_eq!(
        bray_testing::diagnostic_at(outcome.diagnostics(), 0).kind(),
        DiagnosticKind::RetainedArtifactDigestMismatch
    );

    assert_goal_state_diagnostic_kind(
        outcome.diagnostics(),
        DiagnosticKind::RetainedArtifactDigestMismatch,
    );

    assert!(outcome.artifacts().artifacts().is_empty());
    assert!(outcome.generation().is_none());
}

#[cfg(unix)]
#[test]
fn existing_generation_reuse_requires_manifest_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let Ok(output) = tempfile::tempdir() else {
        panic!("test output directory must be created");
    };

    let plan = filesystem_plan(output.path(), ReplacementPolicy::ReplaceExisting);

    let first = ArtifactPublisher::new(&never_cancelled)
        .publish(&plan, [contribution(&plan, b"stable", None)]);

    assert_complete_artifact(&first, b"stable");

    let generation = first
        .generation()
        .unwrap_or_else(|| panic!("managed generation must exist"));

    let artifact = &first.artifacts().artifacts()[0];

    let path = generation
        .artifact_path(artifact.id())
        .unwrap_or_else(|| panic!("managed artifact path must resolve"));

    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
        .unwrap_or_else(|error| panic!("test permissions must be changed: {error}"));

    let outcome = ArtifactPublisher::new(&never_cancelled)
        .publish(&plan, [contribution(&plan, b"stable", None)]);

    assert_eq!(
        bray_testing::diagnostic_at(outcome.diagnostics(), 0).kind(),
        DiagnosticKind::RetainedGenerationInvalid
    );

    assert!(matches!(
        outcome.status(),
        EmissionStatus::Failed(EmissionFailure::Publication(_))
    ));
}

#[test]
fn failed_generation_exposes_no_partial_artifacts_or_reference() {
    let Ok(directory) = tempfile::tempdir() else {
        panic!("test output directory must be created");
    };

    let interface_path = directory.path().join("application.brayi");
    let metadata_path = directory.path().join("application.brayd");

    let plan = filesystem_artifact_plan([
        (package_interface_spec(), interface_path.clone()),
        (required_dependency_metadata_spec(), metadata_path.clone()),
    ]);

    let wrong_digest = ArtifactDigest::try_new(ArtifactDigestAlgorithm::Blake3, [0_u8; 32])
        .unwrap_or_else(|| panic!("test digest must be valid"));

    let metadata = contribution_for(
        &plan,
        ArtifactKind::DependencyMetadata,
        b"metadata",
        Some(wrong_digest),
        None,
    );

    let outcome = ArtifactPublisher::new(&never_cancelled).publish(&plan, [metadata]);

    assert!(matches!(
        outcome.status(),
        EmissionStatus::Failed(EmissionFailure::InvalidContribution(_))
    ));

    assert!(outcome.artifacts().artifacts().is_empty());
    assert!(outcome.generation().is_none());

    assert!(
        !test_generation_store(directory.path())
            .join("published-generation.json")
            .exists()
    );
}

#[test]
fn digest_mismatches_fail_before_any_bytes_are_published() {
    let Some(collector) = OutputSinkId::try_new("test.digest") else {
        panic!("test collector identity must be valid");
    };

    let plan = memory_plan(collector.clone());
    let resolver = CapturingResolver::new([collector]);

    let Some(wrong_digest) = ArtifactDigest::try_new(ArtifactDigestAlgorithm::Blake3, [0_u8; 32])
    else {
        panic!("test digest must be valid");
    };

    let contribution = contribution(&plan, b"content", Some(wrong_digest));

    let outcome = ArtifactPublisher::with_sink_resolver(&never_cancelled, &resolver)
        .publish(&plan, [contribution]);

    assert!(matches!(
        outcome.status(),
        EmissionStatus::Failed(EmissionFailure::InvalidContribution(_))
    ));

    assert_eq!(
        bray_testing::diagnostic_at(outcome.diagnostics(), 0).kind(),
        DiagnosticKind::EmissionArtifactDigestMismatch
    );

    assert_goal_state_diagnostic_kind(
        outcome.diagnostics(),
        DiagnosticKind::EmissionArtifactDigestMismatch,
    );

    let diagnostic = bray_testing::diagnostic_at(outcome.diagnostics(), 0);

    let Some(expected) = diagnostic
        .args()
        .iter()
        .find(|arg| arg.name() == DiagnosticArgName::ExpectedArtifactDigest)
    else {
        panic!("digest mismatch must retain the declared digest");
    };

    let DiagnosticArgValue::ArtifactDigest(expected) = expected.value() else {
        panic!("expected digest argument must remain typed");
    };

    let Some(actual) = diagnostic
        .args()
        .iter()
        .find(|arg| arg.name() == DiagnosticArgName::ActualArtifactDigest)
    else {
        panic!("digest mismatch must retain the measured digest");
    };

    let DiagnosticArgValue::ArtifactDigest(actual) = actual.value() else {
        panic!("actual digest argument must remain typed");
    };

    assert_eq!(expected.bytes(), &[0_u8; 32]);
    assert_eq!(actual.bytes(), blake3::hash(b"content").as_bytes());

    assert_eq!(resolver.bytes("test.digest"), b"");
}

#[test]
fn optional_structural_failures_warn_and_omit_the_artifact() {
    let Some(collector) = OutputSinkId::try_new("test.optional") else {
        panic!("test collector identity must be valid");
    };

    let plan = memory_artifact_plan(
        collector.clone(),
        [package_interface_spec(), dependency_metadata_spec()],
    );

    let resolver = CapturingResolver::new([collector]);

    let metadata = contribution_for(
        &plan,
        ArtifactKind::DependencyMetadata,
        b"metadata",
        None,
        Some(ArtifactProducer::DependencyMetadata(
            DependencyMetadataProducerId::new(1),
        )),
    );

    let outcome = ArtifactPublisher::with_sink_resolver(&never_cancelled, &resolver)
        .publish(&plan, [metadata]);

    let artifacts = outcome.artifacts();

    assert!(
        matches!(outcome.status(), EmissionStatus::Complete),
        "unexpected emission outcome: {outcome:#?}"
    );

    assert_eq!(artifacts.artifacts().len(), 1);
    assert_eq!(outcome.diagnostics().warnings().count(), 1);

    assert_eq!(
        bray_testing::diagnostic_at(outcome.diagnostics(), 0).kind(),
        DiagnosticKind::EmissionInvalidContribution
    );

    assert_goal_state_diagnostic_kind(
        outcome.diagnostics(),
        DiagnosticKind::EmissionInvalidContribution,
    );
}

#[test]
fn mixed_publication_warnings_follow_canonical_artifact_order() {
    let Some(collector) = OutputSinkId::try_new("test.order") else {
        panic!("test collector identity must be valid");
    };

    let plan = memory_artifact_plan(
        collector.clone(),
        [
            package_interface_spec(),
            dependency_metadata_spec(),
            executable_spec(),
        ],
    );

    let resolver = CapturingResolver::failing_open([collector], ArtifactKind::DependencyMetadata);

    let metadata = contribution_for(
        &plan,
        ArtifactKind::DependencyMetadata,
        b"metadata",
        None,
        None,
    );

    let Some(wrong_digest) = ArtifactDigest::try_new(ArtifactDigestAlgorithm::Blake3, [0_u8; 32])
    else {
        panic!("test digest must be valid");
    };

    let executable = contribution_for(
        &plan,
        ArtifactKind::Executable,
        b"executable",
        Some(wrong_digest),
        None,
    );

    let outcome = ArtifactPublisher::with_sink_resolver(&never_cancelled, &resolver)
        .publish(&plan, [executable, metadata]);

    assert!(matches!(outcome.status(), EmissionStatus::Complete));
    assert_eq!(outcome.artifacts().artifacts().len(), 1);

    assert_eq!(
        outcome
            .diagnostics()
            .iter()
            .map(|diagnostic| diagnostic.kind())
            .collect::<Vec<_>>(),
        vec![
            DiagnosticKind::EmissionArtifactOpenFailed,
            DiagnosticKind::EmissionArtifactDigestMismatch,
        ]
    );

    assert_goal_state_diagnostic_kind(
        outcome.diagnostics(),
        DiagnosticKind::EmissionArtifactOpenFailed,
    );

    assert_goal_state_diagnostic_kind(
        outcome.diagnostics(),
        DiagnosticKind::EmissionArtifactDigestMismatch,
    );
}

#[test]
fn missing_required_contributions_emit_structured_errors() {
    let Some(collector) = OutputSinkId::try_new("test.missing") else {
        panic!("test collector identity must be valid");
    };

    let plan = memory_plan(collector);
    let outcome = ArtifactPublisher::new(&never_cancelled).publish(&plan, []);

    assert!(matches!(
        outcome.status(),
        EmissionStatus::Failed(EmissionFailure::MissingContribution(_))
    ));

    assert_eq!(outcome.diagnostics().len(), 1);

    assert_eq!(
        bray_testing::diagnostic_at(outcome.diagnostics(), 0).kind(),
        DiagnosticKind::EmissionMissingContribution
    );

    assert_goal_state_diagnostic_kind(
        outcome.diagnostics(),
        DiagnosticKind::EmissionMissingContribution,
    );

    assert_eq!(
        bray_testing::diagnostic_at(outcome.diagnostics(), 0).severity(),
        SeverityKind::Error
    );
}

fn dependency_metadata_spec() -> TestArtifactSpec {
    TestArtifactSpec {
        kind: ArtifactKind::DependencyMetadata,
        requirement: ArtifactRequirement::Optional,
        role: ArtifactRole::Companion,
        producer: ArtifactProducer::DependencyMetadata(DependencyMetadataProducerId::new(0)),
    }
}

fn executable_spec() -> TestArtifactSpec {
    TestArtifactSpec {
        kind: ArtifactKind::Executable,
        requirement: ArtifactRequirement::Optional,
        role: ArtifactRole::Product,
        producer: ArtifactProducer::Linker(LinkerProducerId::new(0)),
    }
}

#[test]
fn prior_error_diagnostics_discard_a_completed_generation() {
    let Ok(output) = tempfile::tempdir() else {
        panic!("test output directory must be created");
    };

    let plan = filesystem_plan(output.path(), ReplacementPolicy::ReplaceExisting);

    let outcome = ArtifactPublisher::new(&never_cancelled)
        .publish(&plan, [contribution(&plan, b"artifact bytes", None)]);

    assert!(outcome.generation().is_some());

    let prior = DiagnosticBag::single(Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::CodegenBackendLibraryFailed,
        SeverityKind::Error,
    ));

    let outcome = outcome.with_prior_diagnostics(&prior);

    assert!(matches!(
        outcome.status(),
        EmissionStatus::Failed(EmissionFailure::IncompleteProduct)
    ));

    assert!(outcome.generation().is_none());

    assert!(
        outcome
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.kind() == DiagnosticKind::CodegenBackendLibraryFailed)
    );
}
