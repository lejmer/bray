use bray_symbols::{
    CallableParameterSymbolId, CallableSignatureTemplate, CallableSymbolId,
    GenericDeclarationTemplate, GenericOwnerId, PredicateDefinitionSymbolId,
    UnevaluatedDefaultTemplate,
};

use crate::InterfacePredicateDefinitionState;

/// One imported callable signature template and its exact owner.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImportedCallableSignatureFact {
    pub(super) owner: CallableSymbolId,
    pub(super) signature: CallableSignatureTemplate,
}

impl ImportedCallableSignatureFact {
    /// Returns the callable that owns this signature.
    pub const fn owner(&self) -> CallableSymbolId {
        self.owner
    }

    /// Returns the source-independent callable signature template.
    pub const fn signature(&self) -> &CallableSignatureTemplate {
        &self.signature
    }
}

/// One imported generic declaration template and its exact owner.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImportedGenericDeclarationFact {
    pub(super) owner: GenericOwnerId,
    pub(super) declaration: GenericDeclarationTemplate,
}

impl ImportedGenericDeclarationFact {
    /// Returns the generic declaration that owns this template.
    pub const fn owner(&self) -> GenericOwnerId {
        self.owner
    }

    /// Returns the source-independent generic declaration template.
    pub const fn declaration(&self) -> &GenericDeclarationTemplate {
        &self.declaration
    }
}

/// One imported callable parameter default-template fact.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImportedCallableParameterDefaultFact {
    pub(super) parameter: CallableParameterSymbolId,
    pub(super) default: UnevaluatedDefaultTemplate,
}

impl ImportedCallableParameterDefaultFact {
    /// Returns the callable parameter that owns this fact.
    pub const fn parameter(self) -> CallableParameterSymbolId {
        self.parameter
    }

    /// Returns the source-independent default template.
    pub const fn default(self) -> UnevaluatedDefaultTemplate {
        self.default
    }
}

/// One imported predicate definition state and its exact owner.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImportedPredicateDefinitionFact {
    pub(super) owner: PredicateDefinitionSymbolId,
    pub(super) state: InterfacePredicateDefinitionState,
}

impl ImportedPredicateDefinitionFact {
    /// Returns the predicate declaration that owns this state.
    pub const fn owner(self) -> PredicateDefinitionSymbolId {
        self.owner
    }

    /// Returns the predicate's exported definition state.
    pub const fn state(self) -> InterfacePredicateDefinitionState {
        self.state
    }
}
