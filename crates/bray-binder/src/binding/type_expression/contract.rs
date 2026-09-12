use bray_symbols::{
    AnySymbolId, CallableAbi, CallableConstness, CallableExecution, CallableTrust,
    GenericTypeParameterSymbolId, ModuleSymbolId, ReceiverMode, SelfTypeContext, SymbolName,
};

/// The lexical semantic scope used while binding a declaration's type expressions.
#[derive(Clone)]
pub struct TypeExpressionScope {
    pub(super) owner: AnySymbolId,
    pub(super) module: Option<ModuleSymbolId>,
    pub(super) type_parameters: Vec<TypeParameterBinding>,
    pub(super) self_type: Option<SelfTypeContext>,
}

impl TypeExpressionScope {
    /// Creates an exact lexical type-expression scope.
    pub fn new(
        owner: AnySymbolId,
        module: Option<ModuleSymbolId>,
        type_parameters: impl IntoIterator<Item = TypeParameterBinding>,
        self_type: Option<SelfTypeContext>,
    ) -> Self {
        Self {
            owner,
            module,
            type_parameters: type_parameters.into_iter().collect(),
            self_type,
        }
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallableTypeQualifiers {
    pub(super) execution_properties: std::collections::BTreeSet<bray_symbols::ExecutionProperty>,
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
            execution_properties: std::collections::BTreeSet::new(),
            constness,
            execution,
            trust,
            abi,
            receiver_mode,
        }
    }

    /// Retains declared execution promises for later implementation checking.
    pub fn with_execution_properties(
        mut self,
        properties: impl IntoIterator<Item = bray_symbols::ExecutionProperty>,
    ) -> Self {
        self.execution_properties = properties.into_iter().collect();

        self
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
