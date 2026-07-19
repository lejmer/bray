use std::sync::Arc;

use bray_base::shared_slice;

use crate::{CallableParameterSymbolId, ReceiverParameterSymbolId, TypeExpressionTemplate, TypeId};

/// The ownership and mutation authority carried by an implicit receiver.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ReceiverMode {
    /// Shared observation without mutation or consumption.
    Shared,
    /// Exclusive mutation without consumption.
    Mutable,
    /// Consumption without mutable local authority.
    Consuming,
    /// Consumption with mutable local authority.
    ConsumingMutable,
}

/// One callable parameter and its checked declared type in signature order.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableParameterSignature {
    parameter: CallableParameterSymbolId,
    ty: TypeId,
}

impl CallableParameterSignature {
    /// Creates one checked callable parameter signature entry.
    pub const fn new(parameter: CallableParameterSymbolId, ty: TypeId) -> Self {
        Self { parameter, ty }
    }

    /// Returns the exact parameter symbol.
    pub const fn parameter(self) -> CallableParameterSymbolId {
        self.parameter
    }

    /// Returns the checked declared parameter type.
    pub const fn ty(self) -> TypeId {
        self.ty
    }
}

/// An implicit receiver parameter and its checked declared type.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ReceiverParameterSignature {
    parameter: ReceiverParameterSymbolId,
    ty: TypeId,
    mode: ReceiverMode,
}

impl ReceiverParameterSignature {
    /// Creates one checked receiver signature entry.
    pub const fn new(parameter: ReceiverParameterSymbolId, ty: TypeId, mode: ReceiverMode) -> Self {
        Self {
            parameter,
            ty,
            mode,
        }
    }

    /// Returns the exact receiver parameter symbol.
    pub const fn parameter(self) -> ReceiverParameterSymbolId {
        self.parameter
    }

    /// Returns the checked declared receiver type.
    pub const fn ty(self) -> TypeId {
        self.ty
    }

    /// Returns the receiver's ownership and mutation mode.
    pub const fn mode(self) -> ReceiverMode {
        self.mode
    }
}

/// The immutable checked declaration signature of one callable symbol.
///
/// The canonical callable type carries caller-visible type identity. The entries retained here
/// preserve the exact declaration parameter identities needed by binding and diagnostics.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableSignature {
    callable_type: TypeId,
    receiver: Option<ReceiverParameterSignature>,
    parameters: Arc<[CallableParameterSignature]>,
    result: TypeId,
}

/// The immutable declaration signature template of one callable symbol.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableSignatureTemplate {
    callable_type: TypeExpressionTemplate,
    receiver: Option<ReceiverParameterSignature>,
    parameters: Arc<[CallableParameterSymbolId]>,
    result: TypeExpressionTemplate,
}

impl CallableSignatureTemplate {
    /// Creates a callable signature template in declaration order.
    pub fn new(
        callable_type: TypeExpressionTemplate,
        receiver: Option<ReceiverParameterSignature>,
        parameters: impl IntoIterator<Item = CallableParameterSymbolId>,
        result: TypeExpressionTemplate,
    ) -> Self {
        Self {
            callable_type,
            receiver,
            parameters: shared_slice(parameters),
            result,
        }
    }

    /// Returns the complete callable type template.
    pub const fn callable_type(&self) -> &TypeExpressionTemplate {
        &self.callable_type
    }

    /// Returns the implicit receiver when this callable has one.
    pub const fn receiver(&self) -> Option<ReceiverParameterSignature> {
        self.receiver
    }

    /// Returns ordinary parameters in declaration order.
    pub fn parameters(&self) -> &[CallableParameterSymbolId] {
        &self.parameters
    }

    /// Returns the declared result type template.
    pub const fn result(&self) -> &TypeExpressionTemplate {
        &self.result
    }
}

impl CallableSignature {
    /// Creates a checked callable signature in declaration order.
    pub fn new(
        callable_type: TypeId,
        receiver: Option<ReceiverParameterSignature>,
        parameters: impl IntoIterator<Item = CallableParameterSignature>,
        result: TypeId,
    ) -> Self {
        Self {
            callable_type,
            receiver,
            parameters: shared_slice(parameters),
            result,
        }
    }

    /// Returns the canonical callable type.
    pub const fn callable_type(&self) -> TypeId {
        self.callable_type
    }

    /// Returns the implicit receiver when this callable has one.
    pub const fn receiver(&self) -> Option<ReceiverParameterSignature> {
        self.receiver
    }

    /// Returns ordinary parameters in declaration order.
    pub fn parameters(&self) -> &[CallableParameterSignature] {
        &self.parameters
    }

    /// Returns the checked result type.
    pub const fn result(&self) -> TypeId {
        self.result
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        CallableParameterSignature, CallableParameterSymbolId, CallableSignature,
        SemanticValueStore, SymbolId, TypeData,
    };

    #[test]
    fn callable_signatures_preserve_parameter_order_and_identity() {
        let first = CallableParameterSymbolId::from_symbol_id(SymbolId::new(2));
        let second = CallableParameterSymbolId::from_symbol_id(SymbolId::new(3));

        let Ok(store) = SemanticValueStore::try_new() else {
            panic!("semantic value store identity must be available");
        };

        let Ok(ty) = store.intern_type(TypeData::Error) else {
            panic!("error type must be valid");
        };

        let signature = CallableSignature::new(
            ty,
            None,
            [
                CallableParameterSignature::new(first, ty),
                CallableParameterSignature::new(second, ty),
            ],
            ty,
        );

        assert_eq!(
            signature.parameters(),
            &[
                CallableParameterSignature::new(first, ty),
                CallableParameterSignature::new(second, ty),
            ]
        );

        assert_eq!(signature.callable_type(), ty);
        assert_eq!(signature.result(), ty);
    }
}
