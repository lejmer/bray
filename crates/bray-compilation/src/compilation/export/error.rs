use bray_diagnostics::DiagnosticInterfaceSymbolIdentity;
use bray_package_interface::{
    InterfaceSemanticCommitError, PackageInterfaceExportBuildError,
    PackageInterfaceExportSurfaceError,
};
use bray_symbols::{
    CallableSignatureTemplateError, SemanticValueStoreCreateError, SemanticValueStoreError,
    SymbolKind,
};

use crate::fact::FactQueryError;

/// Exact capacity, runtime compatibility or request failure preventing package-interface export.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum PackageInterfaceInvalidCompilationCause {
    /// One compact package-interface field cannot represent the requested value.
    Capacity { field: &'static str, actual: String },
    /// Runtime requirements selected by executable templates are mutually incompatible.
    RuntimeRequirements(bray_runtime_interface::RuntimeRequirementsMergeError),
    /// The requested library identity or product differs from the selected compilation.
    ExportRequestMismatch,
}

/// Exact package-interface traversal contract that could not be satisfied.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::compilation::export) enum PackageInterfaceExportContract {
    /// The export request does not describe the compilation's library product.
    RequestMismatch,
    /// Dependency loading completed without an interface-view collection.
    MissingLoadedDependencies,
    /// A resolved overload path did not identify an overload arm.
    MissingOverloadArm,
    /// The compilation symbol graph does not contain its package symbol.
    MissingPackageSymbol,
    /// A selected constant callable resolved to an imported interface symbol.
    NonLocalConstantCallable,
    /// A selected symbol does not have its exported declaration identity.
    MissingExportedDeclaration,
    /// A selected native boundary resolved to an imported interface symbol.
    NonLocalNativeBoundary,
    /// A selected executable template resolved to an imported interface symbol.
    NonLocalExecutableTemplate,
    /// The compiler-known catalog does not provide a required target property.
    MissingCompilerKnownTargetProperty,
    /// A nested executable-template key does not have an assigned identity.
    MissingNestedExecutableTemplate,
    /// A runtime-default owner does not have a stable symbol key.
    MissingRuntimeDefaultOwnerKey,
}

#[track_caller]
pub(in crate::compilation::export) fn export_contract_error(
    contract: PackageInterfaceExportContract,
) -> PackageInterfaceExportError {
    match contract {
        PackageInterfaceExportContract::RequestMismatch => {
            PackageInterfaceExportError::InvalidCompilationCause(
                PackageInterfaceInvalidCompilationCause::ExportRequestMismatch,
            )
        }
        _ => panic!("checked interface export contract failed: {contract:?}"),
    }
}

pub(in crate::compilation::export) fn capacity_export_error(
    field: &'static str,
    actual: impl ToString,
) -> PackageInterfaceExportError {
    PackageInterfaceExportError::InvalidCompilationCause(
        PackageInterfaceInvalidCompilationCause::Capacity {
            field,
            actual: actual.to_string(),
        },
    )
}

/// Failure while producing the current library product's public interface.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum PackageInterfaceExportError {
    /// A checked source diagnostic prevented complete export.
    SourceDiagnostic(Box<bray_diagnostics::Diagnostic>),
    /// Export was cancelled before a complete interface could be committed.
    Cancelled,
    /// Query-runtime coordination prevented a complete interface from being committed.
    Query(FactQueryError),
    /// Source or semantic errors make the product invalid.
    // rust-style: broad-failure
    InvalidCompilation,
    /// A capacity, runtime compatibility or request failure prevented export.
    InvalidCompilationCause(PackageInterfaceInvalidCompilationCause),
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
    /// Compiler coordination could not complete fragment discovery.
    FragmentCoordination(FactQueryError),
}

pub(in crate::compilation::export) fn source_diagnostic_export_error(
    diagnostics: &bray_diagnostics::DiagnosticBag,
) -> Option<PackageInterfaceExportError> {
    diagnostics
        .iter()
        .find(|diagnostic| diagnostic.severity() == bray_diagnostics::SeverityKind::Error)
        .map(|diagnostic| {
            // The failure owns the diagnostic after the query snapshot is released.
            PackageInterfaceExportError::SourceDiagnostic(Box::new(diagnostic.clone()))
        })
}

pub(in crate::compilation::export) fn constant_callable_evaluation_export_error(
    declaration: DiagnosticInterfaceSymbolIdentity,
    cause: FactQueryError,
) -> PackageInterfaceExportError {
    contextual_fact_query_export_error(cause, |cause| {
        PackageInterfaceExportError::ConstantCallableEvaluation { declaration, cause }
    })
}

pub(in crate::compilation::export) fn executable_template_evaluation_export_error(
    declaration: DiagnosticInterfaceSymbolIdentity,
    cause: FactQueryError,
) -> PackageInterfaceExportError {
    contextual_fact_query_export_error(cause, |cause| {
        PackageInterfaceExportError::ExecutableTemplateEvaluation { declaration, cause }
    })
}

pub(in crate::compilation::export) fn fragment_coordination_export_error(
    cause: FactQueryError,
) -> PackageInterfaceExportError {
    contextual_fact_query_export_error(cause, PackageInterfaceExportError::FragmentCoordination)
}

fn contextual_fact_query_export_error(
    cause: FactQueryError,
    contextualize: impl FnOnce(FactQueryError) -> PackageInterfaceExportError,
) -> PackageInterfaceExportError {
    match cause {
        FactQueryError::Cancelled => PackageInterfaceExportError::Cancelled,
        cause => contextualize(cause),
    }
}

pub(in crate::compilation::export) fn callable_signature_export_error(
    error: CallableSignatureTemplateError,
) -> PackageInterfaceExportError {
    fact_query_export_error(error.into())
}

pub(in crate::compilation::export) fn fact_query_export_error(
    error: FactQueryError,
) -> PackageInterfaceExportError {
    match error {
        FactQueryError::Cancelled => PackageInterfaceExportError::Cancelled,
        error => PackageInterfaceExportError::Query(error),
    }
}

pub(in crate::compilation::export) fn binding_query_export_error<Upstream>(
    error: bray_binder::BindingQueryError<Upstream>,
) -> PackageInterfaceExportError
where
    Upstream: Into<FactQueryError> + std::fmt::Debug,
{
    fact_query_export_error(crate::compilation::binder::binding_query_error(error))
}

#[cfg(test)]
mod tests {

    use bray_symbols::{SemanticValueKind, SemanticValueStoreCreateError, SemanticValueStoreError};

    use super::{
        PackageInterfaceExportError, binding_query_export_error,
        constant_callable_evaluation_export_error, executable_template_evaluation_export_error,
        fact_query_export_error, fragment_coordination_export_error,
    };
    use crate::fact::{CompilationFactKey, FactCycle, FactQueryError, FactRuntimeFailure};

    #[test]
    fn semantic_commit_invariants_fail_at_the_export_boundary() {
        let panic = std::panic::catch_unwind(|| {
            super::commit_error(
                bray_package_interface::InterfaceSemanticCommitError::MissingReference {
                    table: bray_package_interface::InterfaceSemanticTableKind::Type,
                    reference: 7,
                },
            )
        })
        .err()
        .expect("compiler invariant must panic");

        let message = bray_testing::panic_payload_text(panic.as_ref());

        assert!(message.contains("MissingReference"));
        assert!(message.contains("Type"));
        assert!(message.contains("7"));
    }

    #[test]
    fn fact_query_export_preserves_direct_and_nested_semantic_value_failures() {
        let semantic = SemanticValueStoreError::CapacityExhausted {
            kind: SemanticValueKind::Type,
        };

        let expected = |error| PackageInterfaceExportError::Query(error);

        assert_eq!(
            fact_query_export_error(FactQueryError::SemanticValueStore(semantic)),
            expected(FactQueryError::SemanticValueStore(semantic))
        );

        let binding =
            crate::compilation::binder::binding_query_error(bray_binder::BindingQueryError::<
                FactQueryError,
            >::SemanticValue(semantic));

        assert_eq!(fact_query_export_error(binding.clone()), expected(binding));

        let create = FactQueryError::SemanticValueStoreCreate(
            SemanticValueStoreCreateError::IdentitySpaceExhausted,
        );

        assert_eq!(fact_query_export_error(create.clone()), expected(create));
    }

    #[test]
    fn fact_query_export_preserves_coordination_failures_and_cancellation() {
        let runtime = FactQueryError::from(FactRuntimeFailure::UnitSourceCapacityExhausted {
            source_count: usize::MAX,
        });

        assert_eq!(
            fact_query_export_error(runtime.clone()),
            PackageInterfaceExportError::Query(runtime)
        );

        let cycle = FactQueryError::Cycle(FactCycle::new([
            CompilationFactKey::SyntaxTree,
            CompilationFactKey::DeclarationTable,
            CompilationFactKey::SyntaxTree,
        ]));

        assert_eq!(
            fact_query_export_error(cycle.clone()),
            PackageInterfaceExportError::Query(cycle)
        );

        assert_eq!(
            fact_query_export_error(FactQueryError::Cancelled),
            PackageInterfaceExportError::Cancelled
        );

        assert_eq!(
            binding_query_export_error(
                bray_binder::BindingQueryError::<std::convert::Infallible>::Cancelled,
            ),
            PackageInterfaceExportError::Cancelled
        );
    }

    #[test]
    fn contextual_query_export_errors_normalize_cancellation() {
        let declaration = bray_diagnostics::DiagnosticInterfaceSymbolIdentity::Package(
            "example.package".to_owned(),
        );

        assert_eq!(
            constant_callable_evaluation_export_error(
                declaration.clone(),
                FactQueryError::Cancelled,
            ),
            PackageInterfaceExportError::Cancelled
        );

        assert_eq!(
            executable_template_evaluation_export_error(declaration, FactQueryError::Cancelled,),
            PackageInterfaceExportError::Cancelled
        );

        assert_eq!(
            fragment_coordination_export_error(FactQueryError::Cancelled),
            PackageInterfaceExportError::Cancelled
        );
    }
    #[test]
    fn source_export_errors_preserve_the_actual_diagnostic() {
        let compilation = crate::test_support::compilation(
            r#"
            module app;

            func root()
            {
                missing;
            }
        "#,
        );

        let diagnostics = compilation.check_diagnostics();

        let expected = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.severity() == bray_diagnostics::SeverityKind::Error)
            .unwrap();

        assert_eq!(
            super::source_diagnostic_export_error(diagnostics),
            Some(super::PackageInterfaceExportError::SourceDiagnostic(
                Box::new(expected.clone())
            ))
        );

        assert_eq!(
            super::source_diagnostic_export_error(&bray_diagnostics::DiagnosticBag::new()),
            None
        );
    }
}

#[track_caller]
pub(in crate::compilation::export) fn commit_error(
    cause: InterfaceSemanticCommitError,
) -> PackageInterfaceExportError {
    match cause {
        InterfaceSemanticCommitError::IdentityOverflow(table) => {
            capacity_export_error(table.as_str(), u64::from(u32::MAX) + 1)
        }
        _ => panic!("compiler-assembled semantic fragment commit failed: {cause:?}"),
    }
}

#[track_caller]
pub(in crate::compilation::export) fn surface_error(
    cause: PackageInterfaceExportSurfaceError,
) -> PackageInterfaceExportError {
    match cause {
        PackageInterfaceExportSurfaceError::SymbolCountOverflow
        | PackageInterfaceExportSurfaceError::Surface(
            bray_package_interface::PackageInterfaceSurfaceBuildError::Identity(
                bray_symbols::ImportedIdentitySurfaceError::SymbolCountOverflow,
            ),
        ) => capacity_export_error("declarations", u64::from(u32::MAX) + 1),
        PackageInterfaceExportSurfaceError::Surface(
            bray_package_interface::PackageInterfaceSurfaceBuildError::DependencyCountOverflow,
        ) => capacity_export_error("dependencies", u64::from(u32::MAX) + 1),
        _ => panic!("compiler-assembled interface surface failed: {cause:?}"),
    }
}

#[track_caller]
pub(in crate::compilation::export) fn bundle_error(
    cause: PackageInterfaceExportBuildError,
) -> PackageInterfaceExportError {
    match cause {
        PackageInterfaceExportBuildError::Validation(
            cause @ (bray_package_interface::InterfaceValidationError::ResourceLimitExceeded {
                ..
            }
            | bray_package_interface::InterfaceValidationError::AllocationUnavailable {
                ..
            }),
        ) => fact_query_export_error(cause.into()),
        _ => panic!("compiler-assembled interface bundle failed: {cause:?}"),
    }
}
