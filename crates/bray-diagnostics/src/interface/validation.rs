use super::context::DiagnosticInterfaceValidationContext;
use super::identity::{DiagnosticInterfaceDependency, DiagnosticPackageInterfaceIdentity};
use super::inventory::{DiagnosticInterfaceLimit, DiagnosticInterfaceSection};
use super::problem::DiagnosticInterfaceSymbolGraphProblem;
use crate::DiagnosticArtifactDigest;

/// Package-interface field associated with a structural validation failure.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticInterfaceValidationField {
    /// File magic.
    Magic,
    /// Wire-format revision.
    FormatRevision,
    /// Language semantic revision.
    LanguageRevision,
    /// Byte-order marker.
    ByteOrderMarker,
    /// Required compatibility flags.
    RequiredFlags,
    /// Declared complete file length.
    DeclaredFileLength,
    /// Section-directory offset.
    DirectoryOffset,
    /// Section-directory length.
    DirectoryLength,
    /// Semantic content hash.
    ContentHash,
    /// Exact artifact hash.
    ArtifactHash,
    /// Section tag.
    SectionTag,
    /// Section revision.
    SectionRevision,
    /// Section compatibility policy.
    SectionCompatibility,
    /// Section payload encoding.
    SectionEncoding,
    /// Section payload offset.
    SectionOffset,
    /// Encoded byte length.
    EncodedLength,
    /// Decoded byte length.
    DecodedLength,
    /// Declared record count.
    RecordCount,
    /// Record byte offset.
    RecordOffset,
    /// Record byte length.
    RecordLength,
    /// Record payload.
    RecordPayload,
    /// Closed wire discriminant.
    Discriminant,
    /// UTF-8 string value.
    String,
    /// String-table index.
    StringIndex,
    /// Package name.
    PackageName,
    /// Package version.
    PackageVersion,
    /// Product name.
    ProductName,
    /// Symbol name.
    SymbolName,
    /// Symbol category.
    SymbolKind,
    /// Containing declaration or container.
    Container,
    /// Owning record or declaration.
    Owner,
    /// Subject record or declaration.
    Subject,
    /// Semantic role.
    Role,
    /// Owner-relative ordinal.
    Ordinal,
    /// Dependency identity or slot.
    Dependency,
    /// Cross-record reference.
    Reference,
    /// Generic table index.
    Index,
    /// Declaration identity.
    Declaration,
    /// Callable parameter.
    Parameter,
    /// Generic or callable argument.
    Argument,
    /// Semantic type.
    Type,
    /// Constant value or term.
    Constant,
    /// Checked template.
    Template,
    /// Support-graph entry.
    Support,
    /// Nested schema or payload revision.
    SchemaRevision,
    /// Stable semantic identity.
    Identity,
    /// Stored digest.
    Hash,
    /// Runtime-requirements contract.
    RuntimeRequirements,
    /// Implementation configuration.
    Configuration,
    /// Target property.
    TargetProperty,
    /// Implementation entry offset.
    EntryOffset,
    /// Implementation entry length.
    EntryLength,
    /// Implementation entry encoding.
    EntryEncoding,
    /// Implementation entry category.
    EntryKind,
    /// Stable ordering contract.
    Ordering,
    /// Generic structural value.
    Value,
}

impl DiagnosticInterfaceValidationField {
    /// Returns the stable machine key for this field category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Magic => "magic",
            Self::FormatRevision => "format_revision",
            Self::LanguageRevision => "language_revision",
            Self::ByteOrderMarker => "byte_order_marker",
            Self::RequiredFlags => "required_flags",
            Self::DeclaredFileLength => "declared_file_length",
            Self::DirectoryOffset => "directory_offset",
            Self::DirectoryLength => "directory_length",
            Self::ContentHash => "content_hash",
            Self::ArtifactHash => "artifact_hash",
            Self::SectionTag => "section_tag",
            Self::SectionRevision => "section_revision",
            Self::SectionCompatibility => "section_compatibility",
            Self::SectionEncoding => "section_encoding",
            Self::SectionOffset => "section_offset",
            Self::EncodedLength => "encoded_length",
            Self::DecodedLength => "decoded_length",
            Self::RecordCount => "record_count",
            Self::RecordOffset => "record_offset",
            Self::RecordLength => "record_length",
            Self::RecordPayload => "record_payload",
            Self::Discriminant => "discriminant",
            Self::String => "string",
            Self::StringIndex => "string_index",
            Self::PackageName => "package_name",
            Self::PackageVersion => "package_version",
            Self::ProductName => "product_name",
            Self::SymbolName => "symbol_name",
            Self::SymbolKind => "symbol_kind",
            Self::Container => "container",
            Self::Owner => "owner",
            Self::Subject => "subject",
            Self::Role => "role",
            Self::Ordinal => "ordinal",
            Self::Dependency => "dependency",
            Self::Reference => "reference",
            Self::Index => "index",
            Self::Declaration => "declaration",
            Self::Parameter => "parameter",
            Self::Argument => "argument",
            Self::Type => "type",
            Self::Constant => "constant",
            Self::Template => "template",
            Self::Support => "support",
            Self::SchemaRevision => "schema_revision",
            Self::Identity => "identity",
            Self::Hash => "hash",
            Self::RuntimeRequirements => "runtime_requirements",
            Self::Configuration => "configuration",
            Self::TargetProperty => "target_property",
            Self::EntryOffset => "entry_offset",
            Self::EntryLength => "entry_length",
            Self::EntryEncoding => "entry_encoding",
            Self::EntryKind => "entry_kind",
            Self::Ordering => "ordering",
            Self::Value => "value",
        }
    }
}

/// Integer representation that rejected an interface value.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticInterfaceIntegerTarget {
    /// Unsigned 16-bit value.
    U16,
    /// Unsigned 32-bit value.
    U32,
    /// Unsigned 64-bit value.
    U64,
    /// Host-sized allocation or slice value.
    Usize,
}

impl DiagnosticInterfaceIntegerTarget {
    /// Returns the stable machine key for this integer representation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::Usize => "usize",
        }
    }
}

/// Exact invalid UTF-8 condition retained by interface validation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticInterfaceUtf8Failure {
    /// An invalid byte sequence was encountered.
    InvalidSequence {
        /// Length of the invalid byte sequence when known.
        error_length: Option<u64>,
    },
    /// Input ended within an otherwise valid UTF-8 sequence.
    IncompleteSequence,
}

impl DiagnosticInterfaceUtf8Failure {
    /// Returns the stable machine key for this UTF-8 failure.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidSequence { .. } => "invalid_sequence",
            Self::IncompleteSequence => "incomplete_sequence",
        }
    }
}

/// Exact malformed structural contract retained by interface validation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticInterfaceMalformedCause {
    /// A required value was absent.
    Missing {
        /// Missing field.
        field: DiagnosticInterfaceValidationField,
    },
    /// A value failed its domain or shape contract.
    InvalidValue {
        /// Invalid field.
        field: DiagnosticInterfaceValidationField,
    },
    /// A closed wire discriminant was unknown.
    InvalidDiscriminant {
        /// Field containing the discriminant.
        field: DiagnosticInterfaceValidationField,
        /// Unknown discriminant value.
        actual: u64,
    },
    /// A declared count disagreed with retained values.
    CountMismatch {
        /// Counted field.
        field: DiagnosticInterfaceValidationField,
        /// Required count.
        expected: u64,
        /// Retained count.
        actual: u64,
    },
    /// A reference addressed a missing table slot.
    InvalidReference {
        /// Reference field.
        field: DiagnosticInterfaceValidationField,
        /// Requested index.
        index: u64,
        /// Number of available entries.
        available: u64,
    },
    /// Values violated strict ordering.
    OrderingViolation {
        /// Ordered field.
        field: DiagnosticInterfaceValidationField,
        /// Previous value.
        previous: u64,
        /// Later out-of-order value.
        actual: u64,
    },
    /// A unique value or slot appeared more than once.
    Duplicate {
        /// Duplicated field.
        field: DiagnosticInterfaceValidationField,
        /// Duplicate index or value.
        index: u64,
    },
    /// A supposedly acyclic record graph contains a cycle.
    Cycle {
        /// Cyclic field.
        field: DiagnosticInterfaceValidationField,
        /// Record at which the cycle was observed.
        index: u64,
    },
    /// A numeric value cannot fit its required representation.
    NumericOverflow {
        /// Overflowing field.
        field: DiagnosticInterfaceValidationField,
        /// Rejected value.
        value: u64,
        /// Required integer representation.
        target: DiagnosticInterfaceIntegerTarget,
    },
    /// A byte range overflowed its representation.
    RangeOverflow {
        /// Range offset.
        offset: u64,
        /// Range length.
        length: u64,
    },
    /// A structural value disagreed with its required value.
    ValueMismatch {
        /// Mismatched field.
        field: DiagnosticInterfaceValidationField,
        /// Required value.
        expected: u64,
        /// Retained value.
        actual: u64,
    },
    /// A value violates its required byte alignment.
    InvalidAlignment {
        /// Misaligned field.
        field: DiagnosticInterfaceValidationField,
        /// Misaligned value.
        value: u64,
        /// Required alignment.
        alignment: u64,
    },
    /// Two independently declared byte ranges overlap.
    RangeOverlap {
        /// First range offset.
        offset: u64,
        /// First range length.
        length: u64,
        /// Conflicting range offset.
        conflicting_offset: u64,
        /// Conflicting range length.
        conflicting_length: u64,
    },
    /// A declared length disagreed with the retained value.
    LengthMismatch {
        /// Length-bearing field.
        field: DiagnosticInterfaceValidationField,
        /// Declared length.
        expected: u64,
        /// Retained length.
        actual: u64,
    },
}

impl DiagnosticInterfaceMalformedCause {
    /// Returns the stable machine key for this malformed contract.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Missing { .. } => "missing",
            Self::InvalidValue { .. } => "invalid_value",
            Self::InvalidDiscriminant { .. } => "invalid_discriminant",
            Self::CountMismatch { .. } => "count_mismatch",
            Self::InvalidReference { .. } => "invalid_reference",
            Self::OrderingViolation { .. } => "ordering_violation",
            Self::Duplicate { .. } => "duplicate",
            Self::Cycle { .. } => "cycle",
            Self::NumericOverflow { .. } => "numeric_overflow",
            Self::RangeOverflow { .. } => "range_overflow",
            Self::RangeOverlap { .. } => "range_overlap",
            Self::ValueMismatch { .. } => "value_mismatch",
            Self::InvalidAlignment { .. } => "invalid_alignment",
            Self::LengthMismatch { .. } => "length_mismatch",
        }
    }
}

/// Exact compression or framing failure retained by interface validation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticInterfaceCompressionFailure {
    /// Compression library initialization failed.
    EncoderInitialization,
    /// Deterministic encoder configuration failed.
    EncoderConfiguration,
    /// Section compression failed.
    Encoding,
    /// Frame-length inspection failed.
    FrameLength,
    /// Frame length disagreed with retained bytes.
    FrameLengthMismatch {
        /// Declared length.
        expected: u64,
        /// Retained length.
        actual: u64,
    },
    /// Frame content-size inspection failed.
    ContentSize,
    /// Declared decoded size disagreed with the frame.
    ContentSizeMismatch {
        /// Declared size.
        expected: u64,
        /// Frame size when present.
        actual: Option<u64>,
    },
    /// Required frame-header byte was absent.
    MissingHeaderByte {
        /// Missing byte offset.
        offset: u64,
    },
    /// Frame-header flags violate the encoding contract.
    InvalidHeaderFlags {
        /// Frame descriptor byte.
        descriptor: u8,
    },
    /// Frame window arithmetic overflowed.
    WindowSizeOverflow {
        /// Frame descriptor byte.
        descriptor: u8,
    },
    /// Frame window exceeds the validation ceiling.
    WindowSizeExceeded {
        /// Declared window size.
        actual: u64,
        /// Configured maximum.
        maximum: u64,
    },
    /// Decompression library initialization failed.
    DecoderInitialization,
    /// Bounded decoder configuration failed.
    DecoderConfiguration,
    /// Section decompression failed.
    Decoding,
    /// Decoded bytes disagree with the committed length.
    DecodedLengthMismatch {
        /// Committed length.
        expected: u64,
        /// Decoded length.
        actual: u64,
    },
}

impl DiagnosticInterfaceCompressionFailure {
    /// Returns the stable machine key for this compression failure.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EncoderInitialization => "encoder_initialization",
            Self::EncoderConfiguration => "encoder_configuration",
            Self::Encoding => "encoding",
            Self::FrameLength => "frame_length",
            Self::FrameLengthMismatch { .. } => "frame_length_mismatch",
            Self::ContentSize => "content_size",
            Self::ContentSizeMismatch { .. } => "content_size_mismatch",
            Self::MissingHeaderByte { .. } => "missing_header_byte",
            Self::InvalidHeaderFlags { .. } => "invalid_header_flags",
            Self::WindowSizeOverflow { .. } => "window_size_overflow",
            Self::WindowSizeExceeded { .. } => "window_size_exceeded",
            Self::DecoderInitialization => "decoder_initialization",
            Self::DecoderConfiguration => "decoder_configuration",
            Self::Decoding => "decoding",
            Self::DecodedLengthMismatch { .. } => "decoded_length_mismatch",
        }
    }
}

/// Exact locale-neutral package-interface validation failure.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticInterfaceValidationFailure {
    /// File magic differs from the package-interface identity.
    InvalidMagic {
        /// Bytes found at the file-magic position.
        actual: [u8; 8],
    },
    /// Wire-format revision is unsupported.
    UnsupportedFormatRevision {
        /// Format revision required by this compiler.
        expected: u64,
        /// Format revision declared by the artifact.
        actual: u64,
    },
    /// Language semantic revision is unsupported.
    UnsupportedLanguageRevision {
        /// Language revision required by the validation policy.
        expected: u64,
        /// Language revision declared by the artifact.
        actual: u64,
    },
    /// Byte-order marker is unsupported.
    UnsupportedByteOrder {
        /// Required byte-order marker.
        expected: u32,
        /// Byte-order marker declared by the artifact.
        actual: u32,
    },
    /// Required compatibility flags are unsupported.
    UnsupportedRequiredFlags {
        /// Required compatibility flags declared by the artifact.
        actual: u64,
    },
    /// Input ended before a field was complete.
    Truncated {
        /// Region being decoded.
        context: DiagnosticInterfaceValidationContext,
        /// Field whose bytes are incomplete.
        field: DiagnosticInterfaceValidationField,
        /// Byte offset relative to the region.
        offset: u64,
        /// Number of bytes required by the field.
        expected_length: u64,
        /// Number of bytes available at the offset.
        actual_length: u64,
    },
    /// Bytes remained after an exact payload was consumed.
    TrailingBytes {
        /// Region being decoded.
        context: DiagnosticInterfaceValidationContext,
        /// Offset of the first trailing byte.
        offset: u64,
        /// Number of trailing bytes.
        count: u64,
    },
    /// A structural contract failed.
    Malformed {
        /// Region whose structural contract failed.
        context: DiagnosticInterfaceValidationContext,
        /// Exact violated contract.
        cause: DiagnosticInterfaceMalformedCause,
    },
    /// A string contains invalid UTF-8.
    InvalidUtf8 {
        /// Region containing the string.
        context: DiagnosticInterfaceValidationContext,
        /// String field being decoded.
        field: DiagnosticInterfaceValidationField,
        /// Byte offset of the invalid sequence.
        offset: u64,
        /// Number of bytes in the string range.
        length: u64,
        /// Exact UTF-8 failure.
        cause: DiagnosticInterfaceUtf8Failure,
    },
    /// Compression or framing failed.
    Compression {
        /// Compressed region being processed.
        context: DiagnosticInterfaceValidationContext,
        /// Exact compression or framing failure.
        cause: DiagnosticInterfaceCompressionFailure,
    },
    /// A digest could not be computed from invalid retained bytes.
    DigestUnavailable {
        /// Region whose digest was requested.
        context: DiagnosticInterfaceValidationContext,
        /// Digest field being computed.
        field: DiagnosticInterfaceValidationField,
    },
    /// A bounded allocation required by validation was unavailable.
    AllocationUnavailable {
        /// Region requiring the allocation.
        context: DiagnosticInterfaceValidationContext,
        /// Field requiring allocated storage.
        field: DiagnosticInterfaceValidationField,
        /// Requested byte count.
        requested: u64,
    },
    /// Surface reconstruction failed.
    SurfaceBuild {
        /// Exact declaration-surface construction failure.
        cause: Box<DiagnosticInterfaceSymbolGraphProblem>,
    },
    /// Exact artifact hash does not match retained bytes.
    ArtifactHashMismatch {
        /// Hash declared by the artifact.
        expected: DiagnosticArtifactDigest,
        /// Hash computed from retained bytes.
        actual: DiagnosticArtifactDigest,
    },
    /// Semantic content hash does not match retained section identities.
    ContentHashMismatch {
        /// Content hash declared by the artifact.
        expected: DiagnosticArtifactDigest,
        /// Hash computed from retained section identities.
        actual: DiagnosticArtifactDigest,
    },
    /// An implementation payload checksum does not match its encoded bytes.
    PayloadChecksumMismatch {
        /// Implementation payload entry whose bytes failed validation.
        context: DiagnosticInterfaceValidationContext,
        /// Checksum declared by the implementation directory.
        expected: DiagnosticArtifactDigest,
        /// Checksum computed from the encoded payload bytes.
        actual: DiagnosticArtifactDigest,
    },
    /// An implementation payload content hash does not match its decoded content.
    PayloadContentHashMismatch {
        /// Implementation payload entry whose decoded content failed validation.
        context: DiagnosticInterfaceValidationContext,
        /// Content hash declared by the implementation directory.
        expected: DiagnosticArtifactDigest,
        /// Hash computed from the decoded payload content.
        actual: DiagnosticArtifactDigest,
    },
    /// A pre-specialized payload key does not match its encoded specialization.
    SpecializationKeyMismatch {
        /// Specialization key declared by the implementation directory.
        expected: [u8; 32],
        /// Specialization key reconstructed from the payload.
        actual: [u8; 32],
    },
    /// An implementation artifact targets a different configuration.
    ImplementationConfigurationMismatch {
        /// Configuration digest required by the consumer.
        expected: [u8; 32],
        /// Configuration digest declared by the implementation artifact.
        actual: [u8; 32],
    },
    /// An implementation artifact belongs to a different package interface.
    ImplementationInterfaceIdentityMismatch {
        /// Package-interface identity required by the consumer.
        expected: Box<DiagnosticPackageInterfaceIdentity>,
        /// Package-interface identity declared by the implementation artifact.
        actual: Box<DiagnosticPackageInterfaceIdentity>,
    },
    /// One implementation dependency does not match the package-interface dependency graph.
    ImplementationDependencyMismatch {
        /// Zero-based dependency index.
        index: u64,
        /// Dependency required by the package interface, if present.
        expected: Option<Box<DiagnosticInterfaceDependency>>,
        /// Dependency declared by the implementation artifact, if present.
        actual: Option<Box<DiagnosticInterfaceDependency>>,
    },
    /// A known section checksum does not match its bytes.
    SectionChecksumMismatch {
        /// Section whose bytes failed checksum validation.
        section: DiagnosticInterfaceSection,
        /// Checksum declared by the section directory.
        expected: DiagnosticArtifactDigest,
        /// Checksum computed from retained bytes.
        actual: DiagnosticArtifactDigest,
    },
    /// An unknown optional section checksum does not match its bytes.
    UnknownSectionChecksumMismatch {
        /// Raw tag of the unknown optional section.
        raw_tag: u32,
        /// Checksum declared by the section directory.
        expected: DiagnosticArtifactDigest,
        /// Checksum computed from retained bytes.
        actual: DiagnosticArtifactDigest,
    },
    /// A decoded section content hash does not match its payload.
    SectionContentHashMismatch {
        /// Section whose decoded content failed validation.
        section: DiagnosticInterfaceSection,
        /// Content hash declared by the section directory.
        expected: DiagnosticArtifactDigest,
        /// Hash computed from decoded content.
        actual: DiagnosticArtifactDigest,
    },
    /// Externally controlled input exceeds a configured ceiling.
    ResourceLimitExceeded {
        /// Resource category that exceeded its ceiling.
        limit: DiagnosticInterfaceLimit,
        /// Externally supplied size or count.
        actual: u64,
        /// Configured maximum.
        maximum: u64,
    },
}

impl DiagnosticInterfaceValidationFailure {
    /// Returns the stable machine key for this validation failure.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::InvalidMagic { .. } => "invalid_magic",
            Self::UnsupportedFormatRevision { .. } => "unsupported_format_revision",
            Self::UnsupportedLanguageRevision { .. } => "unsupported_language_revision",
            Self::UnsupportedByteOrder { .. } => "unsupported_byte_order",
            Self::UnsupportedRequiredFlags { .. } => "unsupported_required_flags",
            Self::Truncated { .. } => "truncated",
            Self::TrailingBytes { .. } => "trailing_bytes",
            Self::Malformed { .. } => "malformed",
            Self::InvalidUtf8 { .. } => "invalid_utf8",
            Self::Compression { .. } => "compression",
            Self::DigestUnavailable { .. } => "digest_unavailable",
            Self::AllocationUnavailable { .. } => "allocation_unavailable",
            Self::SurfaceBuild { .. } => "surface_build",
            Self::ArtifactHashMismatch { .. } => "artifact_hash_mismatch",
            Self::ContentHashMismatch { .. } => "content_hash_mismatch",
            Self::PayloadChecksumMismatch { .. } => "payload_checksum_mismatch",
            Self::PayloadContentHashMismatch { .. } => "payload_content_hash_mismatch",
            Self::SpecializationKeyMismatch { .. } => "specialization_key_mismatch",
            Self::ImplementationConfigurationMismatch { .. } => {
                "implementation_configuration_mismatch"
            }
            Self::ImplementationInterfaceIdentityMismatch { .. } => {
                "implementation_interface_identity_mismatch"
            }
            Self::ImplementationDependencyMismatch { .. } => "implementation_dependency_mismatch",
            Self::SectionChecksumMismatch { .. } => "section_checksum_mismatch",
            Self::UnknownSectionChecksumMismatch { .. } => "unknown_section_checksum_mismatch",
            Self::SectionContentHashMismatch { .. } => "section_content_hash_mismatch",
            Self::ResourceLimitExceeded { .. } => "resource_limit_exceeded",
        }
    }
}
