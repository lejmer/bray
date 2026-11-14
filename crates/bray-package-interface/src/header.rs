use crate::hash::{InterfaceArtifactHash, InterfaceContentHash};
use crate::wire::{WireDecodeError, WireReader};

pub(crate) const BYTE_ORDER_MARKER: u32 = 0x0102_0304;
pub(crate) const MAGIC: [u8; 8] = *b"BRAYI\0\r\n";

/// Exact package-interface format revision implemented by this crate.
pub const CURRENT_FORMAT_REVISION: InterfaceFormatRevision = InterfaceFormatRevision::new(1);

/// Exact revision of the package-interface wire format.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceFormatRevision(u16);

impl InterfaceFormatRevision {
    /// Creates a format revision from its stable wire value.
    pub const fn new(raw: u16) -> Self {
        Self(raw)
    }

    /// Returns the stable wire value.
    pub const fn raw(self) -> u16 {
        self.0
    }
}

/// Language semantic revision required by a package interface.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceLanguageRevision(u16);

impl InterfaceLanguageRevision {
    /// Creates a language semantic revision from its stable wire value.
    pub const fn new(raw: u16) -> Self {
        Self(raw)
    }

    /// Returns the stable wire value.
    pub const fn raw(self) -> u16 {
        self.0
    }
}

/// Required compatibility flags declared by the current wire revision.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceRequiredFlags(u64);

impl InterfaceRequiredFlags {
    /// No optional compatibility behavior is required.
    pub const NONE: Self = Self(0);

    pub(crate) const fn from_bits(bits: u64) -> Self {
        Self(bits)
    }

    /// Returns the stable wire bits.
    pub const fn bits(self) -> u64 {
        self.0
    }
}

/// Validated fixed package-interface header fields.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct InterfaceHeader {
    format_revision: InterfaceFormatRevision,
    language_revision: InterfaceLanguageRevision,
    required_flags: InterfaceRequiredFlags,
    declared_file_length: u64,
    directory_offset: u64,
    directory_length: u64,
    content_hash: InterfaceContentHash,
    artifact_hash: InterfaceArtifactHash,
}

impl InterfaceHeader {
    pub(crate) const LENGTH: usize = 112;
    pub(crate) const CONTENT_HASH_OFFSET: usize = 48;
    pub(crate) const ARTIFACT_HASH_OFFSET: usize = 80;

    pub(crate) fn decode(bytes: &[u8]) -> Result<DecodedHeader, WireDecodeError> {
        let mut reader = WireReader::new(bytes);

        let magic = reader.read_array::<8>()?;
        let format_revision = InterfaceFormatRevision::new(reader.read_u16()?);
        let language_revision = InterfaceLanguageRevision::new(reader.read_u16()?);
        let byte_order_marker = reader.read_u32()?;
        let required_flags = InterfaceRequiredFlags::from_bits(reader.read_u64()?);
        let declared_file_length = reader.read_u64()?;
        let directory_offset = reader.read_u64()?;
        let directory_length = reader.read_u64()?;
        let content_hash = InterfaceContentHash::from_bytes(reader.read_array::<32>()?);
        let artifact_hash = InterfaceArtifactHash::from_bytes(reader.read_array::<32>()?);

        Ok(DecodedHeader {
            magic,
            byte_order_marker,
            header: Self {
                format_revision,
                language_revision,
                required_flags,
                declared_file_length,
                directory_offset,
                directory_length,
                content_hash,
                artifact_hash,
            },
        })
    }

    /// Returns the exact wire-format revision.
    pub const fn format_revision(self) -> InterfaceFormatRevision {
        self.format_revision
    }

    /// Returns the required language semantic revision.
    pub const fn language_revision(self) -> InterfaceLanguageRevision {
        self.language_revision
    }

    /// Returns the required compatibility flags.
    pub const fn required_flags(self) -> InterfaceRequiredFlags {
        self.required_flags
    }

    /// Returns the semantic content hash declared by the artifact.
    pub const fn content_hash(self) -> InterfaceContentHash {
        self.content_hash
    }

    /// Returns the exact artifact hash declared by the artifact.
    pub const fn artifact_hash(self) -> InterfaceArtifactHash {
        self.artifact_hash
    }

    pub(crate) const fn declared_file_length(self) -> u64 {
        self.declared_file_length
    }

    pub(crate) const fn directory_offset(self) -> u64 {
        self.directory_offset
    }

    pub(crate) const fn directory_length(self) -> u64 {
        self.directory_length
    }
}

pub(crate) struct DecodedHeader {
    pub(crate) magic: [u8; 8],
    pub(crate) byte_order_marker: u32,
    pub(crate) header: InterfaceHeader,
}
