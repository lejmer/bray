use bray_codegen::{
    CodegenInstanceBuildError, CodegenPartitionError, CodegenReachabilityBuildError,
    CodegenTargetBuildError, CodegenUnitBuildError,
};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind,
    DiagnosticNativeProductFailureKind, DiagnosticNote, DiagnosticNoteKind, SeverityKind,
};
use bray_emitter::EmissionBackendBuildError;
use bray_linker::LinkTargetBuildError;
use bray_runtime_interface::{ExecutableHostContractBuildError, RuntimeArtifactSelectionError};
use bray_symbols::ProductIdentity;

use crate::fact::FactQueryError;

/// A failure to derive complete native product plans.
#[derive(Debug, Hash)]
pub enum NativeProductPlanningError {
    /// The compilation has no selected code generation backend.
    CodegenUnavailable,
    /// The selected product has no executable code root.
    MissingProductRoot,
    /// The checked executable entry result is inconsistent with product semantics.
    InvalidEntryResult,
    /// An asynchronous product has no selected runtime artifact.
    MissingRuntime,
    /// A library static cleanup closure requires unavailable main-thread execution.
    LibraryCleanupRequiresMainThread,
    /// A generated binary symbol name is invalid.
    InvalidSymbolName,
    /// A configured native link input is invalid.
    InvalidNativeLinkInput,
    /// A lazy compilation plan could not be evaluated.
    Query(FactQueryError),
    /// The selected code generation target is invalid.
    InvalidCodegenTarget(CodegenTargetBuildError),
    /// Code generation reachability is inconsistent.
    InvalidReachability(CodegenReachabilityBuildError),
    /// One concrete code generation instance is invalid.
    InvalidCodegenInstance(CodegenInstanceBuildError),
    /// One code generation unit is invalid.
    InvalidCodegenUnit(CodegenUnitBuildError),
    /// Reachable definitions cannot be partitioned under the selected policy.
    InvalidCodegenPartition(CodegenPartitionError),
    /// The compiler-generated executable host MIR is invalid.
    InvalidHostMir(bray_ir::MirUnitBuildError),
    /// The compiler-generated executable host contract is invalid.
    InvalidExecutableHost(ExecutableHostContractBuildError),
    /// Runtime metadata cannot supply an exact physical component selection.
    InvalidRuntimeSelection(RuntimeArtifactSelectionError),
    /// The selected emitter backend description is invalid.
    InvalidEmissionBackend(EmissionBackendBuildError),
    /// The selected linker target is invalid.
    InvalidLinkTarget(LinkTargetBuildError),
    /// The configured standard library cannot supply a required native artifact.
    StandardLibrary(bray_standard_library::StandardLibraryLoadError),
    /// One code generation plan is unavailable.
    Codegen(super::super::super::CodegenPreparationError),
}

impl NativeProductPlanningError {
    /// Returns whether the selected target cannot realize a demanded native representation.
    pub const fn is_unsupported(&self) -> bool {
        matches!(
            self,
            Self::CodegenUnavailable
                | Self::MissingRuntime
                | Self::Codegen(super::super::super::CodegenPreparationError::UnsupportedType(_))
        )
    }

    /// Returns structured diagnostics produced while deriving native product plans.
    pub const fn diagnostics(&self) -> Option<&DiagnosticBag> {
        match self {
            Self::Codegen(super::super::super::CodegenPreparationError::Diagnostics(
                diagnostics,
            )) => Some(diagnostics),
            _ => None,
        }
    }

    /// Returns whether plan evaluation observed cancellation rather than a terminal failure.
    pub const fn is_cancelled(&self) -> bool {
        matches!(
            self,
            Self::Query(FactQueryError::Cancelled)
                | Self::Codegen(super::super::super::CodegenPreparationError::Query(
                    FactQueryError::Cancelled
                ))
        )
    }

    /// Converts one terminal native-product planning failure with exact product and target context.
    pub fn diagnostic(&self, product: &ProductIdentity, target: &str) -> Option<DiagnosticBag> {
        let failure = native_product_failure_kind(self)?;

        Some(DiagnosticBag::single(
            native_product_preparation_diagnostic(failure, product, target),
        ))
    }
}

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
    .with_arg(DiagnosticArg::native_product_failure_kind(failure));

    match failure {
        DiagnosticNativeProductFailureKind::CodegenMirUnavailable => diagnostic.with_note(
            DiagnosticNote::new(DiagnosticNoteKind::NativeProductPreparationRecovery),
        ),
        _ => diagnostic,
    }
}

fn native_product_failure_kind(
    error: &NativeProductPlanningError,
) -> Option<DiagnosticNativeProductFailureKind> {
    use DiagnosticNativeProductFailureKind as Kind;

    Some(match error {
        NativeProductPlanningError::CodegenUnavailable => Kind::CodegenBackendNotSelected,
        NativeProductPlanningError::MissingProductRoot => Kind::MissingProductRoot,
        NativeProductPlanningError::InvalidEntryResult => Kind::InvalidEntryResult,
        NativeProductPlanningError::MissingRuntime => Kind::MissingRuntime,
        NativeProductPlanningError::LibraryCleanupRequiresMainThread => {
            Kind::LibraryCleanupRequiresMainThread
        }
        NativeProductPlanningError::InvalidSymbolName => Kind::InvalidSymbolName,
        NativeProductPlanningError::InvalidNativeLinkInput => Kind::InvalidNativeLinkInput,
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
        NativeProductPlanningError::InvalidRuntimeSelection(error) => match error {
            RuntimeArtifactSelectionError::IncompatibleRuntime(_) => {
                Kind::RuntimeSelectionIncompatible
            }
            RuntimeArtifactSelectionError::MissingRoleOwner(_) => {
                Kind::RuntimeSelectionMissingRoleOwner
            }
            RuntimeArtifactSelectionError::MissingCapabilityOwner(_) => {
                Kind::RuntimeSelectionMissingCapabilityOwner
            }
            RuntimeArtifactSelectionError::UnreadableArchive { .. } => {
                Kind::RuntimeSelectionUnreadableArchive
            }
            RuntimeArtifactSelectionError::InvalidArchive { .. } => {
                Kind::RuntimeSelectionInvalidArchive
            }
            RuntimeArtifactSelectionError::ArchiveDigestMismatch { .. } => {
                Kind::RuntimeSelectionArchiveDigestMismatch
            }
        },
        NativeProductPlanningError::InvalidEmissionBackend(
            EmissionBackendBuildError::DuplicateCodegenUnit,
        ) => Kind::EmissionBackendDuplicateUnit,
        NativeProductPlanningError::InvalidLinkTarget(LinkTargetBuildError::EmptyTriple) => {
            Kind::LinkTargetEmptyTriple
        }
        NativeProductPlanningError::StandardLibrary(_) => return None,
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

pub(in crate::compilation) fn codegen_preparation_failure_kind(
    error: &super::super::super::CodegenPreparationError,
) -> Option<DiagnosticNativeProductFailureKind> {
    use super::super::super::CodegenPreparationError;
    use DiagnosticNativeProductFailureKind as Kind;

    Some(match error {
        CodegenPreparationError::CodegenUnavailable => Kind::CodegenBackendUnavailable,
        CodegenPreparationError::InvalidRequest(_) => Kind::CodegenInvalidRequest,
        CodegenPreparationError::MirUnavailable(_) => Kind::CodegenMirUnavailable,
        CodegenPreparationError::MissingEntrypoint => Kind::CodegenMissingEntrypoint,
        CodegenPreparationError::InvalidInstance(_) => Kind::CodegenInvalidInstance,
        CodegenPreparationError::InvalidUnit(_) => Kind::CodegenInvalidUnit,
        CodegenPreparationError::UnitMismatch(_) => Kind::CodegenUnitMismatch,
        CodegenPreparationError::InvalidHostMir(_) => Kind::CodegenInvalidHostMir,
        CodegenPreparationError::InvalidGeneratedLifecycleMir(_) => {
            Kind::CodegenInvalidLifecycleMir
        }
        CodegenPreparationError::InvalidMappings(_) => Kind::CodegenInvalidMappings,
        CodegenPreparationError::MissingRuntimeRole(_) => Kind::CodegenMissingRuntimeRole,
        CodegenPreparationError::OpenConstantTerm(_) => Kind::CodegenOpenConstantTerm,
        CodegenPreparationError::InvalidArrayLength(_) => Kind::CodegenInvalidArrayLength,
        CodegenPreparationError::RecursiveValueType(_) => Kind::CodegenRecursiveValueType,
        CodegenPreparationError::UnresolvedType(_) => Kind::CodegenUnresolvedType,
        CodegenPreparationError::UnsizedTypeByValue(_) => Kind::CodegenUnsizedTypeByValue,
        CodegenPreparationError::InvalidAbiMapping => Kind::CodegenInvalidAbiMapping,
        CodegenPreparationError::UnsupportedType(_) => Kind::CodegenUnsupportedType,
        CodegenPreparationError::MissingHelperInstance(_) => Kind::CodegenMissingHelperInstance,
        CodegenPreparationError::Diagnostics(_) => return None,
        CodegenPreparationError::LayoutOverflow(_) => Kind::CodegenLayoutOverflow,
        CodegenPreparationError::InvalidSymbolName => Kind::CodegenInvalidSymbolName,
        CodegenPreparationError::Query(error) => fact_query_failure_kind(error)?,
    })
}

const fn fact_query_failure_kind(
    error: &FactQueryError,
) -> Option<DiagnosticNativeProductFailureKind> {
    use DiagnosticNativeProductFailureKind as Kind;

    match error {
        FactQueryError::Cancelled => None,
        FactQueryError::Cycle(_) => Some(Kind::EvaluationCycle),
        FactQueryError::InfrastructureFailure => Some(Kind::EvaluationInfrastructure),
        FactQueryError::SemanticUnitContext(_) => Some(Kind::SemanticContextFailure),
        FactQueryError::CheckerInfrastructure(_) => Some(Kind::CheckingInfrastructureFailure),
    }
}

impl From<FactQueryError> for NativeProductPlanningError {
    fn from(error: FactQueryError) -> Self {
        Self::Query(error)
    }
}

impl From<super::super::super::CodegenPreparationError> for NativeProductPlanningError {
    fn from(error: super::super::super::CodegenPreparationError) -> Self {
        Self::Codegen(error)
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        DiagnosticArgName, DiagnosticArgValue, DiagnosticKind, DiagnosticNativeProductFailureKind,
        DiagnosticNoteKind,
    };
    use bray_symbols::{PackageIdentity, ProductIdentity};
    use bray_testing::assert_goal_state_diagnostic_kind;

    use super::{NativeProductPlanningError, native_product_preparation_diagnostic};

    #[test]
    fn native_product_failures_preserve_exact_product_target_and_reason() {
        let package = PackageIdentity::try_new("example")
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        let product = ProductIdentity::try_new(package, "application")
            .unwrap_or_else(|| panic!("test product identity must be valid"));

        let diagnostics = NativeProductPlanningError::MissingRuntime
            .diagnostic(&product, "x86_64-pc-windows-msvc")
            .unwrap_or_else(|| panic!("terminal native product failure must diagnose"));

        assert_goal_state_diagnostic_kind(
            &diagnostics,
            DiagnosticKind::NativeProductPreparationFailed,
        );

        let diagnostic = diagnostics
            .iter()
            .next()
            .unwrap_or_else(|| panic!("test diagnostic must exist"));

        assert!(diagnostic.args().iter().any(|arg| {
            arg.name() == DiagnosticArgName::NativeProductFailureKind
                && matches!(
                    arg.value(),
                    DiagnosticArgValue::NativeProductFailureKind(
                        DiagnosticNativeProductFailureKind::MissingRuntime
                    )
                )
        }));

        assert!(diagnostic.notes().is_empty());
    }

    #[test]
    fn unavailable_native_program_provides_recovery_guidance() {
        let package = PackageIdentity::try_new("example")
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        let product = ProductIdentity::try_new(package, "application")
            .unwrap_or_else(|| panic!("test product identity must be valid"));

        let diagnostic = native_product_preparation_diagnostic(
            DiagnosticNativeProductFailureKind::CodegenMirUnavailable,
            &product,
            "x86_64-pc-windows-msvc",
        );

        assert_eq!(diagnostic.notes().len(), 1);

        assert_eq!(
            diagnostic.notes()[0].kind(),
            DiagnosticNoteKind::NativeProductPreparationRecovery
        );
    }
}
