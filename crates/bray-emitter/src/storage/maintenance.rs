use std::collections::{BTreeMap, BTreeSet};
use std::hash::Hash;
use std::path::Path;
use std::time::SystemTime;

use bray_base::{
    Cancellation, StableDigestHasher, atomic_rename_exclusive, lowercase_hex, sync_directory,
};

use super::error::{StorageError, StorageErrorKind, StorageOperation};
use super::files::{create_managed_path, managed_directory_exists};
use super::index::ManagedStore;
use super::lease::StorageLease;
use super::policy::{StoragePolicy, elapsed};
use super::record::{EntryKind, EntryState, StorageIndex};
use super::tree::{check_cancelled, remove_owned_tree, tree_bytes};

impl ManagedStore {
    pub(crate) fn maintain(
        &mut self,
        policy: StoragePolicy,
        cancellation: &dyn Cancellation,
    ) -> Result<(), StorageError> {
        self.maintain_at(policy, SystemTime::now(), 16, cancellation)
    }

    pub(super) fn maintain_at(
        &mut self,
        policy: StoragePolicy,
        now: SystemTime,
        limit: usize,
        cancellation: &dyn Cancellation,
    ) -> Result<(), StorageError> {
        let start = self.index.cursor.as_deref();

        let keys: Vec<_> = self
            .index
            .entries
            .keys()
            .filter(|key| start.is_none_or(|start| key.as_str() > start))
            .chain(
                self.index
                    .entries
                    .keys()
                    .filter(|key| start.is_some_and(|start| key.as_str() <= start)),
            )
            .take(limit)
            .cloned()
            .collect();

        for key in keys {
            check_cancelled(&self.metadata, cancellation)?;
            self.inspect_entry(&key, policy, now, cancellation)?;
            self.index.cursor = Some(key);
        }

        self.evict_caches(policy, limit, cancellation)?;

        self.save()
    }

    fn inspect_entry(
        &mut self,
        key: &str,
        policy: StoragePolicy,
        now: SystemTime,
        cancellation: &dyn Cancellation,
    ) -> Result<(), StorageError> {
        let path = self.metadata.join(key);

        let Some(record) = self.index.entries.get(key) else {
            return Ok(());
        };

        if record.state != EntryState::Live {
            return self.finish_retirement(key, cancellation);
        }

        if !managed_directory_exists(&self.metadata, Path::new(key))? {
            self.index.entries.remove(key);
            self.forget_content_directory(&path)?;

            return Ok(());
        }

        let Some(lease) = StorageLease::try_exclusive(&path.join("entry.lock"))? else {
            return Ok(());
        };

        let age = elapsed(now, record.last_used);

        let expired = match record.kind {
            EntryKind::Product => age >= policy.inactive_product_age,
            EntryKind::Cache => age >= policy.cache_max_age,
            EntryKind::Operation => true,
        };

        if record.kind == EntryKind::Product && !expired {
            crate::publication::generation::maintain_product(
                self,
                &path,
                key,
                false,
                cancellation,
            )?;
        }

        let measured = tree_bytes(&path, cancellation)?;

        if let Some(record) = self.index.entries.get_mut(key) {
            record.bytes = measured;
        }

        if expired {
            if let Some(record) = self.index.entries.get_mut(key) {
                record.state = EntryState::Retiring;
            }

            self.save()?;

            // The root lock prevents any new pin after the durable retirement decision.
            drop(lease);

            return self.finish_retirement(key, cancellation);
        }

        Ok(())
    }

    fn evict_caches(
        &mut self,
        policy: StoragePolicy,
        limit: usize,
        cancellation: &dyn Cancellation,
    ) -> Result<(), StorageError> {
        let mut active = BTreeSet::new();

        for (key, record) in &self.index.entries {
            if record.kind != EntryKind::Cache || record.state != EntryState::Live {
                continue;
            }

            check_cancelled(&self.metadata, cancellation)?;
            let path = self.metadata.join(key);

            if managed_directory_exists(&self.metadata, Path::new(key))?
                && StorageLease::try_exclusive(&path.join("entry.lock"))?.is_none()
            {
                active.insert(key.clone());
            }
        }

        let candidates = cache_budget_reclaimable(
            &self.index,
            policy.cache_max_bytes,
            &BTreeMap::new(),
            &active,
            &self.metadata,
        )?;

        for key in candidates.into_iter().take(limit) {
            check_cancelled(&self.metadata, cancellation)?;

            if !managed_directory_exists(&self.metadata, Path::new(&key))? {
                self.index.entries.remove(&key);
                continue;
            }

            let Some(lease) =
                StorageLease::try_exclusive(&self.metadata.join(&key).join("entry.lock"))?
            else {
                continue;
            };

            if let Some(record) = self.index.entries.get_mut(&key) {
                record.state = EntryState::Retiring;
            }

            self.save()?;
            drop(lease);
            self.finish_retirement(&key, cancellation)?;
        }

        Ok(())
    }

    fn retirement_path(&self, key: &str) -> std::path::PathBuf {
        let mut digest = StableDigestHasher::new();

        key.hash(&mut digest);

        self.metadata
            .join("retired")
            .join(lowercase_hex(&digest.finalize()))
    }

    pub(super) fn entry_path(&self, key: &str) -> Result<std::path::PathBuf, StorageError> {
        if managed_directory_exists(&self.metadata, Path::new(key))? {
            return Ok(self.metadata.join(key));
        }

        let retired = self.retirement_path(key);

        let relative = retired
            .strip_prefix(&self.metadata)
            .map_err(|_| StorageError::new(&retired, StorageErrorKind::UnsafePath))?;

        if !managed_directory_exists(&self.metadata, relative)? {
            return Err(StorageError::new(&retired, StorageErrorKind::Unavailable));
        }

        Ok(retired)
    }

    pub(super) fn finish_retirement(
        &mut self,
        key: &str,
        cancellation: &dyn Cancellation,
    ) -> Result<(), StorageError> {
        let source = self.metadata.join(key);
        let retired = create_managed_path(&self.metadata, Path::new("retired"))?;
        let destination = self.retirement_path(key);

        self.forget_content_directory(&source)?;

        if let Some(record) = self.index.entries.get(key)
            && record.state == EntryState::Retiring
            && record.kind == EntryKind::Product
        {
            let store = self.entry_path(key)?;

            crate::publication::generation::maintain_product(
                self,
                &store,
                key,
                true,
                cancellation,
            )?;
        }

        if managed_directory_exists(&self.metadata, Path::new(key))? {
            atomic_rename_exclusive(&source, &destination)
                .map_err(|error| StorageError::io(&source, StorageOperation::Rename, error))?;

            sync_directory(&retired)
                .map_err(|error| StorageError::io(&retired, StorageOperation::Flush, error))?;
        }

        if let Some(record) = self.index.entries.get(key)
            && record.state == EntryState::Retiring
        {
            if let Some(record) = self.index.entries.get_mut(key) {
                record.state = EntryState::RemovingFiles;
            }

            self.save()?;
        }

        remove_owned_tree(&destination, cancellation)?;
        self.index.entries.remove(key);

        self.index
            .public_paths
            .retain(|_, claim| claim.owner != key);

        self.save()
    }
}

pub(super) fn cache_budget_reclaimable(
    index: &StorageIndex,
    cache_max_bytes: u64,
    measured: &BTreeMap<String, u64>,
    active: &BTreeSet<String>,
    path: &Path,
) -> Result<Vec<String>, StorageError> {
    let effective_bytes = |key: &str, bytes| measured.get(key).copied().unwrap_or(bytes);

    let mut candidates: Vec<_> = index
        .entries
        .iter()
        .filter(|(key, record)| {
            record.kind == EntryKind::Cache
                && record.state == EntryState::Live
                && !active.contains(*key)
        })
        .map(|(key, record)| {
            (
                record.last_used,
                key.to_owned(),
                effective_bytes(key, record.bytes),
            )
        })
        .collect();

    let mut bytes = index
        .entries
        .iter()
        .filter(|(_, record)| record.kind == EntryKind::Cache)
        .try_fold(0_u64, |sum, (key, record)| {
            sum.checked_add(effective_bytes(key, record.bytes))
        })
        .ok_or_else(|| {
            StorageError::io(
                path,
                StorageOperation::Inspect,
                std::io::ErrorKind::FileTooLarge.into(),
            )
        })?;

    candidates.sort_unstable();
    let mut reclaimable = Vec::new();

    for (_, key, size) in candidates {
        if bytes <= cache_max_bytes {
            break;
        }

        reclaimable.push(key);
        bytes = bytes.saturating_sub(size);
    }

    Ok(reclaimable)
}
