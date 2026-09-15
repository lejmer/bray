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

/// Exact internal compilation contract that prevented package-interface export.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum PackageInterfaceInvalidCompilationCause {
    /// The selected target could not form its backend-neutral code generation contract.
    CodegenTarget(bray_codegen::CodegenTargetBuildError),
    /// One compact package-interface field cannot represent the requested value.
    Capacity { field: &'static str, actual: String },
    /// Runtime requirements selected by executable templates are mutually incompatible.
    RuntimeRequirements(bray_runtime_interface::RuntimeRequirementsMergeError),
    /// Checked-template export violated one exact internal semantic contract.
    CheckedTemplate { reason: &'static str },
    /// Executable-template export violated one exact internal MIR contract.
    ExecutableTemplate { reason: &'static str },
    /// Package-interface traversal violated one exact export contract.
    ExportContract(PackageInterfaceExportContract),
}

/// Exact package-interface traversal contract that could not be satisfied.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PackageInterfaceExportContract {
    /// The export request does not describe the compilation's library product.
    RequestMismatch,
    /// Dependency loading completed without an interface-view collection.
    MissingLoadedDependencies,
    /// A resolved overload path did not identify an overload arm.
    MissingOverloadArm,
    /// The compilation symbol graph does not contain its package symbol.
    MissingPackageSymbol,
    /// A selected constant callable cannot identify a generic owner.
    MissingGenericOwner,
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

impl PackageInterfaceExportContract {
    /// Returns the stable machine-readable contract name.
    pub const fn name(self) -> &'static str {
        match self {
            Self::RequestMismatch => "request_mismatch",
            Self::MissingLoadedDependencies => "missing_loaded_dependencies",
            Self::MissingOverloadArm => "missing_overload_arm",
            Self::MissingPackageSymbol => "missing_package_symbol",
            Self::MissingGenericOwner => "missing_generic_owner",
            Self::NonLocalConstantCallable => "non_local_constant_callable",
            Self::MissingExportedDeclaration => "missing_exported_declaration",
            Self::NonLocalNativeBoundary => "non_local_native_boundary",
            Self::NonLocalExecutableTemplate => "non_local_executable_template",
            Self::MissingCompilerKnownTargetProperty => "missing_compiler_known_target_property",
            Self::MissingNestedExecutableTemplate => "missing_nested_executable_template",
            Self::MissingRuntimeDefaultOwnerKey => "missing_runtime_default_owner_key",
        }
    }
}

pub(in crate::compilation::export) const fn export_contract_error(
    contract: PackageInterfaceExportContract,
) -> PackageInterfaceExportError {
    PackageInterfaceExportError::InvalidCompilationCause(
        PackageInterfaceInvalidCompilationCause::ExportContract(contract),
    )
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
    /// Execution evidence cannot be represented, retaining the exact source diagnostic.
    ExecutionEvidence(Box<bray_diagnostics::Diagnostic>),
    /// Export was cancelled before a complete interface could be committed.
    Cancelled,
    /// Query-runtime coordination prevented a complete interface from being committed.
    Query(FactQueryError),
    /// Source or semantic errors make the product invalid.
    // rust-style: broad-failure
    InvalidCompilation,
    /// An exact internal compilation contract prevented export.
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
    /// A reachable public declaration does not have complete serializable semantics.
    IncompletePublicDeclarationSemantics(SymbolKind),
    /// A resolved fragment omitted a value required by its declaration records.
    IncompleteSemanticFragment {
        declaration: Option<DiagnosticInterfaceSymbolIdentity>,
        table: InterfaceSemanticTableKind,
        reference: u32,
    },
    /// A resolved fragment's stable symbol identity is absent from the compilation symbol graph.
    MissingSemanticFragmentSymbol(ExternalSymbolKey),
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

pub(in crate::compilation::export) fn checker_infrastructure_export_error(
    error: bray_checker::CheckerInfrastructureError,
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
    Upstream: Into<FactQueryError>,
{
    fact_query_export_error(crate::compilation::binder::binding_query_error(error))
}

pub(in crate::compilation::export) fn invalid_compilation_fact_error(
    error: FactQueryError,
) -> PackageInterfaceExportError {
    fact_query_export_error(error)
}

pub(in crate::compilation::export) fn invalid_compilation_binding_error<Upstream>(
    error: bray_binder::BindingQueryError<Upstream>,
) -> PackageInterfaceExportError
where
    Upstream: Into<FactQueryError>,
{
    binding_query_export_error(error)
}

#[cfg(test)]
mod tests {
    use bray_binder::BoundUnitBindingError;
    use bray_checker::CheckerInfrastructureError;
    use bray_symbols::{SemanticValueKind, SemanticValueStoreCreateError, SemanticValueStoreError};

    use super::{
        PackageInterfaceExportError, binding_query_export_error,
        constant_callable_evaluation_export_error, executable_template_evaluation_export_error,
        fact_query_export_error, fragment_coordination_export_error,
    };
    use crate::fact::{CompilationFactKey, FactCycle, FactQueryError, FactRuntimeFailure};

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

        let checker = FactQueryError::CheckerInfrastructure(
            CheckerInfrastructureError::SemanticValueStore(semantic),
        );

        assert_eq!(fact_query_export_error(checker.clone()), expected(checker));

        let binding = FactQueryError::Binding(BoundUnitBindingError::SemanticValue(semantic));

        assert_eq!(fact_query_export_error(binding.clone()), expected(binding));

        let nested = FactQueryError::Binding(BoundUnitBindingError::CheckerInfrastructure(
            CheckerInfrastructureError::SemanticValueStore(semantic),
        ));

        assert_eq!(fact_query_export_error(nested.clone()), expected(nested));

        let create = FactQueryError::SemanticValueStoreCreate(
            SemanticValueStoreCreateError::IdentitySpaceExhausted,
        );

        assert_eq!(fact_query_export_error(create.clone()), expected(create));
    }

    #[test]
    fn fact_query_export_preserves_coordination_failures_and_cancellation() {
        let runtime = FactQueryError::from(FactRuntimeFailure::WorkerTerminated {
            worker: Some(2),
            item: None,
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
}
