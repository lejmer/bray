mod accounting;
mod error;
mod files;
mod index;
mod lease;
mod maintenance;
mod policy;
mod record;
mod report;
mod resource;
mod tree;

#[cfg(test)]
mod tests;

pub(crate) use accounting::StorageAccounting;
pub use error::{StorageError, StorageErrorKind, StorageOperation};
pub(crate) use files::open_lock;
pub(crate) use files::{
    create_managed_path, managed_directory_exists, read_owned_file, require_directory,
    require_file, stage_owned_copy, write_owned_json,
};
pub(crate) use index::ManagedStore;
pub(crate) use lease::StorageLease;
pub use policy::StoragePolicy;
pub use report::{
    StorageCategory, StorageEntryReport, StorageKind, StorageSelection, clean_storage,
    inspect_storage,
};
pub use resource::{ManagedCache, ManagedOperation};
pub(crate) use tree::{check_cancelled, children, remove_owned_tree};

pub(crate) use record::{StorageContext, StorageProduct};
