use std::sync::Arc;

use bray_base::shared_str;

use crate::PackageIdentity;

/// Stable package-layer identity of one selected product.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProductIdentity {
    package: PackageIdentity,
    name: Arc<str>,
}

impl ProductIdentity {
    /// Creates a product identity unless its package-local canonical name is empty.
    pub fn try_new(package: PackageIdentity, name: impl Into<Arc<str>>) -> Option<Self> {
        let name = shared_str(name);

        if name.is_empty() {
            return None;
        }

        Some(Self { package, name })
    }

    /// Returns the package that owns the product.
    pub const fn package(&self) -> &PackageIdentity {
        &self.package
    }

    /// Returns the package-local canonical product name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Language-level category of one selected package product.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ProductKind {
    /// A product with a runtime entry point.
    Executable,
    /// An importable package library.
    Library,
    /// A product selected for test compilation and execution.
    Test,
}

#[cfg(test)]
mod tests {
    use super::ProductIdentity;
    use crate::PackageIdentity;

    #[test]
    fn product_identities_require_a_package_local_name() {
        let Some(package) = PackageIdentity::try_new("example.package") else {
            panic!("test package identity must be valid");
        };

        assert_eq!(ProductIdentity::try_new(package.clone(), ""), None);

        let Some(product) = ProductIdentity::try_new(package.clone(), "application") else {
            panic!("test product identity must be valid");
        };

        assert_eq!(product.package(), &package);
        assert_eq!(product.name(), "application");
    }
}
