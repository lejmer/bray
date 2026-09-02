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

/// Exact locale-neutral context retained by one native-product planning failure.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticNativeProductFailureDetail {
    reason: &'static str,
    context: Box<[crate::DiagnosticFailureField]>,
}

impl DiagnosticNativeProductFailureDetail {
    /// Creates one exact failure detail from its stable leaf reason and typed context.
    pub fn new(
        reason: &'static str,
        context: impl Into<Box<[crate::DiagnosticFailureField]>>,
    ) -> Self {
        Self {
            reason,
            context: context.into(),
        }
    }

    /// Returns the stable leaf reason.
    pub const fn reason(&self) -> &'static str {
        self.reason
    }

    /// Returns the complete typed leaf context.
    pub const fn context(&self) -> &[crate::DiagnosticFailureField] {
        &self.context
    }
}

/// Exact native link-input failure retained by native product planning.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticNativeLinkInputFailure {
    /// An imported standard-library artifact has no supported static link representation.
    UnsupportedStandardLibraryArtifact {
        /// Exact imported artifact path.
        path: std::path::PathBuf,
        /// Stable rejected standard-library artifact category.
        artifact_kind: &'static str,
    },
    /// An imported standard-library artifact could not form a link-input specification.
    InvalidStandardLibraryArtifact {
        /// Exact imported artifact path.
        path: std::path::PathBuf,
        /// Stable selected linker-input category.
        input_kind: &'static str,
        /// Stable exact link-input contract failure.
        cause: &'static str,
    },
    /// A source or platform native-link requirement could not form a linker input.
    InvalidRequirement {
        /// Exact requested native input name.
        name: String,
        /// Stable requested native-link category.
        link_kind: String,
        /// Exact retained provider identity.
        provenance_kind: &'static str,
        /// Stable provider identity when the provenance names one.
        provenance_identity: Option<String>,
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
    EvaluationCycle(crate::DiagnosticEvaluationFailureDetail),
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
    SemanticContextFailure(crate::DiagnosticEvaluationFailureDetail),
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
    PartitionMissingCompatibility(DiagnosticNativeProductFailureDetail),
    PartitionInvalidUnit(DiagnosticNativeProductFailureDetail),
    GeneratedHostMirInvalid(DiagnosticNativeProductFailureDetail),
    ExecutableHostDuplicateRole(DiagnosticNativeProductFailureDetail),
    ExecutableHostMissingRuntime,
    ExecutableHostRuntimeOwnedBinding(DiagnosticNativeProductFailureDetail),
    ExecutableHostIncompatibleRuntime(DiagnosticNativeProductFailureDetail),
    ExecutableHostMissingMainThreadLane,
    ExecutableHostMissingProtectedFrameAbi,
    ExecutableHostMissingRole(DiagnosticNativeProductFailureDetail),
    RuntimeSelectionIncompatible(DiagnosticNativeProductFailureDetail),
    RuntimeSelectionMissingRoleOwner(DiagnosticNativeProductFailureDetail),
    RuntimeSelectionMissingCapabilityOwner(DiagnosticNativeProductFailureDetail),
    RuntimeSelectionUnreadableArchive(DiagnosticNativeProductFailureDetail),
    RuntimeSelectionInvalidArchive(DiagnosticNativeProductFailureDetail),
    RuntimeSelectionArchiveDigestMismatch(DiagnosticNativeProductFailureDetail),
    /// The configured standard library could not supply a required native artifact.
    StandardLibraryUnavailable,
    EmissionBackendDuplicateUnit,
    LinkTargetEmptyTriple,
    CodegenBackendUnsupportedTarget,
    CodegenBackendUnsupportedTargetDetail(DiagnosticNativeProductFailureDetail),
    CodegenBackendUnsupportedArtifact(DiagnosticNativeProductFailureDetail),
    CodegenBackendInvalidConfiguration,
    CodegenBackendInvalidConfigurationDetail(DiagnosticNativeProductFailureDetail),
    CodegenBackendResourceExhausted,
    CodegenBackendResourceLimit(DiagnosticNativeProductFailureDetail),
    CodegenBackendLibraryFailure(DiagnosticNativeProductFailureDetail),
    CodegenBackendToolFailure(DiagnosticNativeProductFailureDetail),
    CodegenBackendGeneratedModuleInvariant,
    CodegenBackendGeneratedModuleInvariantDetail(DiagnosticNativeProductFailureDetail),
    CodegenBackendInvalidRuntimeMetadata(DiagnosticNativeProductFailureDetail),
    CodegenBackendInvalidOutcome(DiagnosticNativeProductFailureDetail),
    CodegenBackendRejectedModule(DiagnosticNativeProductFailureDetail),
    CodegenBackendArtifactConstruction(DiagnosticNativeProductFailureDetail),
    CodegenBackendUnavailable,
    CodegenInvalidRequest(DiagnosticNativeProductFailureDetail),
    CodegenMirUnavailable(DiagnosticNativeProductFailureDetail),
    CodegenMissingEntrypoint,
    CodegenInvalidInstance(DiagnosticNativeProductFailureDetail),
    CodegenInvalidUnit(DiagnosticNativeProductFailureDetail),
    CodegenUnitMismatch(DiagnosticNativeProductFailureDetail),
    CodegenInvalidHostMir(DiagnosticNativeProductFailureDetail),
    CodegenInvalidLifecycleMir(DiagnosticNativeProductFailureDetail),
    CodegenInvalidMappings(DiagnosticNativeProductFailureDetail),
    CodegenMissingRuntimeRole(DiagnosticNativeProductFailureDetail),
    CodegenOpenConstantTerm(DiagnosticNativeProductFailureDetail),
    CodegenInvalidArrayLength(DiagnosticNativeProductFailureDetail),
    CodegenRecursiveValueType(DiagnosticNativeProductFailureDetail),
    CodegenUnresolvedType(DiagnosticNativeProductFailureDetail),
    CodegenUnsizedTypeByValue(DiagnosticNativeProductFailureDetail),
    CodegenInvalidAbiMapping(Option<DiagnosticNativeProductFailureDetail>),
    CodegenUnsupportedType(DiagnosticNativeProductFailureDetail),
    CodegenMissingHelperInstance(DiagnosticNativeProductFailureDetail),
    CodegenLayoutOverflow(DiagnosticNativeProductFailureDetail),
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
            Self::EvaluationCycle(_) => "evaluation_cycle",
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
            Self::SemanticContextFailure(failure) => failure.reason(),
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
            Self::PartitionMissingCompatibility(detail)
            | Self::PartitionInvalidUnit(detail)
            | Self::GeneratedHostMirInvalid(detail)
            | Self::ExecutableHostDuplicateRole(detail)
            | Self::ExecutableHostRuntimeOwnedBinding(detail)
            | Self::ExecutableHostIncompatibleRuntime(detail)
            | Self::ExecutableHostMissingRole(detail) => detail.reason(),
            Self::ExecutableHostMissingRuntime => "executable_host_missing_runtime",
            Self::ExecutableHostMissingMainThreadLane => "executable_host_missing_main_thread_lane",
            Self::ExecutableHostMissingProtectedFrameAbi => {
                "executable_host_missing_protected_frame_abi"
            }
            Self::RuntimeSelectionIncompatible(detail)
            | Self::RuntimeSelectionMissingRoleOwner(detail)
            | Self::RuntimeSelectionMissingCapabilityOwner(detail)
            | Self::RuntimeSelectionUnreadableArchive(detail)
            | Self::RuntimeSelectionInvalidArchive(detail)
            | Self::RuntimeSelectionArchiveDigestMismatch(detail) => detail.reason(),
            Self::StandardLibraryUnavailable => "standard_library_unavailable",
            Self::EmissionBackendDuplicateUnit => "emission_backend_duplicate_unit",
            Self::LinkTargetEmptyTriple => "link_target_empty_triple",
            Self::CodegenBackendUnsupportedTarget => "codegen_backend_unsupported_target",
            Self::CodegenBackendUnsupportedTargetDetail(detail)
            | Self::CodegenBackendUnsupportedArtifact(detail)
            | Self::CodegenBackendInvalidConfigurationDetail(detail)
            | Self::CodegenBackendLibraryFailure(detail)
            | Self::CodegenBackendToolFailure(detail)
            | Self::CodegenBackendInvalidRuntimeMetadata(detail)
            | Self::CodegenBackendInvalidOutcome(detail)
            | Self::CodegenBackendRejectedModule(detail)
            | Self::CodegenBackendArtifactConstruction(detail) => detail.reason(),
            Self::CodegenBackendInvalidConfiguration => "codegen_backend_invalid_configuration",
            Self::CodegenBackendResourceExhausted => "codegen_backend_resource_exhausted",
            Self::CodegenBackendResourceLimit(detail) => detail.reason(),
            Self::CodegenBackendGeneratedModuleInvariant => {
                "codegen_backend_generated_module_invariant"
            }
            Self::CodegenBackendGeneratedModuleInvariantDetail(detail) => detail.reason(),
            Self::CodegenBackendUnavailable => "codegen_backend_unavailable",
            Self::CodegenInvalidRequest(detail)
            | Self::CodegenMirUnavailable(detail)
            | Self::CodegenInvalidInstance(detail)
            | Self::CodegenInvalidUnit(detail)
            | Self::CodegenUnitMismatch(detail)
            | Self::CodegenInvalidHostMir(detail)
            | Self::CodegenInvalidLifecycleMir(detail)
            | Self::CodegenInvalidMappings(detail)
            | Self::CodegenMissingRuntimeRole(detail)
            | Self::CodegenOpenConstantTerm(detail)
            | Self::CodegenInvalidArrayLength(detail)
            | Self::CodegenRecursiveValueType(detail)
            | Self::CodegenUnresolvedType(detail)
            | Self::CodegenUnsizedTypeByValue(detail)
            | Self::CodegenUnsupportedType(detail)
            | Self::CodegenMissingHelperInstance(detail)
            | Self::CodegenLayoutOverflow(detail) => detail.reason(),
            Self::CodegenMissingEntrypoint => "codegen_missing_entrypoint",
            Self::CodegenInvalidAbiMapping(Some(detail)) => detail.reason(),
            Self::CodegenInvalidAbiMapping(None) => "codegen_invalid_abi_mapping",
            Self::CodegenInvalidSymbolName => "codegen_invalid_symbol_name",
        }
    }
}

impl From<crate::DiagnosticEmissionEvaluationFailure> for DiagnosticNativeProductFailureKind {
    fn from(failure: crate::DiagnosticEmissionEvaluationFailure) -> Self {
        use crate::DiagnosticEmissionEvaluationFailure as Failure;

        match failure {
            Failure::Cancelled => Self::EvaluationCancelled,
            Failure::Cycle(failure) => Self::EvaluationCycle(failure),
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
            Failure::SemanticContext(failure) => Self::SemanticContextFailure(failure),
            Failure::SemanticQuery(failure) => Self::EvaluationSemanticQuery(failure),
            Failure::Product(failure) => Self::EvaluationProduct(failure),
            Failure::Foreign(failure) => Self::EvaluationForeign(failure),
            Failure::Checker(failure) => Self::EvaluationChecker(failure),
        }
    }
}
