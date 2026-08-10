use std::fmt;

use blake3::Hasher;

use crate::header::InterfaceHeader;
use crate::section::DirectoryEntry;
use crate::wire::WireEncoder;
use crate::InterfaceSectionTag;

const CONTENT_HASH_DOMAIN: &[u8] = b"bray.package-interface.content.v1";
const SECTION_CONTENT_HASH_DOMAIN: &[u8] = b"bray.package-interface.section-content.v1";
const SECTION_HASH_DOMAIN: &[u8] = b"bray.package-interface.section.v1";

macro_rules! interface_hash {
    ($visibility:vis $name:ident, $documentation:literal) => {
        #[doc = $documentation]
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        $visibility struct $name([u8; Self::LENGTH]);

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

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                for byte in self.as_bytes() {
                    write!(formatter, "{byte:02x}")?;
                }

                Ok(())
            }
        }
    };
}

interface_hash!(
    pub InterfaceArtifactHash,
    "BLAKE3 digest of one exact canonical package-interface artifact."
);
interface_hash!(
    pub InterfaceContentHash,
    "BLAKE3 digest of the semantic package-interface sections."
);
interface_hash!(
    pub InterfaceSectionHash,
    "Domain-separated BLAKE3 digest of one package-interface section."
);
interface_hash!(
    pub(crate) InterfaceSectionContentHash,
    "Domain-separated BLAKE3 digest of one decoded package-interface section payload."
);

pub(crate) fn compute_section_hash(entry: &DirectoryEntry, payload: &[u8]) -> InterfaceSectionHash {
    let mut prefix = WireEncoder::new();

    prefix.write_u32(entry.raw_tag());
    prefix.write_u16(entry.section_revision().raw());
    prefix.write_u8(entry.raw_compatibility());
    prefix.write_u8(entry.raw_encoding());
    prefix.write_u64(entry.record_count());
    prefix.write_u64(entry.encoded_length());
    prefix.write_u64(entry.decoded_length());
    prefix.write_bytes(entry.content_hash().as_bytes());

    let mut hasher = Hasher::new();

    hasher.update(SECTION_HASH_DOMAIN);
    hasher.update(prefix.bytes());
    hasher.update(payload);

    InterfaceSectionHash::from_bytes(*hasher.finalize().as_bytes())
}

pub(crate) fn compute_section_content_hash(
    tag: InterfaceSectionTag,
    payload: &[u8],
) -> InterfaceSectionContentHash {
    let mut prefix = WireEncoder::new();

    prefix.write_u32(tag.wire_value());
    prefix.write_u64(u64::try_from(payload.len()).unwrap_or(u64::MAX));

    let mut hasher = Hasher::new();

    hasher.update(SECTION_CONTENT_HASH_DOMAIN);
    hasher.update(prefix.bytes());
    hasher.update(payload);

    InterfaceSectionContentHash::from_bytes(*hasher.finalize().as_bytes())
}

pub(crate) fn compute_content_hash(
    header: &InterfaceHeader,
    sections: impl IntoIterator<
        Item = (InterfaceSectionTag, u64, InterfaceSectionContentHash),
    >,
) -> InterfaceContentHash {
    let mut prefix = WireEncoder::new();

    prefix.write_u16(header.format_revision().raw());
    prefix.write_u16(header.language_revision().raw());
    prefix.write_u64(header.required_flags().bits());

    let mut hasher = Hasher::new();

    hasher.update(CONTENT_HASH_DOMAIN);
    hasher.update(prefix.bytes());

    for (tag, decoded_length, content_hash) in sections {
        if !tag.contributes_to_content_hash() {
            continue;
        }

        let mut section_prefix = WireEncoder::new();

        section_prefix.write_u32(tag.wire_value());
        section_prefix.write_u64(decoded_length);
        section_prefix.write_bytes(content_hash.as_bytes());

        hasher.update(section_prefix.bytes());
    }

    InterfaceContentHash::from_bytes(*hasher.finalize().as_bytes())
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
    use super::{
        InterfaceArtifactHash, InterfaceContentHash, InterfaceSectionContentHash,
        InterfaceSectionHash,
    };

    #[test]
    fn interface_hashes_preserve_exact_digest_bytes() {
        let bytes = [0x5a; 32];

        assert_eq!(InterfaceArtifactHash::from_bytes(bytes).as_bytes(), &bytes);
        assert_eq!(InterfaceContentHash::from_bytes(bytes).as_bytes(), &bytes);

        assert_eq!(
            InterfaceSectionContentHash::from_bytes(bytes).as_bytes(),
            &bytes
        );

        assert_eq!(InterfaceSectionHash::from_bytes(bytes).as_bytes(), &bytes);

        assert_eq!(
            InterfaceContentHash::from_bytes(bytes).to_string(),
            "5a".repeat(32)
        );
    }
}
