use std::sync::Arc;

use bray_base::shared_slice;

use crate::{
    CallableConstness, CallableExecution, CallableParameterSymbolId, ReceiverParameterSymbolId,
    SemanticValueStore, SemanticValueStoreError, TypeData, TypeExpressionTemplate, TypeId,
};

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
    has_body: bool,
}

/// A canonical callable signature template could not expose its parameter type templates.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CallableSignatureTemplateError {
    /// The semantic value store rejected the callable type identity.
    SemanticValue(SemanticValueStoreError),
    /// The signature's canonical type is not callable.
    InvalidCallableType,
    /// The callable type and declaration parameter identities have different lengths.
    ParameterCountMismatch,
    /// The requested ordinal does not identify the exact declaration parameter.
    ParameterIdentityMismatch,
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
            has_body: false,
        }
    }

    /// Returns a signature with its declaration-body presence set.
    pub fn with_body(mut self, has_body: bool) -> Self {
        self.has_body = has_body;

        self
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

    /// Returns whether the declaration supplies an executable body.
    pub const fn has_body(&self) -> bool {
        self.has_body
    }

    /// Returns whether calls through this signature are allowed in constant contexts.
    pub fn constness(
        &self,
        semantic_values: &SemanticValueStore,
    ) -> Result<CallableConstness, CallableSignatureTemplateError> {
        match self.callable_type() {
            TypeExpressionTemplate::Callable(callable) => Ok(callable.constness()),
            TypeExpressionTemplate::Resolved(ty) => {
                let data = semantic_values
                    .type_data(*ty)
                    .map_err(CallableSignatureTemplateError::SemanticValue)?;

                let TypeData::Callable(callable) = data.as_ref() else {
                    return Err(CallableSignatureTemplateError::InvalidCallableType);
                };

                Ok(callable.constness())
            }
            _ => Err(CallableSignatureTemplateError::InvalidCallableType),
        }
    }

    /// Returns the callable execution mode carried by this signature.
    pub fn execution(
        &self,
        semantic_values: &SemanticValueStore,
    ) -> Result<CallableExecution, CallableSignatureTemplateError> {
        match self.callable_type() {
            TypeExpressionTemplate::Callable(callable) => Ok(callable.execution()),
            TypeExpressionTemplate::Resolved(ty) => {
                let data = semantic_values
                    .type_data(*ty)
                    .map_err(CallableSignatureTemplateError::SemanticValue)?;

                let TypeData::Callable(callable) = data.as_ref() else {
                    return Err(CallableSignatureTemplateError::InvalidCallableType);
                };

                Ok(callable.execution())
            }
            _ => Err(CallableSignatureTemplateError::InvalidCallableType),
        }
    }

    /// Returns parameter type templates in declaration parameter order.
    pub fn parameter_type_templates(
        &self,
        semantic_values: &SemanticValueStore,
    ) -> Result<Vec<TypeExpressionTemplate>, CallableSignatureTemplateError> {
        // Callers own the returned templates. Recursive template storage remains Arc-shared.
        let types = match self.callable_type() {
            TypeExpressionTemplate::Callable(callable) => callable
                .parameters()
                .iter()
                .map(|parameter| parameter.ty().clone())
                .collect::<Vec<_>>(),
            TypeExpressionTemplate::Resolved(ty) => {
                let data = semantic_values
                    .type_data(*ty)
                    .map_err(CallableSignatureTemplateError::SemanticValue)?;

                let TypeData::Callable(callable) = data.as_ref() else {
                    return Err(CallableSignatureTemplateError::InvalidCallableType);
                };

                callable
                    .parameters()
                    .iter()
                    .map(|parameter| TypeExpressionTemplate::Resolved(parameter.ty()))
                    .collect()
            }
            _ => return Err(CallableSignatureTemplateError::InvalidCallableType),
        };

        if types.len() != self.parameters().len() {
            return Err(CallableSignatureTemplateError::ParameterCountMismatch);
        }

        Ok(types)
    }

    /// Returns caller-visible parameter names in declaration order.
    pub fn parameter_names(
        &self,
        semantic_values: &SemanticValueStore,
    ) -> Result<Vec<crate::CallableParameterName>, CallableSignatureTemplateError> {
        // Callers own the returned names. Canonical spellings remain Arc-shared.
        let names = match self.callable_type() {
            TypeExpressionTemplate::Callable(callable) => callable
                .parameters()
                .iter()
                .map(|parameter| parameter.name().clone())
                .collect::<Vec<_>>(),
            TypeExpressionTemplate::Resolved(ty) => {
                let data = semantic_values
                    .type_data(*ty)
                    .map_err(CallableSignatureTemplateError::SemanticValue)?;

                let TypeData::Callable(callable) = data.as_ref() else {
                    return Err(CallableSignatureTemplateError::InvalidCallableType);
                };

                callable
                    .parameters()
                    .iter()
                    .map(|parameter| parameter.name().clone())
                    .collect()
            }
            _ => return Err(CallableSignatureTemplateError::InvalidCallableType),
        };

        if names.len() != self.parameters().len() {
            return Err(CallableSignatureTemplateError::ParameterCountMismatch);
        }

        Ok(names)
    }

    /// Returns one parameter's type template by exact declaration identity and ordinal.
    pub fn parameter_type_template(
        &self,
        parameter: CallableParameterSymbolId,
        ordinal: u32,
        semantic_values: &SemanticValueStore,
    ) -> Result<TypeExpressionTemplate, CallableSignatureTemplateError> {
        let ordinal = usize::try_from(ordinal)
            .map_err(|_| CallableSignatureTemplateError::ParameterIdentityMismatch)?;

        if self.parameters().get(ordinal).copied() != Some(parameter) {
            return Err(CallableSignatureTemplateError::ParameterIdentityMismatch);
        }

        // The caller needs an independent template. Recursive template storage remains Arc-shared.
        match self.callable_type() {
            TypeExpressionTemplate::Callable(callable) => {
                if callable.parameters().len() != self.parameters().len() {
                    return Err(CallableSignatureTemplateError::ParameterCountMismatch);
                }

                callable
                    .parameters()
                    .get(ordinal)
                    .map(|parameter| parameter.ty().clone())
                    .ok_or(CallableSignatureTemplateError::ParameterIdentityMismatch)
            }
            TypeExpressionTemplate::Resolved(ty) => {
                let data = semantic_values
                    .type_data(*ty)
                    .map_err(CallableSignatureTemplateError::SemanticValue)?;

                let TypeData::Callable(callable) = data.as_ref() else {
                    return Err(CallableSignatureTemplateError::InvalidCallableType);
                };

                if callable.parameters().len() != self.parameters().len() {
                    return Err(CallableSignatureTemplateError::ParameterCountMismatch);
                }

                callable
                    .parameters()
                    .get(ordinal)
                    .map(|parameter| TypeExpressionTemplate::Resolved(parameter.ty()))
                    .ok_or(CallableSignatureTemplateError::ParameterIdentityMismatch)
            }
            _ => Err(CallableSignatureTemplateError::InvalidCallableType),
        }
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

    /// Transforms every type in the signature while preserving declaration identities and modes.
    pub fn try_map_types<E>(
        self,
        mut transform: impl FnMut(TypeId) -> Result<TypeId, E>,
    ) -> Result<Self, E> {
        let callable_type = transform(self.callable_type)?;

        let receiver = self
            .receiver
            .map(|receiver| {
                transform(receiver.ty()).map(|ty| {
                    ReceiverParameterSignature::new(receiver.parameter(), ty, receiver.mode())
                })
            })
            .transpose()?;

        let parameters = self
            .parameters
            .iter()
            .map(|parameter| {
                transform(parameter.ty())
                    .map(|ty| CallableParameterSignature::new(parameter.parameter(), ty))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let result = transform(self.result)?;

        Ok(Self::new(callable_type, receiver, parameters, result))
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
    use std::sync::Arc;

    use crate::{
        CallableAbi, CallableConstness, CallableDependencyContracts, CallableParameterData,
        CallableParameterMode, CallableParameterName, CallableParameterSignature,
        CallableParameterSymbolId, CallableParameterTypeTemplate, CallablePosition,
        CallableSignature, CallableSignatureTemplate, CallableSignatureTemplateError,
        CallableTrust, CallableTypeData, CallableTypeTemplate, DependencyContractTemplateData,
        ReceiverMode, ReceiverParameterSignature, ReceiverParameterSymbolId, SemanticValueStore,
        SymbolId, TypeData, TypeExpressionTemplate,
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

    #[test]
    fn callable_signature_type_mapping_preserves_declaration_structure() {
        let receiver = ReceiverParameterSymbolId::from_symbol_id(SymbolId::new(1));
        let parameter = CallableParameterSymbolId::from_symbol_id(SymbolId::new(2));

        let Ok(store) = SemanticValueStore::try_new() else {
            panic!("semantic value store identity must be available");
        };

        let Ok(source) = store.intern_type(TypeData::Error) else {
            panic!("error type must be valid");
        };

        let Ok(mapped) = store.intern_type(TypeData::Tuple(Arc::from([]))) else {
            panic!("empty tuple type must be valid");
        };

        let signature = CallableSignature::new(
            source,
            Some(ReceiverParameterSignature::new(
                receiver,
                source,
                ReceiverMode::ConsumingMutable,
            )),
            [CallableParameterSignature::new(parameter, source)],
            source,
        );

        let transformed = signature
            .try_map_types(|ty| {
                assert_eq!(ty, source);

                Ok::<_, ()>(mapped)
            })
            .unwrap_or_else(|()| panic!("signature type mapping must succeed"));

        assert_eq!(transformed.callable_type(), mapped);
        assert_eq!(transformed.result(), mapped);

        assert_eq!(
            transformed.receiver(),
            Some(ReceiverParameterSignature::new(
                receiver,
                mapped,
                ReceiverMode::ConsumingMutable,
            ))
        );

        assert_eq!(
            transformed.parameters(),
            &[CallableParameterSignature::new(parameter, mapped)]
        );
    }

    #[test]
    fn callable_signature_templates_expose_canonical_parameter_types() {
        let Ok(store) = SemanticValueStore::try_new() else {
            panic!("semantic value store identity must be available");
        };

        let Ok(ty) = store.intern_type(TypeData::Error) else {
            panic!("error type must be valid");
        };

        let Ok(dependency) =
            store.intern_dependency_contract_template(DependencyContractTemplateData::new([]))
        else {
            panic!("empty dependency contract must be valid");
        };

        let Some(name) = CallableParameterName::try_new("value") else {
            panic!("test parameter name must be valid");
        };

        let callable = CallableTypeData::new(
            [CallableParameterData::new(
                name,
                CallablePosition::NamedOnly,
                CallableParameterMode::Immutable,
                ty,
            )],
            ty,
            CallableConstness::Runtime,
            CallableTrust::Safe,
            CallableAbi::Bray,
            CallableDependencyContracts::synchronous(dependency),
        );

        let Ok(callable) = store.intern_type(TypeData::Callable(callable)) else {
            panic!("callable type must be valid");
        };

        let parameter = CallableParameterSymbolId::from_symbol_id(SymbolId::new(2));

        let signature = CallableSignatureTemplate::new(
            TypeExpressionTemplate::Resolved(callable),
            None,
            [parameter],
            TypeExpressionTemplate::Resolved(ty),
        );

        assert_eq!(
            signature.parameter_type_templates(&store),
            Ok(vec![TypeExpressionTemplate::Resolved(ty)])
        );

        assert_eq!(
            signature.parameter_type_template(parameter, 0, &store),
            Ok(TypeExpressionTemplate::Resolved(ty))
        );
    }

    #[test]
    fn callable_signature_templates_expose_one_exact_unresolved_parameter_type() {
        let Ok(store) = SemanticValueStore::try_new() else {
            panic!("semantic value store identity must be available");
        };

        let Ok(ty) = store.intern_type(TypeData::Error) else {
            panic!("error type must be valid");
        };

        let Ok(dependency) =
            store.intern_dependency_contract_template(DependencyContractTemplateData::new([]))
        else {
            panic!("empty dependency contract must be valid");
        };

        let Some(first_name) = CallableParameterName::try_new("first") else {
            panic!("test parameter name must be valid");
        };

        let Some(second_name) = CallableParameterName::try_new("second") else {
            panic!("test parameter name must be valid");
        };

        let first = CallableParameterSymbolId::from_symbol_id(SymbolId::new(2));
        let second = CallableParameterSymbolId::from_symbol_id(SymbolId::new(3));
        let first_type = TypeExpressionTemplate::Resolved(ty);
        let second_type = TypeExpressionTemplate::Slice(Arc::new(first_type.clone()));

        let callable = CallableTypeTemplate::new(
            [
                CallableParameterTypeTemplate::new(
                    first_name,
                    CallablePosition::NamedOnly,
                    CallableParameterMode::Immutable,
                    first_type,
                ),
                CallableParameterTypeTemplate::new(
                    second_name,
                    CallablePosition::NamedOnly,
                    CallableParameterMode::Immutable,
                    second_type.clone(),
                ),
            ],
            TypeExpressionTemplate::Resolved(ty),
            CallableConstness::Runtime,
            CallableTrust::Safe,
            CallableAbi::Bray,
            CallableDependencyContracts::synchronous(dependency),
        );

        let signature = CallableSignatureTemplate::new(
            TypeExpressionTemplate::Callable(callable),
            None,
            [first, second],
            TypeExpressionTemplate::Resolved(ty),
        );

        assert_eq!(
            signature.parameter_type_template(second, 1, &store),
            Ok(second_type)
        );

        assert_eq!(
            signature.parameter_type_template(first, 1, &store),
            Err(CallableSignatureTemplateError::ParameterIdentityMismatch)
        );
    }
}
