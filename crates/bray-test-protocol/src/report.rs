use std::sync::Arc;

use bray_base::shared_slice;
use bray_symbols::ProductIdentity;

use crate::{TestCatalogDigest, TestInvocationResult, TestOutcome};

/// Deterministic discovery and filtering counts for one test command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TestSelectionSummary {
    discovered: usize,
    selected: usize,
}

impl TestSelectionSummary {
    /// Creates a summary after every product catalog has been filtered.
    pub const fn new(discovered: usize, selected: usize) -> Self {
        Self {
            discovered,
            selected,
        }
    }

    /// Returns the number of discovered test entries.
    pub const fn discovered(self) -> usize {
        self.discovered
    }

    /// Returns the number of selected test entries.
    pub const fn selected(self) -> usize {
        self.selected
    }

    /// Returns the number of entries excluded by selection policy.
    pub const fn filtered_out(self) -> usize {
        self.discovered.saturating_sub(self.selected)
    }
}

/// Canonical results produced by one native test product.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestProductReport {
    product: ProductIdentity,
    catalog_digest: TestCatalogDigest,
    results: Arc<[TestInvocationResult]>,
}

impl TestProductReport {
    /// Creates a product report from results in catalog order.
    pub fn new(
        product: ProductIdentity,
        catalog_digest: TestCatalogDigest,
        results: impl IntoIterator<Item = TestInvocationResult>,
    ) -> Self {
        Self {
            product,
            catalog_digest,
            results: shared_slice(results),
        }
    }

    /// Returns the represented package product.
    pub const fn product(&self) -> &ProductIdentity {
        &self.product
    }

    /// Returns the digest of the catalog used for execution.
    pub const fn catalog_digest(&self) -> TestCatalogDigest {
        self.catalog_digest
    }

    /// Returns invocation results in canonical catalog order.
    pub fn results(&self) -> &[TestInvocationResult] {
        &self.results
    }
}

/// Counts for every terminal test outcome category.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TestOutcomeCounts {
    passed: usize,
    failed: usize,
}

impl TestOutcomeCounts {
    fn include(&mut self, outcome: &TestOutcome) {
        if matches!(outcome, TestOutcome::Passed) {
            self.passed += 1;
        } else {
            self.failed += 1;
        }
    }

    /// Returns the number of successful invocations.
    pub const fn passed(self) -> usize {
        self.passed
    }

    /// Returns the number of unsuccessful invocations.
    pub const fn failed(self) -> usize {
        self.failed
    }
}

/// Complete deterministic result of one Bray test command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestCommandReport {
    selection: TestSelectionSummary,
    products: Arc<[TestProductReport]>,
    counts: TestOutcomeCounts,
}

impl TestCommandReport {
    /// Assembles one command report and derives its outcome counts.
    pub fn new(
        selection: TestSelectionSummary,
        products: impl IntoIterator<Item = TestProductReport>,
    ) -> Self {
        let products = shared_slice(products);
        let mut counts = TestOutcomeCounts::default();

        for result in products.iter().flat_map(|product| product.results()) {
            counts.include(result.outcome());
        }

        Self {
            selection,
            products,
            counts,
        }
    }

    /// Returns command-wide discovery and selection counts.
    pub const fn selection(&self) -> TestSelectionSummary {
        self.selection
    }

    /// Returns product reports in canonical product order.
    pub fn products(&self) -> &[TestProductReport] {
        &self.products
    }

    /// Returns aggregate terminal outcome counts.
    pub const fn counts(&self) -> TestOutcomeCounts {
        self.counts
    }

    /// Returns whether every selected invocation passed.
    pub const fn succeeded(&self) -> bool {
        self.counts.failed == 0
    }
}
