use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use bray_base::{Cancellation, is_lowercase_hex};

use super::error::{StorageError, StorageErrorKind, StorageOperation};
use super::files::{
    create_managed_path, managed_directory_exists, open_lock, read_owned_file, write_owned_json,
};
use super::lease::StorageLease;
use super::policy::{StoragePolicy, elapsed};
use super::record::{
    EntryKind, EntryState, INDEX_REVISION, PublicPathClaim, StorageIndex, StorageOwner,
    StorageRecord,
};
use crate::{ManagedArtifactPath, OutputSink, PlannedArtifactDestination};

pub(crate) struct ManagedStore {
    pub(super) root: PathBuf,
    pub(super) metadata: PathBuf,
    pub(super) index: StorageIndex,
    _lock: File,
}

impl ManagedStore {
    pub(crate) fn open(root: &Path) -> Result<Self, StorageError> {
        let metadata = create_managed_path(root, Path::new(".bray"))?;
        let lock_path = metadata.join("storage.lock");
        let lock = open_lock(&lock_path)?;

        lock.lock()
            .map_err(|error| StorageError::io(&lock_path, StorageOperation::Lock, error))?;

        let path = metadata.join("storage-index.json");

        let index = match read_owned_file(&path) {
            Ok(bytes) => serde_json::from_slice::<StorageIndex>(&bytes)
                .map_err(|error| StorageError::json(&path, error))?,
            Err(error)
                if matches!(
                    error.kind(),
                    StorageErrorKind::Io {
                        cause: std::io::ErrorKind::NotFound,
                        ..
                    }
                ) =>
            {
                StorageIndex::default()
            }
            Err(error) => return Err(error),
        };

        if index.revision != INDEX_REVISION {
            return Err(StorageError::new(
                &path,
                StorageErrorKind::Revision {
                    expected: INDEX_REVISION,
                    actual: index.revision,
                },
            ));
        }

        for (key, record) in &index.entries {
            validate_entry_key(&metadata, key, record.kind)?;

            record
                .owner
                .context
                .validate(&path, record.owner.product.as_ref())?;

            if record.kind == EntryKind::Product && record.owner.product.is_none() {
                return Err(StorageError::new(
                    &path,
                    StorageErrorKind::Metadata {
                        cause: bray_diagnostics::DiagnosticDocumentParseKind::Schema,
                        line: None,
                        column: None,
                    },
                ));
            }
        }

        for (key, paths) in &index.content {
            if key.len() != 64 || !is_lowercase_hex(key) {
                return Err(StorageError::new(
                    &path,
                    StorageErrorKind::Metadata {
                        cause: bray_diagnostics::DiagnosticDocumentParseKind::Schema,
                        line: None,
                        column: None,
                    },
                ));
            }

            for relative in paths {
                validate_content_path(root, relative)?;
            }
        }

        for (claim_key, claim) in &index.public_paths {
            let Some(relative) = ManagedArtifactPath::try_new(claim.path.as_str()) else {
                return Err(StorageError::new(&path, StorageErrorKind::UnsafePath));
            };

            let valid_owner = index
                .entries
                .get(&claim.owner)
                .is_some_and(|record| record.kind == EntryKind::Product);

            if !valid_owner || public_path_key(&relative) != *claim_key {
                return Err(StorageError::new(
                    &path,
                    StorageErrorKind::Metadata {
                        cause: bray_diagnostics::DiagnosticDocumentParseKind::Schema,
                        line: None,
                        column: None,
                    },
                ));
            }
        }

        Ok(Self {
            root: root.to_owned(),
            metadata,
            index,
            _lock: lock,
        })
    }

    pub(crate) fn save(&self) -> Result<(), StorageError> {
        let path = self.metadata.join("storage-index.json");

        write_owned_json(&path, &self.index)
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn content_candidates(&self, key: &str) -> impl Iterator<Item = &str> {
        self.index
            .content
            .get(key)
            .into_iter()
            .flatten()
            .map(String::as_str)
    }

    pub(crate) fn record_content(&mut self, key: String, path: &Path) -> Result<(), StorageError> {
        let relative = path
            .strip_prefix(&self.root)
            .ok()
            .and_then(Path::to_str)
            .ok_or_else(|| StorageError::new(path, StorageErrorKind::UnsafePath))?
            .replace('\\', "/");

        self.index.content.entry(key).or_default().insert(relative);

        Ok(())
    }

    pub(crate) fn forget_content_directory(
        &mut self,
        directory: &Path,
    ) -> Result<(), StorageError> {
        let relative = directory
            .strip_prefix(&self.root)
            .map_err(|_| StorageError::new(directory, StorageErrorKind::UnsafePath))?;

        self.index.content.retain(|_, paths| {
            paths.retain(|path| !Path::new(path).starts_with(relative));

            !paths.is_empty()
        });

        self.save()
    }

    pub(super) fn register(
        &mut self,
        key: &str,
        kind: EntryKind,
        owner: StorageOwner,
        now: SystemTime,
        cancellation: &dyn Cancellation,
    ) -> Result<StorageLease, StorageError> {
        validate_entry_key(&self.metadata, key, kind)?;
        let path = self.metadata.join(key);

        if self
            .index
            .entries
            .get(key)
            .is_some_and(|record| record.state != EntryState::Live)
        {
            self.finish_retirement(key, cancellation)?;
        }

        let bytes = self.index.entries.get(key).map_or(0, |record| record.bytes);

        self.index.entries.insert(
            key.to_owned(),
            StorageRecord {
                kind,
                owner,
                last_used: now,
                bytes,
                state: EntryState::Live,
            },
        );

        // Record ownership before creating files so interrupted initialization can be recovered.
        self.save()?;
        create_managed_path(&self.metadata, Path::new(key))?;

        StorageLease::acquire(&path.join("entry.lock"))
    }

    pub(crate) fn register_product(
        &mut self,
        relative_store: &Path,
        plan: &crate::EmissionPlan,
        cancellation: &dyn Cancellation,
    ) -> Result<StorageLease, StorageError> {
        let key = self.product_key(relative_store)?;
        let public_paths = managed_public_paths(&self.root, plan)?;

        for (claim_key, relative) in &public_paths {
            if let Some(claim) = self.index.public_paths.get(claim_key)
                && claim.owner != key
            {
                return Err(StorageError::io(
                    &self.root.join(relative.to_path_buf()),
                    StorageOperation::Create,
                    std::io::ErrorKind::AlreadyExists.into(),
                ));
            }
        }

        let lease = self.register(
            &key,
            EntryKind::Product,
            StorageOwner::product(plan),
            SystemTime::now(),
            cancellation,
        )?;

        for (claim_key, relative) in public_paths {
            self.index.public_paths.insert(
                claim_key,
                PublicPathClaim {
                    owner: key.clone(),
                    path: relative.as_str().to_owned(),
                },
            );
        }

        self.save()?;

        Ok(lease)
    }

    pub(crate) fn reconcile_product_public_paths(
        &mut self,
        key: &str,
        paths: impl IntoIterator<Item = ManagedArtifactPath>,
    ) -> Result<(), StorageError> {
        validate_entry_key(&self.metadata, key, EntryKind::Product)?;

        let desired: BTreeMap<_, _> = paths
            .into_iter()
            .map(|path| (public_path_key(&path), path))
            .collect();

        for (claim_key, relative) in &desired {
            if let Some(claim) = self.index.public_paths.get(claim_key)
                && claim.owner != key
            {
                return Err(StorageError::io(
                    &self.root.join(relative.to_path_buf()),
                    StorageOperation::Create,
                    std::io::ErrorKind::AlreadyExists.into(),
                ));
            }
        }

        self.index
            .public_paths
            .retain(|_, claim| claim.owner != key);

        for (claim_key, relative) in desired {
            self.index.public_paths.insert(
                claim_key,
                PublicPathClaim {
                    owner: key.to_owned(),
                    path: relative.as_str().to_owned(),
                },
            );
        }

        self.save()
    }

    pub(crate) fn product_public_paths(&self, key: &str) -> Result<Vec<PathBuf>, StorageError> {
        validate_entry_key(&self.metadata, key, EntryKind::Product)?;

        self.index
            .public_paths
            .values()
            .filter(|claim| claim.owner == key)
            .map(|claim| {
                ManagedArtifactPath::try_new(claim.path.as_str())
                    .map(|path| path.to_path_buf())
                    .ok_or_else(|| StorageError::new(&self.metadata, StorageErrorKind::UnsafePath))
            })
            .collect()
    }

    fn product_key(&self, relative_store: &Path) -> Result<String, StorageError> {
        relative_store
            .strip_prefix(".bray")
            .ok()
            .and_then(|path| path.to_str())
            .map(|path| path.replace('\\', "/"))
            .ok_or_else(|| {
                StorageError::new(
                    &self.root.join(relative_store),
                    StorageErrorKind::UnsafePath,
                )
            })
    }

    pub(crate) fn pin_product(
        &mut self,
        relative_store: &Path,
        policy: StoragePolicy,
    ) -> Result<StorageLease, StorageError> {
        let key = self.product_key(relative_store)?;
        let path = self.metadata.join(&key);
        let now = SystemTime::now();

        let record = self
            .index
            .entries
            .get(&key)
            .filter(|record| record.state == EntryState::Live && record.kind == EntryKind::Product)
            .ok_or_else(|| StorageError::new(&path, StorageErrorKind::Unavailable))?;

        validate_entry_key(&self.metadata, &key, record.kind)?;

        if elapsed(now, record.last_used) >= policy.inactive_product_age {
            return Err(StorageError::new(&path, StorageErrorKind::Unavailable));
        }

        if !managed_directory_exists(&self.metadata, Path::new(&key))? {
            return Err(StorageError::new(&path, StorageErrorKind::Unavailable));
        }

        let lease = StorageLease::acquire(&path.join("entry.lock"))?;

        if let Some(record) = self.index.entries.get_mut(&key) {
            record.last_used = now;
        }

        self.save()?;

        Ok(lease)
    }
}

fn managed_public_paths(
    root: &Path,
    plan: &crate::EmissionPlan,
) -> Result<BTreeMap<String, ManagedArtifactPath>, StorageError> {
    plan.published_artifacts()
        .filter_map(|artifact| match artifact.destination() {
            PlannedArtifactDestination::Publish(OutputSink::ManagedFilesystem {
                root: artifact_root,
                published,
                ..
            }) if artifact_root == root => Some(published),
            _ => None,
        })
        .map(|published| {
            let relative = published
                .strip_prefix(root)
                .ok()
                .and_then(Path::to_str)
                .map(|path| path.replace('\\', "/"))
                .and_then(ManagedArtifactPath::try_new)
                .ok_or_else(|| StorageError::new(published, StorageErrorKind::UnsafePath))?;

            Ok((public_path_key(&relative), relative))
        })
        .collect()
}

#[cfg(windows)]
fn public_path_key(path: &ManagedArtifactPath) -> String {
    path.as_str()
        .split('/')
        .map(|component| component.trim_end_matches(['.', ' ']).to_lowercase())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(not(windows))]
fn public_path_key(path: &ManagedArtifactPath) -> String {
    path.as_str().to_owned()
}

fn validate_entry_key(metadata: &Path, key: &str, kind: EntryKind) -> Result<(), StorageError> {
    let valid = key.split_once('/').is_some_and(|(directory, identity)| {
        directory == kind.directory() && identity.len() == 64 && is_lowercase_hex(identity)
    });

    if !valid {
        return Err(StorageError::new(
            &metadata.join(key),
            StorageErrorKind::UnsafePath,
        ));
    }

    Ok(())
}

fn validate_content_path(root: &Path, relative: &str) -> Result<(), StorageError> {
    let parts: Vec<_> = relative.split('/').collect();

    let valid = parts.len() >= 5
        && parts[0] == ".bray"
        && parts[1] == "products"
        && parts[2].len() == 64
        && is_lowercase_hex(parts[2])
        && parts[3].len() == 16
        && is_lowercase_hex(parts[3])
        && crate::ManagedArtifactPath::try_new(relative).is_some();

    if !valid {
        return Err(StorageError::new(
            &root.join(relative),
            StorageErrorKind::UnsafePath,
        ));
    }

    Ok(())
}
