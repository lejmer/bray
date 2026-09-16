use crate::compilation::product_emission::diagnostics::product_query::mir_helper_kind;

use bray_codegen::{CodegenInstanceBuildError, CodegenUnitBuildError};
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
        CodegenPreparationError::InvalidRequest(cause) => {
            Kind::CodegenInvalidRequest(failure_detail(codegen_request_failure(*cause), []))
        }
        CodegenPreparationError::MirUnavailable(unit) => {
            Kind::CodegenMirUnavailable(failure_detail(
                "codegen_mir_unavailable",
                [crate::fact::diagnostic_context::identity_field(
                    "mir_unit", unit,
                )],
            ))
        }
        CodegenPreparationError::MissingEntrypoint => Kind::CodegenMissingEntrypoint,
        CodegenPreparationError::MissingCallableImplementation {
            callable, source, ..
        } => {
            // The diagnostic owns the failure-only declaration identity independently of the error.
            Kind::CodegenMissingCallableImplementation {
                callable: callable.clone(),
                source: *source,
            }
        }
        CodegenPreparationError::InvalidInstance(cause) => {
            Kind::CodegenInvalidInstance(failure_detail(codegen_instance_failure(*cause), []))
        }
        CodegenPreparationError::InvalidUnit(cause) => {
            Kind::CodegenInvalidUnit(failure_detail(codegen_unit_preparation_failure(*cause), []))
        }
        CodegenPreparationError::UnitMismatch(unit) => Kind::CodegenUnitMismatch(failure_detail(
            "codegen_unit_mismatch",
            [crate::fact::diagnostic_context::identity_field(
                "codegen_unit",
                unit,
            )],
        )),
        CodegenPreparationError::MirCapacity(_) => Kind::CodegenMirCapacityExceeded,
        CodegenPreparationError::MissingRuntimeRole(role) => {
            Kind::CodegenMissingRuntimeRole(failure_detail(
                "codegen_missing_runtime_role",
                [text_failure_field("role", role.as_str())],
            ))
        }
        CodegenPreparationError::InvalidRuntimeRoleSourceBinding {
            role,
            expected,
            actual,
        } => Kind::CodegenInvalidAbiMapping(Some(runtime_role_signature_failure_detail(
            *role, expected, actual,
        ))),
        CodegenPreparationError::OpenConstantTerm(term) => Kind::CodegenOpenConstantTerm(
            identity_failure_detail("codegen_open_constant_term", "constant_term", term),
        ),
        CodegenPreparationError::InvalidArrayLength(term) => Kind::CodegenInvalidArrayLength(
            identity_failure_detail("codegen_invalid_array_length", "constant_term", term),
        ),
        CodegenPreparationError::RecursiveValueType(ty) => Kind::CodegenRecursiveValueType(
            identity_failure_detail("codegen_recursive_value_type", "semantic_type", ty),
        ),
        CodegenPreparationError::UnresolvedType(ty) => Kind::CodegenUnresolvedType(
            identity_failure_detail("codegen_unresolved_type", "semantic_type", ty),
        ),
        CodegenPreparationError::UnsizedTypeByValue(ty) => Kind::CodegenUnsizedTypeByValue(
            identity_failure_detail("codegen_unsized_type_by_value", "semantic_type", ty),
        ),
        CodegenPreparationError::InvalidAbiMapping => Kind::CodegenInvalidAbiMapping(None),
        CodegenPreparationError::UnsupportedType(ty) => Kind::CodegenUnsupportedType(
            identity_failure_detail("codegen_unsupported_type", "semantic_type", ty),
        ),
        CodegenPreparationError::MissingHelperInstance(helper) => {
            Kind::CodegenMissingHelperInstance(failure_detail(
                "codegen_missing_helper_instance",
                [
                    text_failure_field("helper_kind", mir_helper_kind(helper)),
                    crate::fact::diagnostic_context::identity_field("helper", helper),
                ],
            ))
        }
        CodegenPreparationError::Diagnostics(_) => return None,
        CodegenPreparationError::LayoutOverflow(ty) => Kind::CodegenLayoutOverflow(
            identity_failure_detail("codegen_layout_overflow", "semantic_type", ty),
        ),
        CodegenPreparationError::InvalidSymbolName => Kind::CodegenInvalidSymbolName,
        CodegenPreparationError::Query(error) => fact_query_failure_kind(error)?,
    })
}
const fn codegen_request_failure(error: bray_codegen::CodegenRequestBuildError) -> &'static str {
    use bray_codegen::CodegenRequestBuildError as Error;

    match error {
        Error::ArtifactUnitMismatch => "codegen_request_artifact_unit_mismatch",
        Error::MappingUnitMismatch => "codegen_request_mapping_unit_mismatch",
        Error::MappingTargetMismatch => "codegen_request_mapping_target_mismatch",
        Error::DebugInformationMismatch => "codegen_request_debug_information_mismatch",
        Error::DebugMappingCoverageMismatch => "codegen_request_debug_mapping_coverage_mismatch",
        Error::TargetMismatch => "codegen_request_target_mismatch",
    }
}

const fn codegen_instance_failure(error: CodegenInstanceBuildError) -> &'static str {
    match error {
        CodegenInstanceBuildError::TemplateMismatch => "codegen_instance_template_mismatch",
        CodegenInstanceBuildError::TargetMismatch => "codegen_instance_target_mismatch",
        CodegenInstanceBuildError::DependencyTargetMismatch => {
            "codegen_instance_dependency_target_mismatch"
        }
    }
}

pub(super) const fn codegen_unit_preparation_failure(error: CodegenUnitBuildError) -> &'static str {
    match error {
        CodegenUnitBuildError::Empty => "codegen_unit_empty",
        CodegenUnitBuildError::DuplicateInstance => "codegen_unit_duplicate_instance",
        CodegenUnitBuildError::MissingCompatibility => "codegen_unit_missing_compatibility",
        CodegenUnitBuildError::TargetMismatch => "codegen_unit_target_mismatch",
        CodegenUnitBuildError::WorkBoundExceeded => "codegen_unit_work_bound_exceeded",
        CodegenUnitBuildError::RecipeMismatch => "codegen_unit_recipe_mismatch",
    }
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
