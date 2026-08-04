//! Stable metadata and command protocol shared by Bray test producers and consumers.

#![forbid(unsafe_code)]

mod catalog;
mod filter;
mod identity;

pub use catalog::{TestCatalog, TestCatalogBuildError, TestEntryMetadata};
pub use filter::{TestFilter, TestSelection, TestSelectionQuery, TestShard};
pub use identity::{TestDeclarationPath, TestIdentity, TestSourceAnchor};
