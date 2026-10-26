use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticId, DiagnosticKind, DiagnosticNativeProductFailureKind,
    SeverityKind,
};
use bray_symbols::ProductIdentity;

use super::backend::codegen_backend_failure_kind;
use super::context::{failure_detail, path_failure_field};
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

    Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::NativeProductPreparationFailed,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::actual_product_identity(product.to_string()))
    .with_arg(DiagnosticArg::target_triple(target))
    .with_arg(DiagnosticArg::native_product_failure_kind(failure))
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
        NativeProductPlanningError::MissingRuntime => Kind::MissingRuntime,
        NativeProductPlanningError::InvalidNativeLinkInput(error) => {
            Kind::InvalidNativeLinkInput(diagnostic_native_link_input_failure(error))
        }
        NativeProductPlanningError::Query(error) => fact_query_failure_kind(error)?,
        NativeProductPlanningError::MirCapacity(_) => Kind::CodegenMirCapacityExceeded,
        NativeProductPlanningError::InvalidRuntimeSelection(error) => {
            runtime_selection_failure_kind(error)
        }
        NativeProductPlanningError::ConflictingNativeArtifacts { first, second } => {
            Kind::NativeResolution(failure_detail(
                "native_variant_conflict",
                [
                    path_failure_field("first_artifact", first),
                    path_failure_field("second_artifact", second),
                ],
            ))
        }
        NativeProductPlanningError::NativeResolution(error) => {
            Kind::NativeResolution(super::runtime_selection::native_resolution_detail(error))
        }
        NativeProductPlanningError::Codegen(error) => codegen_preparation_failure_kind(error)?,
    })
}
