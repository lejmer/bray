use bray_diagnostics::DiagnosticInterfaceSymbolIdentity;
use bray_package_interface::{
    InterfaceSemanticCommitError, InterfaceSemanticTableKind, PackageInterfaceExportBuildError,
    PackageInterfaceExportSurfaceError,
};
use bray_source::SourceSpan;
use bray_symbols::{ExternalSymbolKey, SymbolKind};

use crate::fact::FactQueryError;

/// Failure while producing the current library product's public interface.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum PackageInterfaceExportError {
    /// Export was cancelled before a complete interface could be committed.
    Cancelled,
    /// Source or semantic errors make the product invalid.
    InvalidCompilation,
    /// A recovered public symbol cannot supply a stable external identity.
    RecoveredPublicSymbol(SymbolKind),
    /// Constant body evaluation could not complete for one exported callable.
    ConstantCallableEvaluation {
        declaration: DiagnosticInterfaceSymbolIdentity,
        cause: FactQueryError,
    },
    /// Preparing reusable native code for one exported declaration could not complete.
    ExecutableTemplateEvaluation {
        declaration: DiagnosticInterfaceSymbolIdentity,
        cause: FactQueryError,
    },
    /// A reachable public declaration does not have complete serializable semantics.
    IncompletePublicDeclarationSemantics(SymbolKind),
    /// A resolved fragment omitted a value required by its declaration records.
    IncompleteSemanticFragment {
        declaration: Option<DiagnosticInterfaceSymbolIdentity>,
        table: InterfaceSemanticTableKind,
        reference: u32,
    },
    /// Two independently resolved fragments use one stable symbol identity.
    ConflictingSemanticFragment {
        first: DiagnosticInterfaceSymbolIdentity,
        second: DiagnosticInterfaceSymbolIdentity,
        first_span: Option<SourceSpan>,
        second_span: Option<SourceSpan>,
        identity: ExternalSymbolKey,
    },
    /// A semantic value graph contains a cycle that cannot be represented in table order.
    CyclicSemanticFragment {
        declaration: Option<DiagnosticInterfaceSymbolIdentity>,
        table: InterfaceSemanticTableKind,
        reference: u32,
    },
    /// Compiler coordination could not complete fragment discovery.
    FragmentCoordination(FactQueryError),
    /// Stable fragment commit rejected a reference or declaration record.
    FragmentCommit(InterfaceSemanticCommitError),
    /// Canonical identity-surface validation rejected the selected graph.
    Surface(PackageInterfaceExportSurfaceError),
    /// Semantic or support-graph validation rejected the export bundle.
    Bundle(PackageInterfaceExportBuildError),
}
