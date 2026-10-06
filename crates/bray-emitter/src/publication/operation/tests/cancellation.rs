use std::sync::atomic::{AtomicUsize, Ordering};

use super::fixtures::{
    always_cancelled, assert_complete_artifact, contribution, contribution_for, file_bytes,
    never_cancelled,
};
use super::plans::{
    filesystem_artifact_plan, filesystem_plan, package_interface_spec,
    required_dependency_metadata_spec, test_generation_store,
};

use crate::{ArtifactKind, ArtifactPublisher, EmissionStatus, ReplacementPolicy};

#[test]
fn cancellation_observed_after_reference_commit_cannot_retract_success() {
    let Ok(output) = tempfile::tempdir() else {
        panic!("test output directory must be created");
    };

    let reference = test_generation_store(output.path()).join("published-generation.json");
    let cancellation = || reference.exists();
    let plan = filesystem_plan(output.path(), ReplacementPolicy::ReplaceExisting);

    let outcome = ArtifactPublisher::new(&cancellation)
        .publish(&plan, [contribution(&plan, b"complete", None)]);

    assert_complete_artifact(&outcome, b"complete");
    assert!(reference.is_file());
}

#[test]
fn cancellation_preserves_the_published_generation() {
    let Ok(directory) = tempfile::tempdir() else {
        panic!("test output directory must be created");
    };

    let plan = filesystem_plan(directory.path(), ReplacementPolicy::ReplaceExisting);

    let existing = ArtifactPublisher::new(&never_cancelled)
        .publish(&plan, [contribution(&plan, b"existing", None)]);

    assert_complete_artifact(&existing, b"existing");

    let reference = test_generation_store(directory.path()).join("published-generation.json");
    let existing_reference = file_bytes(&reference);

    let existing_artifact = existing
        .generation()
        .and_then(|generation| {
            generation.published_artifact_path(existing.artifacts().artifacts()[0].id())
        })
        .unwrap_or_else(|| panic!("stable existing artifact path must resolve"));

    let contribution = contribution(&plan, b"replacement", None);
    let outcome = ArtifactPublisher::new(&always_cancelled).publish(&plan, [contribution]);

    assert!(matches!(outcome.status(), EmissionStatus::Cancelled));
    assert!(outcome.artifacts().artifacts().is_empty());
    assert!(outcome.diagnostics().is_empty());
    assert_eq!(file_bytes(&reference), existing_reference);
    assert_eq!(file_bytes(&existing_artifact), b"existing");
}

#[test]
fn cancellation_discards_private_generation_without_partial_records() {
    let Ok(directory) = tempfile::tempdir() else {
        panic!("test output directory must be created");
    };

    let interface_path = directory.path().join("application.brayi");
    let metadata_path = directory.path().join("application.brayd");

    let plan = filesystem_artifact_plan([
        (package_interface_spec(), interface_path.clone()),
        (required_dependency_metadata_spec(), metadata_path.clone()),
    ]);

    let metadata = contribution_for(
        &plan,
        ArtifactKind::DependencyMetadata,
        b"metadata",
        None,
        None,
    );

    let observations = AtomicUsize::new(0);
    let cancellation = || observations.fetch_add(1, Ordering::AcqRel) >= 4;
    let outcome = ArtifactPublisher::new(&cancellation).publish(&plan, [metadata]);

    assert!(matches!(outcome.status(), EmissionStatus::Cancelled));
    assert!(outcome.artifacts().artifacts().is_empty());
    assert!(outcome.generation().is_none());

    assert!(
        !test_generation_store(directory.path())
            .join("published-generation.json")
            .exists()
    );
}
