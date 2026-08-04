use std::num::NonZeroU32;
use std::sync::Arc;

use bray_base::{NonEmptySharedStr, shared_slice};
use bray_symbols::{ModulePathKey, PackageIdentity, ProductIdentity, TestExecutionConstraint};

use crate::{TestCatalog, TestDeclarationPath, TestEntryMetadata, TestIdentity};

/// One metadata-only test selection condition.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TestFilter {
    /// Selects entries owned by one package.
    Package(PackageIdentity),
    /// Selects entries owned by one exact package product.
    Product(ProductIdentity),
    /// Selects entries declared directly in one module.
    Module(ModulePathKey),
    /// Selects entries whose declaration name contains the supplied text.
    NameContains(NonEmptySharedStr),
    /// Selects one exact fully qualified declaration path.
    Declaration(TestDeclarationPath),
    /// Selects one exact product-qualified test identity.
    Identity(TestIdentity),
    /// Selects entries with one scheduling constraint.
    Constraint(TestExecutionConstraint),
}

impl TestFilter {
    /// Creates a non-empty declaration-name substring filter.
    pub fn name_contains(text: impl Into<Arc<str>>) -> Option<Self> {
        NonEmptySharedStr::try_new(text).map(Self::NameContains)
    }

    fn matches(&self, entry: &TestEntryMetadata) -> bool {
        let identity = entry.identity();
        let declaration = identity.declaration();

        match self {
            Self::Package(package) => identity.product().package() == package,
            Self::Product(product) => identity.product() == product,
            Self::Module(module) => declaration.module() == module,
            Self::NameContains(text) => declaration.name().as_str().contains(text.as_str()),
            Self::Declaration(expected) => declaration == expected,
            Self::Identity(expected) => identity == expected,
            Self::Constraint(constraint) => entry.constraint() == *constraint,
        }
    }
}

/// One zero-based deterministic partition of a complete catalog.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TestShard {
    index: u32,
    count: NonZeroU32,
}

impl TestShard {
    /// Creates a shard when its zero-based index is inside the shard count.
    pub const fn try_new(index: u32, count: NonZeroU32) -> Option<Self> {
        if index >= count.get() {
            return None;
        }

        Some(Self { index, count })
    }

    /// Returns the zero-based shard index.
    pub const fn index(self) -> u32 {
        self.index
    }

    /// Returns the total number of shards.
    pub const fn count(self) -> NonZeroU32 {
        self.count
    }

    fn contains_catalog_index(self, index: usize) -> bool {
        let Ok(index) = u64::try_from(index) else {
            return false;
        };

        index % u64::from(self.count.get()) == u64::from(self.index)
    }
}

/// Immutable metadata filters and optional deterministic shard selection.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TestSelectionQuery {
    filters: Arc<[TestFilter]>,
    shard: Option<TestShard>,
}

impl TestSelectionQuery {
    /// Creates a selection query whose filters are combined with logical AND.
    pub fn new(filters: impl IntoIterator<Item = TestFilter>, shard: Option<TestShard>) -> Self {
        Self {
            filters: shared_slice(filters),
            shard,
        }
    }

    /// Returns filters in caller-supplied order.
    pub fn filters(&self) -> &[TestFilter] {
        &self.filters
    }

    /// Returns the selected catalog shard when sharding is enabled.
    pub const fn shard(&self) -> Option<TestShard> {
        self.shard
    }
}

/// One immutable selection from a canonical test catalog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestSelection {
    entries: Arc<[TestEntryMetadata]>,
    discovered_count: usize,
}

impl TestSelection {
    /// Selects catalog entries using metadata only.
    pub fn from_catalog(catalog: &TestCatalog, query: &TestSelectionQuery) -> Self {
        let entries = catalog
            .entries()
            .iter()
            .enumerate()
            .filter(|(index, entry)| {
                query
                    .shard()
                    .is_none_or(|shard| shard.contains_catalog_index(*index))
                    && query.filters().iter().all(|filter| filter.matches(entry))
            })
            .map(|(_, entry)| entry.clone());

        Self {
            entries: shared_slice(entries),
            discovered_count: catalog.entries().len(),
        }
    }

    /// Returns selected entries in canonical catalog order.
    pub fn entries(&self) -> &[TestEntryMetadata] {
        &self.entries
    }

    /// Returns the number of entries in the unfiltered catalog.
    pub const fn discovered_count(&self) -> usize {
        self.discovered_count
    }

    /// Returns the number of entries excluded by the query.
    pub fn filtered_out_count(&self) -> usize {
        self.discovered_count - self.entries.len()
    }
}
