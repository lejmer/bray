use bray_declarations::SyntaxAnchor;

use super::{
    AnonymousCallableParameterSymbolId, AnonymousCallableSymbolId, LocalBindingSymbolId,
    LocalConstantSymbolId, LocalScopeId, LocalSymbolKey, PostconditionResultSymbolId,
};
use crate::{SymbolKind, SymbolName, SymbolOrdinal};

macro_rules! define_named_local_record {
    ($record:ident, $id:ident, $kind:ident) => {
        #[doc = concat!("The immutable identity record for one `", stringify!($kind), "` symbol.")]
        #[derive(Clone, Debug, Eq, PartialEq)]
        pub struct $record {
            id: $id,
            key: LocalSymbolKey,
            scope: LocalScopeId,
            name: SymbolName,
            is_recovered: bool,
        }

        impl $record {
            pub(super) const fn new(
                id: $id,
                key: LocalSymbolKey,
                scope: LocalScopeId,
                name: SymbolName,
                is_recovered: bool,
            ) -> Self {
                Self {
                    id,
                    key,
                    scope,
                    name,
                    is_recovered,
                }
            }

            /// Returns this symbol's exact region-scoped ID.
            pub const fn id(&self) -> $id {
                self.id
            }

            /// Returns this symbol's deterministic key within its region.
            pub const fn key(&self) -> &LocalSymbolKey {
                &self.key
            }

            /// Returns the lexical scope that contains this symbol.
            pub const fn scope(&self) -> LocalScopeId {
                self.scope
            }

            /// Returns this symbol's ordinary lookup name.
            pub const fn name(&self) -> &SymbolName {
                &self.name
            }

            /// Returns whether recovery contributed to this symbol identity.
            pub const fn is_recovered(&self) -> bool {
                self.is_recovered
            }
        }
    };
}

define_named_local_record!(LocalBindingSymbol, LocalBindingSymbolId, LocalBinding);
define_named_local_record!(LocalConstantSymbol, LocalConstantSymbolId, LocalConstant);

/// The immutable identity record for one anonymous callable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnonymousCallableSymbol {
    id: AnonymousCallableSymbolId,
    key: LocalSymbolKey,
    scope: LocalScopeId,
    callable_scope: LocalScopeId,
    parameters: Box<[AnonymousCallableParameterSymbolId]>,
    is_recovered: bool,
}

impl AnonymousCallableSymbol {
    pub(super) fn new(
        id: AnonymousCallableSymbolId,
        key: LocalSymbolKey,
        scope: LocalScopeId,
        callable_scope: LocalScopeId,
        is_recovered: bool,
    ) -> Self {
        Self {
            id,
            key,
            scope,
            callable_scope,
            parameters: Box::new([]),
            is_recovered,
        }
    }

    pub(super) fn with_parameters(
        mut self,
        parameters: Box<[AnonymousCallableParameterSymbolId]>,
    ) -> Self {
        self.parameters = parameters;

        self
    }

    /// Returns this callable's exact region-scoped ID.
    pub const fn id(&self) -> AnonymousCallableSymbolId {
        self.id
    }

    /// Returns this callable's deterministic key within its region.
    pub const fn key(&self) -> &LocalSymbolKey {
        &self.key
    }

    /// Returns the enclosing lexical scope in which this callable is introduced.
    pub const fn scope(&self) -> LocalScopeId {
        self.scope
    }

    /// Returns the callable boundary that owns this callable's parameters and body names.
    pub const fn callable_scope(&self) -> LocalScopeId {
        self.callable_scope
    }

    /// Returns this callable's parameters in declaration order.
    pub fn parameters(&self) -> &[AnonymousCallableParameterSymbolId] {
        &self.parameters
    }

    /// Returns whether recovery contributed to this callable identity.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}

/// The immutable identity record for one anonymous callable parameter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnonymousCallableParameterSymbol {
    id: AnonymousCallableParameterSymbolId,
    key: LocalSymbolKey,
    callable: AnonymousCallableSymbolId,
    scope: LocalScopeId,
    name: SymbolName,
    ordinal: SymbolOrdinal,
    mode: crate::CallableParameterMode,
    is_recovered: bool,
}

impl AnonymousCallableParameterSymbol {
    #[expect(
        clippy::too_many_arguments,
        reason = "each argument initializes one required immutable parameter field"
    )]
    pub(super) const fn new(
        id: AnonymousCallableParameterSymbolId,
        key: LocalSymbolKey,
        callable: AnonymousCallableSymbolId,
        scope: LocalScopeId,
        name: SymbolName,
        ordinal: SymbolOrdinal,
        mode: crate::CallableParameterMode,
        is_recovered: bool,
    ) -> Self {
        Self {
            id,
            key,
            callable,
            scope,
            name,
            ordinal,
            mode,
            is_recovered,
        }
    }

    /// Returns this parameter's exact region-scoped ID.
    pub const fn id(&self) -> AnonymousCallableParameterSymbolId {
        self.id
    }

    /// Returns this parameter's deterministic key within its region.
    pub const fn key(&self) -> &LocalSymbolKey {
        &self.key
    }

    /// Returns the anonymous callable that owns this parameter.
    pub const fn callable(&self) -> AnonymousCallableSymbolId {
        self.callable
    }

    /// Returns the callable scope containing this parameter.
    pub const fn scope(&self) -> LocalScopeId {
        self.scope
    }

    /// Returns this parameter's ordinary lookup name.
    pub const fn name(&self) -> &SymbolName {
        &self.name
    }

    /// Returns this parameter's declaration-order ordinal.
    pub const fn ordinal(&self) -> SymbolOrdinal {
        self.ordinal
    }

    /// Returns the owned binding's local mutation mode.
    pub const fn mode(&self) -> crate::CallableParameterMode {
        self.mode
    }

    /// Returns whether recovery contributed to this parameter identity.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}

/// The immutable identity record for a contextual postcondition result binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PostconditionResultSymbol {
    id: PostconditionResultSymbolId,
    key: LocalSymbolKey,
    scope: LocalScopeId,
    syntax: SyntaxAnchor,
    is_recovered: bool,
}

impl PostconditionResultSymbol {
    pub(super) const fn new(
        id: PostconditionResultSymbolId,
        key: LocalSymbolKey,
        scope: LocalScopeId,
        syntax: SyntaxAnchor,
        is_recovered: bool,
    ) -> Self {
        Self {
            id,
            key,
            scope,
            syntax,
            is_recovered,
        }
    }

    /// Returns this contextual symbol's exact region-scoped ID.
    pub const fn id(&self) -> PostconditionResultSymbolId {
        self.id
    }

    /// Returns this contextual symbol's deterministic key within its region.
    pub const fn key(&self) -> &LocalSymbolKey {
        &self.key
    }

    /// Returns the contract scope containing this result binding.
    pub const fn scope(&self) -> LocalScopeId {
        self.scope
    }

    /// Returns the contract syntax that introduced this contextual binding.
    pub const fn syntax_anchor(&self) -> SyntaxAnchor {
        self.syntax
    }

    /// Returns whether recovery contributed to this contextual binding.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }

    /// Returns this record's exact semantic kind.
    pub const fn kind(&self) -> SymbolKind {
        SymbolKind::PostconditionResult
    }
}
