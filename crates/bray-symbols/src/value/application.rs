use crate::{AnySymbolId, ImplementationSymbolId, SymbolKind, TraitSymbolId};

use super::GenericSubstitutionId;

/// A validated symbol definition that can produce a callable value.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableDefinitionId(AnySymbolId);

impl CallableDefinitionId {
    /// Creates a callable definition from an exact supported symbol category.
    pub const fn try_new(symbol: AnySymbolId) -> Option<Self> {
        if is_callable_definition(symbol.kind()) {
            return Some(Self(symbol));
        }

        None
    }

    /// Returns the exact symbol retained by this callable-definition adapter.
    pub const fn symbol(self) -> AnySymbolId {
        self.0
    }
}

const fn is_callable_definition(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Function
            | SymbolKind::TypeCallableMember
            | SymbolKind::Constructor
            | SymbolKind::Finalizer
            | SymbolKind::Destructor
            | SymbolKind::ScopeEnter
            | SymbolKind::ScopeExit
            | SymbolKind::TraitCallableMember
            | SymbolKind::TraitFinalizerRequirement
            | SymbolKind::TraitDestructorRequirement
            | SymbolKind::TraitScopeEnterRequirement
            | SymbolKind::TraitScopeExitRequirement
            | SymbolKind::TraitCallableFulfillment
            | SymbolKind::TraitScopeEnterFulfillment
            | SymbolKind::TraitScopeExitFulfillment
    )
}

/// The immutable structural key for one applied trait.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TraitApplicationData {
    definition: TraitSymbolId,
    substitution: GenericSubstitutionId,
}

impl TraitApplicationData {
    /// Creates a trait application from its definition and ordered substitution.
    pub const fn new(definition: TraitSymbolId, substitution: GenericSubstitutionId) -> Self {
        Self {
            definition,
            substitution,
        }
    }

    /// Returns the applied trait definition.
    pub const fn definition(self) -> TraitSymbolId {
        self.definition
    }

    /// Returns the ordered generic substitution.
    pub const fn substitution(self) -> GenericSubstitutionId {
        self.substitution
    }
}

/// The immutable structural key for one substituted callable definition.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableInstanceData {
    definition: CallableDefinitionId,
    substitution: GenericSubstitutionId,
}

impl CallableInstanceData {
    /// Creates a callable instance from its definition and ordered substitution.
    pub const fn new(
        definition: CallableDefinitionId,
        substitution: GenericSubstitutionId,
    ) -> Self {
        Self {
            definition,
            substitution,
        }
    }

    /// Returns the exact callable definition.
    pub const fn definition(self) -> CallableDefinitionId {
        self.definition
    }

    /// Returns the ordered generic substitution.
    pub const fn substitution(self) -> GenericSubstitutionId {
        self.substitution
    }
}

/// The immutable structural key for one selected implementation witness.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImplementationInstanceData {
    definition: ImplementationSymbolId,
    substitution: GenericSubstitutionId,
}

impl ImplementationInstanceData {
    /// Creates an implementation instance from its definition and ordered substitution.
    pub const fn new(
        definition: ImplementationSymbolId,
        substitution: GenericSubstitutionId,
    ) -> Self {
        Self {
            definition,
            substitution,
        }
    }

    /// Returns the selected implementation definition.
    pub const fn definition(self) -> ImplementationSymbolId {
        self.definition
    }

    /// Returns the ordered generic substitution.
    pub const fn substitution(self) -> GenericSubstitutionId {
        self.substitution
    }
}
