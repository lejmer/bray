use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

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
    let output = bray_testing::temporary_directory();

    let reference = test_generation_store(output.path()).join("published-generation.json");
    let observed = AtomicBool::new(false);

    let cancellation = || {
        let committed = reference.is_file();

        observed.fetch_or(committed, Ordering::Relaxed);

        committed
    };

    let plan = filesystem_plan(output.path(), ReplacementPolicy::ReplaceExisting);

    let outcome = ArtifactPublisher::new(&cancellation)
        .publish(&plan, [contribution(&plan, b"complete", None)]);

    assert_complete_artifact(&outcome, b"complete");
    assert!(observed.into_inner(), "cancellation must be observed after reference commit");
    assert!(reference.is_file());
}

#[test]
fn cancellation_preserves_the_published_generation() {
    let directory = bray_testing::temporary_directory();

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
    let directory = bray_testing::temporary_directory();

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

    let store = test_generation_store(directory.path());
    let staged = Mutex::new(None);

    let cancellation = || {
        let path = std::fs::read_dir(&store).into_iter().flatten().find_map(|entry| {
            let entry = entry.expect("private product storage must be readable");
            let prefix = crate::publication::generation::layout::PRIVATE_GENERATION_PREFIX;

            entry.file_name().to_string_lossy().starts_with(prefix).then(|| entry.path())
        });

        if let Some(path) = path {
            *staged.lock().expect("private generation observation must be available") = Some(path);

            return true;
        }

        false
    };

    let outcome = ArtifactPublisher::new(&cancellation).publish(&plan, [metadata]);

    assert!(matches!(outcome.status(), EmissionStatus::Cancelled));
    assert!(outcome.artifacts().artifacts().is_empty());
    assert!(outcome.generation().is_none());

    let private = staged.into_inner().expect("private generation observation must be available")
        .expect("cancellation must occur after the private generation is created");

    assert!(!private.exists());
    assert!(!store.join("published-generation.json").exists());
}
