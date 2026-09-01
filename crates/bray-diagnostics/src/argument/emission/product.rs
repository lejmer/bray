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

/// Exact native link-input failure retained by native product planning.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticNativeLinkInputFailure {
    /// An imported standard-library artifact has no supported static link representation.
    UnsupportedStandardLibraryArtifact {
        /// Exact imported artifact path.
        path: String,
        /// Stable rejected standard-library artifact category.
        artifact_kind: String,
    },
    /// An imported standard-library artifact could not form a link-input specification.
    InvalidStandardLibraryArtifact {
        /// Exact imported artifact path.
        path: String,
        /// Stable selected linker-input category.
        input_kind: String,
        /// Stable exact link-input contract failure.
        cause: String,
    },
    /// A source or platform native-link requirement could not form a linker input.
    InvalidRequirement {
        /// Exact requested native input name.
        name: String,
        /// Stable requested native-link category.
        link_kind: String,
        /// Exact retained provider identity.
        provenance: String,
    },
}

impl DiagnosticNativeLinkInputFailure {
    /// Returns the stable machine key for this link-input failure.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::UnsupportedStandardLibraryArtifact { .. } => {
                "native_link_input_unsupported_standard_library_artifact"
            }
            Self::InvalidStandardLibraryArtifact { .. } => {
                "native_link_input_invalid_standard_library_artifact"
            }
            Self::InvalidRequirement { .. } => "native_link_input_invalid_requirement",
        }
    }
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
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticNativeProductFailureKind {
    CodegenBackendNotSelected,
    MissingProductRoot,
    InvalidEntryResult,
    MissingRuntime,
    LibraryCleanupRequiresMainThread,
    InvalidSymbolName,
    InvalidNativeLinkInput(DiagnosticNativeLinkInputFailure),
    EvaluationCancelled,
    EvaluationCycle,
    EvaluationInfrastructure,
    EvaluationRuntime(crate::DiagnosticFactRuntimeFailure),
    EvaluationSemanticValueStoreCreate,
    EvaluationSemanticValue(crate::DiagnosticSemanticValueFailure),
    EvaluationBinding(crate::DiagnosticBindingFailure),
    EvaluationLoweringInput(super::DiagnosticLoweringInputFailure),
    EvaluationLowering(super::DiagnosticLoweringFailure),
    EvaluationConstantCallableBodyUnavailable,
    EvaluationConstantCallableRootUnavailable,
    EvaluationAtomicRepresentationTypeUnavailable,
    EvaluationAtomicRepresentationArgumentsUnavailable,
    EvaluationAtomicInitializerArgumentUnavailable,
    EvaluationAtomicInitializerResultUnavailable,
    EvaluationUninitInitializerResultUnavailable,
    EvaluationImportedExecutableTemplateMismatch,
    SemanticContextFailure,
    EvaluationSemanticQuery(crate::DiagnosticSemanticQueryFailure),
    /// Product specialization or realization violated an exact retained contract.
    EvaluationProduct(crate::DiagnosticProductQueryFailure),
    /// Foreign-boundary construction violated an exact retained contract.
    EvaluationForeign(crate::DiagnosticForeignQueryFailure),
    CheckingInfrastructureFailure,
    EvaluationChecker(crate::DiagnosticCheckerFailure),
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
    RuntimeSelectionInvalidArchive,
    RuntimeSelectionArchiveDigestMismatch,
    /// The configured standard library could not supply a required native artifact.
    StandardLibraryUnavailable,
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
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::CodegenBackendNotSelected => "codegen_backend_not_selected",
            Self::MissingProductRoot => "missing_product_root",
            Self::InvalidEntryResult => "invalid_entry_result",
            Self::MissingRuntime => "missing_runtime",
            Self::LibraryCleanupRequiresMainThread => "library_cleanup_requires_main_thread",
            Self::InvalidSymbolName => "invalid_symbol_name",
            Self::InvalidNativeLinkInput(failure) => failure.as_str(),
            Self::EvaluationCancelled => "evaluation_cancelled",
            Self::EvaluationCycle => "evaluation_cycle",
            Self::EvaluationInfrastructure => "evaluation_infrastructure",
            Self::EvaluationRuntime(failure) => failure.reason(),
            Self::EvaluationSemanticValueStoreCreate => "evaluation_semantic_value_store_create",
            Self::EvaluationSemanticValue(failure) => failure.as_str(),
            Self::EvaluationBinding(failure) => failure.as_str(),
            Self::EvaluationLoweringInput(failure) => failure.as_str(),
            Self::EvaluationLowering(failure) => failure.as_str(),
            Self::EvaluationConstantCallableBodyUnavailable => {
                "evaluation_constant_callable_body_unavailable"
            }
            Self::EvaluationConstantCallableRootUnavailable => {
                "evaluation_constant_callable_root_unavailable"
            }
            Self::EvaluationAtomicRepresentationTypeUnavailable => {
                "evaluation_atomic_representation_type_unavailable"
            }
            Self::EvaluationAtomicRepresentationArgumentsUnavailable => {
                "evaluation_atomic_representation_arguments_unavailable"
            }
            Self::EvaluationAtomicInitializerArgumentUnavailable => {
                "evaluation_atomic_initializer_argument_unavailable"
            }
            Self::EvaluationAtomicInitializerResultUnavailable => {
                "evaluation_atomic_initializer_result_unavailable"
            }
            Self::EvaluationUninitInitializerResultUnavailable => {
                "evaluation_uninit_initializer_result_unavailable"
            }
            Self::EvaluationImportedExecutableTemplateMismatch => {
                "evaluation_imported_executable_template_mismatch"
            }
            Self::SemanticContextFailure => "semantic_context_failure",
            Self::EvaluationSemanticQuery(failure) => failure.as_str(),
            Self::EvaluationProduct(failure) => failure.as_str(),
            Self::EvaluationForeign(failure) => failure.as_str(),
            Self::CheckingInfrastructureFailure => "checking_infrastructure_failure",
            Self::EvaluationChecker(failure) => failure.as_str(),
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
            Self::RuntimeSelectionInvalidArchive => "runtime_selection_invalid_archive",
            Self::RuntimeSelectionArchiveDigestMismatch => {
                "runtime_selection_archive_digest_mismatch"
            }
            Self::StandardLibraryUnavailable => "standard_library_unavailable",
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

impl From<crate::DiagnosticEmissionEvaluationFailure> for DiagnosticNativeProductFailureKind {
    fn from(failure: crate::DiagnosticEmissionEvaluationFailure) -> Self {
        use crate::DiagnosticEmissionEvaluationFailure as Failure;

        match failure {
            Failure::Cancelled => Self::EvaluationCancelled,
            Failure::Cycle => Self::EvaluationCycle,
            Failure::Infrastructure => Self::EvaluationInfrastructure,
            Failure::Runtime(failure) => Self::EvaluationRuntime(failure),
            Failure::SemanticValueStoreCreate => Self::EvaluationSemanticValueStoreCreate,
            Failure::SemanticValue(failure) => Self::EvaluationSemanticValue(failure),
            Failure::Binding(failure) => Self::EvaluationBinding(failure),
            Failure::LoweringInput(failure) => Self::EvaluationLoweringInput(failure),
            Failure::Lowering(failure) => Self::EvaluationLowering(failure),
            Failure::ConstantCallableBodyUnavailable => {
                Self::EvaluationConstantCallableBodyUnavailable
            }
            Failure::ConstantCallableRootUnavailable => {
                Self::EvaluationConstantCallableRootUnavailable
            }
            Failure::AtomicRepresentationTypeUnavailable => {
                Self::EvaluationAtomicRepresentationTypeUnavailable
            }
            Failure::AtomicRepresentationArgumentsUnavailable => {
                Self::EvaluationAtomicRepresentationArgumentsUnavailable
            }
            Failure::AtomicInitializerArgumentUnavailable => {
                Self::EvaluationAtomicInitializerArgumentUnavailable
            }
            Failure::AtomicInitializerResultUnavailable => {
                Self::EvaluationAtomicInitializerResultUnavailable
            }
            Failure::UninitInitializerResultUnavailable => {
                Self::EvaluationUninitInitializerResultUnavailable
            }
            Failure::ImportedExecutableTemplateMismatch => {
                Self::EvaluationImportedExecutableTemplateMismatch
            }
            Failure::SemanticContext => Self::SemanticContextFailure,
            Failure::SemanticQuery(failure) => Self::EvaluationSemanticQuery(failure),
            Failure::Product(failure) => Self::EvaluationProduct(failure),
            Failure::Foreign(failure) => Self::EvaluationForeign(failure),
            Failure::Checker(failure) => Self::EvaluationChecker(failure),
        }
    }
}
