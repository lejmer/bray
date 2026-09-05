use std::path::Path;
use std::time::{Duration, SystemTime};

use super::index::ManagedStore;
use crate::test_support::{product_identity, target_identity};
use crate::{
    ManagedCache, ManagedOperation, StorageCategory, StoragePolicy, StorageSelection,
    inspect_storage,
};

#[test]
fn cache_budget_evicts_the_least_recently_used_entry() {
    let root = tempfile::tempdir().unwrap();

    let older =
        ManagedCache::thin_lto(root.path(), &target_identity(), "older", &|| false).unwrap();

    let newer =
        ManagedCache::thin_lto(root.path(), &target_identity(), "newer", &|| false).unwrap();

    let old_path = older.directory().to_owned();
    let new_path = newer.directory().to_owned();

    std::fs::write(old_path.join("cache"), b"old").unwrap();
    std::fs::write(new_path.join("cache"), b"new").unwrap();
    drop(older);
    drop(newer);

    let now = SystemTime::now();
    let mut store = ManagedStore::open(root.path()).unwrap();

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
        .unwrap();

    assert!(!old_path.exists());
    assert_eq!(std::fs::read(new_path.join("cache")).unwrap(), b"new");
}

#[test]
fn storage_report_marks_the_least_recently_used_cache_over_budget() {
    let root = tempfile::tempdir().unwrap();

    let older =
        ManagedCache::thin_lto(root.path(), &target_identity(), "older", &|| false).unwrap();

    let newer =
        ManagedCache::thin_lto(root.path(), &target_identity(), "newer", &|| false).unwrap();

    std::fs::write(older.directory().join("cache"), b"old").unwrap();
    std::fs::write(newer.directory().join("cache"), b"new").unwrap();
    drop(older);
    drop(newer);

    let now = SystemTime::now();
    let mut store = ManagedStore::open(root.path()).unwrap();

    for record in store.index.entries.values_mut() {
        record.last_used = if record.owner.context.toolchain.as_deref() == Some("older") {
            now - Duration::from_secs(10)
        } else {
            now
        };
    }

    store.save().unwrap();
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
    .unwrap();

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
    let root = tempfile::tempdir().unwrap();
    let first = managed_library_plan(root.path(), "example.first");
    let second = managed_library_plan(root.path(), "example.second");

    let first_store = Path::new(
        ".bray/products/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    );

    let second_store = Path::new(
        ".bray/products/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    );

    let mut store = ManagedStore::open(root.path()).unwrap();

    let first_lease = store
        .register_product(first_store, &first, &|| false)
        .unwrap();

    drop(first_lease);

    let error = store
        .register_product(second_store, &second, &|| false)
        .unwrap_err();

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
        .unwrap();

    let second_lease = store
        .register_product(second_store, &second, &|| false)
        .unwrap();

    drop(second_lease);
    let public = root.path().join("application.brayi");

    std::fs::write(&public, b"second product").unwrap();

    let first_key = first_store
        .strip_prefix(".bray")
        .unwrap()
        .to_str()
        .unwrap()
        .replace('\\', "/");

    store.index.entries.get_mut(&first_key).unwrap().state = super::record::EntryState::Retiring;
    store.save().unwrap();
    store.finish_retirement(&first_key, &|| false).unwrap();

    assert_eq!(std::fs::read(public).unwrap(), b"second product");
}

#[test]
fn expired_product_cannot_be_pinned_and_renewed_before_maintenance() {
    let root = tempfile::tempdir().unwrap();
    let plan = managed_library_plan(root.path(), "example.expired");

    let relative_store = Path::new(
        ".bray/products/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    );

    let mut store = ManagedStore::open(root.path()).unwrap();

    let lease = store
        .register_product(relative_store, &plan, &|| false)
        .unwrap();

    drop(lease);

    let key = relative_store
        .strip_prefix(".bray")
        .unwrap()
        .to_str()
        .unwrap()
        .replace('\\', "/");

    let expired_at = SystemTime::now() - Duration::from_secs(2);
    store.index.entries.get_mut(&key).unwrap().last_used = expired_at;
    store.save().unwrap();

    let error = store
        .pin_product(
            relative_store,
            StoragePolicy {
                inactive_product_age: Duration::from_secs(1),
                ..StoragePolicy::default()
            },
        )
        .unwrap_err();

    assert_eq!(error.kind(), &crate::StorageErrorKind::Unavailable);
    assert_eq!(store.index.entries[&key].last_used, expired_at);
}

#[test]
fn cache_age_and_budget_never_evict_an_active_linker() {
    let root = tempfile::tempdir().unwrap();

    let cache =
        ManagedCache::thin_lto(root.path(), &target_identity(), "active", &|| false).unwrap();

    let path = cache.directory().join("cache");

    std::fs::write(&path, b"active cache").unwrap();

    let mut store = ManagedStore::open(root.path()).unwrap();

    let policy = StoragePolicy {
        cache_max_age: Duration::ZERO,
        cache_max_bytes: 0,
        ..StoragePolicy::default()
    };

    store
        .maintain_at(policy, SystemTime::now(), 16, &|| false)
        .unwrap();

    assert_eq!(std::fs::read(&path).unwrap(), b"active cache");
    drop(cache);

    store
        .maintain_at(policy, SystemTime::now(), 16, &|| false)
        .unwrap();

    assert!(!path.exists());
    assert!(store.index.entries.is_empty());
}

#[test]
fn repeated_operations_reclaim_completed_intermediates_and_keep_live_work() {
    let root = tempfile::tempdir().unwrap();

    let active = ManagedOperation::begin(
        root.path(),
        &product_identity(),
        &target_identity(),
        &|| false,
    )
    .unwrap();

    let active_path = active.directory().join("input");

    std::fs::write(&active_path, b"live input").unwrap();

    for _ in 0..24 {
        let operation = ManagedOperation::begin(
            root.path(),
            &product_identity(),
            &target_identity(),
            &|| false,
        )
        .unwrap();

        std::fs::write(operation.directory().join("temporary"), b"completed output").unwrap();
    }

    let next = ManagedOperation::begin(
        root.path(),
        &product_identity(),
        &target_identity(),
        &|| false,
    )
    .unwrap();

    let rows = inspect_storage(
        root.path(),
        &StorageSelection::default(),
        StoragePolicy::default(),
        &|| false,
    )
    .unwrap();

    assert_eq!(std::fs::read(&active_path).unwrap(), b"live input");
    assert_eq!(rows.len(), 2);

    assert!(
        rows.iter()
            .all(|row| row.active && row.category == StorageCategory::ActiveWork)
    );

    assert!(next.directory().exists());
}

#[test]
fn a_process_exit_releases_ownership_and_allows_abandoned_work_to_be_reclaimed() {
    let root = tempfile::tempdir().unwrap();

    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "storage::tests::abandoned_operation_child",
            "--nocapture",
        ])
        .env("BRAY_TEST_ABANDONED_STORAGE_ROOT", root.path())
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();

    let started = std::time::Instant::now();

    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }

        if started.elapsed() >= Duration::from_secs(10) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("abandoned-operation subprocess exceeded its deadline");
        }

        std::thread::sleep(Duration::from_millis(10));
    }

    let abandoned = std::path::PathBuf::from(
        std::fs::read_to_string(root.path().join("abandoned-path")).unwrap(),
    );

    assert!(abandoned.join("partial").exists());

    let rows = inspect_storage(
        root.path(),
        &StorageSelection::default(),
        StoragePolicy::default(),
        &|| false,
    )
    .unwrap();

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
    .unwrap();

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
        ManagedOperation::begin(&root, &product_identity(), &target_identity(), &|| false).unwrap();

    std::fs::write(
        operation.directory().join("partial"),
        b"interrupted operation",
    )
    .unwrap();

    std::fs::write(
        root.join("abandoned-path"),
        operation.directory().to_str().unwrap(),
    )
    .unwrap();

    // Exit without running destructors, as an abruptly terminated build would.
    std::process::exit(0);
}

#[test]
fn storage_metadata_rejects_invalid_owners_and_non_directory_managed_roots() {
    let root = tempfile::tempdir().unwrap();
    let cache = ManagedCache::thin_lto(root.path(), &target_identity(), "llvm", &|| false).unwrap();
    drop(cache);
    let index = root.path().join(".bray/storage-index.json");

    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&index).unwrap()).unwrap();

    let record = value["entries"]
        .as_object_mut()
        .unwrap()
        .values_mut()
        .next()
        .unwrap();

    record["owner"]["context"]["target"] = serde_json::json!("");
    std::fs::write(&index, serde_json::to_vec(&value).unwrap()).unwrap();

    let error = inspect_storage(
        root.path(),
        &StorageSelection::default(),
        StoragePolicy::default(),
        &|| false,
    )
    .unwrap_err();

    assert_eq!(error.path(), index);

    assert!(matches!(
        error.kind(),
        crate::StorageErrorKind::Metadata {
            cause: bray_diagnostics::DiagnosticDocumentParseKind::Schema,
            ..
        }
    ));

    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join(".bray"), b"unmanaged file").unwrap();

    let error = crate::clean_storage(root.path(), &StorageSelection::default(), false, &|| false)
        .unwrap_err();

    assert_eq!(error.kind(), &crate::StorageErrorKind::UnsafePath);

    assert_eq!(
        std::fs::read(root.path().join(".bray")).unwrap(),
        b"unmanaged file"
    );
}

#[cfg(unix)]
#[test]
fn cleanup_rejects_symlinks_without_touching_their_target() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let file = outside.path().join("user-data");
    std::fs::write(&file, b"keep").unwrap();
    let cache = ManagedCache::thin_lto(root.path(), &target_identity(), "llvm", &|| false).unwrap();
    std::os::unix::fs::symlink(&file, cache.directory().join("link")).unwrap();
    drop(cache);

    let error = crate::clean_storage(root.path(), &StorageSelection::default(), false, &|| false)
        .unwrap_err();

    assert_eq!(error.kind(), &crate::StorageErrorKind::UnsafePath);
    assert_eq!(std::fs::read(&file).unwrap(), b"keep");
}

fn managed_library_plan(root: &std::path::Path, package: &str) -> crate::EmissionPlan {
    let package = bray_symbols::PackageIdentity::try_new(package).unwrap();
    let product = bray_symbols::ProductIdentity::try_new(package.clone(), "application").unwrap();

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
    .unwrap();

    let interface =
        bray_package_interface::test_support::interface_artifact_for(package, "application");

    crate::EmissionPlanner::new(
        crate::test_support::target_output_description(),
        None,
        Some(interface),
    )
    .plan(request)
    .unwrap()
}
