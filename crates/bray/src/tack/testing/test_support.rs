use std::path::{Path, PathBuf};

use bray_symbols::{PackageIdentity, ProductIdentity};
use bray_test_protocol::{TestCatalog, encode_test_catalog};

pub(super) fn product() -> ProductIdentity {
    let package = PackageIdentity::try_new("example.tests")
        .unwrap_or_else(|| panic!("test package identity must be valid"));

    ProductIdentity::try_new(package, "tests")
        .unwrap_or_else(|| panic!("test product identity must be valid"))
}

pub(super) fn write_empty_test_host(directory: &Path) -> (PathBuf, PathBuf) {
    let executable = directory.join("tests.exe");
    let catalog_path = directory.join("tests.braytests");

    std::fs::write(&executable, b"host-one")
        .unwrap_or_else(|error| panic!("test executable should be written: {error:?}"));

    let catalog = TestCatalog::try_new(product(), [])
        .unwrap_or_else(|error| panic!("empty test catalog should be valid: {error:?}"));

    let (catalog, _) = encode_test_catalog(&catalog)
        .unwrap_or_else(|error| panic!("test catalog should encode: {error:?}"));

    std::fs::write(&catalog_path, catalog)
        .unwrap_or_else(|error| panic!("test catalog should be written: {error:?}"));

    (executable, catalog_path)
}
