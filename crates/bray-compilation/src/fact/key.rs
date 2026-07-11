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
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CompilationFactKey {
    /// The target-filtered compiler-known declaration symbol view.
    AvailableCompilerKnownSymbols,
    /// Diagnostics for the current whole-compilation check boundary.
    CheckDiagnostics,
    /// Declaration discovery for one source unit.
    DeclarationChunk(SourceId),
    /// The deterministically merged declaration table.
    DeclarationTable,
    /// Parsed syntax for one source unit.
    SourceUnitSyntax(SourceId),
    /// One symbol-owned semantic fact.
    Symbol(SymbolFactKey),
    /// The syntax tree composed from every loaded source unit.
    SyntaxTree,
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
