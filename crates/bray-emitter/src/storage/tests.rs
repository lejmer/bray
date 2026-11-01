use std::path::Path;
use std::time::{Duration, SystemTime};

use super::index::ManagedStore;
use super::record::{EntryKind, EntryState, StorageOwner};
use crate::test_support::{product_identity, target_identity, wait_for_lease_release};
use crate::{
    ManagedCache, ManagedOperation, StorageCategory, StoragePolicy, StorageSelection,
    inspect_storage,
};

#[test]
fn failed_directory_formation_can_be_inspected_cleaned_and_retried() {
    let states = [EntryState::Live, EntryState::Retiring, EntryState::RemovingFiles];

    for (kind, state) in [EntryKind::Product, EntryKind::Cache, EntryKind::Operation]
        .into_iter()
        .flat_map(|kind| states.map(|state| (kind, state)))
    {
        let root = tempfile::tempdir().expect("fixture directory must be created");
        let plan = managed_library_plan(root.path(), "example.interrupted");
        let mut store = ManagedStore::open(root.path()).expect("fixture store must open");
        let key = format!("{}/{}", kind.directory(), "a".repeat(64));
        let blocker = store.metadata.join(kind.directory());

        std::fs::write(&blocker, b"unmanaged file").expect("fixture blocker must be written");

        let error = store
            .register(
                &key,
                kind,
                StorageOwner::product(&plan),
                SystemTime::now(),
                &|| false,
            )
            .expect_err("directory formation must fail");

        assert_eq!(error.path(), blocker);
        assert_eq!(error.kind(), &crate::StorageErrorKind::UnsafePath);

        store
            .index
            .entries
            .get_mut(&key)
            .expect("failed formation must retain its ownership reservation")
            .state = state;

        store.save().expect("fixture retirement state must persist");
        drop(store);

        std::fs::remove_file(&blocker).expect("fixture blocker must be removed");

        let selection = StorageSelection::default();

        let rows = inspect_storage(root.path(), &selection, StoragePolicy::default(), &|| false)
            .expect("an unformed reservation must remain inspectable");

        assert_eq!(rows.len(), 1, "{kind:?}");
        assert_eq!(rows[0].category, StorageCategory::Reclaimable);
        assert_eq!(rows[0].bytes, Some(0));
        assert!(!rows[0].active);

        let preview = crate::clean_storage(root.path(), &selection, true, &|| false)
            .expect("cleanup preview must accept an unformed reservation");

        assert_eq!(preview, rows);

        assert!(
            ManagedStore::open(root.path())
                .expect("fixture store must reopen")
                .index
                .entries
                .contains_key(&key)
        );

        let cleaned = crate::clean_storage(root.path(), &selection, false, &|| false)
            .expect("cleanup must retire an unformed reservation");

        assert_eq!(cleaned.len(), 1);
        assert!(cleaned[0].removed);

        let mut store = ManagedStore::open(root.path()).expect("fixture store must reopen");

        assert!(store.index.entries.is_empty());

        let lease = store
            .register(
                &key,
                kind,
                StorageOwner::product(&plan),
                SystemTime::now(),
                &|| false,
            )
            .expect("admission must succeed after recovery");

        assert!(store.metadata.join(&key).is_dir());
        drop(lease);
    }
}

#[test]
fn missing_product_maintenance_releases_claims_and_preserves_unproven_public_files() {
    for state in [EntryState::Live, EntryState::Retiring, EntryState::RemovingFiles] {
        let root = tempfile::tempdir().expect("fixture directory must be created");
        let plan = managed_library_plan(root.path(), "example.missing");
        let key = format!("products/{}", "a".repeat(64));
        let relative = Path::new(".bray").join(&key);
        let mut store = ManagedStore::open(root.path()).expect("fixture store must open");

        let lease = store
            .register_product(&relative, &plan, &|| false)
            .expect("fixture product must register");

        let public = root.path().join("application.brayi");
        let private = root.path().join(&relative);

        std::fs::write(&public, b"unproven output").expect("fixture output must be written");

        store
            .record_content("b".repeat(64), &private.join("0000000000000001/body.o"))
            .expect("fixture content reference must register");

        drop(lease);
        std::fs::remove_dir_all(&private).expect("fixture backing must be removed");

        let error = store
            .pin_product(&relative, StoragePolicy::default())
            .expect_err("missing product backing must not be pinnable");

        assert_eq!(error.path(), private);
        assert_eq!(error.kind(), &crate::StorageErrorKind::Unavailable);

        store
            .index
            .entries
            .get_mut(&key)
            .expect("fixture product must remain indexed")
            .state = state;

        store.save().expect("fixture retirement state must persist");

        store
            .maintain(StoragePolicy::default(), &|| false)
            .expect("maintenance must retire missing backing and its claims");

        drop(store);

        let store = ManagedStore::open(root.path()).expect("recovered index must remain valid");

        assert!(store.index.entries.is_empty());
        assert!(store.index.public_paths.is_empty());
        assert!(store.index.content.is_empty());
        assert_eq!(std::fs::read(&public).expect("unproven output must remain"), b"unproven output");
    }
}

#[cfg(unix)]
#[test]
fn missing_backing_rejects_unsafe_source_and_retired_directory_chains() {
    for directory in ["products", "retired"] {
        let root = tempfile::tempdir().expect("fixture directory must be created");
        let outside = tempfile::tempdir().expect("fixture directory must be created");
        let sentinel = outside.path().join("user-data");
        let plan = managed_library_plan(root.path(), "example.unsafe");
        let key = format!("products/{}", "a".repeat(64));
        let relative = Path::new(".bray").join(&key);
        let mut store = ManagedStore::open(root.path()).expect("fixture store must open");

        let lease = store
            .register_product(&relative, &plan, &|| false)
            .expect("fixture product must register");

        drop(lease);
        std::fs::remove_dir_all(root.path().join(&relative)).expect("fixture backing must be removed");

        let unsafe_path = store.metadata.join(directory);

        if unsafe_path.exists() {
            std::fs::remove_dir(&unsafe_path).expect("empty fixture directory must be removed");
        }

        std::fs::write(&sentinel, b"keep").expect("fixture sentinel must be written");
        std::os::unix::fs::symlink(outside.path(), &unsafe_path).expect("fixture symlink must form");
        drop(store);

        for dry_run in [true, false] {
            let error = crate::clean_storage(root.path(), &StorageSelection::default(), dry_run, &|| false)
                .expect_err("unsafe owned directory chains must be rejected");

            assert_eq!(error.path(), unsafe_path);
            assert_eq!(error.kind(), &crate::StorageErrorKind::UnsafePath);
        }

        assert_eq!(std::fs::read(&sentinel).expect("outside data must remain"), b"keep");
    }
}

#[test]
fn cache_budget_evicts_the_least_recently_used_entry() {
    let root = tempfile::tempdir().expect("temporary fixture directory must be created");

    let older = ManagedCache::thin_lto(root.path(), &target_identity(), "older", &|| false)
        .expect("fixture `ManagedCache::thin_lto` call must succeed");

    let newer = ManagedCache::thin_lto(root.path(), &target_identity(), "newer", &|| false)
        .expect("fixture `ManagedCache::thin_lto` call must succeed");

    let old_path = older.directory().to_owned();
    let new_path = newer.directory().to_owned();

    std::fs::write(old_path.join("cache"), b"old").expect("fixture contents must be written");
    std::fs::write(new_path.join("cache"), b"new").expect("fixture contents must be written");
    drop(older);
    drop(newer);
    wait_for_lease_release(&old_path.join("entry.lock"));
    wait_for_lease_release(&new_path.join("entry.lock"));

    let now = SystemTime::now();
    let mut store = ManagedStore::open(root.path()).expect("fixture file must open");

    for record in store.index.entries.values_mut() {
        record.last_used = if record.owner.context.toolchain.as_deref() == Some("older") {
            now - Duration::from_secs(10)
        } else {
            now
        };
    }

    store
        .maintain_at(
            StoragePolicy {
                cache_max_bytes: 3,
                ..StoragePolicy::default()
            },
            now,
            16,
            &|| false,
        )
        .expect("fixture `maintain_at` call must succeed");

    assert!(!old_path.exists());

    assert_eq!(
        std::fs::read(new_path.join("cache")).expect("fixture contents must be readable"),
        b"new"
    );
}

#[test]
fn storage_report_marks_the_least_recently_used_cache_over_budget() {
    let root = tempfile::tempdir().expect("temporary fixture directory must be created");

    let older = ManagedCache::thin_lto(root.path(), &target_identity(), "older", &|| false)
        .expect("fixture `ManagedCache::thin_lto` call must succeed");

    let newer = ManagedCache::thin_lto(root.path(), &target_identity(), "newer", &|| false)
        .expect("fixture `ManagedCache::thin_lto` call must succeed");

    std::fs::write(older.directory().join("cache"), b"old")
        .expect("fixture contents must be written");

    std::fs::write(newer.directory().join("cache"), b"new")
        .expect("fixture contents must be written");

    let old_lease = older.directory().join("entry.lock");
    let new_lease = newer.directory().join("entry.lock");

    drop(older);
    drop(newer);
    wait_for_lease_release(&old_lease);
    wait_for_lease_release(&new_lease);

    let now = SystemTime::now();
    let mut store = ManagedStore::open(root.path()).expect("fixture file must open");

    for record in store.index.entries.values_mut() {
        record.last_used = if record.owner.context.toolchain.as_deref() == Some("older") {
            now - Duration::from_secs(10)
        } else {
            now
        };
    }

    store.save().expect("fixture `save` call must succeed");
    drop(store);

    let rows = inspect_storage(
        root.path(),
        &StorageSelection {
            kind: Some(crate::StorageKind::Caches),
            ..StorageSelection::default()
        },
        StoragePolicy {
            cache_max_bytes: 3,
            ..StoragePolicy::default()
        },
        &|| false,
    )
    .expect("fixture storage must be inspectable");

    assert_eq!(rows.len(), 2);

    assert_eq!(
        rows.iter()
            .find(|row| row.toolchain.as_deref() == Some("older"))
            .map(|row| row.category),
        Some(StorageCategory::Reclaimable)
    );

    assert_eq!(
        rows.iter()
            .find(|row| row.toolchain.as_deref() == Some("newer"))
            .map(|row| row.category),
        Some(StorageCategory::ReusableCache)
    );
}

#[test]
fn public_path_claims_prevent_cross_product_ownership() {
    let root = tempfile::tempdir().expect("temporary fixture directory must be created");
    let first = managed_library_plan(root.path(), "example.first");
    let second = managed_library_plan(root.path(), "example.second");

    let first_store = Path::new(
        ".bray/products/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    );

    let second_store = Path::new(
        ".bray/products/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    );

    let mut store = ManagedStore::open(root.path()).expect("fixture file must open");

    let first_lease = store
        .register_product(first_store, &first, &|| false)
        .expect("fixture `register_product` call must succeed");

    drop(first_lease);

    let error = store
        .register_product(second_store, &second, &|| false)
        .expect_err("conflicting product ownership must be rejected");

    assert_eq!(error.path(), root.path().join("application.brayi"));

    assert!(matches!(
        error.kind(),
        crate::StorageErrorKind::Io {
            operation: crate::StorageOperation::Create,
            cause: std::io::ErrorKind::AlreadyExists,
        }
    ));

    store
        .reconcile_product_public_paths(
            "products/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            [],
        )
        .expect("fixture `reconcile_product_public_paths` call must succeed");

    let second_lease = store
        .register_product(second_store, &second, &|| false)
        .expect("fixture `register_product` call must succeed");

    drop(second_lease);

    let public = root.path().join("application.brayi");

    std::fs::write(&public, b"second product").expect("fixture contents must be written");

    let first_key = first_store
        .strip_prefix(".bray")
        .expect("fixture `strip_prefix` call must succeed")
        .to_str()
        .expect("fixture path must contain UTF-8")
        .replace('\\', "/");

    store
        .index
        .entries
        .get_mut(&first_key)
        .expect("fixture lookup must contain its requested mutable entry")
        .state = super::record::EntryState::Retiring;

    store.save().expect("fixture `save` call must succeed");

    store
        .finish_retirement(&first_key, &|| false)
        .expect("fixture `finish_retirement` call must succeed");

    assert_eq!(
        std::fs::read(public).expect("fixture contents must be readable"),
        b"second product"
    );
}

#[test]
fn expired_product_cannot_be_pinned_and_renewed_before_maintenance() {
    let root = tempfile::tempdir().expect("temporary fixture directory must be created");
    let plan = managed_library_plan(root.path(), "example.expired");

    let relative_store = Path::new(
        ".bray/products/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    );

    let mut store = ManagedStore::open(root.path()).expect("fixture file must open");

    let lease = store
        .register_product(relative_store, &plan, &|| false)
        .expect("fixture `register_product` call must succeed");

    drop(lease);

    let key = relative_store
        .strip_prefix(".bray")
        .expect("fixture `strip_prefix` call must succeed")
        .to_str()
        .expect("fixture path must contain UTF-8")
        .replace('\\', "/");

    let expired_at = SystemTime::now() - Duration::from_secs(2);

    store
        .index
        .entries
        .get_mut(&key)
        .expect("fixture lookup must contain its requested mutable entry")
        .last_used = expired_at;

    store.save().expect("fixture `save` call must succeed");

    let error = store
        .pin_product(
            relative_store,
            StoragePolicy {
                inactive_product_age: Duration::from_secs(1),
                ..StoragePolicy::default()
            },
        )
        .expect_err("expired product must be unavailable");

    assert_eq!(error.kind(), &crate::StorageErrorKind::Unavailable);
    assert_eq!(store.index.entries[&key].last_used, expired_at);
}

#[test]
fn cache_age_and_budget_never_evict_an_active_linker() {
    let root = tempfile::tempdir().expect("temporary fixture directory must be created");

    let cache = ManagedCache::thin_lto(root.path(), &target_identity(), "active", &|| false)
        .expect("fixture `ManagedCache::thin_lto` call must succeed");

    let path = cache.directory().join("cache");

    std::fs::write(&path, b"active cache").expect("fixture contents must be written");

    let mut store = ManagedStore::open(root.path()).expect("fixture file must open");

    let policy = StoragePolicy {
        cache_max_age: Duration::ZERO,
        cache_max_bytes: 0,
        ..StoragePolicy::default()
    };

    store
        .maintain_at(policy, SystemTime::now(), 16, &|| false)
        .expect("fixture `maintain_at` call must succeed");

    assert_eq!(
        std::fs::read(&path).expect("fixture contents must be readable"),
        b"active cache"
    );

    drop(cache);

    wait_for_lease_release(
        &path
            .parent()
            .expect("fixture cache must have a directory")
            .join("entry.lock"),
    );

    store
        .maintain_at(policy, SystemTime::now(), 16, &|| false)
        .expect("fixture `maintain_at` call must succeed");

    assert!(!path.exists());
    assert!(store.index.entries.is_empty());
}

#[test]
fn repeated_operations_reclaim_completed_intermediates_and_keep_live_work() {
    let root = tempfile::tempdir().expect("temporary fixture directory must be created");

    let active = ManagedOperation::begin(
        root.path(),
        &product_identity(),
        &target_identity(),
        &|| false,
    )
    .expect("fixture operation must begin successfully");

    let active_path = active.directory().join("input");

    std::fs::write(&active_path, b"live input").expect("fixture contents must be written");

    for _ in 0..24 {
        let operation = ManagedOperation::begin(
            root.path(),
            &product_identity(),
            &target_identity(),
            &|| false,
        )
        .expect("fixture operation must begin successfully");

        std::fs::write(operation.directory().join("temporary"), b"completed output")
            .expect("fixture contents must be written");

        let lease = operation.directory().join("entry.lock");

        drop(operation);
        wait_for_lease_release(&lease);
    }

    let next = ManagedOperation::begin(
        root.path(),
        &product_identity(),
        &target_identity(),
        &|| false,
    )
    .expect("fixture operation must begin successfully");

    let rows = inspect_storage(
        root.path(),
        &StorageSelection::default(),
        StoragePolicy::default(),
        &|| false,
    )
    .expect("fixture storage must be inspectable");

    assert_eq!(
        std::fs::read(&active_path).expect("fixture contents must be readable"),
        b"live input"
    );

    assert_eq!(rows.len(), 2);

    assert!(
        rows.iter()
            .all(|row| row.active && row.category == StorageCategory::ActiveWork)
    );

    assert!(next.directory().exists());
}

#[test]
fn a_process_exit_releases_ownership_and_allows_abandoned_work_to_be_reclaimed() {
    let root = tempfile::tempdir().expect("temporary fixture directory must be created");

    let mut child = std::process::Command::new(
        std::env::current_exe().expect("fixture `std::env::current_exe` call must succeed"),
    )
    .args([
        "--exact",
        "storage::tests::abandoned_operation_child",
        "--nocapture",
    ])
    .env("BRAY_TEST_ABANDONED_STORAGE_ROOT", root.path())
    .stdout(std::process::Stdio::null())
    .spawn()
    .expect("fixture worker must start");

    let started = std::time::Instant::now();

    loop {
        if let Some(status) = child.try_wait().expect("fixture wait must succeed") {
            assert!(status.success());
            break;
        }

        if started.elapsed() >= Duration::from_secs(10) {
            child.kill().expect("fixture `kill` call must succeed");
            child.wait().expect("fixture `wait` call must succeed");
            panic!("abandoned-operation subprocess exceeded its deadline");
        }

        std::thread::sleep(Duration::from_millis(10));
    }

    let abandoned = std::path::PathBuf::from(
        std::fs::read_to_string(root.path().join("abandoned-path"))
            .expect("fixture text must be readable"),
    );

    assert!(abandoned.join("partial").exists());

    let rows = inspect_storage(
        root.path(),
        &StorageSelection::default(),
        StoragePolicy::default(),
        &|| false,
    )
    .expect("fixture storage must be inspectable");

    assert!(
        rows.iter()
            .all(|row| !row.active && row.category == StorageCategory::Reclaimable)
    );

    let next = ManagedOperation::begin(
        root.path(),
        &product_identity(),
        &target_identity(),
        &|| false,
    )
    .expect("fixture operation must begin successfully");

    assert!(!abandoned.exists());
    assert!(next.directory().exists());
}

#[test]
fn abandoned_operation_child() {
    let Some(root) = std::env::var_os("BRAY_TEST_ABANDONED_STORAGE_ROOT") else {
        return;
    };

    let root = std::path::PathBuf::from(root);

    let operation =
        ManagedOperation::begin(&root, &product_identity(), &target_identity(), &|| false)
            .expect("fixture operation must begin successfully");

    std::fs::write(
        operation.directory().join("partial"),
        b"interrupted operation",
    )
    .expect("fixture contents must be written");

    std::fs::write(
        root.join("abandoned-path"),
        operation
            .directory()
            .to_str()
            .expect("fixture path must contain UTF-8"),
    )
    .expect("fixture contents must be written");

    // Exit without running destructors, as an abruptly terminated build would.
    std::process::exit(0);
}

#[test]
fn storage_metadata_rejects_invalid_owners_and_non_directory_managed_roots() {
    let root = tempfile::tempdir().expect("temporary fixture directory must be created");

    let cache = ManagedCache::thin_lto(root.path(), &target_identity(), "llvm", &|| false)
        .expect("fixture `ManagedCache::thin_lto` call must succeed");

    drop(cache);

    let index = root.path().join(".bray/storage-index.json");

    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&index).expect("fixture contents must be readable"))
            .expect("fixture data must deserialize");

    let record = value["entries"]
        .as_object_mut()
        .expect("fixture `as_object_mut` call must succeed")
        .values_mut()
        .next()
        .expect("fixture iterator must provide its expected item");

    record["owner"]["context"]["target"] = serde_json::json!("");

    std::fs::write(
        &index,
        serde_json::to_vec(&value).expect("fixture data must serialize"),
    )
    .expect("fixture contents must be written");

    let error = inspect_storage(
        root.path(),
        &StorageSelection::default(),
        StoragePolicy::default(),
        &|| false,
    )
    .expect_err("malformed product context must be rejected");

    assert_eq!(error.path(), index);

    assert!(matches!(
        error.kind(),
        crate::StorageErrorKind::Metadata {
            cause: bray_diagnostics::DiagnosticDocumentParseKind::Schema,
            ..
        }
    ));

    let root = tempfile::tempdir().expect("temporary fixture directory must be created");

    std::fs::write(root.path().join(".bray"), b"unmanaged file")
        .expect("fixture contents must be written");

    let error = crate::clean_storage(root.path(), &StorageSelection::default(), false, &|| false)
        .expect_err("unmanaged storage path must be rejected");

    assert_eq!(error.kind(), &crate::StorageErrorKind::UnsafePath);

    assert_eq!(
        std::fs::read(root.path().join(".bray")).expect("fixture contents must be readable"),
        b"unmanaged file"
    );
}

#[cfg(unix)]
#[test]
fn cleanup_rejects_symlinks_without_touching_their_target() {
    let root = tempfile::tempdir().expect("temporary fixture directory must be created");
    let outside = tempfile::tempdir().expect("temporary fixture directory must be created");
    let file = outside.path().join("user-data");

    std::fs::write(&file, b"keep").expect("fixture contents must be written");

    let cache = ManagedCache::thin_lto(root.path(), &target_identity(), "llvm", &|| false)
        .expect("fixture `ManagedCache::thin_lto` call must succeed");

    std::os::unix::fs::symlink(&file, cache.directory().join("link"))
        .expect("fixture `std::os::unix::fs::symlink` call must succeed");

    let lease = cache.directory().join("entry.lock");

    drop(cache);
    wait_for_lease_release(&lease);

    let error = crate::clean_storage(root.path(), &StorageSelection::default(), false, &|| false)
        .expect_err("symlink in managed storage must be rejected");

    assert_eq!(error.kind(), &crate::StorageErrorKind::UnsafePath);

    assert_eq!(
        std::fs::read(&file).expect("fixture contents must be readable"),
        b"keep"
    );
}

fn managed_library_plan(root: &std::path::Path, package: &str) -> crate::EmissionPlan {
    let package = bray_symbols::PackageIdentity::try_new(package)
        .expect("fixture package identity must be valid");

    let product = bray_symbols::ProductIdentity::try_new(package.clone(), "application")
        .expect("fixture product identity must be valid");

    let request = crate::EmissionRequest::try_new(
        product,
        crate::ProductKind::Library,
        None,
        target_identity(),
        crate::RequestedArtifactDestination::FilesystemDirectory(root.into()),
        [crate::RequestedArtifact::new(
            crate::ArtifactKind::PackageInterface,
            crate::ArtifactRequirement::Required,
        )],
        crate::ReplacementPolicy::RequireAbsent,
    )
    .expect("fixture emission request must be valid");

    let interface =
        bray_package_interface::test_support::interface_artifact_for(package, "application");

    crate::EmissionPlanner::new(
        crate::test_support::target_output_description(),
        None,
        Some(interface),
    )
    .plan(request)
    .expect("fixture `plan` call must succeed")
}
