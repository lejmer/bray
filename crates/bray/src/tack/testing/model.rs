use std::fs;
use std::hash::Hasher;
use std::path::{Path, PathBuf};

use bray_base::StableDigestHasher;
use bray_test_protocol::{TestCatalog, TestCatalogDigest, TestCatalogEntryId, decode_test_catalog};

#[derive(Clone, Copy)]
pub(super) struct TestHostPublicationIdentity {
    pub(super) catalog: TestCatalogDigest,
    executable: [u8; 32],
}

pub(crate) struct BuiltTestHost {
    pub(super) executable: PathBuf,
    pub(super) catalog_path: PathBuf,
    pub(super) publication: TestHostPublicationIdentity,
}

impl BuiltTestHost {
    pub(crate) fn try_new(executable: PathBuf, catalog_path: PathBuf) -> Option<Self> {
        let publication = publication_identity(&executable, &catalog_path)?;

        Some(Self {
            executable,
            catalog_path,
            publication,
        })
    }

    pub(super) fn load(self) -> Option<LoadedTestHost> {
        let executable = fs::read(&self.executable).ok()?;
        let catalog = fs::read(&self.catalog_path).ok()?;

        let (catalog, digest) = decode_test_catalog(&catalog).ok()?;

        let mut executable_digest = StableDigestHasher::new();

        executable_digest.write(&executable);

        if digest != self.publication.catalog
            || executable_digest.finalize() != self.publication.executable
        {
            return None;
        }

        Some(LoadedTestHost {
            executable: self.executable,
            catalog,
            digest,
        })
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
    pub(super) catalog_digest: TestCatalogDigest,
}

pub(super) fn publication_identity(
    executable: &Path,
    catalog_path: &Path,
) -> Option<TestHostPublicationIdentity> {
    let executable = fs::read(executable).ok()?;
    let catalog = fs::read(catalog_path).ok()?;

    let (_, catalog) = decode_test_catalog(&catalog).ok()?;

    let mut executable_digest = StableDigestHasher::new();

    executable_digest.write(&executable);

    Some(TestHostPublicationIdentity {
        catalog,
        executable: executable_digest.finalize(),
    })
}

#[cfg(test)]
mod tests {
    use super::super::test_support::write_empty_test_host;

    #[test]
    fn publication_identity_rejects_replaced_host_artifacts() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("test directory should be created: {error:?}"));

        let (executable, catalog_path) = write_empty_test_host(directory.path());

        let host = super::BuiltTestHost::try_new(executable.clone(), catalog_path.clone())
            .unwrap_or_else(|| panic!("test host publication should be valid"));

        std::fs::write(&executable, b"host-two")
            .unwrap_or_else(|error| panic!("replacement executable should be written: {error:?}"));

        assert!(host.load().is_none());
    }
}
