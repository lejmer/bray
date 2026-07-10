use bray_declarations::{ContainerId, DeclarationId, DeclarationKind, ModulePartId};

use crate::SymbolKind;

/// A structural failure while constructing an immutable symbol identity graph.
///
/// Malformed source remains representable in the graph. These failures describe violated phase
/// contracts or exhausted compact identity space rather than ordinary source diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SymbolGraphBuildError {
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
