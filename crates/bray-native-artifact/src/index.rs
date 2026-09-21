use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bray_target::{NativeTarget, ObjectFormat};

use crate::model::{
    NativeCoRetentionGroup, NativeContentDigest, NativeDefinitionSelection, NativeUnit,
    NativeUnitKind, NativeUnitSummary,
};
use crate::wire::{IndexWire, WireError};

const MAXIMUM_INDEX_BYTES: usize = 16 * 1024 * 1024;

/// Immutable, target-specific description of independently selectable native units.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeArtifactIndex {
    target: NativeTarget,
    producer: NativeContentDigest,
    units: Arc<[NativeUnit]>,
    co_retention_groups: Arc<[NativeCoRetentionGroup]>,
}

impl NativeArtifactIndex {
    /// Validates a published unit set without inspecting its payload files.
    ///
    /// The producer digest must commit to the toolchain/backend revision and all effective
    /// native-generation options, including optimization, size, debug, and observation policy.
    /// Reusable source templates have a separate identity in their owning package interface.
    pub fn try_new(
        target: NativeTarget,
        producer: NativeContentDigest,
        units: impl IntoIterator<Item = NativeUnit>,
        co_retention_groups: impl IntoIterator<Item = NativeCoRetentionGroup>,
    ) -> Result<Self, NativeIndexError> {
        let mut units: Vec<_> = units.into_iter().collect();

        units.sort_by_key(NativeUnit::digest);

        let mut previous = None;

        for unit in &units {
            if previous == Some(unit.digest()) {
                return Err(NativeIndexError::DuplicateUnit(unit.digest()));
            }

            previous = Some(unit.digest());

            if matches!(unit.kind(), NativeUnitKind::OpaqueArchive)
                && !matches!(unit.summary(), NativeUnitSummary::Opaque)
            {
                return Err(NativeIndexError::InvalidSummary(unit.digest()));
            }

            if unit
                .link_options()
                .iter()
                .any(|option| option.is_empty() || option.contains(['\0', '\n', '\r']))
            {
                return Err(NativeIndexError::InvalidLinkOption(unit.digest()));
            }

            if let NativeUnitSummary::Exact {
                definitions,
                references,
                roots,
            } = unit.summary()
            {
                if !strictly_sorted(definitions)
                    || !strictly_sorted(references)
                    || !strictly_sorted(roots)
                {
                    return Err(NativeIndexError::NoncanonicalSummary(unit.digest()));
                }

                if definitions.iter().any(|definition| {
                    definition.symbol().presence() != bray_symbols::NativeSymbolPresence::Required
                }) {
                    return Err(NativeIndexError::InvalidSummary(unit.digest()));
                }

                if definitions.windows(2).any(|pair| {
                    pair[0].symbol().identity() == pair[1].symbol().identity()
                        && pair[0].symbol().version() == pair[1].symbol().version()
                }) {
                    return Err(NativeIndexError::DuplicateDefinition(unit.digest()));
                }

                if definitions.iter().any(|definition| {
                    matches!(
                        definition.selection(),
                        NativeDefinitionSelection::Comdat {
                            associative_with: Some(parent),
                            ..
                        } if !definitions.iter().any(|candidate| candidate.symbol().identity() == parent)
                    )
                }) {
                    return Err(NativeIndexError::InvalidAssociation(unit.digest()));
                }
            }
        }

        let mut co_retention_groups = co_retention_groups.into_iter().collect::<Vec<_>>();

        co_retention_groups.sort_unstable();

        for group in &co_retention_groups {
            for member in group.members().iter().copied() {
                if units
                    .binary_search_by_key(&member, NativeUnit::digest)
                    .is_err()
                {
                    return Err(NativeIndexError::MissingCoRetentionMember(member));
                }
            }
        }

        if !strictly_sorted(&co_retention_groups) {
            return Err(NativeIndexError::DuplicateCoRetentionGroup);
        }

        Ok(Self {
            target,
            producer,
            units: units.into(),
            co_retention_groups: co_retention_groups.into(),
        })
    }

    /// Encodes the validated index in stable unit order.
    pub fn encode(&self) -> Result<Vec<u8>, NativeIndexError> {
        let mut bytes = serde_json::to_vec(&IndexWire::from_index(self))
            .map_err(|_| NativeIndexError::Malformed)?;

        bytes.push(b'\n');

        if bytes.len() > MAXIMUM_INDEX_BYTES {
            return Err(NativeIndexError::SizeLimitExceeded);
        }

        Ok(bytes)
    }

    /// Authenticates a bounded index and every referenced payload at an artifact import boundary.
    pub fn import(
        bytes: &[u8],
        expected_digest: NativeContentDigest,
        expected_target: NativeTarget,
        expected_producer: NativeContentDigest,
        payload_directory: &Path,
    ) -> Result<ValidatedNativeArtifact, NativeIndexError> {
        if bytes.len() > MAXIMUM_INDEX_BYTES {
            return Err(NativeIndexError::SizeLimitExceeded);
        }

        let digest = bray_base::sha256_reader(bytes)
            .expect("reading an in-memory index byte slice cannot fail");

        if NativeContentDigest::new(digest) != expected_digest {
            return Err(NativeIndexError::IndexDigestMismatch {
                expected: expected_digest,
                actual: NativeContentDigest::new(digest),
            });
        }

        let wire: IndexWire =
            serde_json::from_slice(bytes).map_err(|_| NativeIndexError::Malformed)?;
        let index = wire.into_index()?;

        if index.target != expected_target {
            return Err(NativeIndexError::WrongTarget {
                expected: expected_target,
                actual: index.target,
            });
        }

        if index.producer != expected_producer {
            return Err(NativeIndexError::WrongProducer {
                expected: expected_producer,
                actual: index.producer,
            });
        }

        let mut payloads = Vec::with_capacity(index.units.len());

        for unit in index.units() {
            let path = payload_directory.join(unit.kind().file_name(unit.digest(), index.target()));

            let actual = match bray_base::sha256_file(&path) {
                Ok(actual) => actual,
                Err(error) => return Err(NativeIndexError::Read { path, error }),
            };

            if NativeContentDigest::new(actual) != unit.digest() {
                return Err(NativeIndexError::PayloadDigestMismatch {
                    expected: unit.digest(),
                    actual: NativeContentDigest::new(actual),
                });
            }

            payloads.push(path);
        }

        Ok(ValidatedNativeArtifact {
            index,
            payloads: payloads.into(),
        })
    }

    /// Returns the compilation target of these units.
    pub const fn target(&self) -> NativeTarget {
        self.target
    }

    /// Returns the producer's exact code-generation policy identity.
    pub const fn producer(&self) -> NativeContentDigest {
        self.producer
    }

    /// Returns the target object format.
    pub const fn object_format(&self) -> ObjectFormat {
        self.target.object_format()
    }

    /// Returns units ordered by content identity.
    pub fn units(&self) -> &[NativeUnit] {
        &self.units
    }

    /// Returns unordered co-retention sets in stable group order.
    pub fn co_retention_groups(&self) -> &[NativeCoRetentionGroup] {
        &self.co_retention_groups
    }
}

/// Authenticated index and payload paths in the index's stable order.
#[derive(Clone, Debug)]
pub struct ValidatedNativeArtifact {
    index: NativeArtifactIndex,
    payloads: Arc<[PathBuf]>,
}

impl ValidatedNativeArtifact {
    /// Returns the validated unit contract.
    pub const fn index(&self) -> &NativeArtifactIndex {
        &self.index
    }

    /// Returns the authenticated path of the exact content-addressed unit.
    pub fn payload(&self, identity: NativeContentDigest) -> Option<&Path> {
        let index = self
            .index
            .units()
            .binary_search_by_key(&identity, NativeUnit::digest)
            .ok()?;

        Some(self.payloads[index].as_path())
    }
}

/// Typed native artifact import or construction failure.
#[derive(Debug)]
pub enum NativeIndexError {
    /// Index exceeds its bounded wire limit.
    SizeLimitExceeded,
    /// Invalid or truncated index encoding.
    Malformed,
    /// Index wire value is unsupported or invalid.
    Wire(WireError),
    /// Index bytes do not match the external artifact commitment.
    IndexDigestMismatch {
        /// Committed index digest.
        expected: NativeContentDigest,
        /// Digest of the supplied index bytes.
        actual: NativeContentDigest,
    },
    /// A payload differs from its content-addressed identity.
    PayloadDigestMismatch {
        /// Unit's content identity.
        expected: NativeContentDigest,
        /// Digest of the supplied payload.
        actual: NativeContentDigest,
    },
    /// The index is for another compilation target.
    WrongTarget {
        /// Target requested at import.
        expected: NativeTarget,
        /// Target recorded by the producer.
        actual: NativeTarget,
    },
    /// The index was produced with another native code-generation policy.
    WrongProducer {
        /// Producer identity requested at import.
        expected: NativeContentDigest,
        /// Producer identity recorded by the index.
        actual: NativeContentDigest,
    },
    /// Payload could not be read.
    Read {
        /// Exact content-addressed payload path.
        path: PathBuf,
        /// Filesystem failure.
        error: io::Error,
    },
    /// Two payloads have the same content identity.
    DuplicateUnit(NativeContentDigest),
    /// A unit kind and summary disagree.
    InvalidSummary(NativeContentDigest),
    /// A unit defines the same native symbol more than once.
    DuplicateDefinition(NativeContentDigest),
    /// An associative COMDAT has no parent definition in its unit.
    InvalidAssociation(NativeContentDigest),
    /// A target-linker option is empty or contains an invalid control character.
    InvalidLinkOption(NativeContentDigest),
    /// Exact summary sets are not in stable unique order.
    NoncanonicalSummary(NativeContentDigest),
    /// A group refers to a unit absent from the index.
    MissingCoRetentionMember(NativeContentDigest),
    /// More than one identical group was published.
    DuplicateCoRetentionGroup,
    /// A group has fewer than two distinct unit identities.
    InvalidCoRetentionGroup,
}

fn strictly_sorted<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}
