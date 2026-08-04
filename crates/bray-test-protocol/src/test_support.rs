use bray_symbols::{PackageIdentity, ProductIdentity};

pub(crate) fn product() -> ProductIdentity {
    product_named("tests")
}

pub(crate) fn product_named(name: &str) -> ProductIdentity {
    let package = PackageIdentity::try_new("example.tests")
        .unwrap_or_else(|| panic!("test package identity must be valid"));

    ProductIdentity::try_new(package, name)
        .unwrap_or_else(|| panic!("test product identity must be valid"))
}
