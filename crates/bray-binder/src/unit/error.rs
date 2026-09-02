use std::hash::{Hash, Hasher};

use bray_bound_tree::BoundTreeBuildError;
use bray_source::{SourceId, SourceVersion};
use bray_symbols::{
    AnonymousCallableSymbolId, AnyLocalSymbolId, AnySymbolId, LocalScopeId, LocalSymbolBuildError,
    SymbolOrdinal,
};

/// A structural failure while completing a bound tree and local-symbol region.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoundUnitConstructionError {
    /// The bound-tree contract rejected a node relationship.
    BoundTree(BoundTreeBuildError),
    /// The local-symbol contract rejected a scope or symbol relationship.
    LocalSymbol(LocalSymbolBuildError),
    /// A named local identity was activated more than once.
    LocalAlreadyActivated(AnyLocalSymbolId),
    /// A surface symbol referenced by a local scope is absent from the supplied symbol graph.
    UnknownSurfaceSymbol(AnySymbolId),
    /// An anonymous callable boundary belongs to another enclosing semantic unit.
    AnonymousCallableBoundaryMismatch,
    /// The same anonymous callable identity was assigned more than once.
    AnonymousCallableAlreadyAssigned {
        /// The lexical scope containing the callable expression.
        introduction_scope: LocalScopeId,
        /// The source-order role distinguishing repeated callable syntax.
        ordinal: Option<SymbolOrdinal>,
    },
    /// One anonymous callable parameter ordinal was assigned more than once.
    AnonymousCallableParameterAlreadyAssigned {
        /// The anonymous callable owning the parameter list.
        callable: AnonymousCallableSymbolId,
        /// The repeated declaration-order ordinal.
        ordinal: SymbolOrdinal,
    },
    /// An anonymous callable anchor belongs to a different source.
    AnonymousCallableSourceMismatch {
        /// The source containing the enclosing semantic unit.
        expected: SourceId,
        /// The source supplied for the anonymous callable.
        actual: SourceId,
    },
    /// An anonymous callable source belongs to a different logical source revision.
    AnonymousCallableSourceVersionMismatch {
        /// The source revision of the enclosing semantic unit.
        expected: SourceVersion,
        /// The source revision supplied for the anonymous callable.
        actual: SourceVersion,
    },
}

impl Hash for BoundUnitConstructionError {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);

        match self {
            Self::BoundTree(error) => hash_bound_tree_error(*error, state),
            Self::LocalSymbol(error) => std::mem::discriminant(error).hash(state),
            Self::LocalAlreadyActivated(symbol) => symbol.hash(state),
            Self::UnknownSurfaceSymbol(symbol) => symbol.hash(state),
            Self::AnonymousCallableBoundaryMismatch => {}
            Self::AnonymousCallableAlreadyAssigned {
                introduction_scope,
                ordinal,
            } => {
                introduction_scope.hash(state);
                ordinal.hash(state);
            }
            Self::AnonymousCallableParameterAlreadyAssigned { callable, ordinal } => {
                callable.hash(state);
                ordinal.hash(state);
            }
            Self::AnonymousCallableSourceMismatch { expected, actual } => {
                expected.hash(state);
                actual.hash(state);
            }
            Self::AnonymousCallableSourceVersionMismatch { expected, actual } => {
                expected.hash(state);
                actual.hash(state);
            }
        }
    }
}

fn hash_bound_tree_error<H: Hasher>(error: BoundTreeBuildError, state: &mut H) {
    std::mem::discriminant(&error).hash(state);

    match error {
        BoundTreeBuildError::ArenaCapacityExceeded(kind) => kind.hash(state),
        BoundTreeBuildError::ForeignNode {
            expected,
            actual,
            kind,
        } => {
            expected.hash(state);
            actual.hash(state);
            kind.hash(state);
        }
        BoundTreeBuildError::MissingNode { kind, slot } => {
            kind.hash(state);
            slot.hash(state);
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
