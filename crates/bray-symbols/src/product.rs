use std::sync::Arc;

use bray_base::{NonEmptySharedStr, shared_slice};

use crate::{AnySymbolId, FunctionSymbolId, PackageIdentity};

/// Stable package-layer identity of one selected product.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProductIdentity {
    package: PackageIdentity,
    name: NonEmptySharedStr,
}

impl ProductIdentity {
    /// Creates a product identity unless its package-local canonical name is empty.
    pub fn try_new(package: PackageIdentity, name: impl Into<Arc<str>>) -> Option<Self> {
        let name = NonEmptySharedStr::try_new(name)?;

        Some(Self { package, name })
    }

    /// Returns the package that owns the product.
    pub const fn package(&self) -> &PackageIdentity {
        &self.package
    }

    /// Returns the package-local canonical product name.
    pub fn name(&self) -> &str {
        self.name.as_str()
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

/// Semantic roots and public declarations of one selected product.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductSemanticFacts {
    kind: ProductKind,
    entrypoint: Option<FunctionSymbolId>,
    test_entries: Arc<[FunctionSymbolId]>,
    public_symbols: Arc<[AnySymbolId]>,
    requires_async_runtime: bool,
    is_recovered: bool,
}

impl ProductSemanticFacts {
    /// Creates one immutable selected-product semantic surface.
    pub fn new(
        kind: ProductKind,
        entrypoint: Option<FunctionSymbolId>,
        test_entries: impl IntoIterator<Item = FunctionSymbolId>,
        public_symbols: impl IntoIterator<Item = AnySymbolId>,
        requires_async_runtime: bool,
        is_recovered: bool,
    ) -> Self {
        Self {
            kind,
            entrypoint,
            test_entries: shared_slice(test_entries),
            public_symbols: shared_slice(public_symbols),
            requires_async_runtime,
            is_recovered,
        }
    }

    /// Returns the selected package-product category.
    pub const fn kind(&self) -> ProductKind {
        self.kind
    }

    /// Returns the executable entrypoint when this is an executable product.
    pub const fn entrypoint(&self) -> Option<FunctionSymbolId> {
        self.entrypoint
    }

    /// Returns enabled test entries in stable source order.
    pub fn test_entries(&self) -> &[FunctionSymbolId] {
        &self.test_entries
    }

    /// Returns publicly reachable source symbols in stable identity order.
    pub fn public_symbols(&self) -> &[AnySymbolId] {
        &self.public_symbols
    }

    /// Returns whether selected roots require an async runtime.
    pub const fn requires_async_runtime(&self) -> bool {
        self.requires_async_runtime
    }

    /// Returns whether recovery prevented complete product validation.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
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
