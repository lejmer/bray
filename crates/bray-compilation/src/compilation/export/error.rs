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
    /// Query-runtime coordination prevented a complete interface from being committed.
    Query(FactQueryError),
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
    semantic_value_checker_error(error).map_or(fallback, semantic_value_export_error)
}

pub(in crate::compilation::export) fn fact_query_export_error(
    error: FactQueryError,
    fallback: PackageInterfaceExportError,
) -> PackageInterfaceExportError {
    match error {
        FactQueryError::Cancelled => PackageInterfaceExportError::Cancelled,
        error
            @ (FactQueryError::Cycle(_)
            | FactQueryError::InfrastructureFailure
            | FactQueryError::Runtime(_)) => PackageInterfaceExportError::Query(error),
        FactQueryError::SemanticValueStoreCreate(error) => {
            PackageInterfaceExportError::SemanticValueStoreCreate(error)
        }
        FactQueryError::SemanticValueStore(error) => semantic_value_export_error(error),
        FactQueryError::CheckerInfrastructure(error) => {
            checker_infrastructure_export_error(error, fallback)
        }
        FactQueryError::Binding(bray_binder::BoundUnitBindingError::SemanticValue(error)) => {
            semantic_value_export_error(error)
        }
        FactQueryError::Binding(bray_binder::BoundUnitBindingError::CheckerInfrastructure(
            error,
        )) => checker_infrastructure_export_error(error, fallback),
        _ => fallback,
    }
}

pub(in crate::compilation::export) fn binding_query_export_error<Upstream>(
    error: bray_binder::BindingQueryError<Upstream>,
    fallback: PackageInterfaceExportError,
) -> PackageInterfaceExportError
where
    Upstream: Into<FactQueryError>,
{
    fact_query_export_error(
        crate::compilation::binder::binding_query_error(error),
        fallback,
    )
}

pub(in crate::compilation::export) fn invalid_compilation_fact_error(
    error: FactQueryError,
) -> PackageInterfaceExportError {
    fact_query_export_error(error, PackageInterfaceExportError::InvalidCompilation)
}

pub(in crate::compilation::export) fn invalid_compilation_binding_error<Upstream>(
    error: bray_binder::BindingQueryError<Upstream>,
) -> PackageInterfaceExportError
where
    Upstream: Into<FactQueryError>,
{
    binding_query_export_error(error, PackageInterfaceExportError::InvalidCompilation)
}

const fn semantic_value_checker_error(
    error: bray_checker::CheckerInfrastructureError,
) -> Option<SemanticValueStoreError> {
    match error {
        bray_checker::CheckerInfrastructureError::SemanticValueStore(error) => Some(error),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use bray_binder::BoundUnitBindingError;
    use bray_checker::CheckerInfrastructureError;
    use bray_symbols::{
        SemanticValueKind, SemanticValueStoreCreateError, SemanticValueStoreError,
    };

    use super::{
        PackageInterfaceExportError, binding_query_export_error, fact_query_export_error,
    };
    use crate::fact::{CompilationFactKey, FactCycle, FactQueryError, FactRuntimeFailure};

    #[test]
    fn fact_query_export_preserves_direct_and_nested_semantic_value_failures() {
        let semantic = SemanticValueStoreError::UnknownId {
            kind: SemanticValueKind::Type,
        };

        let expected = PackageInterfaceExportError::SemanticValueStore(semantic);
        let fallback = || PackageInterfaceExportError::InvalidCompilation;

        assert_eq!(
            fact_query_export_error(FactQueryError::SemanticValueStore(semantic), fallback()),
            expected
        );

        assert_eq!(
            fact_query_export_error(
                FactQueryError::CheckerInfrastructure(
                    CheckerInfrastructureError::SemanticValueStore(semantic),
                ),
                fallback(),
            ),
            expected
        );

        assert_eq!(
            fact_query_export_error(
                FactQueryError::Binding(BoundUnitBindingError::SemanticValue(semantic)),
                fallback(),
            ),
            expected
        );

        assert_eq!(
            fact_query_export_error(
                FactQueryError::Binding(BoundUnitBindingError::CheckerInfrastructure(
                    CheckerInfrastructureError::SemanticValueStore(semantic),
                )),
                fallback(),
            ),
            expected
        );

        assert_eq!(
            fact_query_export_error(
                FactQueryError::SemanticValueStoreCreate(
                    SemanticValueStoreCreateError::IdentitySpaceExhausted,
                ),
                fallback(),
            ),
            PackageInterfaceExportError::SemanticValueStoreCreate(
                SemanticValueStoreCreateError::IdentitySpaceExhausted,
            )
        );
    }

    #[test]
    fn fact_query_export_preserves_coordination_failures_and_cancellation() {
        let fallback = || PackageInterfaceExportError::InvalidCompilation;

        let runtime = FactQueryError::from(FactRuntimeFailure::WorkerTerminated {
            worker: Some(2),
            item: None,
        });

        assert_eq!(
            fact_query_export_error(runtime.clone(), fallback()),
            PackageInterfaceExportError::Query(runtime)
        );

        let cycle = FactQueryError::Cycle(FactCycle::new([
            CompilationFactKey::SyntaxTree,
            CompilationFactKey::DeclarationTable,
            CompilationFactKey::SyntaxTree,
        ]));

        assert_eq!(
            fact_query_export_error(cycle.clone(), fallback()),
            PackageInterfaceExportError::Query(cycle)
        );

        assert_eq!(
            fact_query_export_error(FactQueryError::Cancelled, fallback()),
            PackageInterfaceExportError::Cancelled
        );

        assert_eq!(
            binding_query_export_error(
                bray_binder::BindingQueryError::<std::convert::Infallible>::Cancelled,
                fallback(),
            ),
            PackageInterfaceExportError::Cancelled
        );
    }
}
