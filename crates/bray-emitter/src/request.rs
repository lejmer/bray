use std::path::PathBuf;
use std::sync::Arc;

use bray_codegen::TargetIdentity;

use crate::{
    ArtifactKind, ArtifactRequirement, OutputSinkId, ProductIdentity, ProductKind,
    ReplacementPolicy,
};

/// High-level destination supplied before deterministic output names are planned.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RequestedArtifactDestination {
    /// Derive one or more final artifact paths beneath this directory.
    FilesystemDirectory(PathBuf),
    /// Publish exactly one externally requested artifact at this path.
    FilesystemFile(PathBuf),
    /// Publish keyed artifacts to a host-owned in-memory collector.
    Memory(OutputSinkId),
    /// Publish exactly one externally requested artifact to a host-owned stream.
    Stream(OutputSinkId),
}

/// One externally requested artifact category and its completion requirement.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RequestedArtifact {
    kind: ArtifactKind,
    requirement: ArtifactRequirement,
}

impl RequestedArtifact {
    /// Creates one typed external artifact selection.
    pub const fn new(kind: ArtifactKind, requirement: ArtifactRequirement) -> Self {
        Self { kind, requirement }
    }

    /// Returns the requested artifact category.
    pub const fn kind(self) -> ArtifactKind {
        self.kind
    }

    /// Returns whether product completion requires this artifact category.
    pub const fn requirement(self) -> ArtifactRequirement {
        self.requirement
    }
}

/// Immutable host request for one compiler product and its external artifacts.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct EmissionRequest {
    product: ProductIdentity,
    product_kind: ProductKind,
    target: TargetIdentity,
    destination: RequestedArtifactDestination,
    artifacts: Arc<[RequestedArtifact]>,
    replacement: ReplacementPolicy,
}

impl EmissionRequest {
    /// Validates and freezes one host artifact request.
    pub fn try_new(
        product: ProductIdentity,
        product_kind: ProductKind,
        target: TargetIdentity,
        destination: RequestedArtifactDestination,
        artifacts: impl IntoIterator<Item = RequestedArtifact>,
        replacement: ReplacementPolicy,
    ) -> Result<Self, EmissionRequestBuildError> {
        let mut artifacts: Vec<_> = artifacts.into_iter().collect();

        artifacts.sort_unstable_by_key(|artifact| artifact.kind());

        if artifacts.is_empty() {
            return Err(EmissionRequestBuildError::Empty);
        }

        if let Some(pair) = artifacts
            .windows(2)
            .find(|pair| pair[0].kind() == pair[1].kind())
        {
            return Err(EmissionRequestBuildError::DuplicateArtifactKind(
                pair[0].kind(),
            ));
        }

        if !artifacts
            .iter()
            .any(|artifact| artifact.requirement() == ArtifactRequirement::Required)
        {
            return Err(EmissionRequestBuildError::MissingRequiredArtifact);
        }

        if artifacts.len() > 1
            && matches!(
                destination,
                RequestedArtifactDestination::FilesystemFile(_)
                    | RequestedArtifactDestination::Stream(_)
            )
        {
            return Err(EmissionRequestBuildError::MultipleArtifactsForSingleSink);
        }

        Ok(Self {
            product,
            product_kind,
            target,
            destination,
            artifacts: artifacts.into(),
            replacement,
        })
    }

    /// Returns the selected product identity.
    pub const fn product(&self) -> &ProductIdentity {
        &self.product
    }

    /// Returns the selected language-level product kind.
    pub const fn product_kind(&self) -> ProductKind {
        self.product_kind
    }

    /// Returns the selected validated target identity.
    pub const fn target(&self) -> &TargetIdentity {
        &self.target
    }

    /// Returns the high-level requested output destination.
    pub const fn destination(&self) -> &RequestedArtifactDestination {
        &self.destination
    }

    /// Returns external artifact selections in canonical kind order.
    pub fn artifacts(&self) -> &[RequestedArtifact] {
        &self.artifacts
    }

    /// Returns the external artifact selection for one category.
    pub fn artifact(&self, kind: ArtifactKind) -> Option<RequestedArtifact> {
        self.artifacts
            .binary_search_by_key(&kind, |artifact| artifact.kind())
            .ok()
            .map(|index| self.artifacts[index])
    }

    /// Returns the requested destination replacement policy.
    pub const fn replacement(&self) -> ReplacementPolicy {
        self.replacement
    }
}

/// A contract violation that prevents an emission request from being frozen.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmissionRequestBuildError {
    /// No external artifact category was requested.
    Empty,
    /// No requested external artifact is required for product completion.
    MissingRequiredArtifact,
    /// One external artifact category was requested more than once.
    DuplicateArtifactKind(ArtifactKind),
    /// More than one artifact was directed to one unkeyed file or stream sink.
    MultipleArtifactsForSingleSink,
}

#[cfg(test)]
mod tests {
    use super::{EmissionRequest, EmissionRequestBuildError, RequestedArtifact};
    use crate::test_support::{emission_request, product_identity, target_identity};
    use crate::{
        ArtifactKind, ArtifactRequirement, ProductKind, ReplacementPolicy,
        RequestedArtifactDestination,
    };

    #[test]
    fn requests_require_at_least_one_required_artifact() {
        let optional =
            RequestedArtifact::new(ArtifactKind::Assembly, ArtifactRequirement::Optional);

        assert_eq!(
            try_request(
                RequestedArtifactDestination::FilesystemDirectory("out".into()),
                []
            ),
            Err(EmissionRequestBuildError::Empty)
        );

        assert_eq!(
            try_request(
                RequestedArtifactDestination::FilesystemDirectory("out".into()),
                [optional]
            ),
            Err(EmissionRequestBuildError::MissingRequiredArtifact)
        );
    }

    #[test]
    fn requests_require_unique_artifact_categories() {
        let object = RequestedArtifact::new(
            ArtifactKind::RelocatableObject,
            ArtifactRequirement::Required,
        );

        assert_eq!(
            try_request(
                RequestedArtifactDestination::FilesystemDirectory("out".into()),
                [object, object]
            ),
            Err(EmissionRequestBuildError::DuplicateArtifactKind(
                ArtifactKind::RelocatableObject
            ))
        );
    }

    #[test]
    fn single_output_sinks_reject_multiple_artifacts() {
        let artifacts = requested_object_and_assembly();

        assert_eq!(
            try_request(
                RequestedArtifactDestination::FilesystemFile("output".into()),
                artifacts
            ),
            Err(EmissionRequestBuildError::MultipleArtifactsForSingleSink)
        );
    }

    #[test]
    fn requests_canonicalize_artifact_order() {
        let request = emission_request(requested_object_and_assembly());

        assert_eq!(
            request.artifacts(),
            &[
                RequestedArtifact::new(ArtifactKind::Assembly, ArtifactRequirement::Optional),
                RequestedArtifact::new(
                    ArtifactKind::RelocatableObject,
                    ArtifactRequirement::Required
                )
            ]
        );
    }

    fn try_request(
        destination: RequestedArtifactDestination,
        artifacts: impl IntoIterator<Item = RequestedArtifact>,
    ) -> Result<EmissionRequest, EmissionRequestBuildError> {
        EmissionRequest::try_new(
            product_identity(),
            ProductKind::Executable,
            target_identity(),
            destination,
            artifacts,
            ReplacementPolicy::RequireAbsent,
        )
    }

    fn requested_object_and_assembly() -> [RequestedArtifact; 2] {
        [
            RequestedArtifact::new(
                ArtifactKind::RelocatableObject,
                ArtifactRequirement::Required,
            ),
            RequestedArtifact::new(ArtifactKind::Assembly, ArtifactRequirement::Optional),
        ]
    }
}
