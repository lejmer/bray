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
use super::context::{failure_detail, identity_failure_detail, text_failure_field};
use super::link_input::diagnostic_native_link_input_failure;
use super::model::NativeProductPlanningError;
use super::preparation::{codegen_preparation_failure_kind, codegen_unit_preparation_failure};
use super::query::fact_query_failure_kind;
use super::runtime_selection::runtime_selection_failure_kind;

pub(in crate::compilation) fn native_product_preparation_diagnostic(
    failure: DiagnosticNativeProductFailureKind,
    product: &ProductIdentity,
    target: &str,
) -> Diagnostic {
    let source = match &failure {
        DiagnosticNativeProductFailureKind::CodegenMissingCallableImplementation {
            source, ..
        } => *source,
        _ => None,
    };

    let add_recovery_note = matches!(
        &failure,
        DiagnosticNativeProductFailureKind::CodegenMirUnavailable(_)
    );

    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::NativeProductPreparationFailed,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::actual_product_identity(product.to_string()))
    .with_arg(DiagnosticArg::target_triple(target))
    .with_arg(DiagnosticArg::native_product_failure_kind(failure));

    if let Some(source) = source {
        crate::compilation::diagnostics::with_compiler_defect_source(diagnostic, source)
    } else if add_recovery_note {
        diagnostic.with_note(DiagnosticNote::new(
            DiagnosticNoteKind::NativeProductPreparationRecovery,
        ))
    } else {
        diagnostic
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
            CodegenPartitionError::MissingCompatibility(instance) => {
                Kind::PartitionMissingCompatibility(identity_failure_detail(
                    "partition_missing_compatibility",
                    "instance",
                    instance,
                ))
            }
            CodegenPartitionError::InvalidUnit(cause) => Kind::PartitionInvalidUnit(
                failure_detail(codegen_unit_preparation_failure(*cause), []),
            ),
        },
        NativeProductPlanningError::MirCapacity(_) => Kind::CodegenMirCapacityExceeded,
        NativeProductPlanningError::InvalidExecutableHost(error) => match error {
            ExecutableHostContractBuildError::DuplicateRole(role) => {
                Kind::ExecutableHostDuplicateRole(runtime_role_detail(
                    "executable_host_duplicate_role",
                    *role,
                ))
            }
            ExecutableHostContractBuildError::MissingRuntime => Kind::ExecutableHostMissingRuntime,
            ExecutableHostContractBuildError::RuntimeOwnedHostBinding(role) => {
                Kind::ExecutableHostRuntimeOwnedBinding(runtime_role_detail(
                    "executable_host_runtime_owned_binding",
                    *role,
                ))
            }
            ExecutableHostContractBuildError::IncompatibleRuntime(cause) => {
                Kind::ExecutableHostIncompatibleRuntime(runtime_compatibility_detail(*cause))
            }
            ExecutableHostContractBuildError::MissingMainThreadLaneCapability => {
                Kind::ExecutableHostMissingMainThreadLane
            }
            ExecutableHostContractBuildError::MissingProtectedFrameAbi => {
                Kind::ExecutableHostMissingProtectedFrameAbi
            }
            ExecutableHostContractBuildError::MissingRole(role) => Kind::ExecutableHostMissingRole(
                runtime_role_detail("executable_host_missing_role", *role),
            ),
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
        NativeProductPlanningError::StandardLibrary { .. } => Kind::StandardLibraryUnavailable,
        NativeProductPlanningError::Codegen(error) => codegen_preparation_failure_kind(error)?,
    })
}

fn runtime_role_detail(
    reason: &'static str,
    role: bray_runtime_interface::RuntimeAbiRole,
) -> bray_diagnostics::DiagnosticNativeProductFailureDetail {
    failure_detail(reason, [text_failure_field("runtime_role", role.as_str())])
}

fn runtime_compatibility_detail(
    cause: bray_runtime_interface::RuntimeCompatibilityError,
) -> bray_diagnostics::DiagnosticNativeProductFailureDetail {
    use bray_runtime_interface::RuntimeCompatibilityError as Error;

    let (reason, context) = match cause {
        Error::RuntimeIdentity => ("executable_host_runtime_identity_mismatch", Vec::new()),
        Error::RuntimeAbi => ("executable_host_runtime_abi_mismatch", Vec::new()),
        Error::Target => ("executable_host_runtime_target_mismatch", Vec::new()),
        Error::PanicAbi => ("executable_host_runtime_panic_abi_mismatch", Vec::new()),
        Error::FrameAbi(operation) => (
            "executable_host_runtime_frame_abi_mismatch",
            vec![text_failure_field(
                "frame_operation",
                super::context::protected_frame_abi_operation(operation),
            )],
        ),
        Error::MissingCapability(capability) => (
            "executable_host_runtime_missing_capability",
            vec![text_failure_field(
                "runtime_capability",
                capability.as_str(),
            )],
        ),
        Error::MissingRole(role) => (
            "executable_host_runtime_missing_role",
            vec![text_failure_field("runtime_role", role.as_str())],
        ),
    };

    failure_detail(reason, context)
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
