use bray_diagnostics::DiagnosticInterfaceSymbolIdentity;
use bray_package_interface::{
    InterfaceSemanticCommitError, InterfaceSemanticTableKind, PackageInterfaceExportBuildError,
    PackageInterfaceExportSurfaceError,
};
use bray_source::SourceSpan;
use bray_symbols::{
    CallableSignatureTemplateError, ExternalSymbolKey, SemanticValueStoreCreateError,
    SemanticValueStoreError, SymbolKind,
};

use crate::fact::FactQueryError;

/// Failure while producing the current library product's public interface.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum PackageInterfaceExportError {
    /// Export was cancelled before a complete interface could be committed.
    Cancelled,
    /// Source or semantic errors make the product invalid.
    InvalidCompilation,
    /// The semantic-value store identity space was exhausted while preparing the interface.
    SemanticValueStoreCreate(SemanticValueStoreCreateError),
    /// A semantic-value operation failed while preparing the interface.
    SemanticValueStore(SemanticValueStoreError),
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

pub(in crate::compilation::export) const fn semantic_value_export_error(
    error: SemanticValueStoreError,
) -> PackageInterfaceExportError {
    PackageInterfaceExportError::SemanticValueStore(error)
}

pub(in crate::compilation::export) fn callable_signature_export_error(
    error: CallableSignatureTemplateError,
    fallback: PackageInterfaceExportError,
) -> PackageInterfaceExportError {
    match error {
        CallableSignatureTemplateError::SemanticValue(error) => semantic_value_export_error(error),
        CallableSignatureTemplateError::InvalidCallableType
        | CallableSignatureTemplateError::ParameterCountMismatch
        | CallableSignatureTemplateError::ParameterIdentityMismatch => fallback,
    }
}

pub(in crate::compilation::export) fn checker_infrastructure_export_error(
    error: bray_checker::CheckerInfrastructureError,
    fallback: PackageInterfaceExportError,
) -> PackageInterfaceExportError {
    match error {
        bray_checker::CheckerInfrastructureError::SemanticValueStore(error) => {
            semantic_value_export_error(error)
        }
        _ => fallback,
    }
}

pub(in crate::compilation::export) fn fact_query_export_error(
    error: FactQueryError,
    fallback: PackageInterfaceExportError,
) -> PackageInterfaceExportError {
    match error {
        FactQueryError::SemanticValueStoreCreate(error) => {
            PackageInterfaceExportError::SemanticValueStoreCreate(error)
        }
        FactQueryError::SemanticValueStore(error) => semantic_value_export_error(error),
        _ => fallback,
    }
}
