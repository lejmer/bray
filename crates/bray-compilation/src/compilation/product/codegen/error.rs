use bray_codegen::{
    CodegenInstanceBuildError, CodegenPartitionError, CodegenReachabilityBuildError,
    CodegenTargetBuildError, CodegenUnitBuildError,
};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind,
    DiagnosticNativeProductFailureKind, SeverityKind,
};
use bray_emitter::EmissionBackendBuildError;
use bray_linker::LinkTargetBuildError;
use bray_runtime_interface::{ExecutableHostContractBuildError, RuntimeArtifactSelectionError};
use bray_symbols::ProductIdentity;

use crate::fact::FactQueryError;

/// A failure to derive complete native product facts.
#[derive(Debug, Hash)]
pub enum NativeProductFactError {
    /// The compilation has no selected code generation backend.
    CodegenUnavailable,
    /// The selected product has no executable code root.
    MissingProductRoot,
    /// The checked executable entry result is inconsistent with product semantics.
    InvalidEntryResult,
    /// An asynchronous product has no selected runtime artifact.
    MissingRuntime,
    /// A generated binary symbol name is invalid.
    InvalidSymbolName,
    /// A configured native link input is invalid.
    InvalidNativeLinkInput,
    /// A lazy compilation fact could not be evaluated.
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
    /// One code generation fact is unavailable.
    Codegen(super::super::super::CodegenFactError),
}

impl NativeProductFactError {
    /// Returns whether the selected target cannot realize a demanded native representation.
    pub const fn is_unsupported(&self) -> bool {
        matches!(
            self,
            Self::CodegenUnavailable
                | Self::MissingRuntime
                | Self::Codegen(super::super::super::CodegenFactError::UnsupportedType(_))
        )
    }

    /// Returns structured diagnostics produced while deriving native product facts.
    pub const fn diagnostics(&self) -> Option<&bray_diagnostics::DiagnosticBag> {
        match self {
            Self::Codegen(super::super::super::CodegenFactError::Diagnostics(diagnostics)) => {
                Some(diagnostics)
            }
            _ => None,
        }
    }

    /// Returns whether fact evaluation observed cancellation rather than a terminal failure.
    pub const fn is_cancelled(&self) -> bool {
        matches!(
            self,
            Self::Query(FactQueryError::Cancelled)
                | Self::Codegen(super::super::super::CodegenFactError::Query(
                    FactQueryError::Cancelled
                ))
        )
    }

    /// Converts one terminal native-product planning failure with exact product and target context.
    pub fn diagnostic(&self, product: &ProductIdentity, target: &str) -> Option<DiagnosticBag> {
        let failure = native_product_failure_kind(self)?;

        Some(DiagnosticBag::single(
            Diagnostic::new(
                DiagnosticId::new(0),
                DiagnosticKind::NativeProductPreparationFailed,
                SeverityKind::Error,
            )
            .with_arg(DiagnosticArg::actual_product_identity(product.to_string()))
            .with_arg(DiagnosticArg::target_triple(target))
            .with_arg(DiagnosticArg::native_product_failure_kind(failure)),
        ))
    }
}

fn native_product_failure_kind(
    error: &NativeProductFactError,
) -> Option<DiagnosticNativeProductFailureKind> {
    use DiagnosticNativeProductFailureKind as Kind;

    Some(match error {
        NativeProductFactError::CodegenUnavailable => Kind::CodegenBackendNotSelected,
        NativeProductFactError::MissingProductRoot => Kind::MissingProductRoot,
        NativeProductFactError::InvalidEntryResult => Kind::InvalidEntryResult,
        NativeProductFactError::MissingRuntime => Kind::MissingRuntime,
        NativeProductFactError::InvalidSymbolName => Kind::InvalidSymbolName,
        NativeProductFactError::InvalidNativeLinkInput => Kind::InvalidNativeLinkInput,
        NativeProductFactError::Query(error) => fact_query_failure_kind(error)?,
        NativeProductFactError::InvalidCodegenTarget(error) => match error {
            CodegenTargetBuildError::UnsupportedProfile => Kind::CodegenTargetUnsupportedProfile,
            CodegenTargetBuildError::EmptyTriple => Kind::CodegenTargetEmptyTriple,
            CodegenTargetBuildError::EmptyCpu => Kind::CodegenTargetEmptyCpu,
            CodegenTargetBuildError::EmptyFeature => Kind::CodegenTargetEmptyFeature,
        },
        NativeProductFactError::InvalidReachability(error) => match error {
            CodegenReachabilityBuildError::EmptyRoots => Kind::ReachabilityEmptyRoots,
            CodegenReachabilityBuildError::DuplicateInstance => Kind::ReachabilityDuplicateInstance,
            CodegenReachabilityBuildError::InstanceWasNotDemanded => {
                Kind::ReachabilityUndemandedInstance
            }
            CodegenReachabilityBuildError::Incomplete => Kind::ReachabilityIncomplete,
        },
        NativeProductFactError::InvalidCodegenInstance(error) => match error {
            CodegenInstanceBuildError::TemplateMismatch => Kind::InstanceTemplateMismatch,
            CodegenInstanceBuildError::TargetMismatch => Kind::InstanceTargetMismatch,
            CodegenInstanceBuildError::DependencyTargetMismatch => {
                Kind::InstanceDependencyTargetMismatch
            }
        },
        NativeProductFactError::InvalidCodegenUnit(error) => codegen_unit_failure_kind(*error),
        NativeProductFactError::InvalidCodegenPartition(error) => match error {
            CodegenPartitionError::MissingCompatibility(_) => Kind::PartitionMissingCompatibility,
            CodegenPartitionError::InvalidUnit(_) => Kind::PartitionInvalidUnit,
        },
        NativeProductFactError::InvalidHostMir(_) => Kind::GeneratedHostMirInvalid,
        NativeProductFactError::InvalidExecutableHost(error) => match error {
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
        NativeProductFactError::InvalidRuntimeSelection(error) => match error {
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
        NativeProductFactError::InvalidEmissionBackend(
            EmissionBackendBuildError::DuplicateCodegenUnit,
        ) => Kind::EmissionBackendDuplicateUnit,
        NativeProductFactError::InvalidLinkTarget(LinkTargetBuildError::EmptyTriple) => {
            Kind::LinkTargetEmptyTriple
        }
        NativeProductFactError::StandardLibrary(_) => return None,
        NativeProductFactError::Codegen(error) => codegen_fact_failure_kind(error)?,
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

pub(in crate::compilation) fn codegen_fact_failure_kind(
    error: &super::super::super::CodegenFactError,
) -> Option<DiagnosticNativeProductFailureKind> {
    use super::super::super::CodegenFactError;
    use DiagnosticNativeProductFailureKind as Kind;

    Some(match error {
        CodegenFactError::CodegenUnavailable => Kind::CodegenBackendUnavailable,
        CodegenFactError::InvalidRequest(_) => Kind::CodegenInvalidRequest,
        CodegenFactError::MirUnavailable(_) => Kind::CodegenMirUnavailable,
        CodegenFactError::MissingEntrypoint => Kind::CodegenMissingEntrypoint,
        CodegenFactError::InvalidInstance(_) => Kind::CodegenInvalidInstance,
        CodegenFactError::InvalidUnit(_) => Kind::CodegenInvalidUnit,
        CodegenFactError::UnitMismatch(_) => Kind::CodegenUnitMismatch,
        CodegenFactError::InvalidHostMir(_) => Kind::CodegenInvalidHostMir,
        CodegenFactError::InvalidGeneratedLifecycleMir(_) => Kind::CodegenInvalidLifecycleMir,
        CodegenFactError::InvalidMappings(_) => Kind::CodegenInvalidMappings,
        CodegenFactError::MissingRuntimeRole(_) => Kind::CodegenMissingRuntimeRole,
        CodegenFactError::OpenConstantTerm(_) => Kind::CodegenOpenConstantTerm,
        CodegenFactError::InvalidArrayLength(_) => Kind::CodegenInvalidArrayLength,
        CodegenFactError::RecursiveValueType(_) => Kind::CodegenRecursiveValueType,
        CodegenFactError::UnresolvedType(_) => Kind::CodegenUnresolvedType,
        CodegenFactError::UnsizedTypeByValue(_) => Kind::CodegenUnsizedTypeByValue,
        CodegenFactError::InvalidAbiMapping => Kind::CodegenInvalidAbiMapping,
        CodegenFactError::UnsupportedType(_) => Kind::CodegenUnsupportedType,
        CodegenFactError::MissingHelperInstance(_) => Kind::CodegenMissingHelperInstance,
        CodegenFactError::Diagnostics(_) => return None,
        CodegenFactError::LayoutOverflow(_) => Kind::CodegenLayoutOverflow,
        CodegenFactError::InvalidSymbolName => Kind::CodegenInvalidSymbolName,
        CodegenFactError::Query(error) => fact_query_failure_kind(error)?,
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

impl From<FactQueryError> for NativeProductFactError {
    fn from(error: FactQueryError) -> Self {
        Self::Query(error)
    }
}

impl From<super::super::super::CodegenFactError> for NativeProductFactError {
    fn from(error: super::super::super::CodegenFactError) -> Self {
        Self::Codegen(error)
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        DiagnosticArgName, DiagnosticArgValue, DiagnosticKind, DiagnosticNativeProductFailureKind,
    };
    use bray_symbols::{PackageIdentity, ProductIdentity};
    use bray_testing::assert_goal_state_diagnostic_kind;

    use super::NativeProductFactError;

    #[test]
    fn native_product_failures_preserve_exact_product_target_and_reason() {
        let package = PackageIdentity::try_new("example")
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        let product = ProductIdentity::try_new(package, "application")
            .unwrap_or_else(|| panic!("test product identity must be valid"));

        let diagnostics = NativeProductFactError::MissingRuntime
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
    }
}
