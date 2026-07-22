use bray_bound_tree::BoundUnitKey;
use bray_checker::TargetValidityRequest;
use bray_package_interface::InterfaceSemanticFactKind;
use bray_source::SourceId;
use bray_symbols::{AnySymbolId, ImportedInterfaceId, InterfaceSymbolId, SymbolFactKind};

/// The exact artifact-local address of one imported symbol-owned fact category.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImportedSemanticFactKey {
    interface: ImportedInterfaceId,
    owner: InterfaceSymbolId,
    kind: InterfaceSemanticFactKind,
}

impl ImportedSemanticFactKey {
    /// Creates one exact imported semantic-fact address.
    pub const fn new(
        interface: ImportedInterfaceId,
        owner: InterfaceSymbolId,
        kind: InterfaceSemanticFactKind,
    ) -> Self {
        Self {
            interface,
            owner,
            kind,
        }
    }

    /// Returns the loaded interface containing this fact.
    pub const fn interface(self) -> ImportedInterfaceId {
        self.interface
    }

    /// Returns the interface-local symbol that owns this fact.
    pub const fn owner(self) -> InterfaceSymbolId {
        self.owner
    }

    /// Returns the exact semantic fact category.
    pub const fn kind(self) -> InterfaceSemanticFactKind {
        self.kind
    }
}

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

    #[cfg(test)]
    pub(crate) const fn symbol(self) -> AnySymbolId {
        self.symbol
    }

    #[cfg(test)]
    pub(crate) const fn kind(self) -> SymbolFactKind {
        self.kind
    }
}

/// A compilation-fact identity used only for private dependency coordination.
///
/// The key preserves enough semantic identity to detect dependency cycles and coordinate
/// concurrent requests.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum CompilationFactKey {
    /// The selected target and its target-filtered compiler-known declaration view.
    SelectedTarget,
    /// Post-selection validity of one exact target requirement.
    TargetValidity(TargetValidityRequest),
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
    /// Final expression types for one bound unit.
    CheckedExpressionTypes(BoundUnitKey),
    /// Final semantic selections for one bound unit.
    CheckedSemanticSelections(BoundUnitKey),
    /// Source-declared value type templates and equality constraints for one bound unit.
    DeclaredValueTypeTemplates(BoundUnitKey),
    /// The private fixed-point computation shared by expression type and selection facts.
    ExpressionSemantics(BoundUnitKey),
    /// Declaration discovery for one source unit.
    DeclarationChunk(SourceId),
    /// The deterministically merged declaration table.
    DeclarationTable,
    /// Structural validation and identity decoding for one compiled dependency interface.
    DependencyInterface(ImportedInterfaceId),
    /// Diagnostics owned by all selected compiled dependency interfaces.
    ImportedDiagnostics,
    /// Decoded and remapped semantic facts for one compiled dependency interface.
    ImportedSemanticGraph(ImportedInterfaceId),
    /// One exact decoded and remapped imported symbol-owned fact category.
    ImportedSemanticFact(ImportedSemanticFactKey),
    /// The deterministic compilation-local imported symbol identity skeleton.
    ImportedSymbolSkeleton,
    /// The current library product's complete immutable interface export bundle.
    PackageInterfaceExportBundle,
    /// The canonical semantic value store for this compilation snapshot.
    SemanticValueStore,
    /// Binding and semantic-analysis diagnostics for the source package.
    SemanticDiagnostics,
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
            Self::BoundUnit(key)
            | Self::CheckedControlFlow(key)
            | Self::CheckedExpressionTypes(key)
            | Self::CheckedSemanticSelections(key)
            | Self::DeclaredValueTypeTemplates(key)
            | Self::ExpressionSemantics(key) => Some(key),
            Self::SelectedTarget
            | Self::TargetValidity(_)
            | Self::CompilerKnownSymbols
            | Self::BoundUnitIdentities
            | Self::CheckDiagnostics
            | Self::DeclarationChunk(_)
            | Self::DeclarationTable
            | Self::DependencyInterface(_)
            | Self::ImportedDiagnostics
            | Self::ImportedSemanticGraph(_)
            | Self::ImportedSemanticFact(_)
            | Self::ImportedSymbolSkeleton
            | Self::PackageInterfaceExportBundle
            | Self::SemanticValueStore
            | Self::SemanticDiagnostics
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

    use super::{CompilationFactKey, ImportedSemanticFactKey, SymbolFactKey};

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
        assert_send_sync::<ImportedSemanticFactKey>();
        assert_send_sync::<SymbolFactKey>();
    }
}
