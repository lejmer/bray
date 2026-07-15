use crate::hash::{compute_artifact_hash, compute_content_hash, compute_section_hash};
use crate::header::{BYTE_ORDER_MARKER, InterfaceHeader, MAGIC};
use crate::section::DirectoryEntry;
use crate::surface::{EncodedSurfaceSection, encode_surface};
use crate::wire::WireEncoder;
use crate::{
    CURRENT_FORMAT_REVISION, InterfaceLanguageRevision, InterfaceRequiredFlags,
    InterfaceSectionTag, InterfaceSemanticFacts, InterfaceValidationError,
    InterfaceValidationLimits, PackageInterfaceSurface, encode_semantic_facts,
};

pub(crate) fn encode_interface_artifact(
    surface: &PackageInterfaceSurface,
    facts: &InterfaceSemanticFacts,
    language_revision: InterfaceLanguageRevision,
) -> Result<Vec<u8>, InterfaceValidationError> {
    let mut sections = encode_surface(surface)
        .into_iter()
        .map(EncodedArtifactSection::from_surface)
        .chain(
            encode_semantic_facts(facts, surface, InterfaceValidationLimits::default())?
                .into_iter()
                .map(EncodedArtifactSection::from_semantic),
        )
        .collect::<Vec<_>>();

    sections.sort_by_key(|section| section.tag);

    if sections.windows(2).any(|pair| pair[0].tag == pair[1].tag) {
        return Err(InterfaceValidationError::Malformed);
    }

    assemble_sections(&sections, language_revision)
}

pub(crate) fn assemble_sections(
    sections: &[EncodedArtifactSection],
    language_revision: InterfaceLanguageRevision,
) -> Result<Vec<u8>, InterfaceValidationError> {
    let payload_length = sections.iter().try_fold(0_usize, |total, section| {
        total.checked_add(section.payload.len())
    });

    let directory_length = sections
        .len()
        .checked_mul(DirectoryEntry::LENGTH)
        .ok_or(InterfaceValidationError::Malformed)?;

    let directory_offset = InterfaceHeader::LENGTH
        .checked_add(payload_length.ok_or(InterfaceValidationError::Malformed)?)
        .ok_or(InterfaceValidationError::Malformed)?;

    let file_length = directory_offset
        .checked_add(directory_length)
        .ok_or(InterfaceValidationError::Malformed)?;

    let mut encoder = WireEncoder::new();

    encode_header(
        &mut encoder,
        language_revision,
        file_length,
        directory_offset,
        directory_length,
    )?;

    let mut entries = Vec::with_capacity(sections.len());
    let mut payload_offset = InterfaceHeader::LENGTH;

    for section in sections {
        let entry = encoded_directory_entry(section, payload_offset)?;
        let checksum = compute_section_hash(&entry, &section.payload);

        entries.push(DirectoryEntry::for_encoded(
            section.tag,
            usize_to_u64(payload_offset)?,
            usize_to_u64(section.payload.len())?,
            section.record_count,
            checksum,
        ));

        encoder.write_bytes(&section.payload);

        payload_offset = payload_offset
            .checked_add(section.payload.len())
            .ok_or(InterfaceValidationError::Malformed)?;
    }

    for entry in &entries {
        encode_directory_entry(&mut encoder, *entry);
    }

    finish_hashes(encoder.into_bytes(), &entries)
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

    encoder.write_u64(usize_to_u64(file_length)?);
    encoder.write_u64(usize_to_u64(directory_offset)?);
    encoder.write_u64(usize_to_u64(directory_length)?);

    encoder.write_bytes(&[0; 32]);
    encoder.write_bytes(&[0; 32]);

    Ok(())
}

fn encoded_directory_entry(
    section: &EncodedArtifactSection,
    payload_offset: usize,
) -> Result<DirectoryEntry, InterfaceValidationError> {
    Ok(DirectoryEntry::for_encoded(
        section.tag,
        usize_to_u64(payload_offset)?,
        usize_to_u64(section.payload.len())?,
        section.record_count,
        crate::InterfaceSectionHash::from_bytes([0; 32]),
    ))
}

fn encode_directory_entry(encoder: &mut WireEncoder, entry: DirectoryEntry) {
    encoder.write_u32(entry.tag().wire_value());

    encoder.write_u32(0);

    encoder.write_u64(entry.offset());
    encoder.write_u64(entry.length());
    encoder.write_u64(entry.record_count());

    encoder.write_bytes(entry.checksum().as_bytes());
}

fn finish_hashes(
    mut bytes: Vec<u8>,
    entries: &[DirectoryEntry],
) -> Result<Vec<u8>, InterfaceValidationError> {
    let decoded =
        InterfaceHeader::decode(&bytes).map_err(|_| InterfaceValidationError::Malformed)?;

    let content_hash = compute_content_hash(&decoded.header, entries, &bytes)
        .ok_or(InterfaceValidationError::Malformed)?;

    bytes[InterfaceHeader::CONTENT_HASH_OFFSET..InterfaceHeader::CONTENT_HASH_OFFSET + 32]
        .copy_from_slice(content_hash.as_bytes());

    let artifact_hash = compute_artifact_hash(&bytes).ok_or(InterfaceValidationError::Malformed)?;

    bytes[InterfaceHeader::ARTIFACT_HASH_OFFSET..InterfaceHeader::ARTIFACT_HASH_OFFSET + 32]
        .copy_from_slice(artifact_hash.as_bytes());

    Ok(bytes)
}

fn usize_to_u64(value: usize) -> Result<u64, InterfaceValidationError> {
    u64::try_from(value).map_err(|_| InterfaceValidationError::Malformed)
}

pub(crate) struct EncodedArtifactSection {
    tag: InterfaceSectionTag,
    record_count: u64,
    payload: Vec<u8>,
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

    fn from_surface(section: EncodedSurfaceSection) -> Self {
        Self {
            tag: section.tag,
            record_count: section.record_count,
            payload: section.payload,
        }
    }

    fn from_semantic(section: crate::EncodedSemanticSection) -> Self {
        let (tag, record_count, payload) = section.into_parts();

        Self {
            tag,
            record_count,
            payload,
        }
    }
}
