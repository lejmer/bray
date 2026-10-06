use super::fixtures::{assert_complete_artifact, contribution, file_bytes, never_cancelled};
use super::plans::{filesystem_plan, test_generation_store};

use crate::{ArtifactKind, ArtifactPublisher, EmissionPlan, ReplacementPolicy};

#[test]
fn storage_clean_respects_live_pins_dry_runs_and_unmanaged_files() {
    let output = tempfile::tempdir().unwrap();
    let plan = filesystem_plan(output.path(), ReplacementPolicy::ReplaceExisting);

    let current = ArtifactPublisher::new(&never_cancelled)
        .publish(&plan, [contribution(&plan, b"current", None)]);

    let public = output.path().join("application.brayd");
    let unmanaged = output.path().join("keep-user-data.txt");
    let selection = crate::StorageSelection::default();

    std::fs::write(&unmanaged, b"user data").unwrap();

    let active = crate::clean_storage(output.path(), &selection, false, &never_cancelled).unwrap();

    assert!(active.iter().any(|entry| entry.active));
    assert!(active.iter().all(|entry| !entry.removed));
    assert_eq!(file_bytes(&public), b"current");
    drop(current);

    let index = output.path().join(".bray/storage-index.json");
    let before = file_bytes(&index);

    let preview = crate::clean_storage(output.path(), &selection, true, &never_cancelled).unwrap();

    assert!(
        preview
            .iter()
            .any(|entry| entry.category == crate::StorageCategory::CurrentOutputs)
    );

    assert!(
        preview
            .iter()
            .any(|entry| entry.category == crate::StorageCategory::RetainedRerun)
    );

    assert!(preview.iter().all(|entry| !entry.removed && !entry.active));
    assert_eq!(file_bytes(&index), before);
    assert_eq!(file_bytes(&public), b"current");

    let removed = crate::clean_storage(output.path(), &selection, false, &never_cancelled).unwrap();

    assert!(removed.iter().all(|entry| entry.removed));
    assert!(!public.exists());
    assert!(!test_generation_store(output.path()).exists());
    assert_eq!(file_bytes(&unmanaged), b"user data");

    let rebuilt = ArtifactPublisher::new(&never_cancelled)
        .publish(&plan, [contribution(&plan, b"rebuilt", None)]);

    assert_complete_artifact(&rebuilt, b"rebuilt");
}

#[test]
fn active_product_storage_respects_every_context_filter() {
    let output = tempfile::tempdir().unwrap();
    let plan = filesystem_plan(output.path(), ReplacementPolicy::ReplaceExisting);

    let _active = ArtifactPublisher::new(&never_cancelled)
        .publish(&plan, [contribution(&plan, b"active", None)]);

    let selections = [
        crate::StorageSelection {
            target: Some(
                bray_target::TargetIdentity::try_new("aarch64-unknown-linux-gnu").unwrap(),
            ),
            ..Default::default()
        },
        crate::StorageSelection {
            profile: Some("release".to_owned()),
            ..Default::default()
        },
        crate::StorageSelection {
            toolchain: Some("other-toolchain".to_owned()),
            ..Default::default()
        },
    ];

    for selection in selections {
        let rows = crate::inspect_storage(
            output.path(),
            &selection,
            crate::StoragePolicy::default(),
            &never_cancelled,
        )
        .unwrap();

        assert!(rows.is_empty());
    }
}

#[test]
fn maintenance_removes_uncommitted_first_publication_and_abandoned_staging() {
    let output = tempfile::tempdir().unwrap();
    let plan = filesystem_plan(output.path(), ReplacementPolicy::ReplaceExisting);

    let current = ArtifactPublisher::new(&never_cancelled)
        .publish(&plan, [contribution(&plan, b"never committed", None)]);

    let store = current.generation().unwrap().store().to_owned();
    let public = output.path().join("application.brayd");
    let journal = store.join("pending-publication.json");
    let staging = store.join("staging/abandoned");

    std::fs::write(&journal, br#"{"revision":1,"paths":["application.brayd"]}"#).unwrap();
    std::fs::write(&staging, b"partial output").unwrap();
    std::fs::remove_file(current.generation().unwrap().reference()).unwrap();
    drop(current);

    let operation = crate::ManagedOperation::begin(
        output.path(),
        plan.request().product(),
        plan.request().target(),
        &never_cancelled,
    )
    .unwrap();

    assert!(operation.directory().is_dir());
    assert!(!public.exists());
    assert!(!journal.exists());
    assert!(!staging.exists());

    let rebuilt = ArtifactPublisher::new(&never_cancelled)
        .publish(&plan, [contribution(&plan, b"committed", None)]);

    assert_complete_artifact(&rebuilt, b"committed");
}

#[test]
fn selective_clean_removes_previous_profile_and_staging_without_current_outputs() {
    let output = tempfile::tempdir().unwrap();
    let base = filesystem_plan(output.path(), ReplacementPolicy::ReplaceExisting);

    let profile_plan = |profile| {
        EmissionPlan::new(
            base.request().clone().with_storage_profile(profile),
            None,
            None,
            base.artifacts().iter().cloned(),
            [],
            None,
        )
    };

    let old_plan = profile_plan("debug");

    let old = ArtifactPublisher::new(&never_cancelled)
        .publish(&old_plan, [contribution(&old_plan, b"old", None)]);

    let old_directory = old
        .generation()
        .unwrap()
        .artifact_path(old_plan.artifacts()[0].id())
        .unwrap()
        .parent()
        .unwrap()
        .to_owned();

    drop(old);

    let plan = profile_plan("release");

    let current = ArtifactPublisher::new(&never_cancelled)
        .publish(&plan, [contribution(&plan, b"current", None)]);

    let store = current.generation().unwrap().store().to_owned();

    let current_directory = current
        .generation()
        .unwrap()
        .artifact_path(plan.artifacts()[0].id())
        .unwrap()
        .parent()
        .unwrap()
        .to_owned();

    let staging = store.join("staging/abandoned");

    drop(current);
    std::fs::write(&staging, b"temporary").unwrap();

    let selection = crate::StorageSelection {
        profile: Some("debug".into()),
        ..Default::default()
    };

    let rows = crate::inspect_storage(
        output.path(),
        &selection,
        crate::StoragePolicy::default(),
        &never_cancelled,
    )
    .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].category, crate::StorageCategory::RetainedHistory);
    assert_eq!(rows[0].profile.as_deref(), Some("debug"));
    assert_eq!(rows[0].shared_bytes, Some(0));

    let preview = crate::clean_storage(output.path(), &selection, true, &never_cancelled).unwrap();

    assert_eq!(preview, rows);
    assert!(old_directory.exists());

    let cleaned = crate::clean_storage(output.path(), &selection, false, &never_cancelled).unwrap();

    assert!(cleaned.iter().all(|row| row.removed));
    assert!(!old_directory.exists());
    assert!(current_directory.exists());
    assert!(staging.exists());

    assert_eq!(
        file_bytes(&output.path().join("application.brayd")),
        b"current"
    );

    let product_rows = crate::inspect_storage(
        output.path(),
        &crate::StorageSelection {
            kind: Some(crate::StorageKind::Products),
            ..Default::default()
        },
        crate::StoragePolicy::default(),
        &never_cancelled,
    )
    .unwrap();

    assert!(
        product_rows
            .iter()
            .all(|row| row.path != store.join("staging"))
    );

    let selection = crate::StorageSelection {
        kind: Some(crate::StorageKind::Intermediates),
        ..Default::default()
    };

    let cleaned = crate::clean_storage(output.path(), &selection, false, &never_cancelled).unwrap();

    assert!(!cleaned.is_empty());
    assert!(cleaned.iter().any(|row| row.path == store.join("staging")));
    assert!(cleaned.iter().all(|row| row.removed));
    assert!(!staging.exists());
    assert!(current_directory.exists());

    assert!(
        crate::resolve_published_artifact(
            output.path(),
            plan.request().product(),
            ArtifactKind::DependencyMetadata,
            0
        )
        .is_ok()
    );
}

#[test]
fn inactive_product_reclamation_removes_public_files_and_preserves_unmanaged_neighbors() {
    let output = tempfile::tempdir().unwrap();
    let plan = filesystem_plan(output.path(), ReplacementPolicy::ReplaceExisting);

    let product = ArtifactPublisher::new(&never_cancelled)
        .publish(&plan, [contribution(&plan, b"old", None)]);

    let store = product.generation().unwrap().store().to_owned();
    let public = output.path().join("application.brayd");
    let unmanaged = output.path().join("notes.txt");

    drop(product);
    std::fs::write(&unmanaged, b"user data").unwrap();

    let mut managed = crate::storage::ManagedStore::open(output.path()).unwrap();

    managed
        .maintain(
            crate::StoragePolicy {
                inactive_product_age: std::time::Duration::ZERO,
                ..Default::default()
            },
            &never_cancelled,
        )
        .unwrap();

    drop(managed);

    assert!(!store.exists());
    assert!(!public.exists());
    assert_eq!(file_bytes(&unmanaged), b"user data");

    let error = crate::retain_published_generation(
        output.path(),
        plan.request().product(),
        &never_cancelled,
    )
    .unwrap_err()
    .into_storage_error(&store);

    assert_eq!(error.kind(), &crate::StorageErrorKind::Unavailable);
}

#[test]
fn interrupted_storage_clean_resumes_after_public_output_removal() {
    let output = tempfile::tempdir().unwrap();
    let plan = filesystem_plan(output.path(), ReplacementPolicy::ReplaceExisting);

    let current = ArtifactPublisher::new(&never_cancelled)
        .publish(&plan, [contribution(&plan, b"current", None)]);

    let public = output.path().join("application.brayd");

    drop(current);

    let selection = crate::StorageSelection::default();

    let error =
        crate::clean_storage(output.path(), &selection, false, &|| !public.exists()).unwrap_err();

    assert_eq!(error.kind(), &crate::StorageErrorKind::Cancelled);
    assert!(!public.exists());

    let resumed = crate::clean_storage(output.path(), &selection, false, &never_cancelled).unwrap();

    assert!(resumed.iter().all(|entry| entry.removed));

    assert!(
        crate::inspect_storage(
            output.path(),
            &selection,
            crate::StoragePolicy::default(),
            &never_cancelled
        )
        .unwrap()
        .is_empty()
    );

    let rebuilt = ArtifactPublisher::new(&never_cancelled)
        .publish(&plan, [contribution(&plan, b"rebuilt", None)]);

    assert_complete_artifact(&rebuilt, b"rebuilt");
}
