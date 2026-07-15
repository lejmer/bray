use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_codegen::{
    BackendArtifactId, BackendArtifactRequest, BackendArtifactRequirement, BackendIdentity,
    CodegenUnitKey,
};

use crate::{
    ArtifactId, ArtifactKind, ArtifactProducer, ArtifactRequirement, ArtifactRole, EmissionRequest,
    OutputSink,
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
    /// Creates one exact artifact operation before complete-plan validation.
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

/// Complete immutable emission policy for one product request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmissionPlan {
    request: EmissionRequest,
    backend: Option<BackendIdentity>,
    artifacts: Arc<[PlannedArtifact]>,
    backend_requests: Arc<[BackendArtifactRequest]>,
}

impl EmissionPlan {
    /// Creates a deterministic emission plan after validating its artifacts.
    pub(crate) fn try_new(
        request: EmissionRequest,
        backend: Option<BackendIdentity>,
        artifacts: impl IntoIterator<Item = PlannedArtifact>,
        backend_requests: impl IntoIterator<Item = BackendArtifactRequest>,
    ) -> Result<Self, EmissionPlanBuildError> {
        let mut artifacts: Vec<_> = artifacts.into_iter().collect();
        let mut backend_requests: Vec<_> = backend_requests.into_iter().collect();

        artifacts.sort_unstable_by(|left, right| left.id().cmp(right.id()));
        backend_requests.sort_unstable_by(|left, right| left.unit().cmp(right.unit()));

        validate_artifacts(&request, backend.as_ref(), &artifacts)?;
        validate_backend_requests(&artifacts, &backend_requests)?;

        Ok(Self {
            request,
            backend,
            artifacts: artifacts.into(),
            backend_requests: backend_requests.into(),
        })
    }

    /// Returns the host request from which this plan was derived.
    pub const fn request(&self) -> &EmissionRequest {
        &self.request
    }

    /// Returns the selected backend identity when code generation participates in the plan.
    pub const fn backend(&self) -> Option<&BackendIdentity> {
        self.backend.as_ref()
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

/// A structural contract violation that prevents creation of an emission plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum EmissionPlanBuildError {
    /// The plan contains no artifacts.
    Empty,
    /// One artifact belongs to another selected product.
    ForeignProduct(ArtifactId),
    /// One logical artifact identity appears more than once.
    DuplicateArtifact(ArtifactId),
    /// Two externally published artifacts select the same output sink.
    DuplicateSink(OutputSink),
    /// An in-memory sink uses a key other than its planned artifact identity.
    MemoryArtifactIdentityMismatch(ArtifactId),
    /// A required external artifact category has no published artifact.
    MissingRequestedArtifact(ArtifactKind),
    /// A published artifact category was not present in the host request.
    UnrequestedPublishedArtifact(ArtifactId),
    /// A published artifact changes the completion requirement from the host request.
    RequirementMismatch(ArtifactId),
    /// An artifact role is incompatible with publication or private staging.
    RoleDestinationMismatch(ArtifactId),
    /// An artifact category is incompatible with its product-lifecycle role.
    KindRoleMismatch(ArtifactId),
    /// A producer is not permitted to construct the planned artifact category.
    ProducerKindMismatch(ArtifactId),
    /// A backend producer names a different selected backend.
    BackendIdentityMismatch(BackendArtifactId),
    /// A backend producer is present without a selected backend.
    MissingBackend(BackendArtifactId),
    /// An artifact's category does not match its backend contribution category.
    BackendArtifactKindMismatch(ArtifactId),
    /// Two backend requests cover the same codegen unit.
    DuplicateBackendRequest(CodegenUnitKey),
    /// A backend producer has no matching request entry.
    MissingBackendRequest(BackendArtifactId),
    /// A backend request entry has no planned artifact consumer.
    UnmappedBackendRequest(BackendArtifactId),
    /// A backend request entry does not preserve the strongest mapped artifact requirement.
    BackendRequirementMismatch(BackendArtifactId),
}

fn validate_artifacts(
    request: &EmissionRequest,
    backend: Option<&BackendIdentity>,
    artifacts: &[PlannedArtifact],
) -> Result<(), EmissionPlanBuildError> {
    if artifacts.is_empty() {
        return Err(EmissionPlanBuildError::Empty);
    }

    let mut sinks = BTreeSet::new();

    for (index, artifact) in artifacts.iter().enumerate() {
        if artifact.id().product() != request.product() {
            // Plan errors retain Arc-backed artifact identities after validation returns.
            return Err(EmissionPlanBuildError::ForeignProduct(
                artifact.id().clone(),
            ));
        }

        if index > 0 && artifacts[index - 1].id() == artifact.id() {
            // Plan errors retain Arc-backed artifact identities after validation returns.
            return Err(EmissionPlanBuildError::DuplicateArtifact(
                artifact.id().clone(),
            ));
        }

        validate_role(artifact)?;
        validate_destination(request, artifact, &mut sinks)?;
        validate_producer(backend, artifact)?;
    }

    for requested in request.artifacts() {
        if requested.requirement() == ArtifactRequirement::Required
            && !artifacts.iter().any(|artifact| {
                artifact.id().kind() == requested.kind()
                    && artifact.requirement() == ArtifactRequirement::Required
                    && matches!(
                        artifact.destination(),
                        PlannedArtifactDestination::Publish(_)
                    )
            })
        {
            return Err(EmissionPlanBuildError::MissingRequestedArtifact(
                requested.kind(),
            ));
        }
    }

    Ok(())
}

fn validate_destination<'sink>(
    request: &EmissionRequest,
    artifact: &'sink PlannedArtifact,
    sinks: &mut BTreeSet<&'sink OutputSink>,
) -> Result<(), EmissionPlanBuildError> {
    let PlannedArtifactDestination::Publish(sink) = artifact.destination() else {
        return Ok(());
    };

    let Some(requested) = request.artifact(artifact.id().kind()) else {
        // Plan errors retain Arc-backed artifact identities after validation returns.
        return Err(EmissionPlanBuildError::UnrequestedPublishedArtifact(
            artifact.id().clone(),
        ));
    };

    if artifact.requirement() != requested.requirement() {
        // Plan errors retain Arc-backed artifact identities after validation returns.
        return Err(EmissionPlanBuildError::RequirementMismatch(
            artifact.id().clone(),
        ));
    }

    if let OutputSink::Memory {
        artifact: sink_artifact,
        ..
    } = sink
        && sink_artifact != artifact.id()
    {
        // Plan errors retain Arc-backed artifact identities after validation returns.
        return Err(EmissionPlanBuildError::MemoryArtifactIdentityMismatch(
            artifact.id().clone(),
        ));
    }

    if !sinks.insert(sink) {
        // Plan errors retain owned destination facts after validation returns.
        return Err(EmissionPlanBuildError::DuplicateSink(sink.clone()));
    }

    Ok(())
}

fn validate_role(artifact: &PlannedArtifact) -> Result<(), EmissionPlanBuildError> {
    let destination_is_valid = matches!(
        (artifact.role(), artifact.destination()),
        (ArtifactRole::LinkInput, PlannedArtifactDestination::Stage)
            | (
                ArtifactRole::Product | ArtifactRole::Inspection | ArtifactRole::Companion,
                PlannedArtifactDestination::Publish(_)
            )
    );

    if !destination_is_valid {
        // Plan errors retain Arc-backed artifact identities after validation returns.
        return Err(EmissionPlanBuildError::RoleDestinationMismatch(
            artifact.id().clone(),
        ));
    }

    if !artifact.id().kind().supports_role(artifact.role()) {
        // Plan errors retain Arc-backed artifact identities after validation returns.
        return Err(EmissionPlanBuildError::KindRoleMismatch(
            artifact.id().clone(),
        ));
    }

    Ok(())
}

fn validate_producer(
    backend: Option<&BackendIdentity>,
    artifact: &PlannedArtifact,
) -> Result<(), EmissionPlanBuildError> {
    match artifact.producer() {
        ArtifactProducer::Backend {
            artifact: backend_artifact,
            backend: producer_backend,
        } => {
            let Some(backend) = backend else {
                // Plan errors retain Arc-backed backend artifact identities after validation returns.
                return Err(EmissionPlanBuildError::MissingBackend(
                    backend_artifact.clone(),
                ));
            };

            validate_backend_producer(backend, artifact, backend_artifact, producer_backend)
        }
        ArtifactProducer::PackageInterface
            if artifact.id().kind() == ArtifactKind::PackageInterface =>
        {
            Ok(())
        }
        ArtifactProducer::DependencyMetadata(_)
            if artifact.id().kind() == ArtifactKind::DependencyMetadata =>
        {
            Ok(())
        }
        ArtifactProducer::Linker(_)
            if matches!(
                artifact.id().kind(),
                ArtifactKind::Executable
                    | ArtifactKind::StaticLibrary
                    | ArtifactKind::SharedLibrary
                    | ArtifactKind::LinkedCompanion
            ) =>
        {
            Ok(())
        }
        ArtifactProducer::PackageInterface
        | ArtifactProducer::DependencyMetadata(_)
        | ArtifactProducer::Linker(_) => {
            // Plan errors retain Arc-backed artifact identities after validation returns.
            Err(EmissionPlanBuildError::ProducerKindMismatch(
                artifact.id().clone(),
            ))
        }
    }
}

fn validate_backend_producer(
    backend: &BackendIdentity,
    artifact: &PlannedArtifact,
    backend_artifact: &BackendArtifactId,
    producer_backend: &BackendIdentity,
) -> Result<(), EmissionPlanBuildError> {
    if producer_backend != backend {
        // Plan errors retain Arc-backed backend artifact identities after validation returns.
        return Err(EmissionPlanBuildError::BackendIdentityMismatch(
            backend_artifact.clone(),
        ));
    }

    if artifact.id().kind() == ArtifactKind::from(backend_artifact.kind()) {
        return Ok(());
    }

    // Plan errors retain Arc-backed artifact identities after validation returns.
    Err(EmissionPlanBuildError::BackendArtifactKindMismatch(
        artifact.id().clone(),
    ))
}

fn validate_backend_requests(
    artifacts: &[PlannedArtifact],
    requests: &[BackendArtifactRequest],
) -> Result<(), EmissionPlanBuildError> {
    if let Some(pair) = requests
        .windows(2)
        .find(|pair| pair[0].unit() == pair[1].unit())
    {
        // Plan errors retain structural codegen-unit identities after validation returns.
        return Err(EmissionPlanBuildError::DuplicateBackendRequest(
            pair[0].unit().clone(),
        ));
    }

    let mut mapped_requirements = BTreeMap::new();

    for artifact in artifacts {
        let Some(backend_artifact) = artifact.producer().backend_artifact() else {
            continue;
        };

        let Ok(request_index) =
            requests.binary_search_by(|request| request.unit().cmp(backend_artifact.unit()))
        else {
            // Plan errors retain Arc-backed backend artifact identities after validation returns.
            return Err(EmissionPlanBuildError::MissingBackendRequest(
                backend_artifact.clone(),
            ));
        };

        if requests[request_index].entry(backend_artifact).is_none() {
            // Plan errors retain Arc-backed backend artifact identities after validation returns.
            return Err(EmissionPlanBuildError::MissingBackendRequest(
                backend_artifact.clone(),
            ));
        }

        let is_required = artifact.requirement() == ArtifactRequirement::Required;

        mapped_requirements
            .entry(backend_artifact)
            .and_modify(|required| *required |= is_required)
            .or_insert(is_required);
    }

    for request in requests {
        for entry in request.entries() {
            let Some(&has_required_mapping) = mapped_requirements.get(entry.id()) else {
                // Plan errors retain Arc-backed backend artifact identities after validation returns.
                return Err(EmissionPlanBuildError::UnmappedBackendRequest(
                    entry.id().clone(),
                ));
            };

            let expected_requirement = if has_required_mapping {
                BackendArtifactRequirement::Required
            } else {
                BackendArtifactRequirement::Optional
            };

            if entry.requirement() != expected_requirement {
                // Plan errors retain Arc-backed backend artifact identities after validation returns.
                return Err(EmissionPlanBuildError::BackendRequirementMismatch(
                    entry.id().clone(),
                ));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{EmissionPlan, EmissionPlanBuildError};
    use crate::test_support::{
        backend_artifact_plan_parts, emission_plan, linked_artifact, product_identity,
        target_identity,
    };
    use crate::{
        ArtifactId, ArtifactKind, ArtifactProducer, ArtifactRequirement, ArtifactRole,
        DependencyMetadataProducerId, EmissionRequest, OutputSink, OutputSinkId, PlannedArtifact,
        PlannedArtifactDestination, ProductKind, ReplacementPolicy, RequestedArtifact,
        RequestedArtifactDestination,
    };

    #[test]
    fn plans_are_deterministic_and_reject_sink_collisions() {
        let plan = emission_plan();

        let reversed = EmissionPlan::try_new(
            plan.request().clone(),
            plan.backend().cloned(),
            plan.artifacts().iter().cloned().rev(),
            plan.backend_requests().iter().cloned().rev(),
        );

        assert_eq!(reversed, Ok(plan.clone()));
        assert_eq!(plan.backend(), None);

        let first = linked_artifact(
            ArtifactKind::Executable,
            crate::ArtifactRole::Product,
            "same-output",
            0,
        );

        let second = linked_artifact(
            ArtifactKind::LinkedCompanion,
            crate::ArtifactRole::Companion,
            "same-output",
            1,
        );

        let request = crate::test_support::emission_request([
            crate::RequestedArtifact::new(
                ArtifactKind::Executable,
                crate::ArtifactRequirement::Required,
            ),
            crate::RequestedArtifact::new(
                ArtifactKind::LinkedCompanion,
                crate::ArtifactRequirement::Required,
            ),
        ]);

        assert_eq!(
            EmissionPlan::try_new(request, None, [first, second], [],),
            Err(EmissionPlanBuildError::DuplicateSink(
                OutputSink::Filesystem("same-output".into())
            ))
        );
    }

    #[test]
    fn plans_require_exact_backend_request_mappings() {
        let (request, backend, artifact, backend_request) = backend_artifact_plan_parts();

        assert_eq!(
            EmissionPlan::try_new(
                request.clone(),
                Some(backend.clone()),
                [artifact.clone()],
                [],
            ),
            Err(EmissionPlanBuildError::MissingBackendRequest(
                backend_artifact_id(&artifact)
            ))
        );

        let Ok(plan) = EmissionPlan::try_new(request, Some(backend), [artifact], [backend_request])
        else {
            panic!("matching test backend request must produce a valid plan");
        };

        assert_eq!(plan.backend_requests().len(), 1);

        assert!(matches!(
            plan.artifacts()[0].destination(),
            PlannedArtifactDestination::Publish(_)
        ));
    }

    #[test]
    fn memory_collectors_accept_distinct_planned_artifact_keys() {
        let Some(collector) = OutputSinkId::try_new("host.output") else {
            panic!("test memory collector identity must be valid");
        };

        let request = memory_request(
            collector.clone(),
            [
                ArtifactKind::PackageInterface,
                ArtifactKind::DependencyMetadata,
            ],
        );

        let interface_id =
            ArtifactId::new(request.product().clone(), ArtifactKind::PackageInterface, 0);

        let metadata_id = ArtifactId::new(
            request.product().clone(),
            ArtifactKind::DependencyMetadata,
            0,
        );

        let interface = PlannedArtifact::new(
            interface_id.clone(),
            ArtifactRequirement::Required,
            ArtifactRole::Product,
            ArtifactProducer::PackageInterface,
            PlannedArtifactDestination::Publish(OutputSink::Memory {
                collector: collector.clone(),
                artifact: interface_id,
            }),
        );

        let metadata = PlannedArtifact::new(
            metadata_id.clone(),
            ArtifactRequirement::Required,
            ArtifactRole::Companion,
            ArtifactProducer::DependencyMetadata(DependencyMetadataProducerId::new(0)),
            PlannedArtifactDestination::Publish(OutputSink::Memory {
                collector,
                artifact: metadata_id,
            }),
        );

        let Ok(plan) = EmissionPlan::try_new(request, None, [metadata, interface], []) else {
            panic!("distinct keys in one memory collector must form a valid plan");
        };

        assert_eq!(plan.published_artifacts().count(), 2);
    }

    #[test]
    fn plans_reject_artifact_kinds_with_incompatible_roles() {
        let artifact = linked_artifact(
            ArtifactKind::Executable,
            ArtifactRole::Inspection,
            "application",
            0,
        );

        let artifact_id = artifact.id().clone();

        let request = crate::test_support::emission_request([RequestedArtifact::new(
            ArtifactKind::Executable,
            ArtifactRequirement::Required,
        )]);

        assert_eq!(
            EmissionPlan::try_new(request, None, [artifact], []),
            Err(EmissionPlanBuildError::KindRoleMismatch(artifact_id))
        );
    }

    #[test]
    fn one_immutable_contribution_can_feed_published_and_staged_artifacts() {
        let (request, backend, published, backend_request) = backend_artifact_plan_parts();

        let staged = crate::PlannedArtifact::new(
            crate::ArtifactId::new(
                request.product().clone(),
                ArtifactKind::RelocatableObject,
                1,
            ),
            crate::ArtifactRequirement::Required,
            crate::ArtifactRole::LinkInput,
            published.producer().clone(),
            PlannedArtifactDestination::Stage,
        );

        let Ok(plan) = EmissionPlan::try_new(
            request,
            Some(backend),
            [staged, published],
            [backend_request],
        ) else {
            panic!("one backend contribution may satisfy multiple planned artifact operations");
        };

        assert_eq!(plan.artifacts().len(), 2);
        assert_eq!(plan.published_artifacts().count(), 1);
        assert_eq!(plan.staged_artifacts().count(), 1);

        assert!(
            plan.backend_request(plan.backend_requests()[0].unit())
                .is_some()
        );
    }

    #[test]
    fn plans_are_safe_to_share_between_workers() {
        assert_send_sync::<EmissionPlan>();
    }

    fn backend_artifact_id(artifact: &crate::PlannedArtifact) -> bray_codegen::BackendArtifactId {
        let Some(id) = artifact.producer().backend_artifact() else {
            panic!("test artifact must have a backend producer");
        };

        id.clone()
    }

    fn memory_request(
        collector: OutputSinkId,
        kinds: impl IntoIterator<Item = ArtifactKind>,
    ) -> EmissionRequest {
        let artifacts = kinds
            .into_iter()
            .map(|kind| RequestedArtifact::new(kind, ArtifactRequirement::Required));

        let Ok(request) = EmissionRequest::try_new(
            product_identity(),
            ProductKind::Executable,
            target_identity(),
            RequestedArtifactDestination::Memory(collector),
            artifacts,
            ReplacementPolicy::RequireAbsent,
        ) else {
            panic!("test memory emission request must be valid");
        };

        request
    }

    fn assert_send_sync<T: Send + Sync>() {}
}
