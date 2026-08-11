use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates an expected artifact byte-count argument.
    pub const fn expected_byte_count(byte_count: u64) -> Self {
        Self::new(
            DiagnosticArgName::ExpectedByteCount,
            DiagnosticArgValue::ByteCount(byte_count),
        )
    }

    /// Creates an actual artifact byte-count argument.
    pub const fn actual_byte_count(byte_count: u64) -> Self {
        Self::new(
            DiagnosticArgName::ActualByteCount,
            DiagnosticArgValue::ByteCount(byte_count),
        )
    }

    /// Creates an expected artifact-digest argument.
    pub const fn expected_artifact_digest(digest: DiagnosticArtifactDigest) -> Self {
        Self::new(
            DiagnosticArgName::ExpectedArtifactDigest,
            DiagnosticArgValue::ArtifactDigest(digest),
        )
    }

    /// Creates an actual artifact-digest argument.
    pub const fn actual_artifact_digest(digest: DiagnosticArtifactDigest) -> Self {
        Self::new(
            DiagnosticArgName::ActualArtifactDigest,
            DiagnosticArgValue::ArtifactDigest(digest),
        )
    }
}

/// Locale-neutral deterministic artifact digest used by diagnostics.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticArtifactDigest {
    algorithm: DiagnosticArtifactDigestAlgorithm,
    bytes: [u8; 32],
}

impl DiagnosticArtifactDigest {
    /// Creates a digest from its typed algorithm and exact bytes.
    pub const fn new(algorithm: DiagnosticArtifactDigestAlgorithm, bytes: [u8; 32]) -> Self {
        Self { algorithm, bytes }
    }

    /// Returns the digest algorithm.
    pub const fn algorithm(&self) -> DiagnosticArtifactDigestAlgorithm {
        self.algorithm
    }

    /// Returns the exact digest bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Locale-neutral deterministic artifact digest algorithm.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticArtifactDigestAlgorithm {
    /// BLAKE3 with its standard 256-bit output.
    Blake3,
    /// SHA-256.
    Sha256,
}

impl DiagnosticArtifactDigestAlgorithm {
    /// Returns the stable machine key for this digest algorithm.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Blake3 => "blake3",
            Self::Sha256 => "sha256",
        }
    }
}
