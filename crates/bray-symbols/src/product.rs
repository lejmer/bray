use std::sync::Arc;

use bray_base::{NonEmptySharedStr, shared_slice};

use crate::{
    AnySymbolId, CallableExecution, FunctionSymbolId, ModulePathKey, PackageIdentity, SymbolName,
};

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

/// Command-wide scheduling constraint declared by one test entry.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TestExecutionConstraint {
    /// The runner may overlap this test with other parallel test entries.
    Parallel,
    /// The runner must execute this test without another admitted test entry.
    Serial,
}

/// Accepted result shape of one semantically valid test entry.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TestResultShape {
    /// The test returns `unit` directly.
    Unit,
    /// The test returns `Result<unit, E>` for some checked error type `E`.
    Recoverable,
}

/// Validated semantic properties of one test product entry.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProductTestEntry {
    function: FunctionSymbolId,
    module: ModulePathKey,
    name: SymbolName,
    execution: CallableExecution,
    constraint: TestExecutionConstraint,
    result: TestResultShape,
    error_type: Option<crate::TypeId>,
}

impl ProductTestEntry {
    /// Creates one validated test product entry.
    pub const fn new(
        function: FunctionSymbolId,
        module: ModulePathKey,
        name: SymbolName,
        execution: CallableExecution,
        constraint: TestExecutionConstraint,
        result: TestResultShape,
        error_type: Option<crate::TypeId>,
    ) -> Self {
        Self {
            function,
            module,
            name,
            execution,
            constraint,
            result,
            error_type,
        }
    }

    /// Returns the exact test function symbol.
    pub const fn function(&self) -> FunctionSymbolId {
        self.function
    }

    /// Returns the module that contains the test function.
    pub const fn module(&self) -> &ModulePathKey {
        &self.module
    }

    /// Returns the test function's declared name.
    pub const fn name(&self) -> &SymbolName {
        &self.name
    }

    /// Returns whether the test executes synchronously or asynchronously.
    pub const fn execution(&self) -> CallableExecution {
        self.execution
    }

    /// Returns the test's command-wide scheduling constraint.
    pub const fn constraint(&self) -> TestExecutionConstraint {
        self.constraint
    }

    /// Returns the test's accepted result shape.
    pub const fn result(&self) -> TestResultShape {
        self.result
    }

    /// Returns the concrete recoverable error type when the result is fallible.
    pub const fn error_type(&self) -> Option<crate::TypeId> {
        self.error_type
    }
}

/// Semantic roots and public declarations of one selected product.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductSemanticFacts {
    kind: ProductKind,
    entrypoint: Option<FunctionSymbolId>,
    test_entries: Arc<[ProductTestEntry]>,
    public_symbols: Arc<[AnySymbolId]>,
    requires_async_runtime: bool,
    is_recovered: bool,
}

impl ProductSemanticFacts {
    /// Creates one immutable selected-product semantic surface.
    pub fn new(
        kind: ProductKind,
        entrypoint: Option<FunctionSymbolId>,
        test_entries: impl IntoIterator<Item = ProductTestEntry>,
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
    pub fn test_entries(&self) -> &[ProductTestEntry] {
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
