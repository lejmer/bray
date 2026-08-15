use bray_codegen::{
    AssemblySyntaxKind, BackendArtifactKind, BackendArtifactRequestBuildError,
    DebugInformationMode, DebugInformationOutputMode,
};
use bray_package_interface::{InterfaceArtifact, PackageInterfaceIdentity};
use bray_symbols::ProductKind;
use bray_target::{TargetIdentity, TargetOutputDescription};

use super::{construction::PlanBuilder, validation::validate_request};
use crate::plan::{EmissionBackend, EmissionPlan};
use crate::{ArtifactKind, EmissionRequest, OutputSink, ProductIdentity};

/// Immutable target and completed producer inputs used to plan product emission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmissionPlanner {
    target: TargetOutputDescription,
    backend: Option<EmissionBackend>,
    package_interface: Option<InterfaceArtifact>,
}

impl EmissionPlanner {
    /// Creates a planner from validated target and producer inputs.
    pub const fn new(
        target: TargetOutputDescription,
        backend: Option<EmissionBackend>,
        package_interface: Option<InterfaceArtifact>,
    ) -> Self {
        Self {
            target,
            backend,
            package_interface,
        }
    }

    /// Derives the complete artifact and backend request plan for one host request.
    pub fn plan(&self, request: EmissionRequest) -> Result<EmissionPlan, EmissionPlanningError> {
        validate_request(self, &request, self.package_interface.as_ref())?;

        // The plan retains the immutable Arc-backed artifact independently of the planner.
        PlanBuilder::new(self, request, self.package_interface.clone()).build()
    }

    /// Returns the selected target output inputs.
    pub const fn target(&self) -> &TargetOutputDescription {
        &self.target
    }

    /// Returns the selected backend inputs when code generation is available.
    pub const fn backend(&self) -> Option<&EmissionBackend> {
        self.backend.as_ref()
    }
}

/// A request or selected-input conflict that prevents emission planning.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EmissionPlanningError {
    /// The request and selected target properties name different targets.
    TargetMismatch {
        /// Target selected by the host request.
        requested: TargetIdentity,
        /// Target covered by the planning inputs.
        selected: TargetIdentity,
    },
    /// The artifact category cannot be emitted for the selected product category.
    ProductArtifactMismatch {
        /// Selected language-level product category.
        product: ProductKind,
        /// Incompatible requested artifact category.
        artifact: ArtifactKind,
    },
    /// A required package-interface output has no completed artifact.
    MissingPackageInterfaceArtifact,
    /// A completed package-interface artifact was supplied without a matching request.
    UnexpectedPackageInterfaceArtifact,
    /// The completed package-interface artifact belongs to another product.
    PackageInterfaceProductMismatch {
        /// Product selected by the emission request.
        expected: ProductIdentity,
        /// Product recorded by the completed package interface.
        actual: PackageInterfaceIdentity,
    },
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
    /// The selected backend does not support the requested debug-information output placement.
    UnsupportedDebugOutput(DebugInformationOutputMode),
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
    /// Target output inputs contain no name for a published artifact category.
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
    /// A product or companion artifact selected an independent filesystem file.
    ManagedProductDestinationRequired,
    /// One artifact category exceeds the logical ordinal range.
    ArtifactOrdinalOverflow(ArtifactKind),
    /// Two published artifacts resolve to the same exact sink.
    OutputCollision(OutputSink),
    /// Derived backend policy could not form a valid per-unit request.
    InvalidBackendRequest(BackendArtifactRequestBuildError),
    /// Derived artifacts violated the complete plan contract.
    InconsistentPlan,
}
