use bray_package_interface::{
    PackageInterfaceExportBuildError, PackageInterfaceExportSurfaceError,
};
use bray_symbols::SymbolKind;

/// Failure while producing the current library product's public interface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackageInterfaceExportError {
    /// Source or semantic errors make the product invalid.
    InvalidCompilation,
    /// A recovered public symbol cannot supply a stable external identity.
    RecoveredPublicSymbol(SymbolKind),
    /// A reachable public declaration does not have a complete serializable fact set.
    IncompletePublicDeclarationFacts(SymbolKind),
    /// Canonical identity-surface validation rejected the selected graph.
    Surface(PackageInterfaceExportSurfaceError),
    /// Semantic or support-graph validation rejected the export bundle.
    Bundle(PackageInterfaceExportBuildError),
}
