use std::sync::{Arc, Barrier};

use super::fixtures::{contribution, file_bytes, never_cancelled};
use super::plans::filesystem_plan;

use crate::{ArtifactKind, ArtifactPublisher, EmissionStatus, ReplacementPolicy};

#[test]
fn readers_recover_a_crashed_public_projection_before_consuming_files() {
    let output = tempfile::tempdir().expect("temporary fixture directory must be created");
    let plan = filesystem_plan(output.path(), ReplacementPolicy::ReplaceExisting);

    let current = ArtifactPublisher::new(&never_cancelled)
        .publish(&plan, [contribution(&plan, b"committed", None)]);

    let store = current.generation().expect("fixture `generation` call must succeed").store();
    let public = output.path().join("application.brayd");
    let extra = output.path().join("unfinished.brayd");
    let unrelated = output.path().join("user.txt");
    let journal = store.join("pending-publication.json");

    std::fs::write(
        &journal,
        br#"{"revision":1,"paths":["application.brayd","unfinished.brayd"]}"#,
    )
    .expect("fixture contents must be written");

    std::fs::write(&public, b"uncommitted").expect("fixture contents must be written");
    std::fs::write(&extra, b"uncommitted companion").expect("fixture contents must be written");
    std::fs::write(&unrelated, b"user data").expect("fixture contents must be written");

    let resolved = crate::resolve_published_artifact(
        output.path(),
        plan.request().product(),
        ArtifactKind::DependencyMetadata,
        0,
    )
    .expect("fixture published artifact must resolve");

    assert_eq!(file_bytes(resolved.path()), b"committed");
    assert!(!extra.exists());
    assert!(!journal.exists());
    assert_eq!(file_bytes(&unrelated), b"user data");
}

#[test]
fn a_committed_reference_wins_over_an_unfinished_journal() {
    let output = tempfile::tempdir().expect("temporary fixture directory must be created");
    let plan = filesystem_plan(output.path(), ReplacementPolicy::ReplaceExisting);

    let current = ArtifactPublisher::new(&never_cancelled)
        .publish(&plan, [contribution(&plan, b"committed", None)]);

    let store = current.generation().expect("fixture `generation` call must succeed").store();
    let journal = store.join("pending-publication.json");

    std::fs::write(&journal, br#"{"revision":1,"paths":["application.brayd"]}"#).expect("fixture contents must be written");
    std::fs::remove_file(output.path().join("application.brayd")).expect("fixture file must be removed");

    let resolved = crate::resolve_published_artifact(
        output.path(),
        plan.request().product(),
        ArtifactKind::DependencyMetadata,
        0,
    )
    .expect("fixture published artifact must resolve");

    assert_eq!(file_bytes(resolved.path()), b"committed");
    assert!(!journal.exists());
}

#[test]
fn interrupted_publication_recovery_restarts_before_removing_uncommitted_companions() {
    let output = tempfile::tempdir().expect("temporary fixture directory must be created");
    let plan = filesystem_plan(output.path(), ReplacementPolicy::ReplaceExisting);

    let product = ArtifactPublisher::new(&never_cancelled)
        .publish(&plan, [contribution(&plan, b"committed", None)]);

    let store = product.generation().expect("fixture `generation` call must succeed").store().to_owned();
    let public = output.path().join("application.brayd");
    let extra = output.path().join("unfinished.brayd");
    let journal = store.join("pending-publication.json");

    drop(product);

    std::fs::write(
        &journal,
        br#"{"revision":1,"paths":["application.brayd","unfinished.brayd"]}"#,
    )
    .expect("fixture contents must be written");

    std::fs::write(&public, b"uncommitted").expect("fixture contents must be written");
    std::fs::write(&extra, b"unfinished").expect("fixture contents must be written");

    let mut managed = crate::storage::ManagedStore::open(output.path()).expect("fixture file must open");
    let cancelled = || std::fs::read(&public).is_ok_and(|bytes| bytes == b"committed");

    let error = managed
        .maintain(crate::StoragePolicy::default(), &cancelled)
        .expect_err("cancelled recovery maintenance must return cancellation");

    assert_eq!(error.kind(), &crate::StorageErrorKind::Cancelled);
    assert!(journal.exists());
    assert!(extra.exists());

    managed
        .maintain(crate::StoragePolicy::default(), &never_cancelled)
        .expect("fixture `maintain` call must succeed");

    assert!(!journal.exists());
    assert!(!extra.exists());
    assert_eq!(file_bytes(&public), b"committed");
}

#[test]
fn concurrent_product_publishers_leave_one_complete_visible_generation() {
    let Ok(output) = tempfile::tempdir() else {
        panic!("test output directory must be created");
    };

    let plan = Arc::new(filesystem_plan(
        output.path(),
        ReplacementPolicy::ReplaceExisting,
    ));

    let barrier = Arc::new(Barrier::new(3));

    let outcomes = std::thread::scope(|scope| {
        let publish = |bytes: &'static [u8]| {
            let plan = Arc::clone(&plan);
            let barrier = Arc::clone(&barrier);

            scope.spawn(move || {
                let contribution = contribution(&plan, bytes, None);

                barrier.wait();

                ArtifactPublisher::new(&never_cancelled).publish(&plan, [contribution])
            })
        };

        let first = publish(b"first concurrent product");
        let second = publish(b"second concurrent product");

        barrier.wait();

        [
            first
                .join()
                .unwrap_or_else(|_| panic!("first publisher must finish")),
            second
                .join()
                .unwrap_or_else(|_| panic!("second publisher must finish")),
        ]
    });

    assert!(
        outcomes
            .iter()
            .all(|outcome| matches!(outcome.status(), EmissionStatus::Complete))
    );

    let visible = file_bytes(&output.path().join("application.brayd"));

    assert!(matches!(
        visible.as_slice(),
        b"first concurrent product" | b"second concurrent product"
    ));

    let resolved = crate::resolve_published_artifact(
        output.path(),
        plan.request().product(),
        ArtifactKind::DependencyMetadata,
        0,
    )
    .unwrap_or_else(|error| panic!("visible generation must resolve: {error:?}"));

    assert_eq!(file_bytes(resolved.path()), visible);
}
