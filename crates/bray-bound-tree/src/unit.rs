use std::sync::Arc;

use bray_declarations::SyntaxAnchor;
use bray_source::SourceVersion;
use bray_symbols::{SymbolKey, SymbolKind};

use crate::ExactBoundNodeId;

/// Classifies an independently published checked semantic unit.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BoundUnitKind {
    /// A declared callable or lifecycle body.
    CallableBody,
    /// An anonymous callable signature, contracts, and body.
    AnonymousCallable,
    /// A parameter, field, or payload runtime default.
    RuntimeDefault,
    /// A constant definition template.
    ConstantTemplate,
    /// A predicate definition.
    PredicateDefinition,
    /// A declaration constraint expression.
    Constraint,
    /// A callable contract clause.
    ContractClause,
}

impl BoundUnitKind {
    const fn accepts_owner(self, owner: SymbolKind) -> bool {
        match self {
            Self::CallableBody => is_callable_body_owner(owner),
            Self::AnonymousCallable => false,
            Self::RuntimeDefault => matches!(
                owner,
                SymbolKind::CallableParameterDefaultProvider
                    | SymbolKind::StructFieldDefaultProvider
                    | SymbolKind::UnionPayloadDefaultProvider
            ),
            Self::ConstantTemplate => matches!(
                owner,
                SymbolKind::Constant
                    | SymbolKind::TraitConstantMember
                    | SymbolKind::TraitConstantFulfillment
            ),
            Self::PredicateDefinition => matches!(
                owner,
                SymbolKind::Predicate
                    | SymbolKind::TraitPredicateMember
                    | SymbolKind::TraitPredicateFulfillment
            ),
            Self::Constraint => owner.can_be_source_declared(),
            Self::ContractClause => {
                matches!(owner, SymbolKind::CallableContract) || is_callable_body_owner(owner)
            }
        }
    }
}

const fn is_callable_body_owner(owner: SymbolKind) -> bool {
    matches!(
        owner,
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

/// Identifies one exact published bound unit in a compilation snapshot.
///
/// Raw values are compilation-local handles. Persisted identities must use
/// [`BoundUnitKey`] instead.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BoundUnitId(u32);

impl BoundUnitId {
    /// Creates a compilation-local unit ID from its numeric representation.
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the compilation-local numeric representation.
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Converts this ID to a checked collection index for the current target.
    pub fn to_index(self) -> Option<usize> {
        usize::try_from(self.0).ok()
    }

    /// Creates an ID from a collection index when it fits the compact representation.
    pub fn try_from_index(index: usize) -> Option<Self> {
        u32::try_from(index).ok().map(Self)
    }

    /// Returns an exact node ID only when it belongs to this unit.
    pub fn checked_node<I: ExactBoundNodeId>(self, id: I) -> Option<I> {
        (id.unit() == self).then_some(id)
    }
}

/// Stable source correlation for a checked unit or bound node.
///
/// The syntax anchor identifies the source construct within a parsed snapshot. The source
/// version prevents equivalent ranges in different logical revisions from sharing a key.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BoundSourceAnchor {
    syntax: SyntaxAnchor,
    source_version: SourceVersion,
}

impl BoundSourceAnchor {
    /// Creates a versioned source anchor.
    pub const fn new(syntax: SyntaxAnchor, source_version: SourceVersion) -> Self {
        Self {
            syntax,
            source_version,
        }
    }

    /// Returns the stable declaration-level syntax anchor.
    pub const fn syntax(self) -> SyntaxAnchor {
        self.syntax
    }

    /// Returns the logical source revision containing the syntax anchor.
    pub const fn source_version(self) -> SourceVersion {
        self.source_version
    }
}

/// Stable identity data shared by declared bound-unit categories.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DeclaredBoundUnitKey {
    owner: SymbolKey,
    source: BoundSourceAnchor,
}

impl DeclaredBoundUnitKey {
    /// Returns the declared or synthesized surface symbol owning the unit.
    pub const fn owner(&self) -> &SymbolKey {
        &self.owner
    }

    /// Returns the exact source construct checked by the unit.
    pub const fn source(&self) -> BoundSourceAnchor {
        self.source
    }

    const fn new(owner: SymbolKey, source: BoundSourceAnchor) -> Self {
        Self { owner, source }
    }
}

/// Stable identity data for a nested anonymous callable unit.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AnonymousCallableUnitKey {
    enclosing: BoundUnitKey,
    source: BoundSourceAnchor,
}

impl AnonymousCallableUnitKey {
    /// Returns the independently checked unit containing the anonymous callable expression.
    pub const fn enclosing(&self) -> &BoundUnitKey {
        &self.enclosing
    }

    /// Returns the anonymous callable's exact source construct.
    pub const fn source(&self) -> BoundSourceAnchor {
        self.source
    }

    const fn new(enclosing: BoundUnitKey, source: BoundSourceAnchor) -> Self {
        Self { enclosing, source }
    }
}

/// Structured data forming a deterministic bound-unit key.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BoundUnitKeyData {
    /// A declared callable or lifecycle body.
    CallableBody(DeclaredBoundUnitKey),
    /// An anonymous callable nested in another semantic unit.
    AnonymousCallable(AnonymousCallableUnitKey),
    /// A parameter, field, or payload runtime default.
    RuntimeDefault(DeclaredBoundUnitKey),
    /// A constant definition template.
    ConstantTemplate(DeclaredBoundUnitKey),
    /// A predicate definition.
    PredicateDefinition(DeclaredBoundUnitKey),
    /// A declaration constraint expression.
    Constraint(DeclaredBoundUnitKey),
    /// A callable contract clause.
    ContractClause(DeclaredBoundUnitKey),
}

impl BoundUnitKeyData {
    /// Returns this key's exact semantic unit category.
    pub const fn kind(&self) -> BoundUnitKind {
        match self {
            Self::CallableBody(_) => BoundUnitKind::CallableBody,
            Self::AnonymousCallable(_) => BoundUnitKind::AnonymousCallable,
            Self::RuntimeDefault(_) => BoundUnitKind::RuntimeDefault,
            Self::ConstantTemplate(_) => BoundUnitKind::ConstantTemplate,
            Self::PredicateDefinition(_) => BoundUnitKind::PredicateDefinition,
            Self::Constraint(_) => BoundUnitKind::Constraint,
            Self::ContractClause(_) => BoundUnitKind::ContractClause,
        }
    }

    /// Returns the exact source construct checked by the unit.
    pub const fn source(&self) -> BoundSourceAnchor {
        match self {
            Self::CallableBody(key)
            | Self::RuntimeDefault(key)
            | Self::ConstantTemplate(key)
            | Self::PredicateDefinition(key)
            | Self::Constraint(key)
            | Self::ContractClause(key) => key.source(),
            Self::AnonymousCallable(key) => key.source(),
        }
    }
}

/// A cheaply cloned deterministic semantic construction key for a bound unit.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BoundUnitKey(Arc<BoundUnitKeyData>);

impl BoundUnitKey {
    /// Creates a key for a declared callable or lifecycle body.
    ///
    /// Returns `None` when the owner category cannot have an executable body.
    pub fn callable_body(owner: SymbolKey, source: BoundSourceAnchor) -> Option<Self> {
        Self::declared(
            BoundUnitKind::CallableBody,
            BoundUnitKeyData::CallableBody,
            owner,
            source,
        )
    }

    /// Creates a key for an anonymous callable nested in another semantic unit.
    pub fn anonymous_callable(enclosing: BoundUnitKey, source: BoundSourceAnchor) -> Self {
        Self(Arc::new(BoundUnitKeyData::AnonymousCallable(
            AnonymousCallableUnitKey::new(enclosing, source),
        )))
    }

    /// Creates a key for a parameter, field, or payload runtime default.
    ///
    /// Returns `None` unless the owner is a synthesized runtime-default provider.
    pub fn runtime_default(owner: SymbolKey, source: BoundSourceAnchor) -> Option<Self> {
        Self::declared(
            BoundUnitKind::RuntimeDefault,
            BoundUnitKeyData::RuntimeDefault,
            owner,
            source,
        )
    }

    /// Creates a key for a constant definition template.
    ///
    /// Returns `None` when the owner category cannot define a constant template.
    pub fn constant_template(owner: SymbolKey, source: BoundSourceAnchor) -> Option<Self> {
        Self::declared(
            BoundUnitKind::ConstantTemplate,
            BoundUnitKeyData::ConstantTemplate,
            owner,
            source,
        )
    }

    /// Creates a key for a predicate definition.
    ///
    /// Returns `None` when the owner category cannot define a predicate.
    pub fn predicate_definition(owner: SymbolKey, source: BoundSourceAnchor) -> Option<Self> {
        Self::declared(
            BoundUnitKind::PredicateDefinition,
            BoundUnitKeyData::PredicateDefinition,
            owner,
            source,
        )
    }

    /// Creates a key for a declaration constraint expression.
    ///
    /// Returns `None` unless the owner can be introduced by a source declaration.
    pub fn constraint(owner: SymbolKey, source: BoundSourceAnchor) -> Option<Self> {
        Self::declared(
            BoundUnitKind::Constraint,
            BoundUnitKeyData::Constraint,
            owner,
            source,
        )
    }

    /// Creates a key for a callable contract clause.
    ///
    /// Returns `None` when the owner category cannot declare a callable contract clause.
    pub fn contract_clause(owner: SymbolKey, source: BoundSourceAnchor) -> Option<Self> {
        Self::declared(
            BoundUnitKind::ContractClause,
            BoundUnitKeyData::ContractClause,
            owner,
            source,
        )
    }

    /// Returns the structured data forming this key.
    pub fn data(&self) -> &BoundUnitKeyData {
        &self.0
    }

    /// Returns this key's exact semantic unit category.
    pub fn kind(&self) -> BoundUnitKind {
        self.0.kind()
    }

    /// Returns the exact source construct checked by the unit.
    pub fn source(&self) -> BoundSourceAnchor {
        self.0.source()
    }

    fn declared(
        kind: BoundUnitKind,
        variant: impl FnOnce(DeclaredBoundUnitKey) -> BoundUnitKeyData,
        owner: SymbolKey,
        source: BoundSourceAnchor,
    ) -> Option<Self> {
        if !kind.accepts_owner(owner.kind()) {
            return None;
        }

        Some(Self(Arc::new(variant(DeclaredBoundUnitKey::new(
            owner, source,
        )))))
    }
}

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    use bray_symbols::SymbolKind;

    use super::{BoundUnitId, BoundUnitKey, BoundUnitKeyData, BoundUnitKind};
    use crate::test_support::{runtime_default_key, source_anchor, symbol_key};
    use crate::{BoundExpressionId, BoundPatternId};

    #[test]
    fn unit_ids_are_compact_and_use_checked_indices() {
        assert_eq!(size_of::<BoundUnitId>(), size_of::<u32>());

        let Some(id) = BoundUnitId::try_from_index(42) else {
            panic!("small bound unit ID must fit in u32");
        };

        assert_eq!(id.raw(), 42);
        assert_eq!(id.to_index(), Some(42));

        if usize::BITS > u32::BITS {
            assert_eq!(BoundUnitId::try_from_index((u32::MAX as usize) + 1), None);
        }
    }

    #[test]
    fn unit_ids_reject_nodes_from_another_unit() {
        let unit = BoundUnitId::new(1);
        let expression = BoundExpressionId::from_slot(unit, 0);
        let pattern = BoundPatternId::from_slot(BoundUnitId::new(2), 0);

        assert_eq!(unit.checked_node(expression), Some(expression));
        assert_eq!(unit.checked_node(pattern), None);
    }

    #[test]
    fn declared_and_nested_keys_preserve_exact_categories() {
        let body = valid_key(BoundUnitKey::callable_body(
            symbol_key(SymbolKind::Function, 3),
            source_anchor(),
        ));

        let nested = BoundUnitKey::anonymous_callable(body, source_anchor());

        assert_eq!(nested.kind(), BoundUnitKind::AnonymousCallable);

        let BoundUnitKeyData::AnonymousCallable(key) = nested.data() else {
            panic!("anonymous callable constructor must retain its exact key category");
        };

        assert_eq!(key.enclosing().kind(), BoundUnitKind::CallableBody);
        assert_eq!(key.source(), nested.source());
    }

    #[test]
    fn declared_key_constructors_preserve_all_semantic_categories() {
        let source = source_anchor();

        let keys = [
            (
                valid_key(BoundUnitKey::callable_body(
                    symbol_key(SymbolKind::Function, 0),
                    source,
                )),
                BoundUnitKind::CallableBody,
            ),
            (
                valid_key(BoundUnitKey::runtime_default(
                    runtime_default_key(1),
                    source,
                )),
                BoundUnitKind::RuntimeDefault,
            ),
            (
                valid_key(BoundUnitKey::constant_template(
                    symbol_key(SymbolKind::Constant, 2),
                    source,
                )),
                BoundUnitKind::ConstantTemplate,
            ),
            (
                valid_key(BoundUnitKey::predicate_definition(
                    symbol_key(SymbolKind::Predicate, 3),
                    source,
                )),
                BoundUnitKind::PredicateDefinition,
            ),
            (
                valid_key(BoundUnitKey::constraint(
                    symbol_key(SymbolKind::Function, 4),
                    source,
                )),
                BoundUnitKind::Constraint,
            ),
            (
                valid_key(BoundUnitKey::contract_clause(
                    symbol_key(SymbolKind::CallableContract, 5),
                    source,
                )),
                BoundUnitKind::ContractClause,
            ),
        ];

        for (key, expected_kind) in keys {
            assert_eq!(key.kind(), expected_kind);
            assert_eq!(key.source(), source);
        }
    }

    #[test]
    fn declared_key_constructors_reject_mismatched_owner_categories() {
        let source = source_anchor();

        assert_eq!(
            BoundUnitKey::callable_body(symbol_key(SymbolKind::Constant, 0), source),
            None
        );
        assert_eq!(
            BoundUnitKey::runtime_default(symbol_key(SymbolKind::CallableParameter, 1), source),
            None
        );
        assert_eq!(
            BoundUnitKey::predicate_definition(symbol_key(SymbolKind::Function, 2), source),
            None
        );
    }

    #[test]
    fn source_version_participates_in_unit_identity() {
        let source = source_anchor();

        let next_source = crate::BoundSourceAnchor::new(
            source.syntax(),
            bray_source::SourceVersion::new(source.source_version().raw() + 1),
        );

        let first = valid_key(BoundUnitKey::predicate_definition(
            symbol_key(SymbolKind::Predicate, 4),
            source,
        ));
        let next = valid_key(BoundUnitKey::predicate_definition(
            symbol_key(SymbolKind::Predicate, 4),
            next_source,
        ));

        assert_ne!(first, next);
    }

    #[test]
    fn unit_keys_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<BoundUnitKey>();
    }

    fn valid_key(key: Option<BoundUnitKey>) -> BoundUnitKey {
        match key {
            Some(key) => key,
            None => panic!("test owner must support the requested bound unit category"),
        }
    }
}
