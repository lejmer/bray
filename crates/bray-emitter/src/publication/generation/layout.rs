use std::hash::Hash;
use std::path::{Path, PathBuf};

use bray_base::{StableDigestHasher, lowercase_hex};
use bray_symbols::ProductIdentity;

pub(super) const METADATA_DIRECTORY: &str = ".bray";
pub(super) const PRIVATE_GENERATION_PREFIX: &str = ".bray-generation-";
pub(super) const STAGING_DIRECTORY: &str = "staging";

pub(super) fn is_product_intermediate(name: &str) -> bool {
    name == STAGING_DIRECTORY
        || name.starts_with(PRIVATE_GENERATION_PREFIX)
        || bray_base::is_staged_file_name(name)
}

pub(super) fn product_store_relative(
    public_directory: &Path,
    product: &ProductIdentity,
) -> PathBuf {
    let mut identity = StableDigestHasher::new();

    (public_directory, product).hash(&mut identity);

    Path::new(METADATA_DIRECTORY)
        .join("products")
        .join(lowercase_hex(&identity.finalize()))
}

pub(in crate::publication) fn product_store(
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

        let relative = product_store_relative(Path::new("native/debug/example.package"), &product);

        assert!(relative.starts_with(".bray/products"));
        assert_eq!(relative.components().count(), 3);

        assert_eq!(
            product_store(Path::new("build"), Path::new(""), &product),
            Path::new("build").join(product_store_relative(Path::new(""), &product))
        );
    }

    #[test]
    fn private_stores_distinguish_public_namespaces_and_packages() {
        let first = crate::test_support::product_identity();

        let second = ProductIdentity::try_new(
            PackageIdentity::try_new("other.package").unwrap(),
            first.name(),
        )
        .unwrap();

        assert_ne!(
            product_store_relative(Path::new("native/debug"), &first),
            product_store_relative(Path::new("native/release"), &first)
        );

        assert_ne!(
            product_store_relative(Path::new(""), &first),
            product_store_relative(Path::new(""), &second)
        );
    }
}
