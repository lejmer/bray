use crate::InterfaceSectionEncoding;
use crate::hash::{InterfaceSectionContentHash, InterfaceSectionHash};
use crate::wire::{WireDecodeError, WireReader};

/// Exact revision of one package-interface section's decoded representation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceSectionRevision(u16);

impl InterfaceSectionRevision {
    /// Revision implemented by the current package-interface format.
    pub const CURRENT: Self = Self(1);

    /// Creates a section revision from its stable wire value.
    pub const fn new(raw: u16) -> Self {
        Self(raw)
    }

    /// Returns the stable wire value.
    pub const fn raw(self) -> u16 {
        self.0
    }
}

/// Compatibility behavior for one package-interface section.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum InterfaceSectionCompatibility {
    /// The section must be recognized and decoded successfully.
    Required = 0,
    /// An unknown non-semantic section may be omitted when rewriting the artifact.
    Discardable = 1,
    /// An unknown non-semantic section must be retained byte-for-byte when rewriting the artifact.
    PreserveOpaque = 2,
}

impl InterfaceSectionCompatibility {
    pub(crate) const fn from_wire_value(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Required),
            1 => Some(Self::Discardable),
            2 => Some(Self::PreserveOpaque),
            _ => None,
        }
    }

    /// Returns the stable wire value.
    pub const fn wire_value(self) -> u8 {
        self as u8
    }

    pub(crate) const fn is_optional(self) -> bool {
        matches!(self, Self::Discardable | Self::PreserveOpaque)
    }
}

/// Stable tag for one package-interface section category.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InterfaceSectionTag {
    /// Canonically ordered UTF-8 string table.
    Strings,
    /// Package and product metadata.
    PackageMetadata,
    /// Direct dependency references.
    Dependencies,
    /// Imported symbol identity skeleton.
    SymbolIdentities,
    /// Containment and typed relationships.
    Relationships,
    /// Exported lookup and re-export edges.
    ExportedLookup,
    /// Lazy symbol-record payload directory.
    SemanticRecordDirectory,
    /// Canonical semantic types.
    SemanticTypes,
    /// Constant values and checked const templates.
    Constants,
    /// Constraints, contracts, effects, capabilities, and dependency contracts.
    Contracts,
    /// Declaration-owned checked templates.
    DeclarationTemplates,
    /// Implementation and coherence records.
    Implementations,
    /// Target-property and ABI dependencies.
    TargetDependencies,
    /// Optional source provenance excluded from semantic identity.
    SourceProvenance,
    /// Private support graph and implementation references.
    SupportGraph,
    /// Source-independent declaration signature and default-template semantics.
    DeclarationSemantics,
}

impl InterfaceSectionTag {
    /// Every section category implemented by the current exact format revision.
    pub const ALL: [Self; 16] = [
        Self::Strings,
        Self::PackageMetadata,
        Self::Dependencies,
        Self::SymbolIdentities,
        Self::Relationships,
        Self::ExportedLookup,
        Self::SemanticRecordDirectory,
        Self::SemanticTypes,
        Self::Constants,
        Self::Contracts,
        Self::DeclarationTemplates,
        Self::Implementations,
        Self::TargetDependencies,
        Self::SourceProvenance,
        Self::SupportGraph,
        Self::DeclarationSemantics,
    ];

    pub(crate) const fn from_wire_value(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::Strings),
            2 => Some(Self::PackageMetadata),
            3 => Some(Self::Dependencies),
            4 => Some(Self::SymbolIdentities),
            5 => Some(Self::Relationships),
            6 => Some(Self::ExportedLookup),
            7 => Some(Self::SemanticRecordDirectory),
            8 => Some(Self::SemanticTypes),
            9 => Some(Self::Constants),
            10 => Some(Self::Contracts),
            11 => Some(Self::DeclarationTemplates),
            12 => Some(Self::Implementations),
            13 => Some(Self::TargetDependencies),
            14 => Some(Self::SourceProvenance),
            15 => Some(Self::SupportGraph),
            16 => Some(Self::DeclarationSemantics),
            _ => None,
        }
    }

    /// Returns the stable wire tag.
    pub const fn wire_value(self) -> u32 {
        match self {
            Self::Strings => 1,
            Self::PackageMetadata => 2,
            Self::Dependencies => 3,
            Self::SymbolIdentities => 4,
            Self::Relationships => 5,
            Self::ExportedLookup => 6,
            Self::SemanticRecordDirectory => 7,
            Self::SemanticTypes => 8,
            Self::Constants => 9,
            Self::Contracts => 10,
            Self::DeclarationTemplates => 11,
            Self::Implementations => 12,
            Self::TargetDependencies => 13,
            Self::SourceProvenance => 14,
            Self::SupportGraph => 15,
            Self::DeclarationSemantics => 16,
        }
    }

    /// Returns the stable machine-readable section name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Strings => "strings",
            Self::PackageMetadata => "package_metadata",
            Self::Dependencies => "dependencies",
            Self::SymbolIdentities => "symbol_identities",
            Self::Relationships => "relationships",
            Self::ExportedLookup => "exported_lookup",
            Self::SemanticRecordDirectory => "semantic_record_directory",
            Self::SemanticTypes => "semantic_types",
            Self::Constants => "constants",
            Self::Contracts => "contracts",
            Self::DeclarationTemplates => "declaration_templates",
            Self::Implementations => "implementations",
            Self::TargetDependencies => "target_dependencies",
            Self::SourceProvenance => "source_provenance",
            Self::SupportGraph => "support_graph",
            Self::DeclarationSemantics => "declaration_semantics",
        }
    }

    /// Resolves one exact machine-readable section name.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|section| section.as_str() == name)
    }

    pub(crate) const fn is_optional(self) -> bool {
        matches!(self, Self::SourceProvenance)
    }

    pub(crate) const fn contributes_to_content_hash(self) -> bool {
        !matches!(self, Self::SourceProvenance)
    }

    pub(crate) const fn compatibility(self) -> InterfaceSectionCompatibility {
        match self {
            Self::SourceProvenance => InterfaceSectionCompatibility::Discardable,
            _ => InterfaceSectionCompatibility::Required,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct DirectoryEntry {
    raw_tag: u32,
    section_revision: InterfaceSectionRevision,
    raw_compatibility: u8,
    raw_encoding: u8,
    offset: u64,
    encoded_length: u64,
    decoded_length: u64,
    record_count: u64,
    checksum: InterfaceSectionHash,
    content_hash: InterfaceSectionContentHash,
}

impl DirectoryEntry {
    pub(crate) const LENGTH: usize = 104;
    pub(crate) const WIRE_LENGTH: u64 = 104;
    #[cfg(test)]
    pub(crate) const CHECKSUM_OFFSET: usize = 40;

    pub(crate) fn decode(bytes: &[u8]) -> Result<DecodedDirectoryEntry, WireDecodeError> {
        let mut reader = WireReader::new(bytes);

        let raw_tag = reader.read_u32()?;
        let section_revision = InterfaceSectionRevision::new(reader.read_u16()?);
        let raw_compatibility = reader.read_u8()?;
        let raw_encoding = reader.read_u8()?;
        let offset = reader.read_u64()?;
        let encoded_length = reader.read_u64()?;
        let decoded_length = reader.read_u64()?;
        let record_count = reader.read_u64()?;
        let checksum = InterfaceSectionHash::from_bytes(reader.read_array::<32>()?);
        let content_hash = InterfaceSectionContentHash::from_bytes(reader.read_array::<32>()?);

        Ok(DecodedDirectoryEntry {
            raw_tag,
            section_revision,
            raw_compatibility,
            raw_encoding,
            offset,
            encoded_length,
            decoded_length,
            record_count,
            checksum,
            content_hash,
        })
    }

    pub(crate) const fn from_decoded(decoded: DecodedDirectoryEntry) -> Self {
        Self {
            raw_tag: decoded.raw_tag,
            section_revision: decoded.section_revision,
            raw_compatibility: decoded.raw_compatibility,
            raw_encoding: decoded.raw_encoding,
            offset: decoded.offset,
            encoded_length: decoded.encoded_length,
            decoded_length: decoded.decoded_length,
            record_count: decoded.record_count,
            checksum: decoded.checksum,
            content_hash: decoded.content_hash,
        }
    }

    pub(crate) const fn for_encoded(
        tag: InterfaceSectionTag,
        encoding: InterfaceSectionEncoding,
        offset: u64,
        encoded_length: u64,
        decoded_length: u64,
        record_count: u64,
        checksum: InterfaceSectionHash,
        content_hash: InterfaceSectionContentHash,
    ) -> Self {
        Self {
            raw_tag: tag.wire_value(),
            section_revision: InterfaceSectionRevision::CURRENT,
            raw_compatibility: tag.compatibility().wire_value(),
            raw_encoding: encoding.wire_value(),
            offset,
            encoded_length,
            decoded_length,
            record_count,
            checksum,
            content_hash,
        }
    }

    pub(crate) const fn raw_tag(self) -> u32 {
        self.raw_tag
    }

    pub(crate) const fn tag(self) -> Option<InterfaceSectionTag> {
        let Some(tag) = InterfaceSectionTag::from_wire_value(self.raw_tag) else {
            return None;
        };

        if self.section_revision.raw() != InterfaceSectionRevision::CURRENT.raw()
            || self.raw_compatibility != tag.compatibility().wire_value()
            || InterfaceSectionEncoding::from_wire_value(self.raw_encoding).is_none()
        {
            return None;
        }

        Some(tag)
    }

    pub(crate) const fn section_revision(self) -> InterfaceSectionRevision {
        self.section_revision
    }

    pub(crate) const fn raw_compatibility(self) -> u8 {
        self.raw_compatibility
    }

    pub(crate) const fn encoding(self) -> Option<InterfaceSectionEncoding> {
        InterfaceSectionEncoding::from_wire_value(self.raw_encoding)
    }

    pub(crate) const fn raw_encoding(self) -> u8 {
        self.raw_encoding
    }

    pub(crate) const fn encoded_length(self) -> u64 {
        self.encoded_length
    }

    pub(crate) const fn decoded_length(self) -> u64 {
        self.decoded_length
    }

    pub(crate) const fn offset(self) -> u64 {
        self.offset
    }

    pub(crate) const fn record_count(self) -> u64 {
        self.record_count
    }

    pub(crate) const fn checksum(self) -> InterfaceSectionHash {
        self.checksum
    }

    pub(crate) const fn content_hash(self) -> InterfaceSectionContentHash {
        self.content_hash
    }

    pub(crate) fn payload(self, bytes: &[u8]) -> Option<&[u8]> {
        let start = usize::try_from(self.offset).ok()?;
        let length = usize::try_from(self.encoded_length).ok()?;
        let end = start.checked_add(length)?;

        bytes.get(start..end)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct DecodedDirectoryEntry {
    pub(crate) raw_tag: u32,
    pub(crate) section_revision: InterfaceSectionRevision,
    pub(crate) raw_compatibility: u8,
    pub(crate) raw_encoding: u8,
    pub(crate) offset: u64,
    pub(crate) encoded_length: u64,
    pub(crate) decoded_length: u64,
    pub(crate) record_count: u64,
    pub(crate) checksum: InterfaceSectionHash,
    pub(crate) content_hash: InterfaceSectionContentHash,
}

/// Read-only view of one structurally validated package-interface section.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ValidatedInterfaceSection<'bytes> {
    tag: InterfaceSectionTag,
    revision: InterfaceSectionRevision,
    encoding: InterfaceSectionEncoding,
    record_count: u64,
    checksum: InterfaceSectionHash,
    bytes: &'bytes [u8],
}

impl<'bytes> ValidatedInterfaceSection<'bytes> {
    pub(crate) const fn new(entry: DirectoryEntry, bytes: &'bytes [u8]) -> Option<Self> {
        let Some(tag) = entry.tag() else {
            return None;
        };

        let Some(encoding) = entry.encoding() else {
            return None;
        };

        Some(Self {
            tag,
            revision: entry.section_revision(),
            encoding,
            record_count: entry.record_count(),
            checksum: entry.checksum(),
            bytes,
        })
    }

    #[cfg(test)]
    pub(crate) const fn for_test(
        tag: InterfaceSectionTag,
        record_count: u64,
        bytes: &'bytes [u8],
    ) -> Self {
        Self {
            tag,
            revision: InterfaceSectionRevision::CURRENT,
            encoding: InterfaceSectionEncoding::Raw,
            record_count,
            checksum: InterfaceSectionHash::from_bytes([0; 32]),
            bytes,
        }
    }

    /// Returns the section category.
    pub const fn tag(self) -> InterfaceSectionTag {
        self.tag
    }

    /// Returns the exact decoded representation revision.
    pub const fn revision(self) -> InterfaceSectionRevision {
        self.revision
    }

    /// Returns the exact stored payload encoding.
    pub const fn encoding(self) -> InterfaceSectionEncoding {
        self.encoding
    }

    /// Returns the validated record count declared by the section.
    pub const fn record_count(self) -> u64 {
        self.record_count
    }

    /// Returns the validated section checksum.
    pub const fn checksum(self) -> InterfaceSectionHash {
        self.checksum
    }

    /// Returns the immutable length-delimited section payload.
    pub const fn bytes(self) -> &'bytes [u8] {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::InterfaceSectionTag;

    #[test]
    fn section_tags_round_trip_exact_wire_values() {
        for value in 1..=16 {
            let Some(tag) = InterfaceSectionTag::from_wire_value(value) else {
                panic!("known section tag was rejected: {value}");
            };

            assert_eq!(tag.wire_value(), value);
        }

        assert_eq!(InterfaceSectionTag::from_wire_value(0), None);
        assert_eq!(InterfaceSectionTag::from_wire_value(17), None);
    }

    #[test]
    fn section_names_round_trip_in_canonical_order() {
        for section in InterfaceSectionTag::ALL {
            assert_eq!(
                InterfaceSectionTag::from_name(section.as_str()),
                Some(section)
            );
        }

        assert_eq!(InterfaceSectionTag::from_name("unknown"), None);
    }

    #[test]
    fn provenance_is_excluded_from_semantic_content() {
        assert!(!InterfaceSectionTag::SourceProvenance.contributes_to_content_hash());
        assert!(InterfaceSectionTag::SymbolIdentities.contributes_to_content_hash());
    }
}
