use std::sync::Arc;

use bray_base::shared_slice;
use bray_symbols::{CallableExecution, ProductIdentity, TestExecutionConstraint, TestResultShape};

use crate::{TestErrorTypeIdentity, TestIdentity, TestSourceAnchor};

/// Stable metadata needed to select and invoke one declared test.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TestEntryMetadata {
    identity: TestIdentity,
    source: TestSourceAnchor,
    execution: CallableExecution,
    constraint: TestExecutionConstraint,
    result: TestResultShape,
    error_type: Option<TestErrorTypeIdentity>,
}

impl TestEntryMetadata {
    /// Creates metadata for one semantically valid test declaration.
    pub const fn new(
        identity: TestIdentity,
        source: TestSourceAnchor,
        execution: CallableExecution,
        constraint: TestExecutionConstraint,
        result: TestResultShape,
        error_type: Option<TestErrorTypeIdentity>,
    ) -> Self {
        Self {
            identity,
            source,
            execution,
            constraint,
            result,
            error_type,
        }
    }

    /// Returns the test's stable product-qualified identity.
    pub const fn identity(&self) -> &TestIdentity {
        &self.identity
    }

    /// Returns the exact source occurrence that declares the test.
    pub const fn source(&self) -> TestSourceAnchor {
        self.source
    }

    /// Returns whether the test body executes synchronously or asynchronously.
    pub const fn execution(&self) -> CallableExecution {
        self.execution
    }

    /// Returns the test's command-wide scheduling constraint.
    pub const fn constraint(&self) -> TestExecutionConstraint {
        self.constraint
    }

    /// Returns the accepted result shape of the test declaration.
    pub const fn result(&self) -> TestResultShape {
        self.result
    }

    /// Returns the recoverable error type identity for a fallible test.
    pub const fn error_type(&self) -> Option<&TestErrorTypeIdentity> {
        self.error_type.as_ref()
    }
}

/// Failure to assemble a canonical test catalog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TestCatalogBuildError {
    /// An entry belongs to a different product than the catalog.
    ProductMismatch(TestIdentity),
    /// Two entries claim the same product-qualified test identity.
    DuplicateIdentity(TestIdentity),
    /// An entry's result shape and recoverable error identity disagree.
    InvalidResultMetadata(TestIdentity),
}

/// Canonically ordered metadata for every test in one package product.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct TestCatalog {
    product: ProductIdentity,
    entries: Arc<[TestEntryMetadata]>,
}

impl TestCatalog {
    /// Validates and orders one product's test metadata.
    pub fn try_new(
        product: ProductIdentity,
        entries: impl IntoIterator<Item = TestEntryMetadata>,
    ) -> Result<Self, TestCatalogBuildError> {
        let mut entries = entries.into_iter().collect::<Vec<_>>();

        entries.sort_by(|left, right| {
            left.identity()
                .cmp(right.identity())
                .then_with(|| left.source().cmp(&right.source()))
        });

        for entry in &entries {
            if entry.identity().product() != &product {
                return Err(TestCatalogBuildError::ProductMismatch(
                    entry.identity().clone(),
                ));
            }

            let expects_error = entry.result() == TestResultShape::Recoverable;

            if expects_error != entry.error_type().is_some() {
                return Err(TestCatalogBuildError::InvalidResultMetadata(
                    entry.identity().clone(),
                ));
            }
        }

        for pair in entries.windows(2) {
            if pair[0].identity() == pair[1].identity() {
                return Err(TestCatalogBuildError::DuplicateIdentity(
                    pair[1].identity().clone(),
                ));
            }
        }

        Ok(Self {
            product,
            entries: shared_slice(entries),
        })
    }

    /// Returns the package product represented by this catalog.
    pub const fn product(&self) -> &ProductIdentity {
        &self.product
    }

    /// Returns entries in canonical test-identity order.
    pub fn entries(&self) -> &[TestEntryMetadata] {
        &self.entries
    }

    /// Returns the entry with an exact stable identity.
    pub fn entry(&self, identity: &TestIdentity) -> Option<&TestEntryMetadata> {
        self.entries
            .binary_search_by(|entry| entry.identity().cmp(identity))
            .ok()
            .map(|index| &self.entries[index])
    }
}

#[cfg(test)]
mod tests {
    use bray_source::{SourceId, SourceSpan, SourceVersion, TextRange, TextSize};
    use bray_symbols::{
        CallableExecution, ModulePathKey, ProductIdentity, SymbolName, TestExecutionConstraint,
        TestResultShape,
    };

    use super::{TestCatalog, TestCatalogBuildError, TestEntryMetadata};
    use crate::test_support::{product, product_named};
    use crate::{TestDeclarationPath, TestIdentity, TestSourceAnchor};

    #[test]
    fn catalogs_reject_duplicate_test_identities() {
        let product = product();
        let entry = entry(product.clone(), "runs", 4);
        let result = TestCatalog::try_new(product, [entry.clone(), entry]);

        assert!(matches!(
            result,
            Err(TestCatalogBuildError::DuplicateIdentity(_))
        ));
    }

    #[test]
    fn product_mismatch_errors_do_not_depend_on_input_order() {
        let catalog_product = product();
        let first = entry(product_named("other-a"), "first", 4);
        let second = entry(product_named("other-b"), "second", 8);

        let forward =
            TestCatalog::try_new(catalog_product.clone(), [first.clone(), second.clone()]);

        let reversed = TestCatalog::try_new(catalog_product, [second, first]);

        assert_eq!(forward, reversed);

        assert!(matches!(
            forward,
            Err(TestCatalogBuildError::ProductMismatch(_))
        ));
    }

    fn entry(product: ProductIdentity, name: &str, start: u32) -> TestEntryMetadata {
        let module = ModulePathKey::try_new(["app", "tests"])
            .unwrap_or_else(|| panic!("test module path must be valid"));

        let name =
            SymbolName::try_new(name).unwrap_or_else(|| panic!("test function name must be valid"));

        let identity = TestIdentity::new(product, TestDeclarationPath::new(module, name));
        let start = TextSize::new(start);

        let source = TestSourceAnchor::new(
            [0; 32],
            SourceSpan::new(SourceId::new(0), TextRange::new(start, start)),
            SourceVersion::new(0),
        );

        TestEntryMetadata::new(
            identity,
            source,
            CallableExecution::Synchronous,
            TestExecutionConstraint::Parallel,
            TestResultShape::Unit,
            None,
        )
    }
}
