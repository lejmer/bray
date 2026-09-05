use std::path::{Path, PathBuf};
use std::sync::Arc;

use bray_runtime_interface::ExecutableHostContract;
use bray_target::TargetIdentity;

use crate::{
    ArtifactKind, ArtifactRequirement, ManagedArtifactPath, OutputSinkId, ProductIdentity,
    ProductKind, ReplacementPolicy,
};

/// Canonical portable directory beneath a managed filesystem output root.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ManagedOutputDirectory(ManagedArtifactPath);

impl ManagedOutputDirectory {
    /// Creates a non-empty relative output directory without traversal or host-specific components.
    pub fn try_new(value: impl Into<Arc<str>>) -> Option<Self> {
        ManagedArtifactPath::try_new(value).map(Self)
    }

    /// Returns the canonical `/`-separated relative directory.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub(crate) fn to_path_buf(&self) -> PathBuf {
        self.0.to_path_buf()
    }
}

/// Stable public directory and private state root for one managed filesystem product.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ManagedFilesystemDestination {
    root: PathBuf,
    directory: Option<ManagedOutputDirectory>,
}

impl ManagedFilesystemDestination {
    /// Creates a managed destination whose private state belongs to `root` and whose artifacts
    /// are published directly beneath `directory`.
    pub fn new(root: impl Into<PathBuf>, directory: ManagedOutputDirectory) -> Self {
        Self {
            root: root.into(),
            directory: Some(directory),
        }
    }

    /// Creates a managed destination that publishes artifacts directly beneath its state root.
    pub fn at_root(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            directory: None,
        }
    }

    /// Returns the output root that owns all private managed-publication state.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the stable public directory for the selected product's artifacts.
    pub fn directory(&self) -> PathBuf {
        self.directory.as_ref().map_or_else(
            || self.root.clone(),
            |directory| self.root.join(directory.to_path_buf()),
        )
    }

    /// Returns the canonical root-relative public directory, when artifacts are namespaced.
    pub const fn relative_directory(&self) -> Option<&ManagedOutputDirectory> {
        self.directory.as_ref()
    }
}

impl From<PathBuf> for ManagedFilesystemDestination {
    fn from(root: PathBuf) -> Self {
        Self::at_root(root)
    }
}

impl From<&Path> for ManagedFilesystemDestination {
    fn from(root: &Path) -> Self {
        Self::at_root(root)
    }
}

impl From<&PathBuf> for ManagedFilesystemDestination {
    fn from(root: &PathBuf) -> Self {
        Self::at_root(root)
    }
}

impl From<&str> for ManagedFilesystemDestination {
    fn from(root: &str) -> Self {
        Self::at_root(root)
    }
}

/// High-level destination supplied before deterministic output names are planned.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RequestedArtifactDestination {
    /// Derive one or more final artifact paths beneath this directory.
    FilesystemDirectory(ManagedFilesystemDestination),
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
    executable_host: Option<ExecutableHostContract>,
    target: TargetIdentity,
    destination: RequestedArtifactDestination,
    artifacts: Arc<[RequestedArtifact]>,
    replacement: ReplacementPolicy,
    storage_profile: Option<Arc<str>>,
    build_identity: Option<crate::ProductBuildIdentity>,
}

impl EmissionRequest {
    /// Creates a host artifact request after validating its outputs and sink.
    pub fn try_new(
        product: ProductIdentity,
        product_kind: ProductKind,
        executable_host: Option<ExecutableHostContract>,
        target: TargetIdentity,
        destination: RequestedArtifactDestination,
        artifacts: impl IntoIterator<Item = RequestedArtifact>,
        replacement: ReplacementPolicy,
    ) -> Result<Self, EmissionRequestBuildError> {
        validate_executable_host(&product, product_kind, executable_host.as_ref())?;

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
            executable_host,
            target,
            destination,
            artifacts: artifacts.into(),
            replacement,
            storage_profile: None,
            build_identity: None,
        })
    }

    /// Records the host-selected build profile for retention and storage accounting.
    pub fn with_storage_profile(mut self, profile: impl Into<Arc<str>>) -> Self {
        self.storage_profile = Some(profile.into());

        self
    }

    /// Records the complete identity chain used by explicit no-build consumers.
    pub fn with_build_identity(mut self, identity: crate::ProductBuildIdentity) -> Self {
        self.build_identity = Some(identity);

        self
    }

    /// Returns the identity chain required to reuse this product generation.
    pub const fn build_identity(&self) -> Option<&crate::ProductBuildIdentity> {
        self.build_identity.as_ref()
    }

    /// Returns the host-selected profile identity used for storage retention and accounting.
    pub fn storage_profile(&self) -> Option<&str> {
        self.storage_profile.as_deref()
    }

    /// Returns the selected product identity.
    pub const fn product(&self) -> &ProductIdentity {
        &self.product
    }

    /// Returns the selected language-level product kind.
    pub const fn product_kind(&self) -> ProductKind {
        self.product_kind
    }

    /// Returns the compiler-generated executable-host contract, when this product has a root.
    pub const fn executable_host(&self) -> Option<&ExecutableHostContract> {
        self.executable_host.as_ref()
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

/// A contract violation that prevents creation of an emission request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmissionRequestBuildError {
    /// An executable or test product has no compiler-generated host contract.
    MissingExecutableHost,
    /// A library product contains an inapplicable executable-host contract.
    UnexpectedExecutableHost,
    /// The executable-host contract belongs to another selected product.
    ExecutableHostProductMismatch,
    /// No external artifact category was requested.
    Empty,
    /// No requested external artifact is required for product completion.
    MissingRequiredArtifact,
    /// One external artifact category was requested more than once.
    DuplicateArtifactKind(ArtifactKind),
    /// More than one artifact was directed to one unkeyed file or stream sink.
    MultipleArtifactsForSingleSink,
}

fn validate_executable_host(
    product: &ProductIdentity,
    product_kind: ProductKind,
    executable_host: Option<&ExecutableHostContract>,
) -> Result<(), EmissionRequestBuildError> {
    match (product_kind, executable_host) {
        (ProductKind::Executable | ProductKind::Test, None) => {
            Err(EmissionRequestBuildError::MissingExecutableHost)
        }
        (ProductKind::Library, Some(_)) => Err(EmissionRequestBuildError::UnexpectedExecutableHost),
        (ProductKind::Executable | ProductKind::Test, Some(host)) if host.product() != product => {
            Err(EmissionRequestBuildError::ExecutableHostProductMismatch)
        }
        (ProductKind::Executable | ProductKind::Test, Some(_)) | (ProductKind::Library, None) => {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EmissionRequest, EmissionRequestBuildError, ManagedFilesystemDestination,
        ManagedOutputDirectory, RequestedArtifact,
    };
    use crate::test_support::{
        emission_request, executable_host_contract, product_identity, target_identity,
    };
    use crate::{
        ArtifactKind, ArtifactRequirement, ProductKind, ReplacementPolicy,
        RequestedArtifactDestination,
    };

    #[test]
    fn managed_destinations_separate_public_namespace_from_private_root() {
        let directory = ManagedOutputDirectory::try_new("native/debug/example.package")
            .unwrap_or_else(|| panic!("test managed directory must be valid"));

        let destination = ManagedFilesystemDestination::new("build", directory);

        assert_eq!(destination.root(), std::path::Path::new("build"));

        assert_eq!(
            destination.directory(),
            std::path::Path::new("build/native/debug/example.package")
        );

        for invalid in ["", "/native/debug", "native\\debug", "native/../debug"] {
            assert_eq!(ManagedOutputDirectory::try_new(invalid), None, "{invalid}");
        }
    }

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

    #[test]
    fn executable_hosts_are_required_only_for_root_products() {
        let artifact = RequestedArtifact::new(
            ArtifactKind::RelocatableObject,
            ArtifactRequirement::Required,
        );

        assert_eq!(
            EmissionRequest::try_new(
                product_identity(),
                ProductKind::Executable,
                None,
                target_identity(),
                RequestedArtifactDestination::FilesystemDirectory("out".into()),
                [artifact],
                ReplacementPolicy::RequireAbsent,
            ),
            Err(EmissionRequestBuildError::MissingExecutableHost)
        );

        assert_eq!(
            EmissionRequest::try_new(
                product_identity(),
                ProductKind::Library,
                Some(executable_host_contract()),
                target_identity(),
                RequestedArtifactDestination::FilesystemDirectory("out".into()),
                [artifact],
                ReplacementPolicy::RequireAbsent,
            ),
            Err(EmissionRequestBuildError::UnexpectedExecutableHost)
        );
    }

    fn try_request(
        destination: RequestedArtifactDestination,
        artifacts: impl IntoIterator<Item = RequestedArtifact>,
    ) -> Result<EmissionRequest, EmissionRequestBuildError> {
        EmissionRequest::try_new(
            product_identity(),
            ProductKind::Executable,
            Some(executable_host_contract()),
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
