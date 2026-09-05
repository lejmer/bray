use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use bray_base::Cancellation;
use bray_symbols::ProductIdentity;
use bray_target::TargetIdentity;

use super::accounting::StorageAccounting;
use super::error::{StorageError, StorageErrorKind};
use super::files::managed_directory_exists;
use super::index::ManagedStore;
use super::lease::StorageLease;
use super::maintenance::cache_budget_reclaimable;
use super::policy::{StoragePolicy, elapsed};
use super::record::{EntryKind, EntryState, StorageContext, StorageOwner, StorageProduct};
use super::tree::{check_cancelled, tree_bytes};

/// Coherent ownership group selected by a storage command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageKind {
    /// Published outputs and their retained generations.
    Products,
    /// Optional compiler and toolchain caches.
    Caches,
    /// Completed or abandoned operation staging.
    Intermediates,
}

/// Lifetime category used in storage accounting.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageCategory {
    /// Stable public product files.
    CurrentOutputs,
    /// Immutable current product files available for retained runs.
    RetainedRerun,
    /// The preceding distinct generation of a retained product.
    RetainedHistory,
    /// Files pinned by a live reader or operation.
    ActiveWork,
    /// Optional cache files within their retention policy.
    ReusableCache,
    /// Unpinned files eligible for automatic reclamation.
    Reclaimable,
}

/// Optional identity filters for storage inspection and cleanup.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StorageSelection {
    /// Restrict the operation to this package and product.
    pub product: Option<ProductIdentity>,
    /// Restrict the operation to this target.
    pub target: Option<TargetIdentity>,
    /// Restrict the operation to this build profile.
    pub profile: Option<String>,
    /// Restrict the operation to this toolchain revision.
    pub toolchain: Option<String>,
    /// Restrict the operation to one ownership group.
    pub kind: Option<StorageKind>,
}

/// One storage-accounting row with its owner, lifetime, and cleanup outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageEntryReport {
    /// Lifetime of the reported files.
    pub category: StorageCategory,
    /// Product owning these files, when applicable.
    pub product: Option<ProductIdentity>,
    /// Target for which these files were produced, when applicable.
    pub target: Option<TargetIdentity>,
    /// Build profile recorded by the producer.
    pub profile: Option<String>,
    /// Toolchain revision recorded by the producer.
    pub toolchain: Option<String>,
    /// Sum of file lengths, or absence while an active writer can change them.
    pub bytes: Option<u64>,
    /// Bytes already counted through another hard link earlier in this report.
    /// Subtract these from `bytes` before summing unique file content across rows.
    pub shared_bytes: Option<u64>,
    /// Whether active ownership prevented cleanup or a stable byte count.
    pub active: bool,
    /// Whether a clean operation removed this row's files.
    pub removed: bool,
    /// Directory owning the reported state.
    pub path: PathBuf,
}

impl StorageEntryReport {
    pub(crate) fn new(
        path: &Path,
        product: Option<&StorageProduct>,
        context: &StorageContext,
        category: StorageCategory,
        bytes: Option<u64>,
    ) -> Result<Self, StorageError> {
        let invalid = || {
            StorageError::new(
                path,
                StorageErrorKind::Metadata {
                    cause: bray_diagnostics::DiagnosticDocumentParseKind::Schema,
                    line: None,
                    column: None,
                },
            )
        };

        let product = product
            .map(|product| product.identity().ok_or_else(invalid))
            .transpose()?;

        let target = context
            .target
            .as_ref()
            .map(|target| TargetIdentity::try_new(target.as_str()).ok_or_else(invalid))
            .transpose()?;

        Ok(Self {
            category,
            product,
            target,
            profile: context.profile.clone(),
            toolchain: context.toolchain.clone(),
            bytes,
            shared_bytes: bytes.map(|_| 0),
            active: category == StorageCategory::ActiveWork,
            removed: false,
            path: path.to_owned(),
        })
    }

    pub(crate) fn measure(
        path: &Path,
        product: Option<&StorageProduct>,
        context: &StorageContext,
        category: StorageCategory,
        accounting: &mut StorageAccounting,
        cancellation: &dyn Cancellation,
    ) -> Result<Self, StorageError> {
        let (bytes, shared) = accounting.measure(path, cancellation)?;

        let mut row = Self::new(path, product, context, category, Some(bytes))?;

        row.shared_bytes = Some(shared);

        Ok(row)
    }
}

/// Reports managed storage in stable entry and category order without reclaiming artifacts.
pub fn inspect_storage(
    root: &Path,
    selection: &StorageSelection,
    policy: StoragePolicy,
    cancellation: &dyn Cancellation,
) -> Result<Vec<StorageEntryReport>, StorageError> {
    storage_command(root, selection, policy, None, cancellation)
}

/// Cleans selected managed ownership groups, retaining every actively pinned entry.
///
/// A dry run returns the same selected rows without changing artifacts. Product cleanup includes
/// both public outputs and retained generations, so no publication reference is left dangling.
pub fn clean_storage(
    root: &Path,
    selection: &StorageSelection,
    dry_run: bool,
    cancellation: &dyn Cancellation,
) -> Result<Vec<StorageEntryReport>, StorageError> {
    storage_command(
        root,
        selection,
        StoragePolicy::default(),
        Some(dry_run),
        cancellation,
    )
}

fn storage_command(
    root: &Path,
    selection: &StorageSelection,
    policy: StoragePolicy,
    clean: Option<bool>,
    cancellation: &dyn Cancellation,
) -> Result<Vec<StorageEntryReport>, StorageError> {
    if !managed_directory_exists(root, Path::new(".bray"))? {
        return Ok(Vec::new());
    }

    let mut store = ManagedStore::open(root)?;
    let keys: Vec<_> = store.index.entries.keys().cloned().collect();
    let budget_reclaimable = cache_budget_rows(&store, policy, cancellation)?;
    let mut rows = Vec::new();
    let mut accounting = StorageAccounting::default();

    for key in keys {
        check_cancelled(root, cancellation)?;

        let Some(record) = store.index.entries.get(&key) else {
            continue;
        };

        let live_product = record.kind == EntryKind::Product && record.state == EntryState::Live;

        let selected_product = live_product
            && selection.kind != Some(StorageKind::Caches)
            && selection.matches_product(record.owner.product.as_ref());

        if !selected_product && !selection.matches_entry(record.kind, &record.owner) {
            continue;
        }

        let path = store.entry_path(&key)?;
        let exists = managed_directory_exists(&store.metadata, Path::new(&key))?;

        let lease = if record.state == EntryState::Live && exists {
            StorageLease::try_exclusive(&path.join("entry.lock"))?
        } else {
            None
        };

        let active = lease.is_none() && record.state == EntryState::Live && exists;

        if active && !selection.matches_context(&record.owner.context) {
            continue;
        }

        let retire = if live_product && exists && !active {
            crate::publication::generation::selected_current_product(&path, selection)?
                .unwrap_or_else(|| selection.matches_entry(record.kind, &record.owner))
        } else {
            !live_product || selection.matches_entry(record.kind, &record.owner)
        };

        let first = rows.len();

        if active {
            rows.push(StorageEntryReport::new(
                &path,
                record.owner.product.as_ref(),
                &record.owner.context,
                StorageCategory::ActiveWork,
                None,
            )?);
        } else if live_product && exists {
            // Selecting current outputs cleans the whole publication group, including its history.
            let complete = StorageSelection::default();

            let row_selection = if clean.is_some() && retire {
                &complete
            } else {
                selection
            };

            rows.extend(crate::publication::generation::product_storage_rows(
                root,
                &path,
                record.owner.product.as_ref(),
                &record.owner.context,
                row_selection,
                &mut accounting,
                cancellation,
            )?);

            if elapsed(SystemTime::now(), record.last_used) >= policy.inactive_product_age {
                for row in &mut rows[first..] {
                    row.category = StorageCategory::Reclaimable;
                }
            }
        } else {
            let expired = elapsed(SystemTime::now(), record.last_used) >= policy.cache_max_age;

            let category = if record.state != EntryState::Live
                || record.kind != EntryKind::Cache
                || expired
                || budget_reclaimable.contains(&key)
            {
                StorageCategory::Reclaimable
            } else {
                StorageCategory::ReusableCache
            };

            rows.push(StorageEntryReport::measure(
                &path,
                record.owner.product.as_ref(),
                &record.owner.context,
                category,
                &mut accounting,
                cancellation,
            )?);
        }

        if clean == Some(false) && !active && !retire {
            crate::publication::generation::clean_product_rows(
                &mut store,
                &path,
                &mut rows[first..],
                cancellation,
            )?;
        } else if clean == Some(false) && !active {
            if let Some(record) = store.index.entries.get_mut(&key)
                && record.state == EntryState::Live
            {
                record.state = EntryState::Retiring;
            }

            store.save()?;
            drop(lease);
            store.finish_retirement(&key, cancellation)?;

            for row in &mut rows[first..] {
                row.removed = true;
            }
        }
    }

    Ok(rows)
}

fn cache_budget_rows(
    store: &ManagedStore,
    policy: StoragePolicy,
    cancellation: &dyn Cancellation,
) -> Result<BTreeSet<String>, StorageError> {
    let mut measured = BTreeMap::new();
    let mut active = BTreeSet::new();

    for (key, record) in &store.index.entries {
        if record.kind != EntryKind::Cache || record.state != EntryState::Live {
            continue;
        }

        check_cancelled(&store.metadata, cancellation)?;
        let path = store.metadata.join(key);

        if !managed_directory_exists(&store.metadata, Path::new(key))? {
            measured.insert(key.clone(), 0);
        } else if StorageLease::try_exclusive(&path.join("entry.lock"))?.is_some() {
            measured.insert(key.clone(), tree_bytes(&path, cancellation)?);
        } else {
            active.insert(key.clone());
        }
    }

    cache_budget_reclaimable(
        &store.index,
        policy.cache_max_bytes,
        &measured,
        &active,
        &store.metadata,
    )
    .map(|keys| keys.into_iter().collect())
}

impl StorageSelection {
    pub(crate) fn matches_product(&self, product: Option<&StorageProduct>) -> bool {
        self.product.as_ref().is_none_or(|selected| {
            product.is_some_and(|product| {
                product.package == selected.package().as_str() && product.name == selected.name()
            })
        })
    }

    pub(crate) fn matches_context(&self, context: &StorageContext) -> bool {
        self.target
            .as_ref()
            .is_none_or(|selected| context.target.as_deref() == Some(selected.as_str()))
            && self
                .profile
                .as_ref()
                .is_none_or(|selected| context.profile.as_ref() == Some(selected))
            && self
                .toolchain
                .as_ref()
                .is_none_or(|selected| context.toolchain.as_ref() == Some(selected))
    }

    fn matches_entry(&self, kind: EntryKind, owner: &StorageOwner) -> bool {
        let group = match kind {
            EntryKind::Product => StorageKind::Products,
            EntryKind::Cache => StorageKind::Caches,
            EntryKind::Operation => StorageKind::Intermediates,
        };

        self.kind.is_none_or(|selected| selected == group)
            && self.matches_product(owner.product.as_ref())
            && self.matches_context(&owner.context)
    }
}
