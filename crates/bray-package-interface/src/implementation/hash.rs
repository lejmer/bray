use blake3::Hasher;
use bray_symbols::InterfaceSymbolId;

use crate::InterfaceLanguageRevision;
use crate::wire::WireEncoder;

use super::artifact::{ARTIFACT_HASH_OFFSET, ImplementationDirectoryEntry, REQUIRED_FLAGS};

const CONTENT_HASH_DOMAIN: &[u8] = b"bray.package-implementation.content.v1";
const PAYLOAD_CONTENT_HASH_DOMAIN: &[u8] = b"bray.package-implementation.payload-content.v1";
const PAYLOAD_HASH_DOMAIN: &[u8] = b"bray.package-implementation.payload.v1";

pub(super) fn compute_payload_content_hash(
    owner: InterfaceSymbolId,
    raw_kind: u8,
    discriminator: [u8; 32],
    payload: &[u8],
) -> [u8; 32] {
    let mut prefix = WireEncoder::new();

    prefix.write_u32(owner.raw());
    prefix.write_u8(raw_kind);
    prefix.write_bytes(&discriminator);
    prefix.write_u64(u64::try_from(payload.len()).unwrap_or(u64::MAX));

    let mut hasher = Hasher::new();

    hasher.update(PAYLOAD_CONTENT_HASH_DOMAIN);
    hasher.update(prefix.bytes());
    hasher.update(payload);

    *hasher.finalize().as_bytes()
}

pub(super) fn compute_payload_hash(
    entry: &ImplementationDirectoryEntry,
    payload: &[u8],
) -> [u8; 32] {
    let mut hasher = payload_hasher(entry, payload.len() as u64);

    hasher.update(payload);

    *hasher.finalize().as_bytes()
}

pub(super) fn payload_hasher(entry: &ImplementationDirectoryEntry, length: u64) -> Hasher {
    let mut prefix = WireEncoder::new();

    prefix.write_u32(entry.owner.raw());
    prefix.write_u8(entry.raw_kind);
    prefix.write_u8(entry.compatibility.wire_value());
    prefix.write_u8(entry.encoding.wire_value());
    prefix.write_u16(crate::InterfaceSectionRevision::CURRENT.raw());
    prefix.write_bytes(&entry.discriminator);
    prefix.write_u32(entry.family_size);
    prefix.write_u32(entry.platform_service.map_or(0, |role| role.id()));
    prefix.write_u64(entry.decoded_length);
    prefix.write_u64(entry.record_count);
    prefix.write_u64(length);
    prefix.write_bytes(&entry.content_hash);

    let mut hasher = Hasher::new();

    hasher.update(PAYLOAD_HASH_DOMAIN);
    hasher.update(prefix.bytes());

    hasher
}

pub(super) fn compute_content_hash(
    language_revision: InterfaceLanguageRevision,
    directory: &[ImplementationDirectoryEntry],
) -> [u8; 32] {
    let mut prefix = WireEncoder::new();

    prefix.write_u16(crate::CURRENT_FORMAT_REVISION.raw());
    prefix.write_u16(language_revision.raw());
    prefix.write_u64(REQUIRED_FLAGS);

    let mut hasher = Hasher::new();

    hasher.update(CONTENT_HASH_DOMAIN);
    hasher.update(prefix.bytes());

    for entry in directory.iter().filter(|entry| entry.kind.is_some()) {
        let mut entry_prefix = WireEncoder::new();

        entry_prefix.write_u32(entry.owner.raw());
        entry_prefix.write_u8(entry.raw_kind);
        entry_prefix.write_bytes(&entry.discriminator);
        entry_prefix.write_u32(entry.family_size);
        entry_prefix.write_u32(entry.platform_service.map_or(0, |role| role.id()));
        entry_prefix.write_u64(entry.decoded_length);
        entry_prefix.write_u64(entry.record_count);
        entry_prefix.write_bytes(&entry.content_hash);

        hasher.update(entry_prefix.bytes());
    }

    *hasher.finalize().as_bytes()
}

pub(super) fn compute_artifact_hash(bytes: &[u8]) -> Option<[u8; 32]> {
    let offset = usize::try_from(u64::from_le_bytes(bytes.get(32..40)?.try_into().ok()?)).ok()?;

    compute_metadata_hash(
        bytes.get(..super::artifact::HEADER_LENGTH)?,
        bytes.get(offset..)?,
    )
}

pub(super) fn compute_metadata_hash(header: &[u8], directory: &[u8]) -> Option<[u8; 32]> {
    let mut hasher = metadata_hasher(header)?;

    hasher.update(directory);

    Some(*hasher.finalize().as_bytes())
}

pub(super) fn metadata_hasher(header: &[u8]) -> Option<Hasher> {
    let (before, after_hash) = header.split_at_checked(ARTIFACT_HASH_OFFSET)?;

    let (_, after) = after_hash.split_at_checked(32)?;

    let mut hasher = Hasher::new();

    hasher.update(b"bray.package-implementation.metadata.v1");
    hasher.update(before);
    hasher.update(&[0; 32]);
    hasher.update(after);

    Some(hasher)
}
