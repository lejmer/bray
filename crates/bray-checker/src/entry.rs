use bray_bound_tree::{BoundSourceAnchor, BoundUnitKey, BoundUnitKind};
use bray_symbols::{
    AnonymousCallableParameterSymbolId, AnonymousCallableSymbolId, AnySymbolId,
    CallableContractClauseKind, PostconditionResultSymbolId, SymbolKey,
};

/// Declaration-owned inputs that select the semantic context of one checked unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclaredUnitCheckEntry {
    key: BoundUnitKey,
    owner: AnySymbolId,
    declaration: AnySymbolId,
}

impl DeclaredUnitCheckEntry {
    /// Creates an entry context for one declaration-owned bound unit.
    pub fn new(key: BoundUnitKey, owner: AnySymbolId, declaration: AnySymbolId) -> Self {
        Self {
            key,
            owner,
            declaration,
        }
    }

    /// Returns the stable semantic unit key.
    pub const fn key(&self) -> &BoundUnitKey {
        &self.key
    }

    /// Returns the declaration or synthesized declaration-surface owner.
    pub fn owner_key(&self) -> &SymbolKey {
        self.key.declared_owner()
    }

    /// Returns the exact symbol that owns the semantic unit.
    pub const fn owner(&self) -> AnySymbolId {
        self.owner
    }

    /// Returns the declaration whose semantic context is visible at entry.
    pub const fn declaration(&self) -> AnySymbolId {
        self.declaration
    }

    /// Returns the source construct that establishes the entry context.
    pub fn source(&self) -> BoundSourceAnchor {
        self.key.source()
    }
}

/// Inputs that establish one nested anonymous callable boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnonymousCallableCheckEntry {
    key: BoundUnitKey,
    callable: AnonymousCallableSymbolId,
    parameters: Box<[AnonymousCallableParameterSymbolId]>,
}

impl AnonymousCallableCheckEntry {
    /// Creates an anonymous-callable entry context with parameters in declaration order.
    pub fn new(
        key: BoundUnitKey,
        callable: AnonymousCallableSymbolId,
        parameters: impl IntoIterator<Item = AnonymousCallableParameterSymbolId>,
    ) -> Self {
        Self {
            key,
            callable,
            parameters: parameters.into_iter().collect(),
        }
    }

    /// Returns the stable anonymous-callable unit key.
    pub const fn key(&self) -> &BoundUnitKey {
        &self.key
    }

    /// Returns the local anonymous-callable symbol.
    pub const fn callable(&self) -> AnonymousCallableSymbolId {
        self.callable
    }

    /// Returns anonymous parameters in declaration order.
    pub fn parameters(&self) -> &[AnonymousCallableParameterSymbolId] {
        &self.parameters
    }
}

/// Inputs that establish one callable contract-clause context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractClauseCheckEntry {
    declaration: DeclaredUnitCheckEntry,
    kind: CallableContractClauseKind,
    result: Option<PostconditionResultSymbolId>,
}

impl ContractClauseCheckEntry {
    /// Creates a contract-clause entry context.
    pub fn new(
        declaration: DeclaredUnitCheckEntry,
        kind: CallableContractClauseKind,
        result: Option<PostconditionResultSymbolId>,
    ) -> Self {
        Self {
            declaration,
            kind,
            result,
        }
    }

    /// Returns the declaration-owned entry data.
    pub const fn declaration(&self) -> &DeclaredUnitCheckEntry {
        &self.declaration
    }

    /// Returns the exact callable contract-clause category.
    pub const fn kind(&self) -> CallableContractClauseKind {
        self.kind
    }

    /// Returns the normal-result binding available to a value-producing postcondition.
    pub const fn result(&self) -> Option<PostconditionResultSymbolId> {
        self.result
    }
}

/// The category-specific semantic context active at one bound-unit entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UnitCheckEntryContext {
    /// A declared callable or lifecycle body.
    CallableBody(DeclaredUnitCheckEntry),
    /// An anonymous callable nested in another semantic unit.
    AnonymousCallable(AnonymousCallableCheckEntry),
    /// A parameter, field, or payload runtime default.
    RuntimeDefault(DeclaredUnitCheckEntry),
    /// A constant definition template.
    ConstantTemplate(DeclaredUnitCheckEntry),
    /// A predicate definition.
    PredicateDefinition(DeclaredUnitCheckEntry),
    /// A declaration constraint expression.
    Constraint(DeclaredUnitCheckEntry),
    /// A callable contract clause.
    ContractClause(ContractClauseCheckEntry),
}

impl UnitCheckEntryContext {
    /// Returns the semantic unit category selected by this context.
    pub fn kind(&self) -> BoundUnitKind {
        self.key().kind()
    }

    /// Returns the exact bound-unit key selected by this context.
    pub const fn key(&self) -> &BoundUnitKey {
        match self {
            Self::CallableBody(entry)
            | Self::RuntimeDefault(entry)
            | Self::ConstantTemplate(entry)
            | Self::PredicateDefinition(entry)
            | Self::Constraint(entry) => entry.key(),
            Self::AnonymousCallable(entry) => entry.key(),
            Self::ContractClause(entry) => entry.declaration().key(),
        }
    }
}
