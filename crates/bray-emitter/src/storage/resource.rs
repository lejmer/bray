use std::hash::Hash;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use bray_base::{Cancellation, StableDigestHasher, lowercase_hex};
use bray_symbols::ProductIdentity;
use bray_target::TargetIdentity;

use super::error::{StorageError, StorageOperation};
use super::index::ManagedStore;
use super::lease::StorageLease;
use super::policy::StoragePolicy;
use super::record::{EntryKind, StorageContext, StorageOwner, StorageProduct};

/// OS-owned staging for one product operation. Files remain protected until this value drops.
#[derive(Debug)]
pub struct ManagedOperation {
    directory: PathBuf,
    _lease: StorageLease,
}

impl ManagedOperation {
    /// Registers an operation before exposing its private directory.
    ///
    /// Subsequent storage operations reclaim the directory after ownership is released or its
    /// process exits. Product and target filters include these intermediates in storage reports.
    pub fn begin(
        root: &Path,
        product: &ProductIdentity,
        target: &TargetIdentity,
        cancellation: &dyn Cancellation,
    ) -> Result<Self, StorageError> {
        let owner = StorageOwner {
            product: Some(StorageProduct::new(product)),
            context: StorageContext {
                target: Some(target.as_str().to_owned()),
                ..StorageContext::default()
            },
        };

        Self::begin_owned(root, owner, cancellation)
    }

    pub(crate) fn for_plan(
        root: &Path,
        plan: &crate::EmissionPlan,
        cancellation: &dyn Cancellation,
    ) -> Result<Self, StorageError> {
        Self::begin_owned(root, StorageOwner::product(plan), cancellation)
    }

    fn begin_owned(
        root: &Path,
        owner: StorageOwner,
        cancellation: &dyn Cancellation,
    ) -> Result<Self, StorageError> {
        let mut store = prepare_store(root, cancellation)?;
        let ordinal = store.index.next_operation;

        store.index.next_operation = ordinal.checked_add(1).ok_or_else(|| {
            StorageError::io(
                root,
                StorageOperation::Create,
                std::io::ErrorKind::QuotaExceeded.into(),
            )
        })?;

        let key = format!("operations/{}", key_digest(&("operation", ordinal)));

        let lease = store.register(
            &key,
            EntryKind::Operation,
            owner,
            SystemTime::now(),
            cancellation,
        )?;

        Ok(Self {
            directory: store.metadata.join(key),
            _lease: lease,
        })
    }

    /// Returns the private directory while this operation owns it.
    pub fn directory(&self) -> &Path {
        &self.directory
    }
}

/// A persistent ThinLTO cache protected against eviction for the duration of native linking.
#[derive(Debug)]
pub struct ManagedCache {
    directory: PathBuf,
    _lease: StorageLease,
}

impl ManagedCache {
    /// Acquires the cache for the exact target and LLVM toolchain revision.
    pub fn thin_lto(
        root: &Path,
        target: &TargetIdentity,
        toolchain: &str,
        cancellation: &dyn Cancellation,
    ) -> Result<Self, StorageError> {
        let mut store = prepare_store(root, cancellation)?;

        let key = format!(
            "cache/{}",
            key_digest(&("thin_lto", target.as_str(), toolchain))
        );

        let owner = StorageOwner {
            product: None,
            context: StorageContext {
                target: Some(target.as_str().to_owned()),
                profile: None,
                toolchain: Some(toolchain.to_owned()),
            },
        };

        let lease = store.register(
            &key,
            EntryKind::Cache,
            owner,
            SystemTime::now(),
            cancellation,
        )?;

        Ok(Self {
            directory: store.metadata.join(key),
            _lease: lease,
        })
    }

    /// Returns the cache directory while eviction is prevented by this handle.
    pub fn directory(&self) -> &Path {
        &self.directory
    }
}

fn prepare_store(
    root: &Path,
    cancellation: &dyn Cancellation,
) -> Result<ManagedStore, StorageError> {
    super::tree::check_cancelled(root, cancellation)?;

    std::fs::create_dir_all(root)
        .map_err(|error| StorageError::io(root, StorageOperation::Create, error))?;

    let mut store = ManagedStore::open(root)?;

    store.maintain(StoragePolicy::default(), cancellation)?;

    Ok(store)
}

fn key_digest(value: &impl Hash) -> String {
    let mut hasher = StableDigestHasher::new();

    value.hash(&mut hasher);

    lowercase_hex(&hasher.finalize())
}
