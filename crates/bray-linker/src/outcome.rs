use std::sync::Arc;

use bray_diagnostics::DiagnosticBag;
use bray_symbols::ProductIdentity;
use bray_target::TargetIdentity;

use crate::{
    LinkInputId, LinkPlan, LinkedArtifact, LinkedArtifactRequirement, LinkerDriverIdentity,
    StagingDestinationId,
};

/// Structured reason one native link operation could not produce complete staged outputs.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LinkFailure {
    /// No configured linker driver is available for the request.
    DriverUnavailable,
    /// The selected driver is incompatible with the target or product contract.
    DriverIncompatible,
    /// One planned input was unavailable at invocation time.
    MissingInput(LinkInputId),
    /// Driver-owned response-file construction failed.
    ResponseFile,
    /// Linker or archiver invocation could not complete successfully.
    Invocation,
    /// One required linked output was not produced.
    MissingOutput(StagingDestinationId),
    /// One produced linked output violated its staging contract.
    InvalidOutput(StagingDestinationId),
    /// Linking exceeded an available resource or process budget.
    ResourceExhausted,
}

/// Complete canonically ordered staging records for one link plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinkedArtifactSet {
    product: ProductIdentity,
    target: TargetIdentity,
    driver: LinkerDriverIdentity,
    artifacts: Arc<[LinkedArtifact]>,
}

impl LinkedArtifactSet {
    fn try_new(
        plan: &LinkPlan,
        artifacts: impl IntoIterator<Item = LinkedArtifact>,
    ) -> Result<Self, LinkedArtifactSetBuildError> {
        let mut artifacts: Vec<_> = artifacts.into_iter().collect();

        artifacts.sort_unstable_by_key(LinkedArtifact::destination);

        if let Some(pair) = artifacts
            .windows(2)
            .find(|pair| pair[0].destination() == pair[1].destination())
        {
            return Err(LinkedArtifactSetBuildError::DuplicateArtifact(
                pair[0].destination(),
            ));
        }

        for artifact in &artifacts {
            validate_artifact(plan, artifact)?;
        }

        for output in plan.outputs() {
            if output.requirement() == LinkedArtifactRequirement::Required
                && artifacts
                    .binary_search_by_key(&output.destination().id(), LinkedArtifact::destination)
                    .is_err()
            {
                return Err(LinkedArtifactSetBuildError::MissingRequired(
                    output.destination().id(),
                ));
            }
        }

        Ok(Self {
            // Completed results retain the Arc-backed product identity independently of the plan.
            product: plan.product().clone(),
            // Completed results retain the Arc-backed target identity independently of the plan.
            target: plan.target().identity().clone(),
            // Completed results retain driver revision metadata independently of the plan.
            driver: plan.driver().clone(),
            artifacts: artifacts.into(),
        })
    }

    /// Returns the selected package product.
    pub const fn product(&self) -> &ProductIdentity {
        &self.product
    }

    /// Returns the exact target identity used by the link operation.
    pub const fn target(&self) -> &TargetIdentity {
        &self.target
    }

    /// Returns the exact driver and toolchain identity used by the operation.
    pub const fn driver(&self) -> &LinkerDriverIdentity {
        &self.driver
    }

    /// Returns staged artifacts in canonical staging-identity order.
    pub fn artifacts(&self) -> &[LinkedArtifact] {
        &self.artifacts
    }

    /// Returns one completed artifact by emitter-owned staging identity.
    pub fn artifact(&self, destination: StagingDestinationId) -> Option<&LinkedArtifact> {
        self.artifacts
            .binary_search_by_key(&destination, LinkedArtifact::destination)
            .ok()
            .map(|index| &self.artifacts[index])
    }
}

/// A contract violation that prevents complete linked artifacts from being exposed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkedArtifactSetBuildError {
    /// One staging identity appears more than once.
    DuplicateArtifact(StagingDestinationId),
    /// A required staged output has no completed record.
    MissingRequired(StagingDestinationId),
    /// A completed record was not present in the authoritative plan.
    UnplannedArtifact(StagingDestinationId),
    /// A completed record names the wrong staged artifact category.
    KindMismatch(StagingDestinationId),
}

fn validate_artifact(
    plan: &LinkPlan,
    artifact: &LinkedArtifact,
) -> Result<(), LinkedArtifactSetBuildError> {
    let Some(output) = plan.output(artifact.destination()) else {
        return Err(LinkedArtifactSetBuildError::UnplannedArtifact(
            artifact.destination(),
        ));
    };

    if output.kind() != artifact.kind() {
        return Err(LinkedArtifactSetBuildError::KindMismatch(
            artifact.destination(),
        ));
    }

    Ok(())
}

/// Atomic completion state of one native link operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkStatus {
    /// Every required staged artifact was produced and validated.
    Complete(LinkedArtifactSet),
    /// Linking failed without returning partial staged outputs.
    Failed(LinkFailure),
    /// Cancellation was observed before a successful result was available.
    Cancelled,
}

/// Immutable native-link result and locale-neutral diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinkOutcome {
    status: LinkStatus,
    diagnostics: DiagnosticBag,
}

impl LinkOutcome {
    /// Validates staging records against the authoritative plan before publishing success.
    pub fn try_complete(
        plan: &LinkPlan,
        artifacts: impl IntoIterator<Item = LinkedArtifact>,
        diagnostics: DiagnosticBag,
    ) -> Result<Self, LinkOutcomeBuildError> {
        if diagnostics.has_errors() {
            return Err(LinkOutcomeBuildError::ErrorDiagnostics(diagnostics));
        }

        let artifacts = LinkedArtifactSet::try_new(plan, artifacts)
            .map_err(LinkOutcomeBuildError::InvalidArtifacts)?;

        Ok(Self {
            status: LinkStatus::Complete(artifacts),
            diagnostics,
        })
    }

    /// Creates a failed result without partial staged outputs.
    pub const fn failed(failure: LinkFailure, diagnostics: DiagnosticBag) -> Self {
        Self {
            status: LinkStatus::Failed(failure),
            diagnostics,
        }
    }

    /// Creates a cancelled result without partial staged outputs.
    pub const fn cancelled(diagnostics: DiagnosticBag) -> Self {
        Self {
            status: LinkStatus::Cancelled,
            diagnostics,
        }
    }

    /// Returns the atomic completion state.
    pub const fn status(&self) -> &LinkStatus {
        &self.status
    }

    /// Returns diagnostics produced by the completed or failed operation.
    pub const fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }

    /// Returns complete staged artifacts only after successful validation.
    pub const fn artifacts(&self) -> Option<&LinkedArtifactSet> {
        match &self.status {
            LinkStatus::Complete(artifacts) => Some(artifacts),
            LinkStatus::Failed(_) | LinkStatus::Cancelled => None,
        }
    }
}

/// A contract violation that prevents creation of a successful link outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkOutcomeBuildError {
    /// Error diagnostics contradict a successful status.
    ErrorDiagnostics(DiagnosticBag),
    /// Completed staging records do not satisfy the authoritative link plan.
    InvalidArtifacts(LinkedArtifactSetBuildError),
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind};

    use super::{
        LinkFailure, LinkOutcome, LinkOutcomeBuildError, LinkStatus, LinkedArtifactSetBuildError,
    };
    use crate::test_support::{link_plan, linked_artifact};
    use crate::{LinkedArtifact, LinkedArtifactKind, StagingDestinationId};

    #[test]
    fn complete_outcomes_publish_only_plan_validated_artifacts() {
        let plan = link_plan();
        let artifact = linked_artifact(&plan);

        let Ok(outcome) = LinkOutcome::try_complete(&plan, [artifact], DiagnosticBag::new()) else {
            panic!("matching test artifact must complete linking");
        };

        let Some(artifacts) = outcome.artifacts() else {
            panic!("complete outcome must retain staged artifacts");
        };

        assert_eq!(artifacts.product(), plan.product());
        assert_eq!(artifacts.target(), plan.target().identity());
        assert_eq!(artifacts.driver(), plan.driver());
        assert_eq!(artifacts.artifacts().len(), 1);
    }

    #[test]
    fn complete_outcomes_reject_missing_and_mismatched_outputs() {
        let plan = link_plan();

        assert_eq!(
            LinkOutcome::try_complete(&plan, [], DiagnosticBag::new()),
            Err(LinkOutcomeBuildError::InvalidArtifacts(
                LinkedArtifactSetBuildError::MissingRequired(StagingDestinationId::new(0))
            ))
        );

        let mismatched = LinkedArtifact::new(
            LinkedArtifactKind::SharedLibrary,
            StagingDestinationId::new(0),
            NonZeroU64::MIN,
        );

        assert_eq!(
            LinkOutcome::try_complete(&plan, [mismatched], DiagnosticBag::new()),
            Err(LinkOutcomeBuildError::InvalidArtifacts(
                LinkedArtifactSetBuildError::KindMismatch(StagingDestinationId::new(0))
            ))
        );
    }

    #[test]
    fn complete_outcomes_reject_error_diagnostics() {
        let plan = link_plan();
        let artifact = linked_artifact(&plan);

        let diagnostics = DiagnosticBag::single(Diagnostic::new(
            DiagnosticId::new(1),
            DiagnosticKind::RequestMissingSourceInput,
            SeverityKind::Error,
        ));

        assert_eq!(
            LinkOutcome::try_complete(&plan, [artifact], diagnostics.clone()),
            Err(LinkOutcomeBuildError::ErrorDiagnostics(diagnostics))
        );
    }

    #[test]
    fn failed_and_cancelled_outcomes_expose_no_partial_artifacts() {
        let failed = LinkOutcome::failed(LinkFailure::Invocation, DiagnosticBag::new());

        let diagnostics = DiagnosticBag::single(Diagnostic::new(
            DiagnosticId::new(1),
            DiagnosticKind::RequestMissingSourceInput,
            SeverityKind::Error,
        ));

        let cancelled = LinkOutcome::cancelled(diagnostics.clone());

        assert!(matches!(failed.status(), LinkStatus::Failed(_)));
        assert_eq!(failed.artifacts(), None);

        assert!(matches!(cancelled.status(), LinkStatus::Cancelled));
        assert_eq!(cancelled.artifacts(), None);
        assert_eq!(cancelled.diagnostics(), &diagnostics);
    }
}
