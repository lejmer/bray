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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_base::NonEmptySharedStr;
    use bray_symbols::{
        NativeLinkKind, NativeLinkRequirement, NativeSymbolBinding, NativeSymbolContract,
        NativeSymbolIdentity, NativeSymbolPresence,
    };
    use bray_target::NativeTarget;

    use crate::{
        NativeArtifactIndex, NativeCoRetentionGroup, NativeComdatSelection, NativeContentDigest,
        NativeDefinition, NativeDefinitionSelection, NativeIndexError, NativeRoot, NativeUnit,
        NativeUnitKind, NativeUnitSummary,
    };

    fn digest(bytes: &[u8]) -> NativeContentDigest {
        NativeContentDigest::new(
            bray_base::sha256_reader(bytes)
                .unwrap_or_else(|error| panic!("test content must hash: {error}")),
        )
    }

    fn name(value: &str) -> NonEmptySharedStr {
        NonEmptySharedStr::try_new(value).unwrap_or_else(|| panic!("test symbol must be nonempty"))
    }

    fn symbol(
        identity: NativeSymbolIdentity,
        version: Option<&str>,
        binding: NativeSymbolBinding,
        presence: NativeSymbolPresence,
    ) -> NativeSymbolContract {
        NativeSymbolContract::new(identity, version.map(name), binding, presence)
    }

    fn exact(
        digest: NativeContentDigest,
        definitions: Vec<NativeDefinition>,
        references: Vec<NativeSymbolContract>,
        roots: Vec<NativeRoot>,
    ) -> NativeUnit {
        NativeUnit::new(
            digest,
            NativeUnitKind::Object,
            NativeUnitSummary::Exact {
                definitions: definitions.into(),
                references: references.into(),
                roots: roots.into(),
            },
            [NativeLinkRequirement::new(
                name("pthread"),
                NativeLinkKind::System,
            )],
            [Arc::from("-pthread")],
        )
    }

    fn index() -> (NativeArtifactIndex, Vec<(NativeContentDigest, Vec<u8>)>) {
        let first = b"first object".to_vec();
        let second = b"second object".to_vec();
        let archive = b"opaque native library".to_vec();
        let first_id = digest(&first);
        let second_id = digest(&second);
        let archive_id = digest(&archive);

        let definition = symbol(
            NativeSymbolIdentity::Name(name("entry")),
            Some("VERSION_2"),
            NativeSymbolBinding::Strong,
            NativeSymbolPresence::Required,
        );

        let first_unit = exact(
            first_id,
            vec![
                NativeDefinition::new(
                    definition.clone(),
                    NativeDefinitionSelection::Comdat {
                        group: NativeSymbolIdentity::Name(name("group")),
                        rule: NativeComdatSelection::ExactMatch,
                        associative_with: Some(NativeSymbolIdentity::Ordinal(17)),
                    },
                ),
                NativeDefinition::new(
                    symbol(
                        NativeSymbolIdentity::Ordinal(17),
                        None,
                        NativeSymbolBinding::Weak,
                        NativeSymbolPresence::Required,
                    ),
                    NativeDefinitionSelection::Fallback,
                ),
            ],
            vec![symbol(
                NativeSymbolIdentity::Ordinal(23),
                None,
                NativeSymbolBinding::Weak,
                NativeSymbolPresence::Optional,
            )],
            vec![NativeRoot::Initialization, NativeRoot::Finalization],
        );

        let second_unit = exact(
            second_id,
            vec![NativeDefinition::new(
                definition,
                NativeDefinitionSelection::Ordinary,
            )],
            Vec::new(),
            Vec::new(),
        );

        let archive_unit = NativeUnit::new(
            archive_id,
            NativeUnitKind::OpaqueArchive,
            NativeUnitSummary::Opaque,
            [],
            [],
        );

        let index = NativeArtifactIndex::try_new(
            NativeTarget::X86_64LinuxGnu,
            digest(b"producer policy"),
            [second_unit, archive_unit, first_unit],
            [
                NativeCoRetentionGroup::try_new([first_id, second_id])
                    .unwrap_or_else(|| panic!("test group must be valid")),
                NativeCoRetentionGroup::try_new([second_id, archive_id])
                    .unwrap_or_else(|| panic!("test group must be valid")),
                NativeCoRetentionGroup::try_new([archive_id, first_id])
                    .unwrap_or_else(|| panic!("test group must be valid")),
            ],
        )
        .unwrap_or_else(|error| panic!("test index must validate: {error:?}"));

        (
            index,
            vec![
                (first_id, first),
                (second_id, second),
                (archive_id, archive),
            ],
        )
    }

    #[test]
    fn round_trip_duplicate_providers_cycles_roots_and_opaque_units() {
        let (index, payloads) = index();

        let bytes = index
            .encode()
            .unwrap_or_else(|error| panic!("index must encode: {error:?}"));

        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("test payload directory must exist: {error}"));

        for (digest, bytes) in &payloads {
            let kind = index
                .units()
                .iter()
                .find(|unit| unit.digest() == *digest)
                .unwrap_or_else(|| panic!("test unit must exist"))
                .kind();

            std::fs::write(
                directory
                    .path()
                    .join(kind.file_name(*digest, index.target())),
                bytes,
            )
            .unwrap_or_else(|error| panic!("test payload must be written: {error}"));
        }

        let imported = NativeArtifactIndex::import(
            &bytes,
            digest(&bytes),
            index.target(),
            index.producer(),
            directory.path(),
        )
        .unwrap_or_else(|error| panic!("test index must import: {error:?}"));

        assert_eq!(imported.index(), &index);
        assert_eq!(imported.index().units().len(), 3);
        assert_eq!(imported.index().co_retention_groups().len(), 3);
        assert!(imported.payload(payloads[0].0).is_some());
        assert!(imported.payload(digest(b"absent unit")).is_none());

        let reversed = NativeArtifactIndex::try_new(
            index.target(),
            index.producer(),
            index.units().iter().cloned().rev(),
            index.co_retention_groups().iter().cloned().rev(),
        )
        .unwrap_or_else(|error| panic!("reordered units must validate: {error:?}"));

        assert_eq!(reversed.encode().ok().as_deref(), Some(bytes.as_slice()));

        let reordered = index.units().iter().map(|unit| {
            let NativeUnitSummary::Exact {
                definitions,
                references,
                roots,
            } = unit.summary()
            else {
                return unit.clone();
            };

            NativeUnit::new(
                unit.digest(),
                unit.kind(),
                NativeUnitSummary::Exact {
                    definitions: definitions.iter().rev().cloned().collect(),
                    references: references.iter().rev().cloned().collect(),
                    roots: roots.iter().rev().copied().collect(),
                },
                unit.native_links().iter().cloned().rev(),
                unit.link_options().iter().cloned(),
            )
        });

        let reordered = NativeArtifactIndex::try_new(
            index.target(),
            index.producer(),
            reordered,
            index.co_retention_groups().iter().cloned(),
        )
        .unwrap_or_else(|error| panic!("reordered symbols must validate: {error:?}"));

        assert_eq!(reordered.encode().ok().as_deref(), Some(bytes.as_slice()));
    }

    #[test]
    fn import_rejects_truncation_target_policy_and_payload_mismatch() {
        let (index, _) = index();

        let bytes = index
            .encode()
            .unwrap_or_else(|error| panic!("index must encode: {error:?}"));

        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("test payload directory must exist: {error}"));

        assert!(matches!(
            NativeArtifactIndex::import(
                &bytes[..bytes.len() / 2],
                digest(&bytes[..bytes.len() / 2]),
                index.target(),
                index.producer(),
                directory.path(),
            ),
            Err(NativeIndexError::Malformed)
        ));

        assert!(matches!(
            NativeArtifactIndex::import(
                &bytes,
                digest(b"wrong"),
                index.target(),
                index.producer(),
                directory.path(),
            ),
            Err(NativeIndexError::IndexDigestMismatch { .. })
        ));

        assert!(matches!(
            NativeArtifactIndex::import(
                &bytes,
                digest(&bytes),
                NativeTarget::X86_64WindowsMsvc,
                index.producer(),
                directory.path(),
            ),
            Err(NativeIndexError::WrongTarget { .. })
        ));

        assert!(matches!(
            NativeArtifactIndex::import(
                &bytes,
                digest(&bytes),
                index.target(),
                digest(b"different policy"),
                directory.path(),
            ),
            Err(NativeIndexError::WrongProducer { .. })
        ));

        assert!(matches!(
            NativeArtifactIndex::import(
                &bytes,
                digest(&bytes),
                index.target(),
                index.producer(),
                directory.path(),
            ),
            Err(NativeIndexError::Read { path, error })
                if path.parent() == Some(directory.path())
                    && error.kind() == std::io::ErrorKind::NotFound
        ));

        for unit in index.units() {
            std::fs::write(
                directory
                    .path()
                    .join(unit.kind().file_name(unit.digest(), index.target())),
                b"corrupt",
            )
            .unwrap_or_else(|error| panic!("test payload must be written: {error}"));
        }

        assert!(matches!(
            NativeArtifactIndex::import(
                &bytes,
                digest(&bytes),
                index.target(),
                index.producer(),
                directory.path(),
            ),
            Err(NativeIndexError::PayloadDigestMismatch { .. })
        ));
    }

    #[test]
    fn import_rejects_malformed_ids_and_cross_unit_edges() {
        let (index, _) = index();

        let bytes = index
            .encode()
            .unwrap_or_else(|error| panic!("index must encode: {error:?}"));

        let mut wire: serde_json::Value = serde_json::from_slice(&bytes)
            .unwrap_or_else(|error| panic!("index must be JSON: {error}"));

        wire["units"][0]["digest"] = serde_json::Value::String("not-a-digest".to_owned());

        let malformed =
            serde_json::to_vec(&wire).unwrap_or_else(|error| panic!("test JSON must encode: {error}"));

        assert!(matches!(
            NativeArtifactIndex::import(
                &malformed,
                digest(&malformed),
                index.target(),
                index.producer(),
                tempfile::tempdir()
                    .unwrap_or_else(|error| panic!("test directory must exist: {error}"))
                    .path(),
            ),
            Err(NativeIndexError::Wire(
                crate::wire::WireError::InvalidDigest
            ))
        ));

        wire["units"][0]["digest"] = serde_json::Value::String(index.units()[0].digest().to_string());
        wire["object_format"] = serde_json::Value::String("coff".to_owned());

        let wrong_format =
            serde_json::to_vec(&wire).unwrap_or_else(|error| panic!("test JSON must encode: {error}"));

        assert!(matches!(
            NativeArtifactIndex::import(
                &wrong_format,
                digest(&wrong_format),
                index.target(),
                index.producer(),
                tempfile::tempdir()
                    .unwrap_or_else(|error| panic!("test directory must exist: {error}"))
                    .path(),
            ),
            Err(NativeIndexError::Wire(
                crate::wire::WireError::InvalidTarget
            ))
        ));

        let absent = digest(b"absent unit");
        let unit = exact(digest(b"unit"), Vec::new(), Vec::new(), Vec::new());

        let group = NativeCoRetentionGroup::try_new([unit.digest(), absent])
            .unwrap_or_else(|| panic!("test group must be valid"));

        assert!(matches!(
            NativeArtifactIndex::try_new(index.target(), index.producer(), [unit], [group]),
            Err(NativeIndexError::MissingCoRetentionMember(member)) if member == absent
        ));

        let opaque_object = NativeUnit::new(
            digest(b"object with unknown retention"),
            NativeUnitKind::Object,
            NativeUnitSummary::Opaque,
            [],
            [],
        );

        assert!(
            NativeArtifactIndex::try_new(index.target(), index.producer(), [opaque_object], []).is_ok()
        );

        let orphan = exact(
            digest(b"orphan association"),
            vec![NativeDefinition::new(
                symbol(
                    NativeSymbolIdentity::Name(name("child")),
                    None,
                    NativeSymbolBinding::Strong,
                    NativeSymbolPresence::Required,
                ),
                NativeDefinitionSelection::Comdat {
                    group: NativeSymbolIdentity::Name(name("group")),
                    rule: NativeComdatSelection::Any,
                    associative_with: Some(NativeSymbolIdentity::Name(name("missing_parent"))),
                },
            )],
            Vec::new(),
            Vec::new(),
        );

        assert!(matches!(
            NativeArtifactIndex::try_new(index.target(), index.producer(), [orphan], []),
            Err(NativeIndexError::InvalidAssociation(_))
        ));
    }

    #[test]
    fn construction_rejects_duplicate_units_and_unmodelled_link_options() {
        let (index, _) = index();

        let duplicate = index.units()[0].clone();

        assert!(matches!(
            NativeArtifactIndex::try_new(
                index.target(),
                index.producer(),
                [duplicate.clone(), duplicate],
                [],
            ),
            Err(NativeIndexError::DuplicateUnit(_))
        ));

        let invalid_option = NativeUnit::new(
            digest(b"invalid option"),
            NativeUnitKind::Bitcode,
            NativeUnitSummary::Opaque,
            [],
            [Arc::from("-bad\noption")],
        );

        assert!(matches!(
            NativeArtifactIndex::try_new(index.target(), index.producer(), [invalid_option], []),
            Err(NativeIndexError::InvalidLinkOption(_))
        ));

        assert!(NativeCoRetentionGroup::try_new([index.units()[0].digest()]).is_none());

        let group = index.co_retention_groups()[0].clone();

        assert!(matches!(
            NativeArtifactIndex::try_new(
                index.target(),
                index.producer(),
                index.units().iter().cloned(),
                [group.clone(), group],
            ),
            Err(NativeIndexError::DuplicateCoRetentionGroup)
        ));
    }

    #[test]
    fn import_rejects_unsupported_schema_without_loading_payloads() {
        let (index, _) = index();

        let bytes = index
            .encode()
            .unwrap_or_else(|error| panic!("index must encode: {error:?}"));

        let mut wire: serde_json::Value = serde_json::from_slice(&bytes)
            .unwrap_or_else(|error| panic!("index must be JSON: {error}"));

        wire["revision"] = serde_json::Value::from(2);

        let bytes =
            serde_json::to_vec(&wire).unwrap_or_else(|error| panic!("test JSON must encode: {error}"));

        assert!(matches!(
            NativeArtifactIndex::import(
                &bytes,
                digest(&bytes),
                index.target(),
                index.producer(),
                tempfile::tempdir()
                    .unwrap_or_else(|error| panic!("test directory must exist: {error}"))
                    .path(),
            ),
            Err(NativeIndexError::Wire(
                crate::wire::WireError::UnsupportedSchema
            ))
        ));
    }

    #[test]
    fn comdat_selection_rules_round_trip() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("test payload directory must exist: {error}"));

        for rule in [
            NativeComdatSelection::Any,
            NativeComdatSelection::SameSize,
            NativeComdatSelection::ExactMatch,
            NativeComdatSelection::Largest,
            NativeComdatSelection::NoDuplicates,
        ] {
            let payload = format!("object with {rule:?}");
            let id = digest(payload.as_bytes());

            let unit = exact(
                id,
                vec![NativeDefinition::new(
                    symbol(
                        NativeSymbolIdentity::Name(name("entry")),
                        None,
                        NativeSymbolBinding::Strong,
                        NativeSymbolPresence::Required,
                    ),
                    NativeDefinitionSelection::Comdat {
                        group: NativeSymbolIdentity::Name(name("group")),
                        rule,
                        associative_with: None,
                    },
                )],
                Vec::new(),
                Vec::new(),
            );

            let index = NativeArtifactIndex::try_new(
                NativeTarget::X86_64LinuxGnu,
                digest(b"producer"),
                [unit],
                [],
            )
            .unwrap_or_else(|error| panic!("COMDAT index must validate: {error:?}"));

            std::fs::write(
                directory
                    .path()
                    .join(NativeUnitKind::Object.file_name(id, index.target())),
                payload,
            )
            .unwrap_or_else(|error| panic!("test payload must be written: {error}"));

            let bytes = index
                .encode()
                .unwrap_or_else(|error| panic!("index must encode: {error:?}"));

            let imported = NativeArtifactIndex::import(
                &bytes,
                digest(&bytes),
                index.target(),
                index.producer(),
                directory.path(),
            )
            .unwrap_or_else(|error| panic!("COMDAT index must import: {error:?}"));

            assert_eq!(imported.index(), &index);
        }
    }
}
