use bray_compiler_known::{
    CatalogDeclarationKind, CompilerKnownDeclarationId, CompilerKnownScopeId,
};

use crate::SymbolKind;
use crate::allocator::SymbolIdCapacityError;

/// A violated invariant while materializing the generated compiler-known catalog.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompilerKnownSymbolBuildError {
    /// The catalog-local scope ID does not match canonical descriptor order.
    NonCanonicalScopeId {
        /// The canonical table position.
        expected: CompilerKnownScopeId,
        /// The ID stored by the descriptor.
        actual: CompilerKnownScopeId,
    },
    /// The catalog-local declaration ID does not match canonical descriptor order.
    NonCanonicalDeclarationId {
        /// The canonical table position.
        expected: CompilerKnownDeclarationId,
        /// The ID stored by the descriptor.
        actual: CompilerKnownDeclarationId,
    },
    /// The compilation-wide compact identity space was exhausted.
    SymbolCapacityExceeded {
        /// The collection index that could not be represented.
        index: usize,
    },
    /// More than one catalog scope claimed the ambient environment.
    DuplicateAmbientScope {
        /// The later duplicate scope.
        duplicate: CompilerKnownScopeId,
    },
    /// The catalog contains no ambient environment scope.
    MissingAmbientScope,
    /// A module scope contained an invalid empty path.
    InvalidModulePath {
        /// The affected scope.
        scope: CompilerKnownScopeId,
    },
    /// A declaration referred to a scope absent from the catalog skeleton.
    MissingScopeOwner {
        /// The affected declaration.
        declaration: CompilerKnownDeclarationId,
        /// The missing scope.
        scope: CompilerKnownScopeId,
    },
    /// A declaration referred to another declaration absent from the catalog skeleton.
    MissingDeclarationOwner {
        /// The affected declaration.
        declaration: CompilerKnownDeclarationId,
        /// The missing owner declaration.
        owner: CompilerKnownDeclarationId,
    },
    /// Compiler-known declaration ownership contains a cycle.
    DeclarationOwnerCycle {
        /// A declaration reached again while resolving its owner chain.
        declaration: CompilerKnownDeclarationId,
    },
    /// A catalog declaration category is invalid in its semantic owner context.
    InvalidDeclarationKind {
        /// The affected declaration.
        declaration: CompilerKnownDeclarationId,
        /// The source-shaped catalog category.
        catalog_kind: CatalogDeclarationKind,
        /// The semantic kind of the direct owner.
        owner_kind: SymbolKind,
    },
    /// A generated declaration surface is absent or disagrees with its descriptor.
    InvalidDeclarationSurface {
        /// The affected declaration.
        declaration: CompilerKnownDeclarationId,
    },
}

impl From<SymbolIdCapacityError> for CompilerKnownSymbolBuildError {
    fn from(error: SymbolIdCapacityError) -> Self {
        Self::SymbolCapacityExceeded { index: error.index }
    }
}
