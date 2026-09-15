use std::hash::{Hash, Hasher};

use bray_bound_tree::BoundTreeBuildError;
use bray_symbols::LocalSymbolBuildError;

/// An allocation limit reached while constructing a bound tree or local-symbol region.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoundUnitConstructionError {
    /// A bound-node arena exhausted its identity capacity.
    BoundTree(BoundTreeBuildError),
    /// A local-symbol arena exhausted its identity capacity.
    LocalSymbol(LocalSymbolBuildError),
}

impl Hash for BoundUnitConstructionError {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);

        match self {
            Self::BoundTree(error) => error.hash(state),
            Self::LocalSymbol(error) => std::mem::discriminant(error).hash(state),
        }
    }
}

impl From<BoundTreeBuildError> for BoundUnitConstructionError {
    fn from(error: BoundTreeBuildError) -> Self {
        Self::BoundTree(error)
    }
}

impl From<LocalSymbolBuildError> for BoundUnitConstructionError {
    fn from(error: LocalSymbolBuildError) -> Self {
        Self::LocalSymbol(error)
    }
}
