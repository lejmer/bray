use std::sync::Arc;

use crate::encoding::encode_section;
use crate::hash::{
    InterfaceSectionContentHash, compute_artifact_hash, compute_content_hash,
    compute_section_content_hash, compute_section_hash,
};
use crate::header::{BYTE_ORDER_MARKER, InterfaceHeader, MAGIC};
use crate::section::DirectoryEntry;
use crate::surface::{EncodedSurfaceSection, encode_surface};
use crate::wire::WireEncoder;
use crate::{
    CURRENT_FORMAT_REVISION, InterfaceLanguageRevision, InterfaceRequiredFlags,
    InterfaceSectionTag, InterfaceValidationError, PackageInterfaceExportBundle,
    PackageInterfaceIdentity,
};

/// Complete immutable artifact for one encoded package interface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterfaceArtifact {
    identity: PackageInterfaceIdentity,
    bytes: Arc<[u8]>,
    byte_len: u64,
    section_count: u64,
    content_hash: crate::InterfaceContentHash,
    artifact_hash: crate::InterfaceArtifactHash,
}

impl InterfaceArtifact {
    fn new(
        identity: PackageInterfaceIdentity,
        bytes: Vec<u8>,
        byte_len: u64,
        section_count: u64,
        content_hash: crate::InterfaceContentHash,
        artifact_hash: crate::InterfaceArtifactHash,
    ) -> Self {
        Self {
            identity,
            bytes: bytes.into(),
            byte_len,
            section_count,
            content_hash,
            artifact_hash,
        }
    }

    /// Returns the package and product identity represented by this artifact.
    pub const fn identity(&self) -> &PackageInterfaceIdentity {
        &self.identity
    }

    /// Returns the complete canonical artifact bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns shared ownership of the complete canonical artifact bytes.
    pub fn shared_bytes(&self) -> Arc<[u8]> {
        Arc::clone(&self.bytes)
    }

    /// Returns the exact artifact byte length.
    pub const fn byte_len(&self) -> u64 {
        self.byte_len
    }

    /// Returns the exact number of canonical interface sections.
    pub const fn section_count(&self) -> u64 {
        self.section_count
    }

    /// Returns the semantic content identity of the encoded interface.
    pub const fn content_hash(&self) -> crate::InterfaceContentHash {
        self.content_hash
    }

    /// Returns the identity of the exact encoded artifact bytes.
    pub const fn artifact_hash(&self) -> crate::InterfaceArtifactHash {
        self.artifact_hash
    }

    /// Verifies that the retained bytes match the artifact's declared length and identity hash.
    pub fn validate_integrity(&self) -> Result<(), InterfaceArtifactIntegrityError> {
        let actual_length = u64::try_from(self.bytes.len())
            .map_err(|_| InterfaceArtifactIntegrityError::LengthExceeded)?;

        if actual_length != self.byte_len {
            return Err(InterfaceArtifactIntegrityError::LengthMismatch {
                expected: self.byte_len,
                actual: actual_length,
            });
        }

        let decoded = InterfaceHeader::decode(&self.bytes)
            .map_err(|_| InterfaceArtifactIntegrityError::Malformed)?;

        if decoded.header.content_hash() != self.content_hash {
            return Err(InterfaceArtifactIntegrityError::ContentHashMismatch {
                expected: self.content_hash,
                actual: decoded.header.content_hash(),
            });
        }

        if decoded.header.artifact_hash() != self.artifact_hash {
            return Err(InterfaceArtifactIntegrityError::ArtifactHashMismatch {
                expected: self.artifact_hash,
                actual: decoded.header.artifact_hash(),
            });
        }

        let actual_hash =
            compute_artifact_hash(&self.bytes).ok_or(InterfaceArtifactIntegrityError::Malformed)?;

        if actual_hash != self.artifact_hash {
            return Err(InterfaceArtifactIntegrityError::ArtifactHashMismatch {
                expected: self.artifact_hash,
                actual: actual_hash,
            });
        }

        Ok(())
    }
}

impl AsRef<[u8]> for InterfaceArtifact {
    fn as_ref(&self) -> &[u8] {
        self.bytes()
    }
}

/// A completed package-interface artifact whose bytes violate its integrity contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InterfaceArtifactIntegrityError {
    /// The byte count exceeds the artifact length contract.
    LengthExceeded,
    /// The retained byte count differs from the completed artifact length.
    LengthMismatch {
        /// Length recorded when the artifact was completed.
        expected: u64,
        /// Length observed while validating the retained bytes.
        actual: u64,
    },
    /// The retained bytes cannot be hashed as a package-interface artifact.
    Malformed,
    /// The semantic content identity no longer matches the retained bytes.
    ContentHashMismatch {
        /// Hash recorded when the artifact was completed.
        expected: crate::InterfaceContentHash,
        /// Hash declared by the retained bytes.
        actual: crate::InterfaceContentHash,
    },
    /// The exact-byte identity no longer matches the retained bytes.
    ArtifactHashMismatch {
        /// Hash recorded when the artifact was completed.
        expected: crate::InterfaceArtifactHash,
        /// Hash computed from the retained bytes.
        actual: crate::InterfaceArtifactHash,
    },
}

pub(crate) fn encode_interface_artifact(
    bundle: &PackageInterfaceExportBundle,
) -> Result<InterfaceArtifact, InterfaceValidationError> {
    let mut sections = encode_surface(bundle.surface())
        .into_iter()
        .map(EncodedArtifactSection::from_surface)
        .chain(
            crate::semantic::encode_validated_semantics(bundle.semantics())
                .into_iter()
                .map(EncodedArtifactSection::from_semantic),
        )
        .collect::<Vec<_>>();

    sections.sort_by_key(|section| section.tag);

    if sections.windows(2).any(|pair| pair[0].tag == pair[1].tag) {
        let duplicate = sections
            .windows(2)
            .find(|pair| pair[0].tag == pair[1].tag)
            .map(|pair| pair[0].tag.wire_value())
            .unwrap_or_default();

        return Err(InterfaceValidationError::Malformed {
            context: crate::InterfaceValidationContext::Directory,
            cause: crate::InterfaceMalformedCause::Duplicate {
                field: crate::InterfaceValidationField::SectionTag,
                index: u64::from(duplicate),
            },
        });
    }

    assemble_sections(
        &sections,
        bundle.surface().identity().clone(),
        bundle.language_revision(),
    )
}

pub(crate) fn assemble_sections(
    sections: &[EncodedArtifactSection],
    identity: PackageInterfaceIdentity,
    language_revision: InterfaceLanguageRevision,
) -> Result<InterfaceArtifact, InterfaceValidationError> {
    let stored_sections = sections
        .iter()
        .map(StoredArtifactSection::try_from_decoded)
        .collect::<Result<Vec<_>, _>>()?;

    assemble_stored_sections(&stored_sections, identity, language_revision)
}

fn assemble_stored_sections(
    stored_sections: &[StoredArtifactSection],
    identity: PackageInterfaceIdentity,
    language_revision: InterfaceLanguageRevision,
) -> Result<InterfaceArtifact, InterfaceValidationError> {
    let payload_length = stored_sections.iter().try_fold(0_usize, |total, section| {
        total.checked_add(section.payload.len())
    });

    let directory_length = stored_sections
        .len()
        .checked_mul(DirectoryEntry::LENGTH)
        .ok_or(InterfaceValidationError::Malformed {
            context: crate::InterfaceValidationContext::Directory,
            cause: crate::InterfaceMalformedCause::RangeOverflow {
                offset: stored_sections.len() as u64,
                length: DirectoryEntry::LENGTH as u64,
            },
        })?;

    let directory_offset = InterfaceHeader::LENGTH
        .checked_add(
            payload_length.ok_or(InterfaceValidationError::Malformed {
                context: crate::InterfaceValidationContext::Artifact,
                cause: crate::InterfaceMalformedCause::RangeOverflow {
                    offset: 0,
                    length: stored_sections
                        .iter()
                        .map(|section| section.payload.len() as u64)
                        .sum(),
                },
            })?,
        )
        .ok_or(InterfaceValidationError::Malformed {
            context: crate::InterfaceValidationContext::Artifact,
            cause: crate::InterfaceMalformedCause::RangeOverflow {
                offset: InterfaceHeader::LENGTH as u64,
                length: payload_length.unwrap_or(usize::MAX) as u64,
            },
        })?;

    let file_length = directory_offset.checked_add(directory_length).ok_or(
        InterfaceValidationError::Malformed {
            context: crate::InterfaceValidationContext::Artifact,
            cause: crate::InterfaceMalformedCause::RangeOverflow {
                offset: directory_offset as u64,
                length: directory_length as u64,
            },
        },
    )?;

    let mut encoder = WireEncoder::new();

    encode_header(
        &mut encoder,
        language_revision,
        file_length,
        directory_offset,
        directory_length,
    )?;

    let mut entries = Vec::with_capacity(stored_sections.len());
    let mut payload_offset = InterfaceHeader::LENGTH;

    for section in stored_sections {
        let entry = encoded_directory_entry(section, payload_offset)?;
        let checksum = compute_section_hash(&entry, &section.payload);

        entries.push(DirectoryEntry::for_encoded(
            section.tag,
            section.encoding,
            usize_to_u64(
                crate::InterfaceValidationContext::Section(section.tag),
                crate::InterfaceValidationField::SectionOffset,
                payload_offset,
            )?,
            usize_to_u64(
                crate::InterfaceValidationContext::Section(section.tag),
                crate::InterfaceValidationField::EncodedLength,
                section.payload.len(),
            )?,
            section.decoded_length,
            section.record_count,
            checksum,
            section.content_hash,
        ));

        encoder.write_bytes(&section.payload);

        payload_offset = payload_offset.checked_add(section.payload.len()).ok_or(
            InterfaceValidationError::Malformed {
                context: crate::InterfaceValidationContext::Section(section.tag),
                cause: crate::InterfaceMalformedCause::RangeOverflow {
                    offset: payload_offset as u64,
                    length: section.payload.len() as u64,
                },
            },
        )?;
    }

    for entry in &entries {
        encode_directory_entry(&mut encoder, *entry);
    }

    finish_hashes(identity, encoder.into_bytes(), &entries)
}

fn encode_header(
    encoder: &mut WireEncoder,
    language_revision: InterfaceLanguageRevision,
    file_length: usize,
    directory_offset: usize,
    directory_length: usize,
) -> Result<(), InterfaceValidationError> {
    encoder.write_bytes(&MAGIC);

    encoder.write_u16(CURRENT_FORMAT_REVISION.raw());
    encoder.write_u16(language_revision.raw());
    encoder.write_u32(BYTE_ORDER_MARKER);

    encoder.write_u64(InterfaceRequiredFlags::NONE.bits());

    encoder.write_u64(usize_to_u64(
        crate::InterfaceValidationContext::Header,
        crate::InterfaceValidationField::DeclaredFileLength,
        file_length,
    )?);
    encoder.write_u64(usize_to_u64(
        crate::InterfaceValidationContext::Header,
        crate::InterfaceValidationField::DirectoryOffset,
        directory_offset,
    )?);
    encoder.write_u64(usize_to_u64(
        crate::InterfaceValidationContext::Header,
        crate::InterfaceValidationField::DirectoryLength,
        directory_length,
    )?);

    encoder.write_bytes(&[0; 32]);
    encoder.write_bytes(&[0; 32]);

    Ok(())
}

fn encoded_directory_entry(
    section: &StoredArtifactSection,
    payload_offset: usize,
) -> Result<DirectoryEntry, InterfaceValidationError> {
    Ok(DirectoryEntry::for_encoded(
        section.tag,
        section.encoding,
        usize_to_u64(
            crate::InterfaceValidationContext::Section(section.tag),
            crate::InterfaceValidationField::SectionOffset,
            payload_offset,
        )?,
        usize_to_u64(
            crate::InterfaceValidationContext::Section(section.tag),
            crate::InterfaceValidationField::EncodedLength,
            section.payload.len(),
        )?,
        section.decoded_length,
        section.record_count,
        crate::InterfaceSectionHash::from_bytes([0; 32]),
        section.content_hash,
    ))
}

fn encode_directory_entry(encoder: &mut WireEncoder, entry: DirectoryEntry) {
    encoder.write_u32(entry.raw_tag());
    encoder.write_u16(entry.section_revision().raw());
    encoder.write_u8(entry.raw_compatibility());
    encoder.write_u8(entry.raw_encoding());

    encoder.write_u64(entry.offset());
    encoder.write_u64(entry.encoded_length());
    encoder.write_u64(entry.decoded_length());
    encoder.write_u64(entry.record_count());

    encoder.write_bytes(entry.checksum().as_bytes());
    encoder.write_bytes(entry.content_hash().as_bytes());
}

fn finish_hashes(
    identity: PackageInterfaceIdentity,
    mut bytes: Vec<u8>,
    entries: &[DirectoryEntry],
) -> Result<InterfaceArtifact, InterfaceValidationError> {
    let decoded = InterfaceHeader::decode(&bytes).map_err(|error| {
        crate::decode::map_wire_error(
            crate::InterfaceValidationContext::Header,
            crate::InterfaceValidationField::Value,
            error,
        )
    })?;

    let content_hash = compute_content_hash(
        &decoded.header,
        entries.iter().filter_map(|entry| {
            entry
                .tag()
                .map(|tag| (tag, entry.decoded_length(), entry.content_hash()))
        }),
    );

    bytes[InterfaceHeader::CONTENT_HASH_OFFSET..InterfaceHeader::CONTENT_HASH_OFFSET + 32]
        .copy_from_slice(content_hash.as_bytes());

    let artifact_hash =
        compute_artifact_hash(&bytes).ok_or(InterfaceValidationError::DigestUnavailable {
            context: crate::InterfaceValidationContext::Artifact,
            field: crate::InterfaceValidationField::ArtifactHash,
        })?;

    bytes[InterfaceHeader::ARTIFACT_HASH_OFFSET..InterfaceHeader::ARTIFACT_HASH_OFFSET + 32]
        .copy_from_slice(artifact_hash.as_bytes());

    let byte_len = usize_to_u64(
        crate::InterfaceValidationContext::Artifact,
        crate::InterfaceValidationField::DeclaredFileLength,
        bytes.len(),
    )?;
    let section_count = usize_to_u64(
        crate::InterfaceValidationContext::Directory,
        crate::InterfaceValidationField::RecordCount,
        entries.len(),
    )?;

    Ok(InterfaceArtifact::new(
        identity,
        bytes,
        byte_len,
        section_count,
        content_hash,
        artifact_hash,
    ))
}

fn usize_to_u64(
    context: crate::InterfaceValidationContext,
    field: crate::InterfaceValidationField,
    value: usize,
) -> Result<u64, InterfaceValidationError> {
    u64::try_from(value).map_err(|_| InterfaceValidationError::Malformed {
        context,
        cause: crate::InterfaceMalformedCause::NumericOverflow {
            field,
            value: u64::MAX,
            target: crate::InterfaceIntegerTarget::U64,
        },
    })
}

pub(crate) struct EncodedArtifactSection {
    tag: InterfaceSectionTag,
    record_count: u64,
    payload: Vec<u8>,
}

struct StoredArtifactSection {
    tag: InterfaceSectionTag,
    encoding: crate::InterfaceSectionEncoding,
    decoded_length: u64,
    record_count: u64,
    content_hash: InterfaceSectionContentHash,
    payload: Vec<u8>,
}

impl StoredArtifactSection {
    fn try_from_decoded(
        section: &EncodedArtifactSection,
    ) -> Result<Self, InterfaceValidationError> {
        let context = crate::InterfaceValidationContext::Section(section.tag);
        let decoded_length = usize_to_u64(
            context,
            crate::InterfaceValidationField::DecodedLength,
            section.payload.len(),
        )?;
        let content_hash = compute_section_content_hash(section.tag, &section.payload);

        let (encoding, payload) = encode_section(context, &section.payload)?;

        Ok(Self {
            tag: section.tag,
            encoding,
            decoded_length,
            record_count: section.record_count,
            content_hash,
            payload,
        })
    }
}

impl EncodedArtifactSection {
    #[cfg(test)]
    pub(crate) fn new(tag: InterfaceSectionTag, record_count: u64, payload: Vec<u8>) -> Self {
        Self {
            tag,
            record_count,
            payload,
        }
    }

    pub(crate) fn from_surface(section: EncodedSurfaceSection) -> Self {
        Self {
            tag: section.tag,
            record_count: section.record_count,
            payload: section.payload,
        }
    }

    pub(crate) fn from_semantic(section: crate::EncodedSemanticSection) -> Self {
        let (tag, record_count, payload) = section.into_parts();

        Self {
            tag,
            record_count,
            payload,
        }
    }

    #[cfg(test)]
    pub(crate) const fn tag(&self) -> InterfaceSectionTag {
        self.tag
    }

    #[cfg(test)]
    pub(crate) fn payload_mut(&mut self) -> &mut Vec<u8> {
        &mut self.payload
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{
        EncodedArtifactSection, InterfaceArtifactIntegrityError, StoredArtifactSection,
        assemble_sections, assemble_stored_sections,
    };
    use crate::encode_package_interface;
    use crate::header::InterfaceHeader;
    use crate::test_support::package_interface_export_bundle;
    use crate::{
        InterfaceSectionEncoding, InterfaceSectionTag, InterfaceValidationPolicy,
        ValidatedPackageInterface,
    };

    #[test]
    fn completed_artifacts_retain_identity_length_and_exact_hash_integrity() {
        let bundle = package_interface_export_bundle();

        let artifact = encode_package_interface(&bundle)
            .unwrap_or_else(|error| panic!("test interface must encode: {error:?}"));

        let byte_len = u64::try_from(artifact.bytes().len())
            .unwrap_or_else(|_| panic!("test artifact length must fit the contract"));

        assert_eq!(artifact.identity(), bundle.surface().identity());
        assert_eq!(artifact.byte_len(), byte_len);
        assert!(artifact.section_count() > 0);
        assert_eq!(artifact.validate_integrity(), Ok(()));

        let mut wrong_length = artifact.clone();

        wrong_length.byte_len += 1;

        assert!(matches!(
            wrong_length.validate_integrity(),
            Err(InterfaceArtifactIntegrityError::LengthMismatch { .. })
        ));

        let mut wrong_hash = artifact;
        let mut bytes = wrong_hash.bytes().to_vec();

        bytes[0] ^= 0xff;

        wrong_hash.bytes = Arc::from(bytes);

        assert!(matches!(
            wrong_hash.validate_integrity(),
            Err(InterfaceArtifactIntegrityError::ArtifactHashMismatch { .. })
        ));
    }

    #[test]
    fn completed_artifacts_reject_embedded_hash_mismatches() {
        let bundle = package_interface_export_bundle();

        let artifact = encode_package_interface(&bundle)
            .unwrap_or_else(|error| panic!("test interface must encode: {error:?}"));

        let mut wrong_content_hash = artifact.clone();
        let mut bytes = wrong_content_hash.bytes().to_vec();

        bytes[InterfaceHeader::CONTENT_HASH_OFFSET] ^= 0xff;

        wrong_content_hash.bytes = Arc::from(bytes);

        assert!(matches!(
            wrong_content_hash.validate_integrity(),
            Err(InterfaceArtifactIntegrityError::ContentHashMismatch { .. })
        ));

        let mut wrong_artifact_hash = artifact;
        let mut bytes = wrong_artifact_hash.bytes().to_vec();

        bytes[InterfaceHeader::ARTIFACT_HASH_OFFSET] ^= 0xff;

        wrong_artifact_hash.bytes = Arc::from(bytes);

        assert!(matches!(
            wrong_artifact_hash.validate_integrity(),
            Err(InterfaceArtifactIntegrityError::ArtifactHashMismatch { .. })
        ));
    }

    #[test]
    fn canonical_compressed_sections_round_trip_to_their_exact_decoded_bytes() {
        let bundle = package_interface_export_bundle();
        let decoded = vec![0x5a; 4096];

        let sections = [EncodedArtifactSection::new(
            InterfaceSectionTag::Strings,
            1,
            decoded.clone(),
        )];

        let artifact = assemble_sections(
            &sections,
            bundle.surface().identity().clone(),
            bundle.language_revision(),
        )
        .unwrap_or_else(|error| panic!("compressed section must assemble: {error:?}"));

        let interface = ValidatedPackageInterface::try_new(
            artifact.shared_bytes(),
            InterfaceValidationPolicy::new(bundle.language_revision()),
        )
        .unwrap_or_else(|error| panic!("compressed section must validate: {error:?}"));

        let section = interface
            .section(InterfaceSectionTag::Strings)
            .unwrap_or_else(|error| panic!("compressed section must decode: {error:?}"))
            .unwrap_or_else(|| panic!("compressed section must be present"));

        assert_eq!(section.encoding(), InterfaceSectionEncoding::ZstdFrame);
        assert_eq!(section.bytes(), decoded);
    }

    #[test]
    fn storage_encoding_changes_exact_identity_but_not_semantic_identity() {
        let bundle = package_interface_export_bundle();
        let decoded = vec![0x5a; 4096];

        let sections = [EncodedArtifactSection::new(
            InterfaceSectionTag::Strings,
            1,
            decoded.clone(),
        )];

        let compressed = assemble_sections(
            &sections,
            bundle.surface().identity().clone(),
            bundle.language_revision(),
        )
        .unwrap_or_else(|error| panic!("compressed section must assemble: {error:?}"));

        let raw_sections = [StoredArtifactSection {
            tag: InterfaceSectionTag::Strings,
            encoding: InterfaceSectionEncoding::Raw,
            decoded_length: 4096,
            record_count: 1,
            content_hash: crate::hash::compute_section_content_hash(
                InterfaceSectionTag::Strings,
                &decoded,
            ),
            payload: decoded,
        }];

        let raw = assemble_stored_sections(
            &raw_sections,
            bundle.surface().identity().clone(),
            bundle.language_revision(),
        )
        .unwrap_or_else(|error| panic!("raw section must assemble: {error:?}"));

        assert_eq!(compressed.content_hash(), raw.content_hash());
        assert_ne!(compressed.artifact_hash(), raw.artifact_hash());
        assert_ne!(compressed.bytes(), raw.bytes());

        let repeated = assemble_sections(
            &sections,
            bundle.surface().identity().clone(),
            bundle.language_revision(),
        )
        .unwrap_or_else(|error| panic!("repeated section must assemble: {error:?}"));

        assert_eq!(compressed, repeated);
    }
}
