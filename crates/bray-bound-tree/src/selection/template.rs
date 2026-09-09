use std::sync::Arc;

use bray_base::shared_slice;
use bray_symbols::{
    CallableContractTemplate, CallableDefinitionId, CallableParameterDefaultProviderSymbolId,
    CallableParameterSymbolId, CallableSignatureTemplate, GenericArgumentTemplate,
    GenericDeclarationTemplate, SymbolKey, UnevaluatedDefaultTemplate,
};

/// One declaration parameter and its unevaluated default surface.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableParameterDefaultTemplate {
    parameter: CallableParameterSymbolId,
    value: UnevaluatedDefaultTemplate,
    provider: Option<CallableParameterDefaultProviderSymbolId>,
}

impl CallableParameterDefaultTemplate {
    /// Creates one declaration parameter default entry.
    pub const fn new(
        parameter: CallableParameterSymbolId,
        value: UnevaluatedDefaultTemplate,
        provider: Option<CallableParameterDefaultProviderSymbolId>,
    ) -> Self {
        Self {
            parameter,
            value,
            provider,
        }
    }

    /// Returns the declaration parameter.
    pub const fn parameter(self) -> CallableParameterSymbolId {
        self.parameter
    }

    /// Returns the unevaluated default expression surface.
    pub const fn value(self) -> UnevaluatedDefaultTemplate {
        self.value
    }

    /// Returns the declaration-owned provider for a present runtime default.
    pub const fn provider(self) -> Option<CallableParameterDefaultProviderSymbolId> {
        self.provider
    }
}

/// One callable declaration with bound generic arguments awaiting semantic resolution.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableDeclarationTemplate {
    data: Arc<CallableDeclarationTemplateData>,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct CallableDeclarationTemplateData {
    key: SymbolKey,
    definition: CallableDefinitionId,
    signature: CallableSignatureTemplate,
    contract: CallableContractTemplate,
    generic: GenericDeclarationTemplate,
    generic_arguments: Arc<[GenericArgumentTemplate]>,
    defaults: Arc<[CallableParameterDefaultTemplate]>,
    contextual_self: Option<(
        bray_symbols::SelfTypeContext,
        bray_symbols::TypeExpressionTemplate,
    )>,
}

impl CallableDeclarationTemplate {
    /// Creates a callable declaration template from its complete semantic inputs.
    pub fn new(
        key: SymbolKey,
        definition: CallableDefinitionId,
        signature: CallableSignatureTemplate,
        contract: CallableContractTemplate,
        generic: GenericDeclarationTemplate,
        generic_arguments: impl IntoIterator<Item = GenericArgumentTemplate>,
    ) -> Self {
        Self {
            data: Arc::new(CallableDeclarationTemplateData {
                key,
                definition,
                signature,
                contract,
                generic,
                generic_arguments: shared_slice(generic_arguments),
                defaults: Arc::new([]),
                contextual_self: None,
            }),
        }
    }

    /// Supplies declaration-owned parameter defaults in parameter order.
    pub fn with_defaults(
        mut self,
        defaults: impl IntoIterator<Item = CallableParameterDefaultTemplate>,
    ) -> Self {
        Arc::make_mut(&mut self.data).defaults = shared_slice(defaults);

        self
    }

    /// Supplies the selected subject replacing declaration-local `Self` during instantiation.
    pub fn with_contextual_self(
        mut self,
        context: bray_symbols::SelfTypeContext,
        replacement: bray_symbols::TypeExpressionTemplate,
    ) -> Self {
        Arc::make_mut(&mut self.data).contextual_self = Some((context, replacement));

        self
    }

    /// Returns the selected subject replacing declaration-local `Self` during instantiation.
    pub fn contextual_self(
        &self,
    ) -> Option<&(
        bray_symbols::SelfTypeContext,
        bray_symbols::TypeExpressionTemplate,
    )> {
        self.data.contextual_self.as_ref()
    }

    /// Returns the declaration's stable semantic key.
    pub fn key(&self) -> &SymbolKey {
        &self.data.key
    }

    /// Returns the exact callable declaration.
    pub fn definition(&self) -> CallableDefinitionId {
        self.data.definition
    }

    /// Returns the complete unevaluated callable signature.
    pub fn signature(&self) -> &CallableSignatureTemplate {
        &self.data.signature
    }

    /// Returns source or imported contract clauses retained for semantic checking.
    pub fn contract(&self) -> &CallableContractTemplate {
        &self.data.contract
    }

    /// Returns generic parameters and constraint templates.
    pub fn generic(&self) -> &GenericDeclarationTemplate {
        &self.data.generic
    }

    /// Returns bound generic arguments in declaration parameter order.
    pub fn generic_arguments(&self) -> &[GenericArgumentTemplate] {
        &self.data.generic_arguments
    }

    /// Returns parameter defaults in declaration order.
    pub fn defaults(&self) -> &[CallableParameterDefaultTemplate] {
        &self.data.defaults
    }
}
