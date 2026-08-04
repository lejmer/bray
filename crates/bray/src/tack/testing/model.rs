use std::path::PathBuf;

use bray_test_protocol::{TestCatalog, TestCatalogDigest, TestCatalogEntryId};

pub(crate) struct BuiltTestHost {
    pub(super) executable: PathBuf,
    pub(super) catalog_path: PathBuf,
}

impl BuiltTestHost {
    pub(crate) const fn new(executable: PathBuf, catalog_path: PathBuf) -> Self {
        Self {
            executable,
            catalog_path,
        }
    }
}

pub(super) struct LoadedTestHost {
    pub(super) executable: PathBuf,
    pub(super) catalog: TestCatalog,
    pub(super) digest: TestCatalogDigest,
}

#[derive(Clone)]
pub(super) struct HostLocation {
    pub(super) executable: PathBuf,
    pub(super) entry: TestCatalogEntryId,
}
