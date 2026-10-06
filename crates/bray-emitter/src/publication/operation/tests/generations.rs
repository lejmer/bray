use std::path::PathBuf;

use super::fixtures::{
    assert_complete_artifact, contribution, contribution_for, file_bytes, never_cancelled,
};
use super::plans::{
    TestArtifactSpec, filesystem_artifact_plan, filesystem_artifact_plan_for,
    filesystem_artifact_plan_with_product_and_identity, filesystem_plan, package_interface_spec,
    required_dependency_metadata_spec,
};

use crate::test_support::product_identity;
use crate::{ArtifactKind, ArtifactPublisher, EmissionPlan, EmissionStatus, ReplacementPolicy};

#[test]
fn identical_managed_generations_are_reused_by_content_identity() {
    let Ok(output) = tempfile::tempdir() else {
        panic!("test output directory must be created");
    };

    let plan = filesystem_plan(output.path(), ReplacementPolicy::ReplaceExisting);

    let first = ArtifactPublisher::new(&never_cancelled)
        .publish(&plan, [contribution(&plan, b"stable", None)]);

    let second = ArtifactPublisher::new(&never_cancelled)
        .publish(&plan, [contribution(&plan, b"stable", None)]);

    assert_complete_artifact(&first, b"stable");
    assert_complete_artifact(&second, b"stable");

    let first_generation = first
        .generation()
        .unwrap_or_else(|| panic!("first managed generation must exist"));

    let second_generation = second
        .generation()
        .unwrap_or_else(|| panic!("second managed generation must exist"));

    assert_eq!(first_generation.identity(), second_generation.identity());

    let artifact = first.artifacts().artifacts()[0].id();

    assert_eq!(
        first_generation.artifact_path(artifact),
        second_generation.artifact_path(artifact)
    );
}

#[test]
fn managed_generation_retention_keeps_current_and_preceding_unpinned_products() {
    let Ok(output) = tempfile::tempdir() else {
        panic!("test output directory must be created");
    };

    let plan = filesystem_plan(output.path(), ReplacementPolicy::ReplaceExisting);

    let first = ArtifactPublisher::new(&never_cancelled)
        .publish(&plan, [contribution(&plan, b"first", None)]);

    assert_complete_artifact(&first, b"first");

    let first_generation = first
        .generation()
        .unwrap_or_else(|| panic!("first managed generation must exist"));

    let first_path = first_generation
        .artifact_path(first.artifacts().artifacts()[0].id())
        .unwrap_or_else(|| panic!("first private artifact path must resolve"));

    let second = ArtifactPublisher::new(&never_cancelled)
        .publish(&plan, [contribution(&plan, b"second", None)]);

    assert_complete_artifact(&second, b"second");
    assert!(first_path.exists());

    let second_path = second
        .generation()
        .and_then(|generation| generation.artifact_path(second.artifacts().artifacts()[0].id()))
        .unwrap_or_else(|| panic!("second private artifact path must resolve"));

    drop(first);
    drop(second);

    let third = ArtifactPublisher::new(&never_cancelled)
        .publish(&plan, [contribution(&plan, b"third", None)]);

    assert_complete_artifact(&third, b"third");
    assert!(!first_path.exists());
    assert!(second_path.exists());

    assert_eq!(
        file_bytes(&output.path().join("application.brayd")),
        b"third"
    );
}

#[test]
fn pins_survive_replacement_and_repeated_content_preserves_distinct_history() {
    let output = tempfile::tempdir().unwrap();
    let plan = filesystem_plan(output.path(), ReplacementPolicy::ReplaceExisting);

    let first = ArtifactPublisher::new(&never_cancelled)
        .publish(&plan, [contribution(&plan, b"first", None)]);

    let pin = first.generation().unwrap().clone();

    let first_path = pin
        .artifact_path(first.artifacts().artifacts()[0].id())
        .unwrap();

    drop(first);

    let second = ArtifactPublisher::new(&never_cancelled)
        .publish(&plan, [contribution(&plan, b"second", None)]);

    let second_path = second
        .generation()
        .unwrap()
        .artifact_path(second.artifacts().artifacts()[0].id())
        .unwrap();

    drop(second);

    for _ in 0..3 {
        let current = ArtifactPublisher::new(&never_cancelled)
            .publish(&plan, [contribution(&plan, b"third", None)]);

        assert_complete_artifact(&current, b"third");
        assert_eq!(file_bytes(&first_path), b"first");
        assert_eq!(file_bytes(&second_path), b"second");
    }

    drop(pin);

    let current = ArtifactPublisher::new(&never_cancelled)
        .publish(&plan, [contribution(&plan, b"third", None)]);

    assert_complete_artifact(&current, b"third");
    assert!(!first_path.exists());
    assert_eq!(file_bytes(&second_path), b"second");
}

#[test]
fn products_share_immutable_content_until_the_last_retained_reference_is_removed() {
    let output = tempfile::tempdir().unwrap();

    let second_product =
        bray_symbols::ProductIdentity::try_new(product_identity().package().clone(), "second")
            .unwrap();

    let first_plan = filesystem_artifact_plan([(
        required_dependency_metadata_spec(),
        output.path().join("first.brayd"),
    )]);

    let second_plan = filesystem_artifact_plan_for(
        second_product,
        [(
            required_dependency_metadata_spec(),
            output.path().join("second.brayd"),
        )],
    );

    let bytes = b"shared immutable content";

    let first = ArtifactPublisher::new(&never_cancelled)
        .publish(&first_plan, [contribution(&first_plan, bytes, None)]);

    let second = ArtifactPublisher::new(&never_cancelled)
        .publish(&second_plan, [contribution(&second_plan, bytes, None)]);

    assert_complete_artifact(&first, bytes);
    assert_complete_artifact(&second, bytes);

    let first_path = first
        .generation()
        .unwrap()
        .artifact_path(first.artifacts().artifacts()[0].id())
        .unwrap();

    let second_path = second
        .generation()
        .unwrap()
        .artifact_path(second.artifacts().artifacts()[0].id())
        .unwrap();

    assert_eq!(
        file_id::get_file_id(&first_path).unwrap(),
        file_id::get_file_id(&second_path).unwrap()
    );

    assert_ne!(
        file_id::get_file_id(&second_path).unwrap(),
        file_id::get_file_id(output.path().join("second.brayd")).unwrap()
    );

    drop(first);

    let first_selection = crate::StorageSelection {
        product: Some(first_plan.request().product().clone()),
        ..crate::StorageSelection::default()
    };

    let clean =
        crate::clean_storage(output.path(), &first_selection, false, &never_cancelled).unwrap();

    assert!(clean.iter().all(|row| row.removed));
    assert!(!first_path.exists());
    assert_eq!(file_bytes(&second_path), bytes);

    let first = ArtifactPublisher::new(&never_cancelled)
        .publish(&first_plan, [contribution(&first_plan, bytes, None)]);

    assert_complete_artifact(&first, bytes);

    assert_eq!(
        file_id::get_file_id(&first_path).unwrap(),
        file_id::get_file_id(&second_path).unwrap()
    );

    drop(first);
    drop(second);

    let rows = crate::inspect_storage(
        output.path(),
        &crate::StorageSelection::default(),
        crate::StoragePolicy::default(),
        &never_cancelled,
    )
    .unwrap();

    assert_eq!(
        rows.iter().filter_map(|row| row.shared_bytes).sum::<u64>(),
        u64::try_from(bytes.len()).unwrap()
    );

    crate::clean_storage(
        output.path(),
        &crate::StorageSelection::default(),
        false,
        &never_cancelled,
    )
    .unwrap();

    assert!(!first_path.exists());
    assert!(!second_path.exists());

    let index: serde_json::Value =
        serde_json::from_slice(&file_bytes(&output.path().join(".bray/storage-index.json")))
            .unwrap();

    assert!(index["content"].as_object().unwrap().is_empty());
}

#[test]
fn retained_reads_pin_every_companion_while_publication_and_cleanup_advance() {
    let output = tempfile::tempdir().unwrap();

    let first_plan = filesystem_artifact_plan([
        (
            package_interface_spec(),
            output.path().join("application.brayi"),
        ),
        (
            required_dependency_metadata_spec(),
            output.path().join("application.brayd"),
        ),
    ]);

    let first = ArtifactPublisher::new(&never_cancelled)
        .publish(&first_plan, [contribution(&first_plan, b"first", None)]);

    assert!(
        matches!(first.status(), EmissionStatus::Complete),
        "{first:?}"
    );

    let retained = crate::retain_published_generation(
        output.path(),
        first_plan.request().product(),
        &never_cancelled,
    )
    .unwrap();

    let paths: Vec<_> = retained
        .artifacts()
        .map(|(_, _, path)| path.to_owned())
        .collect();

    assert_eq!(paths.len(), 2);
    assert_eq!(retained.identity(), first.generation().unwrap().identity());
    drop(first);

    let plan = filesystem_plan(output.path(), ReplacementPolicy::ReplaceExisting);

    for bytes in [b"second".as_slice(), b"third", b"fourth"] {
        let next = ArtifactPublisher::new(&never_cancelled)
            .publish(&plan, [contribution(&plan, bytes, None)]);

        assert_complete_artifact(&next, bytes);
        assert!(paths.iter().all(|path| path.is_file()));
    }

    let rows = crate::clean_storage(
        output.path(),
        &crate::StorageSelection::default(),
        false,
        &never_cancelled,
    )
    .unwrap();

    assert!(rows.iter().all(|row| row.active && !row.removed));

    assert_eq!(
        file_bytes(
            retained
                .artifact_path(ArtifactKind::DependencyMetadata, 0)
                .unwrap()
        ),
        b"first"
    );

    drop(retained);

    crate::clean_storage(
        output.path(),
        &crate::StorageSelection::default(),
        false,
        &never_cancelled,
    )
    .unwrap();

    assert!(paths.iter().all(|path| !path.exists()));

    let error = crate::retain_published_generation(
        output.path(),
        plan.request().product(),
        &never_cancelled,
    )
    .unwrap_err()
    .into_storage_error(output.path());

    assert_eq!(error.kind(), &crate::StorageErrorKind::Unavailable);
}

#[test]
fn retained_generations_expose_the_atomic_build_identity() {
    let output = tempfile::tempdir().unwrap();

    let identity =
        crate::ProductBuildIdentity::new([1; 32], [2; 32], [3; 32], [4; 32], [5; 32], 6, 7);

    let plan = filesystem_artifact_plan_with_identity(
        [(
            required_dependency_metadata_spec(),
            output.path().join("application.brayd"),
        )],
        identity.clone(),
    );

    let outcome = ArtifactPublisher::new(&never_cancelled)
        .publish(&plan, [contribution(&plan, b"metadata", None)]);

    assert!(matches!(outcome.status(), EmissionStatus::Complete));

    let first_generation = outcome
        .generation()
        .unwrap_or_else(|| panic!("first managed generation must exist"))
        .identity();

    let retained = crate::retain_published_generation(
        output.path(),
        plan.request().product(),
        &never_cancelled,
    )
    .unwrap();

    assert_eq!(retained.build_identity(), Some(&identity));

    let replacement_identity =
        crate::ProductBuildIdentity::new([8; 32], [9; 32], [10; 32], [11; 32], [12; 32], 13, 14);

    let replacement = filesystem_artifact_plan_with_identity(
        [(
            required_dependency_metadata_spec(),
            output.path().join("application.brayd"),
        )],
        replacement_identity.clone(),
    );

    let outcome = ArtifactPublisher::new(&never_cancelled).publish(
        &replacement,
        [contribution(&replacement, b"metadata", None)],
    );

    assert!(matches!(outcome.status(), EmissionStatus::Complete));

    let retained = crate::retain_published_generation(
        output.path(),
        replacement.request().product(),
        &never_cancelled,
    )
    .unwrap();

    assert_ne!(retained.identity(), first_generation);
    assert_eq!(retained.build_identity(), Some(&replacement_identity));
}

#[test]
fn replacement_removes_public_artifacts_absent_from_the_new_generation() {
    let Ok(output) = tempfile::tempdir() else {
        panic!("test output directory must be created");
    };

    let interface = output.path().join("application.brayint");
    let metadata = output.path().join("application.brayd");

    let first = filesystem_artifact_plan([
        (package_interface_spec(), interface.clone()),
        (required_dependency_metadata_spec(), metadata.clone()),
    ]);

    let first_outcome = ArtifactPublisher::new(&never_cancelled).publish(
        &first,
        [contribution_for(
            &first,
            ArtifactKind::DependencyMetadata,
            b"metadata",
            None,
            None,
        )],
    );

    assert!(matches!(first_outcome.status(), EmissionStatus::Complete));
    assert!(metadata.is_file());

    let replacement = filesystem_artifact_plan([(package_interface_spec(), interface)]);

    let replacement_outcome = ArtifactPublisher::new(&never_cancelled).publish(&replacement, []);

    assert!(matches!(
        replacement_outcome.status(),
        EmissionStatus::Complete
    ));

    assert!(!metadata.exists());
}

fn filesystem_artifact_plan_with_identity(
    artifacts: impl IntoIterator<Item = (TestArtifactSpec, PathBuf)>,
    identity: crate::ProductBuildIdentity,
) -> EmissionPlan {
    filesystem_artifact_plan_with_product_and_identity(
        product_identity(),
        artifacts,
        Some(identity),
    )
}
