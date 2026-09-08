use bray_codegen::{CodegenInstanceBuildError, CodegenUnitBuildError};
use bray_diagnostics::{
    DiagnosticFailureField, DiagnosticFailureValue, DiagnosticNativeProductFailureDetail,
    DiagnosticNativeProductFailureKind,
};

use super::context::{
    count_failure_field, failure_detail, identity_failure_detail, text_failure_field,
};
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
        CodegenPreparationError::InvalidHostMir(cause) => {
            Kind::CodegenInvalidHostMir(mir_unit_failure_detail("codegen_invalid_host_mir", *cause))
        }
        CodegenPreparationError::InvalidGeneratedLifecycleMir { owner, cause } => {
            let detail = mir_unit_failure_detail("codegen_invalid_lifecycle_mir", *cause);
            let mut context = detail.context().to_vec();

            context.push(crate::fact::diagnostic_context::identity_field(
                "owner", owner,
            ));

            Kind::CodegenInvalidLifecycleMir(failure_detail(detail.reason(), context))
        }
        CodegenPreparationError::InvalidGeneratedLifecycleFrame(cause) => {
            let reason = match cause {
                bray_ir::MirFrameDescriptorBuildError::MissingState => {
                    "lifecycle_frame_missing_state"
                }
                bray_ir::MirFrameDescriptorBuildError::NonContiguousState => {
                    "lifecycle_frame_non_contiguous_state"
                }
                bray_ir::MirFrameDescriptorBuildError::DuplicateStateOrEntry => {
                    "lifecycle_frame_duplicate_state_or_entry"
                }
                bray_ir::MirFrameDescriptorBuildError::IdentityCapacityExceeded => {
                    "lifecycle_frame_identity_capacity_exceeded"
                }
            };

            Kind::CodegenInvalidLifecycleMir(failure_detail(reason, []))
        }
        CodegenPreparationError::InvalidSpecializedMir { source, cause } => {
            let detail = mir_unit_failure_detail("codegen_invalid_specialized_mir", *cause);
            let mut context = detail.context().to_vec();

            context.push(crate::fact::diagnostic_context::identity_field(
                "source", source,
            ));

            Kind::CodegenInvalidLifecycleMir(failure_detail(detail.reason(), context))
        }
        CodegenPreparationError::InvalidMappings(cause) => {
            Kind::CodegenInvalidMappings(failure_detail(codegen_mappings_failure(*cause), []))
        }
        CodegenPreparationError::InvalidCompilerProvidedMir { definition, cause } => {
            let detail = mir_unit_failure_detail("codegen_invalid_compiler_provided_mir", *cause);
            let mut context = detail.context().to_vec();

            context.push(crate::fact::diagnostic_context::identity_field(
                "callable", definition,
            ));

            Kind::CodegenInvalidCompilerProvidedMir(failure_detail(detail.reason(), context))
        }
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
                    text_failure_field("helper_kind", helper.kind_name()),
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
        Error::RuntimeContractMismatch => "codegen_request_runtime_contract_mismatch",
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

const fn codegen_mappings_failure(error: bray_codegen::CodegenMappingsBuildError) -> &'static str {
    use bray_codegen::CodegenMappingsBuildError as Error;

    match error {
        Error::TargetMismatch => "codegen_mappings_target_mismatch",
        Error::DuplicateType => "codegen_mappings_duplicate_type",
        Error::DuplicateInstanceType => "codegen_mappings_duplicate_instance_type",
        Error::InvalidInstanceType => "codegen_mappings_invalid_instance_type",
        Error::InvalidTypeLayout => "codegen_mappings_invalid_type_layout",
        Error::InvalidAbiTypeLayout => "codegen_mappings_invalid_abi_type_layout",
        Error::DuplicateSymbol => "codegen_mappings_duplicate_symbol",
        Error::DuplicateConstant => "codegen_mappings_duplicate_constant",
        Error::DuplicateConstantTerm => "codegen_mappings_duplicate_constant_term",
        Error::DuplicateCallable => "codegen_mappings_duplicate_callable",
        Error::DuplicateOperation => "codegen_mappings_duplicate_operation",
        Error::DuplicateStaticStorage => "codegen_mappings_duplicate_static_storage",
        Error::DuplicateNativeStaticStorage => "codegen_mappings_duplicate_native_static_storage",
        Error::StaticStorageCoverageMismatch => "codegen_mappings_static_storage_coverage_mismatch",
        Error::InvalidStaticStorage => "codegen_mappings_invalid_static_storage",
        Error::NativeStaticStorageCoverageMismatch => {
            "codegen_mappings_native_static_storage_coverage_mismatch"
        }
        Error::InvalidNativeStaticStorage => "codegen_mappings_invalid_native_static_storage",
        Error::DuplicateTerminator => "codegen_mappings_duplicate_terminator",
        Error::DuplicateBinarySymbolName => "codegen_mappings_duplicate_binary_symbol_name",
        Error::DuplicateDebugLocation => "codegen_mappings_duplicate_debug_location",
        Error::UnsupportedLinkage => "codegen_mappings_unsupported_linkage",
        Error::InvalidNativeEntry => "codegen_mappings_invalid_native_entry",
        Error::InstanceSymbolCoverageMismatch => {
            "codegen_mappings_instance_symbol_coverage_mismatch"
        }
        Error::CallableCoverageMismatch => "codegen_mappings_callable_coverage_mismatch",
        Error::OperationCoverageMismatch => "codegen_mappings_operation_coverage_mismatch",
        Error::TerminatorCoverageMismatch => "codegen_mappings_terminator_coverage_mismatch",
        Error::ConstantCoverageMismatch => "codegen_mappings_constant_coverage_mismatch",
        Error::InvalidConstantRepresentation => "codegen_mappings_invalid_constant_representation",
        Error::RuntimeSymbolCoverageMismatch => "codegen_mappings_runtime_symbol_coverage_mismatch",
        Error::FrameSymbolCoverageMismatch => "codegen_mappings_frame_symbol_coverage_mismatch",
        Error::TypeCoverageMismatch => "codegen_mappings_type_coverage_mismatch",
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

pub(super) fn mir_unit_failure_detail(
    reason: &'static str,
    error: bray_ir::MirUnitBuildError,
) -> DiagnosticNativeProductFailureDetail {
    use bray_diagnostics::DiagnosticMirUnitBuildFailureContext as Context;

    let failure = crate::compilation::lowering_diagnostic::mir_unit_failure(&error);
    let mut context = vec![text_failure_field("cause", failure.as_str())];

    match failure.context() {
        Context::None => {}
        Context::UnitMismatch { expected, actual } => {
            context.push(count_failure_field("expected_unit", expected));
            context.push(count_failure_field("actual_unit", actual));
        }
        Context::Operation(identity) => {
            push_mir_local_identity(&mut context, "operation", identity)
        }
        Context::Storage(identity) => {
            push_mir_local_identity(&mut context, "storage", identity);
        }
        Context::Value(identity) => {
            push_mir_local_identity(&mut context, "value", identity);
        }
        Context::Block(identity) => {
            push_mir_local_identity(&mut context, "block", identity);
        }
        Context::CleanupTarget { phase, target } => {
            context.push(text_failure_field("cleanup_phase", phase));

            push_mir_local_identity(&mut context, "target_block", target);
        }
        Context::RuntimeRoleMismatch { expected, actual } => {
            context.push(text_failure_field("expected_role", expected));
            context.push(text_failure_field("actual_role", actual));
        }
    }

    failure_detail(reason, context)
}

fn push_mir_local_identity(
    context: &mut Vec<DiagnosticFailureField>,
    prefix: &'static str,
    identity: bray_diagnostics::DiagnosticMirUnitLocalIdentity,
) {
    let unit_name = match prefix {
        "operation" => "operation_unit",
        "storage" => "storage_unit",
        "value" => "value_unit",
        "block" => "block_unit",
        "target_block" => "target_block_unit",
        _ => "mir_unit",
    };

    let slot_name = match prefix {
        "operation" => "operation_slot",
        "storage" => "storage_slot",
        "value" => "value_slot",
        "block" => "block_slot",
        "target_block" => "target_block_slot",
        _ => "mir_slot",
    };

    context.push(count_failure_field(unit_name, identity.unit()));
    context.push(count_failure_field(slot_name, identity.slot()));
}
