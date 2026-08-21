use std::path::{Path, PathBuf};

use bray_symbols::ProductIdentity;

pub(super) const METADATA_DIRECTORY: &str = ".bray";
pub(super) const STAGING_DIRECTORY: &str = "staging";

pub(super) fn product_store_relative(
    public_directory: &Path,
    product: &ProductIdentity,
) -> PathBuf {
    public_directory
        .join(METADATA_DIRECTORY)
        .join(product.name())
}

pub(super) fn product_store(
    root: &Path,
    public_directory: &Path,
    product: &ProductIdentity,
) -> PathBuf {
    root.join(product_store_relative(public_directory, product))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use bray_symbols::{PackageIdentity, ProductIdentity};

    use super::{product_store, product_store_relative};

    #[test]
    fn private_product_stores_are_rooted_beneath_managed_metadata() {
        let package = PackageIdentity::try_new("example.package")
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        let product = ProductIdentity::try_new(package, "application")
            .unwrap_or_else(|| panic!("test product identity must be valid"));

        assert_eq!(
            product_store_relative(Path::new("native/debug/example.package"), &product),
            Path::new("native/debug/example.package/.bray/application")
        );

        assert_eq!(
            product_store(Path::new("build"), Path::new(""), &product),
            Path::new("build/.bray/application")
        );
    }
}
