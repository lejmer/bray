use bray_bound_tree::BoundUnitKey;
use bray_source::SourceId;
use bray_symbols::{AnySymbolId, SymbolFactKind};

/// The exact compilation-local key for one symbol-owned semantic fact.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SymbolFactKey {
    symbol: AnySymbolId,
    kind: SymbolFactKind,
}

impl SymbolFactKey {
    /// Creates a key from an exact symbol identity and fact category.
    pub const fn new(symbol: AnySymbolId, kind: SymbolFactKind) -> Self {
        Self { symbol, kind }
    }

    /// Returns the symbol that owns the fact.
    pub const fn symbol(self) -> AnySymbolId {
        self.symbol
    }

    /// Returns the exact category of fact requested from the symbol.
    pub const fn kind(self) -> SymbolFactKind {
        self.kind
    }
}

/// An erased identity used only for compilation query coordination.
///
/// Typed fact caches retain their exact value types. This key erases only enough information to
/// detect dependency cycles and coordinate concurrent evaluation across those caches.
/// Cloning remains cheap because the only non-copy payload, [`BoundUnitKey`], is `Arc`-backed.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CompilationFactKey {
    /// The target-filtered compiler-known declaration symbol view.
    AvailableCompilerKnownSymbols,
    /// The complete canonical compiler-known symbol and fact provider.
    CompilerKnownSymbols,
    /// Deterministic compact identities for bound-unit source anchors.
    BoundUnitIdentities,
    /// One canonical immutable bound unit selected by its exact stable key.
    BoundUnit(BoundUnitKey),
    /// Diagnostics for the current whole-compilation check boundary.
    CheckDiagnostics,
    /// Durable control-flow facts for one bound unit.
    CheckedControlFlow(BoundUnitKey),
    /// Declaration discovery for one source unit.
    DeclarationChunk(SourceId),
    /// The deterministically merged declaration table.
    DeclarationTable,
    /// The canonical semantic value store for this compilation snapshot.
    SemanticValueStore,
    /// Parsed syntax for one source unit.
    SourceUnitSyntax(SourceId),
    /// The deterministic compilation-wide symbol identity graph.
    SymbolGraph,
    /// One symbol-owned semantic fact.
    Symbol(SymbolFactKey),
    /// The syntax tree composed from every loaded source unit.
    SyntaxTree,
}

impl CompilationFactKey {
    pub(crate) const fn bound_unit_key(&self) -> Option<&BoundUnitKey> {
        match self {
            Self::BoundUnit(key) | Self::CheckedControlFlow(key) => Some(key),
            Self::AvailableCompilerKnownSymbols
            | Self::CompilerKnownSymbols
            | Self::BoundUnitIdentities
            | Self::CheckDiagnostics
            | Self::DeclarationChunk(_)
            | Self::DeclarationTable
            | Self::SemanticValueStore
            | Self::SourceUnitSyntax(_)
            | Self::SymbolGraph
            | Self::Symbol(_)
            | Self::SyntaxTree => None,
        }
    }
}

impl From<SymbolFactKey> for CompilationFactKey {
    fn from(key: SymbolFactKey) -> Self {
        Self::Symbol(key)
    }
}

#[cfg(test)]
mod tests {
    use bray_symbols::{AnySymbolId, FunctionSymbolId, SymbolFactKind, SymbolId};

    use super::{CompilationFactKey, SymbolFactKey};

    #[test]
    fn symbol_fact_keys_keep_exact_identity_and_category() {
        let symbol = AnySymbolId::from(FunctionSymbolId::from_symbol_id(SymbolId::new(7)));
        let key = SymbolFactKey::new(symbol, SymbolFactKind::CallableSignature);

        assert_eq!(key.symbol(), symbol);
        assert_eq!(key.kind(), SymbolFactKind::CallableSignature);

        assert_eq!(
            CompilationFactKey::from(key),
            CompilationFactKey::Symbol(key)
        );
    }

    #[test]
    fn compilation_fact_keys_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<CompilationFactKey>();
        assert_send_sync::<SymbolFactKey>();
    }
}
