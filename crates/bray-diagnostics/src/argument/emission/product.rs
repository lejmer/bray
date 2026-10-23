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
    MissingRuntime,
    InvalidNativeLinkInput(DiagnosticNativeLinkInputFailure),
    EvaluationCancelled,
    EvaluationCycle(crate::DiagnosticEvaluationFailureDetail),
    EvaluationRuntime(crate::DiagnosticFactRuntimeFailure),
    EvaluationSemanticValueStoreCreate,
    EvaluationSemanticValue(crate::DiagnosticSemanticValueFailure),
    EvaluationBinding(crate::DiagnosticBindingFailure),
    EvaluationSemanticQuery(crate::DiagnosticSemanticQueryFailure),
    /// Product specialization or realization violated an exact retained contract.
    EvaluationProduct(crate::DiagnosticProductQueryFailure),
    /// Foreign-boundary construction violated an exact retained contract.
    EvaluationForeign(crate::DiagnosticForeignQueryFailure),
    CodegenTargetUnsupportedProfile,
    NativeResolution(DiagnosticNativeProductFailureDetail),
    RuntimeSelectionIncompatible(DiagnosticNativeProductFailureDetail),
    RuntimeSelectionMissingRoleOwner(DiagnosticNativeProductFailureDetail),
    RuntimeSelectionMissingCapabilityOwner(DiagnosticNativeProductFailureDetail),
    CodegenBackendUnsupportedTarget,
    CodegenBackendUnsupportedTargetDetail(DiagnosticNativeProductFailureDetail),
    CodegenBackendUnsupportedArtifact(DiagnosticNativeProductFailureDetail),
    CodegenBackendInvalidConfiguration,
    CodegenBackendInvalidConfigurationDetail(DiagnosticNativeProductFailureDetail),
    CodegenBackendResourceExhausted,
    CodegenBackendResourceLimit(DiagnosticNativeProductFailureDetail),
    CodegenBackendLibraryFailure(DiagnosticNativeProductFailureDetail),
    CodegenBackendToolFailure(DiagnosticNativeProductFailureDetail),
    CodegenBackendArtifactConstruction(DiagnosticNativeProductFailureDetail),
    CodegenBackendArtifactRead(DiagnosticNativeProductFailureDetail),
    CodegenBackendUnavailable,
    CodegenMirCapacityExceeded,
    /// Required exact integer representation exceeds the backend limit.
    CodegenExactIntegerCapacity {
        /// Smallest integer width required by the contract expression.
        minimum_bits: u64,
        /// Largest width this compiler implementation can represent.
        maximum_bits: u16,
    },
    CodegenInvalidAbiMapping(Option<DiagnosticNativeProductFailureDetail>),
    CodegenUnsupportedType(DiagnosticNativeProductFailureDetail),
    CodegenLayoutOverflow(DiagnosticNativeProductFailureDetail),
}

impl DiagnosticNativeProductFailureKind {
    /// Returns the stable machine key for this failure category.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::CodegenBackendNotSelected => "codegen_backend_not_selected",
            Self::MissingRuntime => "missing_runtime",
            Self::InvalidNativeLinkInput(failure) => failure.as_str(),
            Self::EvaluationCancelled => "evaluation_cancelled",
            Self::EvaluationCycle(_) => "evaluation_cycle",
            Self::EvaluationRuntime(failure) => failure.reason(),
            Self::EvaluationSemanticValueStoreCreate => "evaluation_semantic_value_store_create",
            Self::EvaluationSemanticValue(failure) => failure.as_str(),
            Self::EvaluationBinding(failure) => failure.as_str(),
            Self::EvaluationSemanticQuery(failure) => failure.as_str(),
            Self::EvaluationProduct(failure) => failure.as_str(),
            Self::EvaluationForeign(failure) => failure.as_str(),
            Self::CodegenTargetUnsupportedProfile => "codegen_target_unsupported_profile",
            Self::NativeResolution(detail)
            | Self::RuntimeSelectionIncompatible(detail)
            | Self::RuntimeSelectionMissingRoleOwner(detail)
            | Self::RuntimeSelectionMissingCapabilityOwner(detail) => detail.reason(),
            Self::CodegenBackendUnsupportedTarget => "codegen_backend_unsupported_target",
            Self::CodegenBackendUnsupportedTargetDetail(detail)
            | Self::CodegenBackendUnsupportedArtifact(detail)
            | Self::CodegenBackendInvalidConfigurationDetail(detail)
            | Self::CodegenBackendLibraryFailure(detail)
            | Self::CodegenBackendToolFailure(detail)
            | Self::CodegenBackendArtifactConstruction(detail)
            | Self::CodegenBackendArtifactRead(detail) => detail.reason(),
            Self::CodegenBackendInvalidConfiguration => "codegen_backend_invalid_configuration",
            Self::CodegenBackendResourceExhausted => "codegen_backend_resource_exhausted",
            Self::CodegenBackendResourceLimit(detail) => detail.reason(),
            Self::CodegenBackendUnavailable => "codegen_backend_unavailable",
            Self::CodegenUnsupportedType(detail) | Self::CodegenLayoutOverflow(detail) => {
                detail.reason()
            }
            Self::CodegenExactIntegerCapacity { .. } => "codegen_exact_integer_capacity",
            Self::CodegenMirCapacityExceeded => "codegen_mir_capacity_exceeded",
            Self::CodegenInvalidAbiMapping(Some(detail)) => detail.reason(),
            Self::CodegenInvalidAbiMapping(None) => "codegen_invalid_abi_mapping",
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
            Failure::MirCapacity => Self::CodegenMirCapacityExceeded,
            Failure::SemanticQuery(failure) => Self::EvaluationSemanticQuery(failure),
            Failure::Product(failure) => Self::EvaluationProduct(failure),
            Failure::Foreign(failure) => Self::EvaluationForeign(failure),
        }
    }
}
