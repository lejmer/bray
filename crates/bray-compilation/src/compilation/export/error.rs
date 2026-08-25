use bray_package_interface::{
    InterfaceSemanticCommitError, PackageInterfaceExportBuildError,
    PackageInterfaceExportSurfaceError,
};
use bray_symbols::SymbolKind;

/// Failure while producing the current library product's public interface.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum PackageInterfaceExportError {
    /// Export was cancelled before a complete interface could be committed.
    Cancelled,
    /// Source or semantic errors make the product invalid.
    InvalidCompilation,
    /// A recovered public symbol cannot supply a stable external identity.
    RecoveredPublicSymbol(SymbolKind),
    /// A reachable public declaration does not have complete serializable semantics.
    IncompletePublicDeclarationSemantics(SymbolKind),
    /// A resolved fragment omitted a value required by its declaration records.
    IncompleteSemanticFragment,
    /// Two independently resolved fragments use one stable symbol identity.
    ConflictingSemanticFragment,
    /// A semantic value graph contains a cycle that cannot be represented in table order.
    CyclicSemanticFragment,
    /// Compiler coordination could not complete fragment discovery.
    FragmentCoordination,
    /// Stable fragment commit rejected a reference or declaration record.
    FragmentCommit(InterfaceSemanticCommitError),
    /// Canonical identity-surface validation rejected the selected graph.
    Surface(PackageInterfaceExportSurfaceError),
    /// Semantic or support-graph validation rejected the export bundle.
    Bundle(PackageInterfaceExportBuildError),
}
