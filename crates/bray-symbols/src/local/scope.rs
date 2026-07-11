use std::collections::BTreeMap;

use bray_declarations::SyntaxAnchor;
use bray_source::TextSize;

use super::{AnyLocalSymbolId, LocalScopeId, PostconditionResultSymbolId};
use crate::{AnySymbolId, SymbolName};

/// Classifies a lexical lookup boundary without treating the boundary as a symbol.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LocalScopeBoundary {
    /// The root scope of one local semantic region.
    Root,
    /// A callable lookup boundary.
    Callable,
    /// One pattern arm's binding scope.
    PatternArm,
    /// A guard expression's binding scope.
    Guard,
    /// A lexical block scope.
    Block,
    /// A callable contract context.
    Contract,
}

/// An immutable lexical lookup scope inside one local semantic region.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalScope {
    pub(super) id: LocalScopeId,
    pub(super) parent: Option<LocalScopeId>,
    pub(super) boundary: LocalScopeBoundary,
    pub(super) syntax: SyntaxAnchor,
    pub(super) visibility_start: TextSize,
    pub(super) local_names: BTreeMap<SymbolName, Box<[AnyLocalSymbolId]>>,
    pub(super) surface_names: BTreeMap<SymbolName, Box<[AnySymbolId]>>,
    pub(super) postcondition_result: Option<PostconditionResultSymbolId>,
}

impl LocalScope {
    /// Returns this scope's exact region-scoped ID.
    pub const fn id(&self) -> LocalScopeId {
        self.id
    }

    /// Returns the lexical parent scope when one exists.
    pub const fn parent(&self) -> Option<LocalScopeId> {
        self.parent
    }

    /// Returns the semantic boundary introduced by this scope.
    pub const fn boundary(&self) -> LocalScopeBoundary {
        self.boundary
    }

    /// Returns the source syntax associated with this scope.
    pub const fn syntax_anchor(&self) -> SyntaxAnchor {
        self.syntax
    }

    /// Returns the first source position where this scope participates in lookup.
    pub const fn visibility_start(&self) -> TextSize {
        self.visibility_start
    }

    /// Returns local symbols indexed under an ordinary name in insertion order.
    pub fn local_symbols_named(&self, name: &str) -> &[AnyLocalSymbolId] {
        self.local_names.get(name).map_or(&[], Box::as_ref)
    }

    /// Returns declaration-surface symbols visible under an ordinary name.
    pub fn surface_symbols_named(&self, name: &str) -> &[AnySymbolId] {
        self.surface_names.get(name).map_or(&[], Box::as_ref)
    }

    /// Returns the contextual postcondition result visible in this scope.
    pub const fn postcondition_result(&self) -> Option<PostconditionResultSymbolId> {
        self.postcondition_result
    }
}
