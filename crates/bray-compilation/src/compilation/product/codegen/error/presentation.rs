use bray_codegen::{
    CodegenInstanceBuildError, CodegenPartitionError, CodegenReachabilityBuildError,
    CodegenTargetBuildError, CodegenUnitBuildError,
};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticId, DiagnosticKind, DiagnosticNativeProductFailureKind,
    DiagnosticNote, DiagnosticNoteKind, SeverityKind,
};
use bray_emitter::EmissionBackendBuildError;
use bray_linker::LinkTargetBuildError;
use bray_runtime_interface::ExecutableHostContractBuildError;
use bray_symbols::ProductIdentity;

use super::backend::codegen_backend_failure_kind;
use super::link_input::diagnostic_native_link_input_failure;
use super::model::NativeProductPlanningError;
use super::preparation::codegen_preparation_failure_kind;
use super::query::fact_query_failure_kind;
use super::runtime_selection::runtime_selection_failure_kind;

pub(in crate::compilation) fn native_product_preparation_diagnostic(
    failure: DiagnosticNativeProductFailureKind,
    product: &ProductIdentity,
    target: &str,
) -> Diagnostic {
    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::NativeProductPreparationFailed,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::actual_product_identity(product.to_string()))
    .with_arg(DiagnosticArg::target_triple(target))
    .with_arg(DiagnosticArg::native_product_failure_kind(failure.clone()));

    match failure {
        DiagnosticNativeProductFailureKind::EvaluationLoweringInput(failure) => {
            crate::compilation::diagnostics::with_compiler_defect_source(
                diagnostic,
                failure.source(),
            )
        }
        DiagnosticNativeProductFailureKind::EvaluationLowering(failure) => {
            crate::compilation::diagnostics::with_compiler_defect_source(
                diagnostic,
                failure.source(),
            )
        }
        DiagnosticNativeProductFailureKind::CodegenMirUnavailable(_) => diagnostic.with_note(
            DiagnosticNote::new(DiagnosticNoteKind::NativeProductPreparationRecovery),
        ),
        _ => diagnostic,
    }
}

pub(super) fn native_product_failure_kind(
    error: &NativeProductPlanningError,
) -> Option<DiagnosticNativeProductFailureKind> {
    use DiagnosticNativeProductFailureKind as Kind;

    Some(match error {
        NativeProductPlanningError::CodegenUnavailable => Kind::CodegenBackendNotSelected,
        NativeProductPlanningError::BitcodeTargetContract(error) => {
            codegen_backend_failure_kind(error)
        }
        NativeProductPlanningError::MissingProductRoot => Kind::MissingProductRoot,
        NativeProductPlanningError::InvalidEntryResult => Kind::InvalidEntryResult,
        NativeProductPlanningError::MissingRuntime => Kind::MissingRuntime,
        NativeProductPlanningError::LibraryCleanupRequiresMainThread => {
            Kind::LibraryCleanupRequiresMainThread
        }
        NativeProductPlanningError::InvalidSymbolName => Kind::InvalidSymbolName,
        NativeProductPlanningError::InvalidNativeLinkInput(error) => {
            Kind::InvalidNativeLinkInput(diagnostic_native_link_input_failure(error))
        }
        NativeProductPlanningError::Query(error) => fact_query_failure_kind(error)?,
        NativeProductPlanningError::InvalidCodegenTarget(error) => match error {
            CodegenTargetBuildError::UnsupportedProfile => Kind::CodegenTargetUnsupportedProfile,
            CodegenTargetBuildError::EmptyTriple => Kind::CodegenTargetEmptyTriple,
            CodegenTargetBuildError::EmptyCpu => Kind::CodegenTargetEmptyCpu,
            CodegenTargetBuildError::EmptyFeature => Kind::CodegenTargetEmptyFeature,
        },
        NativeProductPlanningError::InvalidReachability(error) => match error {
            CodegenReachabilityBuildError::EmptyRoots => Kind::ReachabilityEmptyRoots,
            CodegenReachabilityBuildError::DuplicateInstance => Kind::ReachabilityDuplicateInstance,
            CodegenReachabilityBuildError::InstanceWasNotDemanded => {
                Kind::ReachabilityUndemandedInstance
            }
            CodegenReachabilityBuildError::Incomplete => Kind::ReachabilityIncomplete,
        },
        NativeProductPlanningError::InvalidCodegenInstance(error) => match error {
            CodegenInstanceBuildError::TemplateMismatch => Kind::InstanceTemplateMismatch,
            CodegenInstanceBuildError::TargetMismatch => Kind::InstanceTargetMismatch,
            CodegenInstanceBuildError::DependencyTargetMismatch => {
                Kind::InstanceDependencyTargetMismatch
            }
        },
        NativeProductPlanningError::InvalidCodegenUnit(error) => codegen_unit_failure_kind(*error),
        NativeProductPlanningError::InvalidCodegenPartition(error) => match error {
            CodegenPartitionError::MissingCompatibility(_) => Kind::PartitionMissingCompatibility,
            CodegenPartitionError::InvalidUnit(_) => Kind::PartitionInvalidUnit,
        },
        NativeProductPlanningError::InvalidHostMir(_) => Kind::GeneratedHostMirInvalid,
        NativeProductPlanningError::InvalidExecutableHost(error) => match error {
            ExecutableHostContractBuildError::DuplicateRole(_) => Kind::ExecutableHostDuplicateRole,
            ExecutableHostContractBuildError::MissingRuntime => Kind::ExecutableHostMissingRuntime,
            ExecutableHostContractBuildError::RuntimeOwnedHostBinding(_) => {
                Kind::ExecutableHostRuntimeOwnedBinding
            }
            ExecutableHostContractBuildError::IncompatibleRuntime(_) => {
                Kind::ExecutableHostIncompatibleRuntime
            }
            ExecutableHostContractBuildError::MissingMainThreadLaneCapability => {
                Kind::ExecutableHostMissingMainThreadLane
            }
            ExecutableHostContractBuildError::MissingProtectedFrameAbi => {
                Kind::ExecutableHostMissingProtectedFrameAbi
            }
            ExecutableHostContractBuildError::MissingRole(_) => Kind::ExecutableHostMissingRole,
        },
        NativeProductPlanningError::InvalidRuntimeSelection(error) => {
            runtime_selection_failure_kind(error)
        }
        NativeProductPlanningError::InvalidEmissionBackend(
            EmissionBackendBuildError::DuplicateCodegenUnit,
        ) => Kind::EmissionBackendDuplicateUnit,
        NativeProductPlanningError::InvalidLinkTarget(LinkTargetBuildError::EmptyTriple) => {
            Kind::LinkTargetEmptyTriple
        }
        NativeProductPlanningError::StandardLibrary(_) => Kind::StandardLibraryUnavailable,
        NativeProductPlanningError::Codegen(error) => codegen_preparation_failure_kind(error)?,
    })
}

const fn codegen_unit_failure_kind(
    error: CodegenUnitBuildError,
) -> DiagnosticNativeProductFailureKind {
    use DiagnosticNativeProductFailureKind as Kind;

    match error {
        CodegenUnitBuildError::Empty => Kind::UnitEmpty,
        CodegenUnitBuildError::DuplicateInstance => Kind::UnitDuplicateInstance,
        CodegenUnitBuildError::MissingCompatibility => Kind::UnitMissingCompatibility,
        CodegenUnitBuildError::TargetMismatch => Kind::UnitTargetMismatch,
        CodegenUnitBuildError::WorkBoundExceeded => Kind::UnitWorkBoundExceeded,
        CodegenUnitBuildError::RecipeMismatch => Kind::UnitRecipeMismatch,
    }
}
