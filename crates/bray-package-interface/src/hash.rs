use blake3::Hasher;

use crate::header::InterfaceHeader;
use crate::section::DirectoryEntry;
use crate::wire::WireEncoder;

const CONTENT_HASH_DOMAIN: &[u8] = b"bray.package-interface.content.v1";
const SECTION_HASH_DOMAIN: &[u8] = b"bray.package-interface.section.v1";

macro_rules! interface_hash {
    ($name:ident, $documentation:literal) => {
        #[doc = $documentation]
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name([u8; Self::LENGTH]);

        impl $name {
            /// Digest length in bytes.
            pub const LENGTH: usize = 32;

            /// Creates a hash from its canonical bytes.
            pub const fn from_bytes(bytes: [u8; Self::LENGTH]) -> Self {
                Self(bytes)
            }

            /// Returns the canonical hash bytes.
            pub const fn as_bytes(&self) -> &[u8; Self::LENGTH] {
                &self.0
            }
        }
    };
}

interface_hash!(
    InterfaceArtifactHash,
    "BLAKE3 digest of one exact canonical package-interface artifact."
);
interface_hash!(
    InterfaceContentHash,
    "BLAKE3 digest of the semantic package-interface sections."
);
interface_hash!(
    InterfaceSectionHash,
    "Domain-separated BLAKE3 digest of one package-interface section."
);

pub(crate) fn compute_section_hash(entry: &DirectoryEntry, payload: &[u8]) -> InterfaceSectionHash {
    let mut prefix = WireEncoder::new();

    prefix.write_u32(entry.raw_tag());
    prefix.write_u64(entry.record_count());
    prefix.write_u64(entry.length());

    let mut hasher = Hasher::new();

    hasher.update(SECTION_HASH_DOMAIN);
    hasher.update(prefix.bytes());
    hasher.update(payload);

    InterfaceSectionHash::from_bytes(*hasher.finalize().as_bytes())
}

pub(crate) fn compute_content_hash(
    header: &InterfaceHeader,
    entries: &[DirectoryEntry],
    bytes: &[u8],
) -> Option<InterfaceContentHash> {
    let mut prefix = WireEncoder::new();

    prefix.write_u16(header.format_revision().raw());
    prefix.write_u16(header.language_revision().raw());
    prefix.write_u64(header.required_flags().bits());

    let mut hasher = Hasher::new();

    hasher.update(CONTENT_HASH_DOMAIN);
    hasher.update(prefix.bytes());

    for entry in entries {
        if !entry.contributes_to_content_hash() {
            continue;
        }

        let payload = entry.payload(bytes)?;

        let mut section_prefix = WireEncoder::new();

        section_prefix.write_u32(entry.raw_tag());
        section_prefix.write_u64(entry.length());

        hasher.update(section_prefix.bytes());
        hasher.update(payload);
    }

    Some(InterfaceContentHash::from_bytes(
        *hasher.finalize().as_bytes(),
    ))
}

pub(crate) fn compute_artifact_hash(bytes: &[u8]) -> Option<InterfaceArtifactHash> {
    let mut hasher = Hasher::new();

    let (before, after_hash) = bytes.split_at_checked(InterfaceHeader::ARTIFACT_HASH_OFFSET)?;
    let (_, after) = after_hash.split_at_checked(InterfaceArtifactHash::LENGTH)?;

    hasher.update(before);
    hasher.update(&[0; InterfaceArtifactHash::LENGTH]);
    hasher.update(after);

    Some(InterfaceArtifactHash::from_bytes(
        *hasher.finalize().as_bytes(),
    ))
}

#[cfg(test)]
mod tests {
    use super::{InterfaceArtifactHash, InterfaceContentHash, InterfaceSectionHash};

    #[test]
    fn interface_hashes_preserve_exact_digest_bytes() {
        let bytes = [0x5a; 32];

        assert_eq!(InterfaceArtifactHash::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(InterfaceContentHash::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(InterfaceSectionHash::from_bytes(bytes).as_bytes(), &bytes);
    }
}
