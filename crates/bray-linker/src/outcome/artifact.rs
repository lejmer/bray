use std::sync::Arc;

use bray_symbols::ProductIdentity;
use bray_target::TargetIdentity;

use crate::{
    LinkPlan, LinkedArtifact, LinkedArtifactRequirement, LinkerDriverIdentity,
    StagingDestinationId,
};

/// Complete canonically ordered staging records for one link plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinkedArtifactSet {
    product: ProductIdentity,
    target: TargetIdentity,
    driver: LinkerDriverIdentity,
    artifacts: Arc<[LinkedArtifact]>,
}

impl LinkedArtifactSet {
    pub(super) fn try_new(
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
