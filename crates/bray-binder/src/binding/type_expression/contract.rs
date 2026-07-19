use bray_symbols::{
    CallableAbi, CallableConstness, CallableExecution, CallableTrust,
    GenericConstParameterSymbolId, GenericTypeParameterSymbolId, ModuleSymbolId, ReceiverMode,
    SelfTypeContext, SymbolName,
};

/// The lexical semantic scope used while binding a declaration's type expressions.
pub struct TypeExpressionScope {
    pub(super) module: Option<ModuleSymbolId>,
    pub(super) type_parameters: Vec<TypeParameterBinding>,
    pub(super) const_parameters: Vec<ConstParameterBinding>,
    pub(super) self_type: Option<SelfTypeContext>,
}

impl TypeExpressionScope {
    /// Creates an exact lexical type-expression scope.
    pub fn new(
        module: Option<ModuleSymbolId>,
        type_parameters: impl IntoIterator<Item = TypeParameterBinding>,
        const_parameters: impl IntoIterator<Item = ConstParameterBinding>,
        self_type: Option<SelfTypeContext>,
    ) -> Self {
        Self {
            module,
            type_parameters: type_parameters.into_iter().collect(),
            const_parameters: const_parameters.into_iter().collect(),
            self_type,
        }
    }
}

/// One lexical generic constant parameter visible while binding a declaration surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConstParameterBinding {
    pub(super) name: SymbolName,
    pub(super) symbol: GenericConstParameterSymbolId,
}

impl ConstParameterBinding {
    /// Creates a lexical generic constant parameter binding.
    pub const fn new(name: SymbolName, symbol: GenericConstParameterSymbolId) -> Self {
        Self { name, symbol }
    }

    /// Returns the declared parameter name.
    pub const fn name(&self) -> &SymbolName {
        &self.name
    }

    /// Returns the exact generic constant parameter symbol.
    pub const fn symbol(&self) -> GenericConstParameterSymbolId {
        self.symbol
    }
}

/// One lexical generic type parameter visible while binding a declaration surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeParameterBinding {
    pub(super) name: SymbolName,
    pub(super) symbol: GenericTypeParameterSymbolId,
}

impl TypeParameterBinding {
    /// Creates a lexical generic type parameter binding.
    pub const fn new(name: SymbolName, symbol: GenericTypeParameterSymbolId) -> Self {
        Self { name, symbol }
    }

    /// Returns the declared parameter name.
    pub const fn name(&self) -> &SymbolName {
        &self.name
    }

    /// Returns the exact generic type parameter symbol.
    pub const fn symbol(&self) -> GenericTypeParameterSymbolId {
        self.symbol
    }
}

/// Caller-visible callable type qualifiers derived from declaration modifiers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CallableTypeQualifiers {
    pub(super) constness: CallableConstness,
    pub(super) execution: CallableExecution,
    pub(super) trust: CallableTrust,
    pub(super) abi: CallableAbi,
    pub(super) receiver_mode: Option<ReceiverMode>,
}

impl CallableTypeQualifiers {
    /// Creates an exact callable qualifier set.
    pub const fn new(
        constness: CallableConstness,
        execution: CallableExecution,
        trust: CallableTrust,
        abi: CallableAbi,
        receiver_mode: Option<ReceiverMode>,
    ) -> Self {
        Self {
            constness,
            execution,
            trust,
            abi,
            receiver_mode,
        }
    }

    /// Returns ordinary safe synchronous runtime qualifiers.
    pub const fn ordinary() -> Self {
        Self::new(
            CallableConstness::Runtime,
            CallableExecution::Synchronous,
            CallableTrust::Safe,
            CallableAbi::Bray,
            None,
        )
    }
}
