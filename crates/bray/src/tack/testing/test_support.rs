use bray_symbols::{PackageIdentity, ProductIdentity};

pub(super) fn product() -> ProductIdentity {
    let package = PackageIdentity::try_new("example.tests")
        .unwrap_or_else(|| panic!("test package identity must be valid"));

    ProductIdentity::try_new(package, "tests")
        .unwrap_or_else(|| panic!("test product identity must be valid"))
}
