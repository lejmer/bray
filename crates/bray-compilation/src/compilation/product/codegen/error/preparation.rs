use bray_diagnostics::{
    DiagnosticFailureField, DiagnosticFailureValue, DiagnosticNativeProductFailureDetail,
    DiagnosticNativeProductFailureKind,
};

use super::context::{failure_detail, identity_failure_detail, text_failure_field};
use super::query::fact_query_failure_kind;

pub(in crate::compilation) fn codegen_preparation_failure_kind(
    error: &crate::compilation::CodegenPreparationError,
) -> Option<DiagnosticNativeProductFailureKind> {
    use crate::compilation::CodegenPreparationError;
    use DiagnosticNativeProductFailureKind as Kind;

    Some(match error {
        CodegenPreparationError::CodegenUnavailable => Kind::CodegenBackendUnavailable,
        CodegenPreparationError::ExactIntegerCapacity { minimum_bits } => {
            Kind::CodegenExactIntegerCapacity {
                minimum_bits: *minimum_bits,
                maximum_bits: u16::MAX,
            }
        }
        CodegenPreparationError::MirCapacity(_) => Kind::CodegenMirCapacityExceeded,
        CodegenPreparationError::InvalidRuntimeRoleSourceBinding {
            role,
            expected,
            actual,
        } => Kind::CodegenInvalidAbiMapping(Some(runtime_role_signature_failure_detail(
            *role, expected, actual,
        ))),
        CodegenPreparationError::InvalidRuntimeRoleSourceContract { role, instance } => {
            Kind::CodegenInvalidAbiMapping(Some(failure_detail(
                "codegen_invalid_runtime_role_source_contract",
                [
                    text_failure_field("role", role.as_str()),
                    crate::fact::diagnostic_context::identity_field("instance", instance),
                ],
            )))
        }
        CodegenPreparationError::UnsupportedType(ty) => Kind::CodegenUnsupportedType(
            identity_failure_detail("codegen_unsupported_type", "semantic_type", ty),
        ),
        CodegenPreparationError::Diagnostics(_) => return None,
        CodegenPreparationError::LayoutOverflow(ty) => Kind::CodegenLayoutOverflow(
            identity_failure_detail("codegen_layout_overflow", "semantic_type", ty),
        ),
        CodegenPreparationError::Query(error) => fact_query_failure_kind(error)?,
    })
}

fn runtime_role_signature_failure_detail(
    role: bray_runtime_interface::RuntimeAbiRole,
    expected: &bray_codegen::CodegenCallableSignature,
    actual: &bray_codegen::CodegenCallableSignature,
) -> DiagnosticNativeProductFailureDetail {
    let expected_parameters = expected
        .parameters()
        .iter()
        .map(crate::fact::diagnostic_context::identity)
        .collect::<Vec<_>>()
        .into_boxed_slice();

    let actual_parameters = actual
        .parameters()
        .iter()
        .map(crate::fact::diagnostic_context::identity)
        .collect::<Vec<_>>()
        .into_boxed_slice();

    failure_detail(
        "codegen_invalid_runtime_role_source_binding",
        [
            text_failure_field("role", role.as_str()),
            text_failure_field(
                "expected_abi",
                bray_symbols::diagnostic_callable_abi(expected.abi()).as_str(),
            ),
            text_failure_field(
                "actual_abi",
                bray_symbols::diagnostic_callable_abi(actual.abi()).as_str(),
            ),
            DiagnosticFailureField::new(
                "expected_variadic",
                DiagnosticFailureValue::Boolean(expected.is_variadic()),
            ),
            DiagnosticFailureField::new(
                "actual_variadic",
                DiagnosticFailureValue::Boolean(actual.is_variadic()),
            ),
            DiagnosticFailureField::new(
                "expected_panic_report_context",
                DiagnosticFailureValue::Boolean(expected.has_panic_report_context()),
            ),
            DiagnosticFailureField::new(
                "actual_panic_report_context",
                DiagnosticFailureValue::Boolean(actual.has_panic_report_context()),
            ),
            DiagnosticFailureField::new(
                "expected_parameters",
                DiagnosticFailureValue::IdentityList(expected_parameters),
            ),
            DiagnosticFailureField::new(
                "actual_parameters",
                DiagnosticFailureValue::IdentityList(actual_parameters),
            ),
            crate::fact::diagnostic_context::identity_field("expected_result", expected.result()),
            crate::fact::diagnostic_context::identity_field("actual_result", actual.result()),
        ],
    )
}
