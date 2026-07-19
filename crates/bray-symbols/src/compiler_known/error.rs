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
    /// A typed role referred to a declaration absent from the symbol provider.
    MissingRoleDeclarationSymbol {
        /// The declaration selected by the role binding.
        declaration: CompilerKnownDeclarationId,
    },
    /// An operation role declaration materialized with an incompatible symbol category.
    InvalidOperationRoleSymbol {
        /// The declaration selected by the operation role.
        declaration: CompilerKnownDeclarationId,
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

impl std::fmt::Display for CompilerKnownSymbolBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonCanonicalScopeId { expected, actual } => write!(
                formatter,
                "compiler-known scope ID {actual:?} does not match canonical ID {expected:?}"
            ),
            Self::NonCanonicalDeclarationId { expected, actual } => write!(
                formatter,
                "compiler-known declaration ID {actual:?} does not match canonical ID {expected:?}"
            ),
            Self::SymbolCapacityExceeded { index } => write!(
                formatter,
                "compiler-known symbol index {index} exceeds compact identity capacity"
            ),
            Self::DuplicateAmbientScope { duplicate } => {
                write!(
                    formatter,
                    "compiler-known ambient scope {duplicate:?} is duplicated"
                )
            }
            Self::MissingAmbientScope => {
                formatter.write_str("compiler-known catalog has no ambient scope")
            }
            Self::InvalidModulePath { scope } => {
                write!(
                    formatter,
                    "compiler-known scope {scope:?} has an invalid module path"
                )
            }
            Self::MissingScopeOwner { declaration, scope } => write!(
                formatter,
                "compiler-known declaration {declaration:?} refers to missing scope {scope:?}"
            ),
            Self::MissingDeclarationOwner { declaration, owner } => write!(
                formatter,
                "compiler-known declaration {declaration:?} refers to missing declaration {owner:?}"
            ),
            Self::MissingRoleDeclarationSymbol { declaration } => write!(
                formatter,
                "compiler-known role refers to declaration {declaration:?} without a symbol"
            ),
            Self::InvalidOperationRoleSymbol { declaration } => write!(
                formatter,
                "compiler-known operation role refers to incompatible declaration {declaration:?}"
            ),
            Self::DeclarationOwnerCycle { declaration } => write!(
                formatter,
                "compiler-known declaration ownership cycles through {declaration:?}"
            ),
            Self::InvalidDeclarationKind {
                declaration,
                catalog_kind,
                owner_kind,
            } => write!(
                formatter,
                "compiler-known declaration {declaration:?} with catalog kind {catalog_kind:?} cannot belong to {owner_kind:?}"
            ),
            Self::InvalidDeclarationSurface { declaration } => write!(
                formatter,
                "compiler-known declaration {declaration:?} has an invalid generated surface"
            ),
        }
    }
}

impl std::error::Error for CompilerKnownSymbolBuildError {}

impl From<SymbolIdCapacityError> for CompilerKnownSymbolBuildError {
    fn from(error: SymbolIdCapacityError) -> Self {
        Self::SymbolCapacityExceeded { index: error.index }
    }
}
