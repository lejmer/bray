use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bray_base::shared_slice;

use crate::{BackendIdentity, CodegenUnitKey, TargetIdentity};

/// Backend artifact categories understood by emission planning.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BackendArtifactKind {
    /// Relocatable native object suitable for a native linker.
    RelocatableObject,
    /// Human-readable target assembly.
    Assembly,
    /// Human-readable backend low-level IR.
    BackendIr,
    /// Backend-owned binary IR or bitcode.
    BackendBitcode,
    /// Directly executable module that does not require native linking.
    ExecutableModule,
    /// Codegen-owned debug data stored separately from another artifact.
    DebugCompanion,
}

/// Exact required and optional backend outputs derived from an emission plan.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BackendArtifactRequest {
    required: Arc<[BackendArtifactKind]>,
    optional: Arc<[BackendArtifactKind]>,
}

impl BackendArtifactRequest {
    /// Creates a non-empty request with disjoint canonical kind sets.
    pub fn try_new(
        required: impl IntoIterator<Item = BackendArtifactKind>,
        optional: impl IntoIterator<Item = BackendArtifactKind>,
    ) -> Result<Self, BackendArtifactRequestBuildError> {
        let required: BTreeSet<_> = required.into_iter().collect();
        let optional: BTreeSet<_> = optional.into_iter().collect();

        if required.is_empty() && optional.is_empty() {
            return Err(BackendArtifactRequestBuildError::Empty);
        }

        if let Some(overlap) = required.intersection(&optional).next().copied() {
            return Err(BackendArtifactRequestBuildError::RequiredOptionalOverlap(
                overlap,
            ));
        }

        Ok(Self {
            required: shared_slice(required),
            optional: shared_slice(optional),
        })
    }

    /// Returns required artifact kinds in canonical order.
    pub fn required(&self) -> &[BackendArtifactKind] {
        &self.required
    }

    /// Returns optional artifact kinds in canonical order.
    pub fn optional(&self) -> &[BackendArtifactKind] {
        &self.optional
    }

    /// Returns whether the emission plan requested this kind.
    pub fn contains(&self, kind: BackendArtifactKind) -> bool {
        self.required.binary_search(&kind).is_ok() || self.optional.binary_search(&kind).is_ok()
    }
}

/// A contract violation that prevents an artifact request from being frozen.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendArtifactRequestBuildError {
    /// No output kind was requested.
    Empty,
    /// One kind was declared both required and optional.
    RequiredOptionalOverlap(BackendArtifactKind),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ArtifactContentStorage {
    Memory(Arc<[u8]>),
    CompilerSpool(PathBuf),
}

/// Borrowed view of immutable artifact content storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactContentSource<'content> {
    /// Bytes retained in immutable shared memory.
    Memory(&'content [u8]),
    /// Bytes retained in a compiler-owned immutable spool.
    CompilerSpool(&'content Path),
}

/// Immutable artifact bytes or a compiler-owned immutable spool containing them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactContent {
    storage: ArtifactContentStorage,
    byte_len: u64,
}

impl ArtifactContent {
    /// Creates memory-backed content when its byte length is representable by the contract.
    pub fn try_memory(bytes: impl Into<Arc<[u8]>>) -> Result<Self, ArtifactContentBuildError> {
        let bytes = bytes.into();
        let Ok(byte_len) = u64::try_from(bytes.len()) else {
            return Err(ArtifactContentBuildError::LengthExceeded);
        };

        Ok(Self {
            storage: ArtifactContentStorage::Memory(bytes),
            byte_len,
        })
    }

    /// Creates content backed by a compiler-owned immutable spool.
    ///
    /// The path identifies compiler-private staging storage, never an emitter-selected final sink.
    pub fn try_compiler_spool(
        path: impl Into<PathBuf>,
        byte_len: u64,
    ) -> Result<Self, ArtifactContentBuildError> {
        let path = path.into();

        if path.as_os_str().is_empty() {
            return Err(ArtifactContentBuildError::EmptySpoolPath);
        }

        Ok(Self {
            storage: ArtifactContentStorage::CompilerSpool(path),
            byte_len,
        })
    }

    /// Returns the exact artifact byte length.
    pub const fn byte_len(&self) -> u64 {
        self.byte_len
    }

    /// Returns the exact immutable content source.
    pub fn source(&self) -> ArtifactContentSource<'_> {
        match &self.storage {
            ArtifactContentStorage::Memory(bytes) => ArtifactContentSource::Memory(bytes),
            ArtifactContentStorage::CompilerSpool(path) => {
                ArtifactContentSource::CompilerSpool(path)
            }
        }
    }
}

/// A contract violation that prevents immutable artifact content construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactContentBuildError {
    /// The in-memory content length exceeds the contract's compact length field.
    LengthExceeded,
    /// A compiler-owned spool path was empty.
    EmptySpoolPath,
}

/// Hash algorithm used by an optional deterministic artifact digest.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ArtifactDigestAlgorithm {
    /// BLAKE3 with its standard 256-bit output.
    Blake3,
    /// SHA-256.
    Sha256,
}

impl ArtifactDigestAlgorithm {
    const fn byte_len(self) -> usize {
        match self {
            Self::Blake3 | Self::Sha256 => 32,
        }
    }
}

/// Optional deterministic digest of one complete artifact contribution.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ArtifactDigest {
    algorithm: ArtifactDigestAlgorithm,
    bytes: Arc<[u8]>,
}

impl ArtifactDigest {
    /// Creates a digest when its byte length matches the selected algorithm.
    pub fn try_new(
        algorithm: ArtifactDigestAlgorithm,
        bytes: impl Into<Arc<[u8]>>,
    ) -> Option<Self> {
        let bytes = bytes.into();

        if bytes.len() != algorithm.byte_len() {
            return None;
        }

        Some(Self { algorithm, bytes })
    }

    /// Returns the digest algorithm.
    pub const fn algorithm(&self) -> ArtifactDigestAlgorithm {
        self.algorithm
    }

    /// Returns the exact digest bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// One immutable artifact contribution produced by a backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendArtifactContribution {
    unit: CodegenUnitKey,
    kind: BackendArtifactKind,
    content: ArtifactContent,
    backend: BackendIdentity,
    target: TargetIdentity,
    digest: Option<ArtifactDigest>,
}

impl BackendArtifactContribution {
    /// Creates one complete backend artifact contribution.
    pub const fn new(
        unit: CodegenUnitKey,
        kind: BackendArtifactKind,
        content: ArtifactContent,
        backend: BackendIdentity,
        target: TargetIdentity,
        digest: Option<ArtifactDigest>,
    ) -> Self {
        Self {
            unit,
            kind,
            content,
            backend,
            target,
            digest,
        }
    }

    /// Returns the codegen unit that produced this artifact.
    pub const fn unit(&self) -> &CodegenUnitKey {
        &self.unit
    }

    /// Returns the artifact category.
    pub const fn kind(&self) -> BackendArtifactKind {
        self.kind
    }

    /// Returns the immutable artifact content.
    pub const fn content(&self) -> &ArtifactContent {
        &self.content
    }

    /// Returns the producing backend identity.
    pub const fn backend(&self) -> &BackendIdentity {
        &self.backend
    }

    /// Returns the target identity used to produce the artifact.
    pub const fn target(&self) -> &TargetIdentity {
        &self.target
    }

    /// Returns the deterministic content digest when one was requested.
    pub const fn digest(&self) -> Option<&ArtifactDigest> {
        self.digest.as_ref()
    }
}

/// Complete immutable artifact contributions for one codegen unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendArtifactSet {
    unit: CodegenUnitKey,
    backend: BackendIdentity,
    target: TargetIdentity,
    contributions: Arc<[BackendArtifactContribution]>,
}

impl BackendArtifactSet {
    /// Validates and freezes one codegen unit's complete requested contributions.
    pub fn try_new(
        unit: CodegenUnitKey,
        backend: BackendIdentity,
        target: TargetIdentity,
        request: &BackendArtifactRequest,
        contributions: impl IntoIterator<Item = BackendArtifactContribution>,
    ) -> Result<Self, BackendArtifactSetBuildError> {
        let mut contributions: Vec<_> = contributions.into_iter().collect();

        contributions.sort_unstable_by_key(BackendArtifactContribution::kind);

        for pair in contributions.windows(2) {
            if pair[0].kind() == pair[1].kind() {
                return Err(BackendArtifactSetBuildError::DuplicateArtifact(
                    pair[0].kind(),
                ));
            }
        }

        for contribution in &contributions {
            validate_contribution(&unit, &backend, &target, request, contribution)?;
        }

        for required in request.required() {
            if contributions
                .binary_search_by_key(required, BackendArtifactContribution::kind)
                .is_err()
            {
                return Err(BackendArtifactSetBuildError::MissingRequired(*required));
            }
        }

        Ok(Self {
            unit,
            backend,
            target,
            contributions: contributions.into(),
        })
    }

    /// Returns the codegen unit represented by this complete set.
    pub const fn unit(&self) -> &CodegenUnitKey {
        &self.unit
    }

    /// Returns the backend that produced every contribution.
    pub const fn backend(&self) -> &BackendIdentity {
        &self.backend
    }

    /// Returns the target shared by every contribution.
    pub const fn target(&self) -> &TargetIdentity {
        &self.target
    }

    /// Returns contributions in canonical artifact-kind order.
    pub fn contributions(&self) -> &[BackendArtifactContribution] {
        &self.contributions
    }
}

/// A contract violation that prevents atomic artifact-set publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendArtifactSetBuildError {
    /// Two contributions have the same artifact kind.
    DuplicateArtifact(BackendArtifactKind),
    /// A required artifact was not produced.
    MissingRequired(BackendArtifactKind),
    /// A contribution was not present in the emitter-derived request.
    UnrequestedArtifact(BackendArtifactKind),
    /// A contribution belongs to another codegen unit.
    UnitMismatch,
    /// A contribution was produced by another backend identity.
    BackendMismatch,
    /// A contribution was produced for another target identity.
    TargetMismatch,
}

fn validate_contribution(
    unit: &CodegenUnitKey,
    backend: &BackendIdentity,
    target: &TargetIdentity,
    request: &BackendArtifactRequest,
    contribution: &BackendArtifactContribution,
) -> Result<(), BackendArtifactSetBuildError> {
    if contribution.unit() != unit {
        return Err(BackendArtifactSetBuildError::UnitMismatch);
    }

    if contribution.backend() != backend {
        return Err(BackendArtifactSetBuildError::BackendMismatch);
    }

    if contribution.target() != target {
        return Err(BackendArtifactSetBuildError::TargetMismatch);
    }

    if !request.contains(contribution.kind()) {
        return Err(BackendArtifactSetBuildError::UnrequestedArtifact(
            contribution.kind(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ArtifactContent, BackendArtifactContribution, BackendArtifactKind, BackendArtifactRequest,
        BackendArtifactRequestBuildError, BackendArtifactSet, BackendArtifactSetBuildError,
    };
    use crate::{BackendIdentity, CodegenUnitKey, TargetIdentity};

    #[test]
    fn artifact_requests_require_disjoint_non_empty_kind_sets() {
        assert_eq!(
            BackendArtifactRequest::try_new([], []),
            Err(BackendArtifactRequestBuildError::Empty)
        );
        assert_eq!(
            BackendArtifactRequest::try_new(
                [BackendArtifactKind::RelocatableObject],
                [BackendArtifactKind::RelocatableObject]
            ),
            Err(BackendArtifactRequestBuildError::RequiredOptionalOverlap(
                BackendArtifactKind::RelocatableObject
            ))
        );
    }

    #[test]
    fn artifact_sets_require_every_required_kind() {
        let request = request();

        assert_eq!(
            BackendArtifactSet::try_new(unit(), backend(), target(), &request, []),
            Err(BackendArtifactSetBuildError::MissingRequired(
                BackendArtifactKind::RelocatableObject
            ))
        );
    }

    #[test]
    fn artifact_sets_publish_requested_contributions_in_canonical_order() {
        let request = request();
        let object = contribution(BackendArtifactKind::RelocatableObject);
        let assembly = contribution(BackendArtifactKind::Assembly);

        let Ok(artifacts) =
            BackendArtifactSet::try_new(unit(), backend(), target(), &request, [assembly, object])
        else {
            panic!("requested test artifacts must form a complete set");
        };

        assert_eq!(
            artifacts
                .contributions()
                .iter()
                .map(BackendArtifactContribution::kind)
                .collect::<Vec<_>>(),
            [
                BackendArtifactKind::RelocatableObject,
                BackendArtifactKind::Assembly,
            ]
        );
    }

    fn request() -> BackendArtifactRequest {
        let Ok(request) = BackendArtifactRequest::try_new(
            [BackendArtifactKind::RelocatableObject],
            [BackendArtifactKind::Assembly],
        ) else {
            panic!("test artifact request must be valid");
        };

        request
    }

    fn contribution(kind: BackendArtifactKind) -> BackendArtifactContribution {
        let Ok(content) = ArtifactContent::try_memory([1_u8, 2, 3].as_slice()) else {
            panic!("test artifact content must be valid");
        };

        BackendArtifactContribution::new(unit(), kind, content, backend(), target(), None)
    }

    fn unit() -> CodegenUnitKey {
        let Some(unit) = CodegenUnitKey::try_new(1, "package.main.0") else {
            panic!("test codegen unit key must be valid");
        };

        unit
    }

    fn backend() -> BackendIdentity {
        let Some(backend) = BackendIdentity::try_new("llvm", "bray-1", "llvm-22") else {
            panic!("test backend identity must be valid");
        };

        backend
    }

    fn target() -> TargetIdentity {
        let Some(target) = TargetIdentity::try_new("x86_64-linux") else {
            panic!("test target identity must be valid");
        };

        target
    }
}
