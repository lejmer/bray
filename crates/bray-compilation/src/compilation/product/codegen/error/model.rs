use bray_codegen::{
    CodegenInstanceBuildError, CodegenPartitionError, CodegenReachabilityBuildError,
    CodegenTargetBuildError, CodegenUnitBuildError,
};
use bray_diagnostics::DiagnosticBag;
use bray_ir::MirCapacityError;
use bray_linker::LinkTargetBuildError;
use bray_runtime_interface::{ExecutableHostContractBuildError, RuntimeArtifactSelectionError};
use bray_symbols::ProductIdentity;

use crate::fact::{BatchCompletionError, FactQueryError};

use super::presentation::{native_product_failure_kind, native_product_preparation_diagnostic};

/// Exact native link-input contract failure found during product planning.
#[derive(Debug, Hash)]
pub enum NativeLinkInputPlanningError {
    /// A source or platform native-link requirement could not form a linker input.
    InvalidRequirement {
        /// Exact requested native input name.
        name: String,
        /// Exact requested native link category.
        kind: bray_symbols::NativeLinkKind,
        /// Exact provider retained for the input.
        provenance: bray_linker::LinkInputProvenance,
    },
}

/// A failure to derive complete native product plans.
#[derive(Debug, Hash)]
pub enum NativeProductPlanningError {
    /// The compilation has no selected code generation backend.
    CodegenUnavailable,
    /// The selected backend could not form its native bitcode target contract.
    BitcodeTargetContract(bray_codegen::CodegenFailure),
    /// The selected product has no executable code root.
    MissingProductRoot,
    /// The checked executable entry result is inconsistent with product semantics.
    InvalidEntryResult,
    /// An asynchronous product has no selected runtime artifact.
    MissingRuntime,
    /// A configured native link input is invalid.
    InvalidNativeLinkInput(NativeLinkInputPlanningError),
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
    /// A MIR identity table exceeded its compact representation.
    MirCapacity(MirCapacityError),
    /// The compiler-generated executable host contract is invalid.
    InvalidExecutableHost(ExecutableHostContractBuildError),
    /// Runtime metadata cannot supply an exact physical component selection.
    InvalidRuntimeSelection(RuntimeArtifactSelectionError),
    /// The selected linker target is invalid.
    InvalidLinkTarget(LinkTargetBuildError),
    /// Two selected artifacts disagree about the same library's native publication.
    ConflictingNativeArtifacts {
        /// First candidate artifact.
        first: std::path::PathBuf,
        /// Conflicting candidate artifact.
        second: std::path::PathBuf,
    },
    /// Required native symbols cannot close across the selected dependency set.
    NativeResolution(bray_native_artifact::NativeResolutionError),
    /// One code generation plan is unavailable.
    Codegen(crate::compilation::CodegenPreparationError),
}

pub(in crate::compilation::product::codegen) fn native_batch_error<K>(
    error: BatchCompletionError<K, NativeProductPlanningError>,
) -> NativeProductPlanningError {
    match error {
        BatchCompletionError::Cancelled => FactQueryError::Cancelled.into(),
        BatchCompletionError::Evaluation { error, .. } => error,
        BatchCompletionError::Scheduler(error) => error.into(),
    }
}

impl NativeProductPlanningError {
    /// Returns whether the selected target cannot realize a demanded native representation.
    pub const fn is_unsupported(&self) -> bool {
        matches!(
            self,
            Self::CodegenUnavailable
                | Self::BitcodeTargetContract(
                    bray_codegen::CodegenFailure::UnsupportedTarget
                        | bray_codegen::CodegenFailure::UnsupportedTargetReport { .. }
                        | bray_codegen::CodegenFailure::UnsupportedTargetValue { .. }
                        | bray_codegen::CodegenFailure::UnsupportedArtifact(_)
                )
                | Self::MissingRuntime
                | Self::Codegen(crate::compilation::CodegenPreparationError::UnsupportedType(_))
        )
    }

    /// Returns structured diagnostics produced while deriving native product plans.
    pub const fn diagnostics(&self) -> Option<&DiagnosticBag> {
        match self {
            Self::Codegen(crate::compilation::CodegenPreparationError::Diagnostics(
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
                | Self::Codegen(crate::compilation::CodegenPreparationError::Query(
                    FactQueryError::Cancelled
                ))
        )
    }

    /// Converts one terminal native-product planning failure with exact product and target context.
    pub fn diagnostic(&self, product: &ProductIdentity, target: &str) -> Option<DiagnosticBag> {
        let failure = native_product_failure_kind(self)?;

        let outer = DiagnosticBag::single(native_product_preparation_diagnostic(
            failure, product, target,
        ));

        Some(outer)
    }
}

impl From<FactQueryError> for NativeProductPlanningError {
    fn from(error: FactQueryError) -> Self {
        Self::Query(error)
    }
}

impl From<crate::compilation::CodegenPreparationError> for NativeProductPlanningError {
    fn from(error: crate::compilation::CodegenPreparationError) -> Self {
        Self::Codegen(error)
    }
}
