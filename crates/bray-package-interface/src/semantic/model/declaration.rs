use std::sync::Arc;

use bray_symbols::ReceiverMode;

use super::InterfaceTypeId;
use crate::InterfaceSymbolReference;

/// One implicit receiver retained by a durable callable signature.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceCallableReceiver {
    pub(crate) parameter: InterfaceSymbolReference,
    pub(crate) ty: InterfaceTypeId,
    pub(crate) mode: ReceiverMode,
}

impl InterfaceCallableReceiver {
    /// Creates one source-independent receiver signature.
    pub const fn new(
        parameter: InterfaceSymbolReference,
        ty: InterfaceTypeId,
        mode: ReceiverMode,
    ) -> Self {
        Self {
            parameter,
            ty,
            mode,
        }
    }

    /// Returns the exact receiver parameter.
    pub const fn parameter(&self) -> &InterfaceSymbolReference {
        &self.parameter
    }

    /// Returns the receiver's checked declared type.
    pub const fn ty(&self) -> InterfaceTypeId {
        self.ty
    }

    /// Returns the receiver's ownership and mutation mode.
    pub const fn mode(&self) -> ReceiverMode {
        self.mode
    }
}

/// One source-independent callable declaration signature.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceCallableSignature {
    pub(crate) owner: InterfaceSymbolReference,
    pub(crate) callable_type: InterfaceTypeId,
    pub(crate) receiver: Option<InterfaceCallableReceiver>,
    pub(crate) parameters: Arc<[InterfaceSymbolReference]>,
    pub(crate) result: InterfaceTypeId,
}

impl InterfaceCallableSignature {
    /// Creates one durable callable signature in declaration parameter order.
    pub fn new(
        owner: InterfaceSymbolReference,
        callable_type: InterfaceTypeId,
        receiver: Option<InterfaceCallableReceiver>,
        parameters: impl IntoIterator<Item = InterfaceSymbolReference>,
        result: InterfaceTypeId,
    ) -> Self {
        Self {
            owner,
            callable_type,
            receiver,
            parameters: parameters.into_iter().collect(),
            result,
        }
    }

    /// Returns the callable declaration that owns this signature.
    pub const fn owner(&self) -> &InterfaceSymbolReference {
        &self.owner
    }

    /// Returns the canonical callable type.
    pub const fn callable_type(&self) -> InterfaceTypeId {
        self.callable_type
    }

    /// Returns the implicit receiver when one is declared.
    pub const fn receiver(&self) -> Option<&InterfaceCallableReceiver> {
        self.receiver.as_ref()
    }

    /// Returns ordinary parameters in declaration order.
    pub fn parameters(&self) -> &[InterfaceSymbolReference] {
        &self.parameters
    }

    /// Returns the callable's checked declared result type.
    pub const fn result(&self) -> InterfaceTypeId {
        self.result
    }
}

/// One source-independent generic declaration surface.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceGenericDeclaration {
    pub(crate) owner: InterfaceSymbolReference,
    pub(crate) parameters: Arc<[InterfaceSymbolReference]>,
}

impl InterfaceGenericDeclaration {
    /// Creates one durable generic declaration in parameter order.
    pub fn new(
        owner: InterfaceSymbolReference,
        parameters: impl IntoIterator<Item = InterfaceSymbolReference>,
    ) -> Self {
        Self {
            owner,
            parameters: parameters.into_iter().collect(),
        }
    }

    /// Returns the declaration that owns this generic surface.
    pub const fn owner(&self) -> &InterfaceSymbolReference {
        &self.owner
    }

    /// Returns generic parameters in declaration order.
    pub fn parameters(&self) -> &[InterfaceSymbolReference] {
        &self.parameters
    }
}

/// One callable parameter's source-independent default-template presence.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceCallableParameterDefault {
    pub(crate) parameter: InterfaceSymbolReference,
    pub(crate) is_present: bool,
}

/// The validated definition form of one exported predicate.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InterfacePredicateDefinitionState {
    /// The predicate has one checked definition template.
    Defined,
    /// A trait predicate member requires an implementation definition.
    Required,
    /// The predicate is an opaque trusted relation.
    OpaqueTrusted,
}

/// One source-independent predicate definition state.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfacePredicateDefinition {
    pub(crate) owner: InterfaceSymbolReference,
    pub(crate) state: InterfacePredicateDefinitionState,
}

impl InterfacePredicateDefinition {
    /// Creates one durable predicate definition fact.
    pub const fn new(
        owner: InterfaceSymbolReference,
        state: InterfacePredicateDefinitionState,
    ) -> Self {
        Self { owner, state }
    }

    /// Returns the predicate declaration that owns this fact.
    pub const fn owner(&self) -> &InterfaceSymbolReference {
        &self.owner
    }

    /// Returns the predicate's validated definition form.
    pub const fn state(&self) -> InterfacePredicateDefinitionState {
        self.state
    }
}

impl InterfaceCallableParameterDefault {
    /// Creates one durable callable parameter default fact.
    pub const fn new(parameter: InterfaceSymbolReference, is_present: bool) -> Self {
        Self {
            parameter,
            is_present,
        }
    }

    /// Returns the exact callable parameter.
    pub const fn parameter(&self) -> &InterfaceSymbolReference {
        &self.parameter
    }

    /// Returns whether the declaration provides a default template.
    pub const fn is_present(&self) -> bool {
        self.is_present
    }
}
