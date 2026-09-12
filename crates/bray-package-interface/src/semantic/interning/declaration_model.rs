use bray_symbols::{
    CallableParameterSymbolId, CallableSignatureTemplate, CallableSymbolId,
    GenericDeclarationTemplate, GenericOwnerId, PredicateDefinitionSymbolId,
    UnevaluatedDefaultTemplate,
};

use crate::InterfacePredicateDefinitionState;

/// One imported callable signature template and its exact owner.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImportedCallableSignature {
    pub(super) owner: CallableSymbolId,
    pub(super) signature: CallableSignatureTemplate,
}

impl ImportedCallableSignature {
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
pub struct ImportedGenericDeclaration {
    pub(super) owner: GenericOwnerId,
    pub(super) declaration: GenericDeclarationTemplate,
}

impl ImportedGenericDeclaration {
    /// Returns the generic declaration that owns this template.
    pub const fn owner(&self) -> GenericOwnerId {
        self.owner
    }

    /// Returns the source-independent generic declaration template.
    pub const fn declaration(&self) -> &GenericDeclarationTemplate {
        &self.declaration
    }
}

/// One imported callable parameter default-template record.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImportedCallableParameterDefault {
    pub(super) parameter: CallableParameterSymbolId,
    pub(super) default: UnevaluatedDefaultTemplate,
}

impl ImportedCallableParameterDefault {
    /// Returns the callable parameter that owns this record.
    pub const fn parameter(self) -> CallableParameterSymbolId {
        self.parameter
    }

    /// Returns the source-independent default template.
    pub const fn default(self) -> UnevaluatedDefaultTemplate {
        self.default
    }
}

/// One imported predicate definition state and its exact owner.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImportedPredicateDefinition {
    pub(super) owner: PredicateDefinitionSymbolId,
    pub(super) state: InterfacePredicateDefinitionState,
    pub(super) signature: bray_symbols::PredicateSignatureTemplate,
}

impl ImportedPredicateDefinition {
    /// Returns the predicate declaration that owns this state.
    pub const fn owner(&self) -> PredicateDefinitionSymbolId {
        self.owner
    }

    /// Returns the predicate's exported definition state.
    pub const fn state(&self) -> InterfacePredicateDefinitionState {
        self.state
    }

    /// Returns the imported predicate's parameter signature.
    pub const fn signature(&self) -> &bray_symbols::PredicateSignatureTemplate {
        &self.signature
    }
}
