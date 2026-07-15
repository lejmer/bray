use bray_codegen::{
    AssemblySyntaxKind, BackendArtifactKind, BackendArtifactRequestBuildError,
    DebugInformationMode, DebugInformationOutputMode,
};
use bray_symbols::ProductKind;
use bray_target::{TargetIdentity, TargetOutputDescription};

use super::{construction::PlanBuilder, validation::validate_request};
use crate::plan::{EmissionBackend, EmissionPlan, PackageInterfacePolicy};
use crate::{ArtifactKind, EmissionRequest, OutputSink};

/// Immutable target, backend, and package policy used to plan product emission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmissionPlanner {
    target: TargetOutputDescription,
    backend: Option<EmissionBackend>,
    package_interface: PackageInterfacePolicy,
}

impl EmissionPlanner {
    /// Creates a planner from validated target and producer facts.
    pub const fn new(
        target: TargetOutputDescription,
        backend: Option<EmissionBackend>,
        package_interface: PackageInterfacePolicy,
    ) -> Self {
        Self {
            target,
            backend,
            package_interface,
        }
    }

    /// Derives the complete artifact and backend request plan for one host request.
    pub fn plan(&self, request: EmissionRequest) -> Result<EmissionPlan, EmissionPlanningError> {
        validate_request(self, &request)?;

        PlanBuilder::new(self, request).build()
    }

    /// Returns the selected target output facts.
    pub const fn target(&self) -> &TargetOutputDescription {
        &self.target
    }

    /// Returns the selected backend facts when code generation is available.
    pub const fn backend(&self) -> Option<&EmissionBackend> {
        self.backend.as_ref()
    }

    /// Returns package-interface availability for planned products.
    pub const fn package_interface_policy(&self) -> PackageInterfacePolicy {
        self.package_interface
    }
}

/// A request or selected-fact conflict that prevents emission planning.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EmissionPlanningError {
    /// The request and selected target facts name different targets.
    TargetMismatch {
        /// Target selected by the host request.
        requested: TargetIdentity,
        /// Target covered by the planning facts.
        selected: TargetIdentity,
    },
    /// The artifact category cannot be emitted for the selected product category.
    ProductArtifactMismatch {
        /// Selected language-level product category.
        product: ProductKind,
        /// Incompatible requested artifact category.
        artifact: ArtifactKind,
    },
    /// Package-interface output is disabled by the selected policy.
    PackageInterfaceDisabled,
    /// A linked companion was requested without a linked product artifact.
    MissingLinkedProduct,
    /// More than one linked product was requested in one emission plan.
    MultipleLinkedProducts,
    /// A linked companion is required more strongly than its linked product.
    LinkedCompanionRequirementMismatch,
    /// The requested outputs require code generation but no backend was selected.
    MissingBackend,
    /// The requested outputs require code generation but no codegen units were selected.
    MissingCodegenUnits,
    /// The selected backend does not support the target machine.
    UnsupportedBackendTarget(TargetIdentity),
    /// The selected backend cannot produce a required artifact category.
    UnsupportedBackendArtifact(BackendArtifactKind),
    /// The selected backend does not support the requested debug-information mode.
    UnsupportedDebugInformation(DebugInformationMode),
    /// The selected backend does not support the requested assembly syntax.
    UnsupportedAssemblySyntax(AssemblySyntaxKind),
    /// Debug generation and serialization policy are incompatible.
    InvalidDebugOutput {
        /// Requested amount of generated debug information.
        information: DebugInformationMode,
        /// Requested placement of generated debug information.
        output: DebugInformationOutputMode,
    },
    /// Separate debug output was not requested as a required companion artifact.
    MissingRequiredDebugCompanion,
    /// A debug companion was requested for a non-separate debug output mode.
    UnexpectedDebugCompanion,
    /// A linked product has no selected backend link-input category.
    MissingLinkableArtifact,
    /// A serialization choice requires an artifact category that was not requested.
    MissingSerializationArtifact(BackendArtifactKind),
    /// Target output facts contain no name for a published artifact category.
    MissingOutputName(ArtifactKind),
    /// The product name cannot be used as one generated filename component.
    InvalidProductName,
    /// A generated artifact name is not a valid filename on the host.
    InvalidGeneratedFileName(ArtifactKind),
    /// One unkeyed output destination would receive more than one artifact.
    MultipleArtifactsForSingleSink,
    /// An explicit filesystem output path has no final filename component.
    MissingExplicitFileName,
    /// An explicit filesystem output path has an invalid host filename.
    InvalidExplicitFileName,
    /// An explicit filesystem output name conflicts with target suffix policy.
    ExplicitOutputSuffixMismatch(ArtifactKind),
    /// One artifact category exceeds the logical ordinal range.
    ArtifactOrdinalOverflow(ArtifactKind),
    /// Two published artifacts resolve to the same exact sink.
    OutputCollision(OutputSink),
    /// Derived backend policy could not form a valid per-unit request.
    InvalidBackendRequest(BackendArtifactRequestBuildError),
    /// Derived artifacts violated the complete plan contract.
    InconsistentPlan,
}
