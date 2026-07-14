use bray_declarations::{ContainerId, DeclarationId, DeclarationKind, ModulePartId};

use crate::{CompilerKnownSymbolBuildError, SymbolKind, allocator::SymbolIdCapacityError};

/// A structural failure while constructing an immutable symbol identity graph.
///
/// Malformed source remains representable in the graph. These failures describe violated phase
/// contracts or exhausted compact identity space rather than ordinary source diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SymbolGraphBuildError {
    /// The generated compiler-known catalog violated its trusted symbol contract.
    CompilerKnown(CompilerKnownSymbolBuildError),
    /// The number of compilation-wide symbols exceeded the compact ID representation.
    SymbolCapacityExceeded {
        /// The collection index that could not be represented.
        index: usize,
    },
    /// A declaration referred to a container absent from the declaration table.
    MissingContainer {
        /// The affected declaration.
        declaration: DeclarationId,
        /// The missing container.
        container: ContainerId,
    },
    /// A source declaration belonged to a module container without a module symbol.
    MissingModuleOwner {
        /// The affected declaration.
        declaration: DeclarationId,
        /// The module container without an identity.
        container: ContainerId,
    },
    /// A logical module container did not retain its required path record.
    MissingModulePath {
        /// The affected logical module container.
        container: ContainerId,
    },
    /// A logical module referred to a module part absent from the declaration table.
    MissingModulePart {
        /// The affected logical module container.
        container: ContainerId,
        /// The missing module part.
        module_part: ModulePartId,
    },
    /// A non-module declaration container had no declaration that introduced it.
    MissingContainingDeclaration {
        /// The affected declaration.
        declaration: DeclarationId,
        /// The container without an introducing declaration.
        container: ContainerId,
    },
    /// A containing declaration did not produce its required semantic symbol first.
    MissingContainingSymbol {
        /// The affected declaration.
        declaration: DeclarationId,
        /// The declaration expected to own it semantically.
        containing_declaration: DeclarationId,
    },
    /// A declaration category could not be represented as a source symbol in its context.
    InvalidSourceSymbolKind {
        /// The affected declaration.
        declaration: DeclarationId,
        /// The syntax-level declaration category.
        declaration_kind: DeclarationKind,
        /// The selected semantic symbol category.
        symbol_kind: SymbolKind,
    },
    /// A recovered empty module path had no module declaration to anchor its stable identity.
    MissingRecoveredModuleAnchor {
        /// The affected logical module container.
        container: ContainerId,
    },
}

impl std::fmt::Display for SymbolGraphBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CompilerKnown(error) => {
                write!(formatter, "compiler-known symbols failed: {error}")
            }
            Self::SymbolCapacityExceeded { index } => {
                write!(
                    formatter,
                    "symbol index {index} exceeds compact identity capacity"
                )
            }
            Self::MissingContainer {
                declaration,
                container,
            } => write!(
                formatter,
                "declaration {declaration:?} refers to missing container {container:?}"
            ),
            Self::MissingModuleOwner {
                declaration,
                container,
            } => write!(
                formatter,
                "declaration {declaration:?} has module container {container:?} without a symbol"
            ),
            Self::MissingModulePath { container } => {
                write!(formatter, "module container {container:?} has no path")
            }
            Self::MissingModulePart {
                container,
                module_part,
            } => write!(
                formatter,
                "module container {container:?} refers to missing part {module_part:?}"
            ),
            Self::MissingContainingDeclaration {
                declaration,
                container,
            } => write!(
                formatter,
                "declaration {declaration:?} has container {container:?} without an introducing declaration"
            ),
            Self::MissingContainingSymbol {
                declaration,
                containing_declaration,
            } => write!(
                formatter,
                "declaration {declaration:?} has containing declaration {containing_declaration:?} without a symbol"
            ),
            Self::InvalidSourceSymbolKind {
                declaration,
                declaration_kind,
                symbol_kind,
            } => write!(
                formatter,
                "declaration {declaration:?} with syntax kind {declaration_kind:?} cannot use symbol kind {symbol_kind:?}"
            ),
            Self::MissingRecoveredModuleAnchor { container } => write!(
                formatter,
                "recovered module container {container:?} has no declaration anchor"
            ),
        }
    }
}

impl std::error::Error for SymbolGraphBuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CompilerKnown(error) => Some(error),
            Self::SymbolCapacityExceeded { .. }
            | Self::MissingContainer { .. }
            | Self::MissingModuleOwner { .. }
            | Self::MissingModulePath { .. }
            | Self::MissingModulePart { .. }
            | Self::MissingContainingDeclaration { .. }
            | Self::MissingContainingSymbol { .. }
            | Self::InvalidSourceSymbolKind { .. }
            | Self::MissingRecoveredModuleAnchor { .. } => None,
        }
    }
}

impl From<CompilerKnownSymbolBuildError> for SymbolGraphBuildError {
    fn from(error: CompilerKnownSymbolBuildError) -> Self {
        Self::CompilerKnown(error)
    }
}

impl From<SymbolIdCapacityError> for SymbolGraphBuildError {
    fn from(error: SymbolIdCapacityError) -> Self {
        Self::SymbolCapacityExceeded { index: error.index }
    }
}
