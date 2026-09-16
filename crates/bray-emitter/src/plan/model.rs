use std::sync::Arc;

use bray_codegen::{
    BackendArtifactRequest, BackendCapabilityRevision, BackendIdentity, CodegenUnitKey,
};
use bray_package_interface::InterfaceArtifact;

use crate::{
    ArtifactId, ArtifactProducer, ArtifactRequirement, ArtifactRole, EmissionRequest, OutputSink,
};

/// Exact destination policy for one planned artifact.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PlannedArtifactDestination {
    /// Publish the complete artifact to this external sink.
    Publish(OutputSink),
    /// Retain the artifact in compiler-private staging for a later operation.
    Stage,
}

/// One exact immutable artifact operation in an emission plan.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PlannedArtifact {
    id: ArtifactId,
    requirement: ArtifactRequirement,
    role: ArtifactRole,
    producer: ArtifactProducer,
    destination: PlannedArtifactDestination,
}

impl PlannedArtifact {
    /// Creates one exact planner-owned artifact operation.
    pub(crate) const fn new(
        id: ArtifactId,
        requirement: ArtifactRequirement,
        role: ArtifactRole,
        producer: ArtifactProducer,
        destination: PlannedArtifactDestination,
    ) -> Self {
        Self {
            id,
            requirement,
            role,
            producer,
            destination,
        }
    }

    /// Returns the artifact's stable logical identity.
    pub const fn id(&self) -> &ArtifactId {
        &self.id
    }

    /// Returns whether product completion requires this artifact.
    pub const fn requirement(&self) -> ArtifactRequirement {
        self.requirement
    }

    /// Returns the artifact's lifecycle role.
    pub const fn role(&self) -> ArtifactRole {
        self.role
    }

    /// Returns the exact producer expected to supply the bytes.
    pub const fn producer(&self) -> &ArtifactProducer {
        &self.producer
    }

    /// Returns the exact publication or staging destination policy.
    pub const fn destination(&self) -> &PlannedArtifactDestination {
        &self.destination
    }
}

/// Complete immutable emission plan for one product request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmissionPlan {
    request: EmissionRequest,
    backend: Option<BackendIdentity>,
    capability_revision: Option<BackendCapabilityRevision>,
    package_interface: Option<InterfaceArtifact>,
    artifacts: Arc<[PlannedArtifact]>,
    backend_requests: Arc<[BackendArtifactRequest]>,
}

impl EmissionPlan {
    /// Creates a deterministic emission plan from planner-owned artifacts.
    pub(crate) fn new(
        request: EmissionRequest,
        backend: Option<BackendIdentity>,
        capability_revision: Option<BackendCapabilityRevision>,
        artifacts: impl IntoIterator<Item = PlannedArtifact>,
        backend_requests: impl IntoIterator<Item = BackendArtifactRequest>,
        package_interface: Option<InterfaceArtifact>,
    ) -> Self {
        let mut artifacts: Vec<_> = artifacts.into_iter().collect();
        let mut backend_requests: Vec<_> = backend_requests.into_iter().collect();

        artifacts.sort_unstable_by(|left, right| left.id().cmp(right.id()));
        backend_requests.sort_unstable_by(|left, right| left.unit().cmp(right.unit()));

        Self {
            request,
            backend,
            capability_revision,
            package_interface,
            artifacts: artifacts.into(),
            backend_requests: backend_requests.into(),
        }
    }

    /// Returns the host request from which this plan was derived.
    pub const fn request(&self) -> &EmissionRequest {
        &self.request
    }

    /// Returns the selected backend identity when code generation participates in the plan.
    pub const fn backend(&self) -> Option<&BackendIdentity> {
        self.backend.as_ref()
    }

    /// Returns the selected backend capability contract when code generation participates.
    pub const fn capability_revision(&self) -> Option<BackendCapabilityRevision> {
        self.capability_revision
    }

    /// Returns the completed package interface included in this plan.
    pub const fn package_interface(&self) -> Option<&InterfaceArtifact> {
        self.package_interface.as_ref()
    }

    /// Returns all planned artifacts in canonical logical-identity order.
    pub fn artifacts(&self) -> &[PlannedArtifact] {
        &self.artifacts
    }

    /// Returns one planned artifact by logical identity.
    pub fn artifact(&self, id: &ArtifactId) -> Option<&PlannedArtifact> {
        self.artifacts
            .binary_search_by(|artifact| artifact.id().cmp(id))
            .ok()
            .map(|index| &self.artifacts[index])
    }

    /// Iterates over externally published artifacts in deterministic plan order.
    pub fn published_artifacts(&self) -> impl Iterator<Item = &PlannedArtifact> {
        self.artifacts.iter().filter(|artifact| {
            matches!(
                artifact.destination(),
                PlannedArtifactDestination::Publish(_)
            )
        })
    }

    /// Iterates over compiler-private staged artifacts in deterministic plan order.
    pub fn staged_artifacts(&self) -> impl Iterator<Item = &PlannedArtifact> {
        self.artifacts
            .iter()
            .filter(|artifact| artifact.destination() == &PlannedArtifactDestination::Stage)
    }

    /// Returns exact per-unit backend requests in canonical codegen-unit order.
    pub fn backend_requests(&self) -> &[BackendArtifactRequest] {
        &self.backend_requests
    }

    /// Returns the exact backend request for one codegen unit.
    pub fn backend_request(&self, unit: &CodegenUnitKey) -> Option<&BackendArtifactRequest> {
        self.backend_requests
            .binary_search_by(|request| request.unit().cmp(unit))
            .ok()
            .map(|index| &self.backend_requests[index])
    }
}

#[cfg(test)]
mod tests {
    use super::EmissionPlan;

    #[test]
    fn plans_are_safe_to_share_between_workers() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<EmissionPlan>();
    }
}
