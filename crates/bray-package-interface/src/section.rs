use crate::hash::InterfaceSectionHash;
use crate::wire::{WireDecodeError, WireReader};

pub(crate) const OPTIONAL_NON_SEMANTIC_SECTION_FLAG: u32 = 1;

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
    /// Lazy symbol-fact payload directory.
    SymbolFactDirectory,
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
    /// Target-fact and ABI dependencies.
    TargetDependencies,
    /// Optional source provenance excluded from semantic identity.
    SourceProvenance,
    /// Private support graph and implementation references.
    SupportGraph,
}

impl InterfaceSectionTag {
    /// Every section category implemented by the current exact format revision.
    pub const ALL: [Self; 15] = [
        Self::Strings,
        Self::PackageMetadata,
        Self::Dependencies,
        Self::SymbolIdentities,
        Self::Relationships,
        Self::ExportedLookup,
        Self::SymbolFactDirectory,
        Self::SemanticTypes,
        Self::Constants,
        Self::Contracts,
        Self::DeclarationTemplates,
        Self::Implementations,
        Self::TargetDependencies,
        Self::SourceProvenance,
        Self::SupportGraph,
    ];

    pub(crate) const fn from_wire_value(value: u32) -> Option<Self> {
        match value {
            1 => Some(Self::Strings),
            2 => Some(Self::PackageMetadata),
            3 => Some(Self::Dependencies),
            4 => Some(Self::SymbolIdentities),
            5 => Some(Self::Relationships),
            6 => Some(Self::ExportedLookup),
            7 => Some(Self::SymbolFactDirectory),
            8 => Some(Self::SemanticTypes),
            9 => Some(Self::Constants),
            10 => Some(Self::Contracts),
            11 => Some(Self::DeclarationTemplates),
            12 => Some(Self::Implementations),
            13 => Some(Self::TargetDependencies),
            14 => Some(Self::SourceProvenance),
            15 => Some(Self::SupportGraph),
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
            Self::SymbolFactDirectory => 7,
            Self::SemanticTypes => 8,
            Self::Constants => 9,
            Self::Contracts => 10,
            Self::DeclarationTemplates => 11,
            Self::Implementations => 12,
            Self::TargetDependencies => 13,
            Self::SourceProvenance => 14,
            Self::SupportGraph => 15,
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
            Self::SymbolFactDirectory => "symbol_fact_directory",
            Self::SemanticTypes => "semantic_types",
            Self::Constants => "constants",
            Self::Contracts => "contracts",
            Self::DeclarationTemplates => "declaration_templates",
            Self::Implementations => "implementations",
            Self::TargetDependencies => "target_dependencies",
            Self::SourceProvenance => "source_provenance",
            Self::SupportGraph => "support_graph",
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
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct DirectoryEntry {
    raw_tag: u32,
    encoding_flags: u32,
    offset: u64,
    length: u64,
    record_count: u64,
    checksum: InterfaceSectionHash,
}

impl DirectoryEntry {
    pub(crate) const LENGTH: usize = 64;
    pub(crate) const WIRE_LENGTH: u64 = 64;

    pub(crate) fn decode(bytes: &[u8]) -> Result<DecodedDirectoryEntry, WireDecodeError> {
        let mut reader = WireReader::new(bytes);

        let raw_tag = reader.read_u32()?;
        let encoding_flags = reader.read_u32()?;
        let offset = reader.read_u64()?;
        let length = reader.read_u64()?;
        let record_count = reader.read_u64()?;
        let checksum = InterfaceSectionHash::from_bytes(reader.read_array::<32>()?);

        Ok(DecodedDirectoryEntry {
            raw_tag,
            encoding_flags,
            offset,
            length,
            record_count,
            checksum,
        })
    }

    pub(crate) const fn from_decoded(decoded: DecodedDirectoryEntry) -> Self {
        Self {
            raw_tag: decoded.raw_tag,
            encoding_flags: decoded.encoding_flags,
            offset: decoded.offset,
            length: decoded.length,
            record_count: decoded.record_count,
            checksum: decoded.checksum,
        }
    }

    pub(crate) const fn for_encoded(
        tag: InterfaceSectionTag,
        offset: u64,
        length: u64,
        record_count: u64,
        checksum: InterfaceSectionHash,
    ) -> Self {
        Self {
            raw_tag: tag.wire_value(),
            encoding_flags: 0,
            offset,
            length,
            record_count,
            checksum,
        }
    }

    pub(crate) const fn raw_tag(self) -> u32 {
        self.raw_tag
    }

    pub(crate) const fn tag(self) -> Option<InterfaceSectionTag> {
        InterfaceSectionTag::from_wire_value(self.raw_tag)
    }

    pub(crate) const fn encoding_flags(self) -> u32 {
        self.encoding_flags
    }

    pub(crate) const fn contributes_to_content_hash(self) -> bool {
        match self.tag() {
            Some(tag) => tag.contributes_to_content_hash(),
            None => false,
        }
    }

    pub(crate) const fn length(self) -> u64 {
        self.length
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

    pub(crate) fn payload(self, bytes: &[u8]) -> Option<&[u8]> {
        let start = usize::try_from(self.offset).ok()?;
        let length = usize::try_from(self.length).ok()?;
        let end = start.checked_add(length)?;

        bytes.get(start..end)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct DecodedDirectoryEntry {
    pub(crate) raw_tag: u32,
    pub(crate) encoding_flags: u32,
    pub(crate) offset: u64,
    pub(crate) length: u64,
    pub(crate) record_count: u64,
    pub(crate) checksum: InterfaceSectionHash,
}

/// Read-only view of one structurally validated package-interface section.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ValidatedInterfaceSection<'bytes> {
    tag: InterfaceSectionTag,
    record_count: u64,
    checksum: InterfaceSectionHash,
    bytes: &'bytes [u8],
}

impl<'bytes> ValidatedInterfaceSection<'bytes> {
    pub(crate) fn new(entry: DirectoryEntry, artifact: &'bytes [u8]) -> Option<Self> {
        Some(Self {
            tag: entry.tag()?,
            record_count: entry.record_count(),
            checksum: entry.checksum(),
            bytes: entry.payload(artifact)?,
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
            record_count,
            checksum: InterfaceSectionHash::from_bytes([0; 32]),
            bytes,
        }
    }

    /// Returns the section category.
    pub const fn tag(self) -> InterfaceSectionTag {
        self.tag
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
        for value in 1..=15 {
            let Some(tag) = InterfaceSectionTag::from_wire_value(value) else {
                panic!("known section tag was rejected: {value}");
            };

            assert_eq!(tag.wire_value(), value);
        }

        assert_eq!(InterfaceSectionTag::from_wire_value(0), None);
        assert_eq!(InterfaceSectionTag::from_wire_value(16), None);
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
