use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates an exact unsupported native-emission reason argument.

    pub fn unsupported_emission_reason(reason: crate::DiagnosticUnsupportedEmissionReason) -> Self {
        Self::new(
            DiagnosticArgName::UnsupportedEmissionReason,
            DiagnosticArgValue::UnsupportedEmissionReason(reason),
        )
    }

    /// Creates an actual package-identity argument.
    pub fn actual_package_identity(identity: impl Into<String>) -> Self {
        Self::new(
            DiagnosticArgName::ActualPackageIdentity,
            DiagnosticArgValue::PackageIdentity(identity.into()),
        )
    }

    /// Creates an actual package-product identity argument.
    pub fn actual_product_identity(identity: impl Into<String>) -> Self {
        Self::new(
            DiagnosticArgName::ActualProductIdentity,
            DiagnosticArgValue::ProductIdentity(identity.into()),
        )
    }

    /// Creates an exact native-product preparation failure category.
    pub const fn native_product_failure_kind(kind: DiagnosticNativeProductFailureKind) -> Self {
        Self::new(
            DiagnosticArgName::NativeProductFailureKind,
            DiagnosticArgValue::NativeProductFailureKind(kind),
        )
    }

    /// Creates an exact terminal emission failure category.
    pub const fn emission_failure(failure: crate::DiagnosticEmissionFailure) -> Self {
        Self::new(
            DiagnosticArgName::EmissionFailure,
            DiagnosticArgValue::EmissionFailure(failure),
        )
    }

    /// Creates the product category found while planning emission.
    pub const fn actual_product_kind(kind: DiagnosticProductKind) -> Self {
        Self::new(
            DiagnosticArgName::ActualProductKind,
            DiagnosticArgValue::ProductKind(kind),
        )
    }
}

/// Locale-neutral language-level product category.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticProductKind {
    Executable,
    Library,
    Test,
}

impl DiagnosticProductKind {
    /// Returns the stable machine key for this product category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Executable => "executable",
            Self::Library => "library",
            Self::Test => "test",
        }
    }
}

/// Locale-neutral reason native product planning could not complete.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticNativeProductFailureKind {
    CodegenBackendNotSelected,
    MissingProductRoot,
    InvalidEntryResult,
    MissingRuntime,
    InvalidSymbolName,
    InvalidNativeLinkInput,
    EvaluationCycle,
    EvaluationInfrastructure,
    SemanticContextFailure,
    CheckingInfrastructureFailure,
    CodegenTargetUnsupportedProfile,
    CodegenTargetEmptyTriple,
    CodegenTargetEmptyCpu,
    CodegenTargetEmptyFeature,
    ReachabilityEmptyRoots,
    ReachabilityDuplicateInstance,
    ReachabilityUndemandedInstance,
    ReachabilityIncomplete,
    InstanceTemplateMismatch,
    InstanceTargetMismatch,
    InstanceDependencyTargetMismatch,
    UnitEmpty,
    UnitDuplicateInstance,
    UnitMissingCompatibility,
    UnitTargetMismatch,
    UnitWorkBoundExceeded,
    UnitRecipeMismatch,
    PartitionMissingCompatibility,
    PartitionInvalidUnit,
    GeneratedHostMirInvalid,
    ExecutableHostDuplicateRole,
    ExecutableHostMissingRuntime,
    ExecutableHostRuntimeOwnedBinding,
    ExecutableHostIncompatibleRuntime,
    ExecutableHostMissingMainThreadLane,
    ExecutableHostMissingProtectedFrameAbi,
    ExecutableHostMissingRole,
    RuntimeSelectionIncompatible,
    RuntimeSelectionMissingRoleOwner,
    RuntimeSelectionMissingCapabilityOwner,
    RuntimeSelectionUnreadableArchive,
    RuntimeSelectionArchiveDigestMismatch,
    EmissionBackendDuplicateUnit,
    LinkTargetEmptyTriple,
    CodegenBackendUnavailable,
    CodegenInvalidRequest,
    CodegenMirUnavailable,
    CodegenMissingEntrypoint,
    CodegenInvalidInstance,
    CodegenInvalidUnit,
    CodegenUnitMismatch,
    CodegenInvalidHostMir,
    CodegenInvalidLifecycleMir,
    CodegenInvalidMappings,
    CodegenMissingRuntimeRole,
    CodegenOpenConstantTerm,
    CodegenInvalidArrayLength,
    CodegenRecursiveValueType,
    CodegenUnresolvedType,
    CodegenUnsizedTypeByValue,
    CodegenInvalidAbiMapping,
    CodegenUnsupportedType,
    CodegenMissingHelperInstance,
    CodegenLayoutOverflow,
    CodegenInvalidSymbolName,
}

impl DiagnosticNativeProductFailureKind {
    /// Returns the stable machine key for this failure category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CodegenBackendNotSelected => "codegen_backend_not_selected",
            Self::MissingProductRoot => "missing_product_root",
            Self::InvalidEntryResult => "invalid_entry_result",
            Self::MissingRuntime => "missing_runtime",
            Self::InvalidSymbolName => "invalid_symbol_name",
            Self::InvalidNativeLinkInput => "invalid_native_link_input",
            Self::EvaluationCycle => "evaluation_cycle",
            Self::EvaluationInfrastructure => "evaluation_infrastructure",
            Self::SemanticContextFailure => "semantic_context_failure",
            Self::CheckingInfrastructureFailure => "checking_infrastructure_failure",
            Self::CodegenTargetUnsupportedProfile => "codegen_target_unsupported_profile",
            Self::CodegenTargetEmptyTriple => "codegen_target_empty_triple",
            Self::CodegenTargetEmptyCpu => "codegen_target_empty_cpu",
            Self::CodegenTargetEmptyFeature => "codegen_target_empty_feature",
            Self::ReachabilityEmptyRoots => "reachability_empty_roots",
            Self::ReachabilityDuplicateInstance => "reachability_duplicate_instance",
            Self::ReachabilityUndemandedInstance => "reachability_undemanded_instance",
            Self::ReachabilityIncomplete => "reachability_incomplete",
            Self::InstanceTemplateMismatch => "instance_template_mismatch",
            Self::InstanceTargetMismatch => "instance_target_mismatch",
            Self::InstanceDependencyTargetMismatch => "instance_dependency_target_mismatch",
            Self::UnitEmpty => "unit_empty",
            Self::UnitDuplicateInstance => "unit_duplicate_instance",
            Self::UnitMissingCompatibility => "unit_missing_compatibility",
            Self::UnitTargetMismatch => "unit_target_mismatch",
            Self::UnitWorkBoundExceeded => "unit_work_bound_exceeded",
            Self::UnitRecipeMismatch => "unit_recipe_mismatch",
            Self::PartitionMissingCompatibility => "partition_missing_compatibility",
            Self::PartitionInvalidUnit => "partition_invalid_unit",
            Self::GeneratedHostMirInvalid => "generated_host_mir_invalid",
            Self::ExecutableHostDuplicateRole => "executable_host_duplicate_role",
            Self::ExecutableHostMissingRuntime => "executable_host_missing_runtime",
            Self::ExecutableHostRuntimeOwnedBinding => "executable_host_runtime_owned_binding",
            Self::ExecutableHostIncompatibleRuntime => "executable_host_incompatible_runtime",
            Self::ExecutableHostMissingMainThreadLane => "executable_host_missing_main_thread_lane",
            Self::ExecutableHostMissingProtectedFrameAbi => {
                "executable_host_missing_protected_frame_abi"
            }
            Self::ExecutableHostMissingRole => "executable_host_missing_role",
            Self::RuntimeSelectionIncompatible => "runtime_selection_incompatible",
            Self::RuntimeSelectionMissingRoleOwner => "runtime_selection_missing_role_owner",
            Self::RuntimeSelectionMissingCapabilityOwner => {
                "runtime_selection_missing_capability_owner"
            }
            Self::RuntimeSelectionUnreadableArchive => "runtime_selection_unreadable_archive",
            Self::RuntimeSelectionArchiveDigestMismatch => {
                "runtime_selection_archive_digest_mismatch"
            }
            Self::EmissionBackendDuplicateUnit => "emission_backend_duplicate_unit",
            Self::LinkTargetEmptyTriple => "link_target_empty_triple",
            Self::CodegenBackendUnavailable => "codegen_backend_unavailable",
            Self::CodegenInvalidRequest => "codegen_invalid_request",
            Self::CodegenMirUnavailable => "codegen_mir_unavailable",
            Self::CodegenMissingEntrypoint => "codegen_missing_entrypoint",
            Self::CodegenInvalidInstance => "codegen_invalid_instance",
            Self::CodegenInvalidUnit => "codegen_invalid_unit",
            Self::CodegenUnitMismatch => "codegen_unit_mismatch",
            Self::CodegenInvalidHostMir => "codegen_invalid_host_mir",
            Self::CodegenInvalidLifecycleMir => "codegen_invalid_lifecycle_mir",
            Self::CodegenInvalidMappings => "codegen_invalid_mappings",
            Self::CodegenMissingRuntimeRole => "codegen_missing_runtime_role",
            Self::CodegenOpenConstantTerm => "codegen_open_constant_term",
            Self::CodegenInvalidArrayLength => "codegen_invalid_array_length",
            Self::CodegenRecursiveValueType => "codegen_recursive_value_type",
            Self::CodegenUnresolvedType => "codegen_unresolved_type",
            Self::CodegenUnsizedTypeByValue => "codegen_unsized_type_by_value",
            Self::CodegenInvalidAbiMapping => "codegen_invalid_abi_mapping",
            Self::CodegenUnsupportedType => "codegen_unsupported_type",
            Self::CodegenMissingHelperInstance => "codegen_missing_helper_instance",
            Self::CodegenLayoutOverflow => "codegen_layout_overflow",
            Self::CodegenInvalidSymbolName => "codegen_invalid_symbol_name",
        }
    }
}
