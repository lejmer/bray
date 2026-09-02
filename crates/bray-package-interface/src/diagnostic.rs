use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticId};

use crate::section::InterfaceSectionTag;
use crate::{
    InterfaceArtifactHash, InterfaceContentHash, InterfaceDependency, InterfaceFormatRevision,
    InterfaceLanguageRevision, InterfaceLimit, InterfaceRequiredFlags, InterfaceSectionHash,
    InterfaceSemanticRecordKind, PackageInterfaceIdentity, PackageInterfaceSurfaceBuildError,
};

/// Exact package-interface region being decoded or validated.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InterfaceValidationContext {
    /// Complete package-interface artifact.
    Artifact,
    /// Fixed artifact header.
    Header,
    /// Section directory as a whole.
    Directory,
    /// One section-directory entry.
    DirectoryEntry {
        /// Zero-based directory index.
        index: u64,
        /// Raw section tag stored by the entry.
        raw_tag: u32,
    },
    /// One package implementation artifact directory entry.
    ImplementationEntry {
        /// Zero-based implementation directory index.
        index: u64,
        /// Open payload-kind byte stored by the entry.
        raw_kind: u8,
    },
    /// One known section payload.
    Section(InterfaceSectionTag),
    /// One indexed record in a known section.
    Record {
        /// Section containing the record.
        section: InterfaceSectionTag,
        /// Zero-based record index.
        index: u64,
    },
    /// One indexed semantic record of a known kind.
    SemanticRecord {
        /// Semantic record category.
        kind: InterfaceSemanticRecordKind,
        /// Zero-based semantic record index.
        index: u64,
    },
    /// One component of a length-delimited external symbol key.
    ExternalSymbolKey {
        /// Zero-based component index.
        component: u64,
    },
}

/// Closed package-interface field categories used by structural failures.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InterfaceValidationField {
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

/// Integer representation that rejected an untrusted value.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InterfaceIntegerTarget {
    /// Unsigned 16-bit wire value.
    U16,
    /// Unsigned 32-bit wire value.
    U32,
    /// Unsigned 64-bit wire value.
    U64,
    /// Host-sized allocation or slice value.
    Usize,
}

/// Stable category for invalid UTF-8 supplied by an interface artifact.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InterfaceUtf8Failure {
    /// An invalid byte sequence of the given length was encountered.
    InvalidSequence {
        /// Length of the invalid byte sequence when known.
        error_length: Option<u64>,
    },
    /// The input ended within an otherwise valid UTF-8 sequence.
    IncompleteSequence,
}

/// Exact malformed structural contract retained by interface validation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InterfaceMalformedCause {
    /// A required value was absent.
    Missing { field: InterfaceValidationField },
    /// A value failed its domain constructor or shape contract.
    InvalidValue { field: InterfaceValidationField },
    /// A closed wire discriminant was unknown.
    InvalidDiscriminant {
        field: InterfaceValidationField,
        actual: u64,
    },
    /// A scalar value disagreed with its exact required value.
    ValueMismatch {
        field: InterfaceValidationField,
        expected: u64,
        actual: u64,
    },
    /// A declared count disagreed with the retained values.
    CountMismatch {
        field: InterfaceValidationField,
        expected: u64,
        actual: u64,
    },
    /// A reference addressed a missing table slot.
    InvalidReference {
        field: InterfaceValidationField,
        index: u64,
        available: u64,
    },
    /// Values violated their required strict ordering.
    OrderingViolation {
        field: InterfaceValidationField,
        previous: u64,
        actual: u64,
    },
    /// A unique value or slot appeared more than once.
    Duplicate {
        field: InterfaceValidationField,
        index: u64,
    },
    /// A supposedly acyclic record graph contains a cycle.
    Cycle {
        field: InterfaceValidationField,
        index: u64,
    },
    /// A numeric value cannot fit its required representation.
    NumericOverflow {
        field: InterfaceValidationField,
        value: u64,
        target: InterfaceIntegerTarget,
    },
    /// A length or offset is not aligned to its required unit.
    InvalidAlignment {
        field: InterfaceValidationField,
        value: u64,
        alignment: u64,
    },
    /// A byte range overflowed its stable representation.
    RangeOverflow { offset: u64, length: u64 },
    /// Two independently declared byte ranges overlap.
    RangeOverlap {
        offset: u64,
        length: u64,
        conflicting_offset: u64,
        conflicting_length: u64,
    },
    /// A declared length disagreed with the retained value.
    LengthMismatch {
        field: InterfaceValidationField,
        expected: u64,
        actual: u64,
    },
}

/// Stable package-interface compression or framing failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InterfaceCompressionFailure {
    /// Compression library initialization failed.
    EncoderInitialization,
    /// Deterministic encoder configuration failed.
    EncoderConfiguration,
    /// Section compression failed.
    Encoding,
    /// Frame-length inspection failed.
    FrameLength,
    /// The frame contains trailing or missing encoded bytes.
    FrameLengthMismatch { expected: u64, actual: u64 },
    /// Frame content-size inspection failed.
    ContentSize,
    /// Declared decoded size disagreed with the frame.
    ContentSizeMismatch { expected: u64, actual: Option<u64> },
    /// Required frame-header byte was absent.
    MissingHeaderByte { offset: u64 },
    /// Frame-header flags violate the deterministic encoding contract.
    InvalidHeaderFlags { descriptor: u8 },
    /// Frame window arithmetic overflowed.
    WindowSizeOverflow { descriptor: u8 },
    /// Frame window exceeds the configured validation ceiling.
    WindowSizeExceeded { actual: u64, maximum: u64 },
    /// Decompression library initialization failed.
    DecoderInitialization,
    /// Bounded decoder configuration failed.
    DecoderConfiguration,
    /// Section decompression failed.
    Decoding,
    /// Decoded bytes disagree with the committed length.
    DecodedLengthMismatch { expected: u64, actual: u64 },
}

/// Structural or resource failure found while validating an untrusted package interface.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum InterfaceValidationError {
    /// The artifact does not start with the required file magic.
    InvalidMagic {
        /// Exact bytes found at the file-magic position.
        actual: [u8; 8],
    },
    /// The artifact uses a format revision this compiler does not implement.
    UnsupportedFormatRevision {
        /// Format revision found in the artifact.
        actual: InterfaceFormatRevision,
    },
    /// The artifact requires a different language semantic revision.
    UnsupportedLanguageRevision {
        /// Language revision required by the validation policy.
        expected: InterfaceLanguageRevision,
        /// Language revision found in the artifact.
        actual: InterfaceLanguageRevision,
    },
    /// The artifact uses a byte-order marker this compiler does not implement.
    UnsupportedByteOrder {
        /// Required byte-order marker.
        expected: u32,
        /// Byte-order marker found in the artifact.
        actual: u32,
    },
    /// The artifact requires unsupported compatibility behavior.
    UnsupportedRequiredFlags {
        /// Required flags found in the artifact.
        actual: InterfaceRequiredFlags,
    },
    /// The byte sequence ends before a declared structural value is complete.
    Truncated {
        /// Region being decoded.
        context: InterfaceValidationContext,
        /// Field whose bytes are incomplete.
        field: InterfaceValidationField,
        /// Byte offset relative to `context`.
        offset: u64,
        /// Number of bytes required by the field.
        expected_length: u64,
        /// Number of bytes still available at `offset`.
        actual_length: u64,
    },
    /// Bytes remain after the exact structural payload was consumed.
    TrailingBytes {
        /// Region being decoded.
        context: InterfaceValidationContext,
        /// Offset of the first trailing byte relative to `context`.
        offset: u64,
        /// Number of trailing bytes.
        count: u64,
    },
    /// The fixed envelope or semantic payload violates an exact structural contract.
    Malformed {
        /// Region whose contract failed.
        context: InterfaceValidationContext,
        /// Exact violated contract.
        cause: InterfaceMalformedCause,
    },
    /// A UTF-8 field contains an invalid byte sequence.
    InvalidUtf8 {
        /// Region containing the string.
        context: InterfaceValidationContext,
        /// String field being decoded.
        field: InterfaceValidationField,
        /// Byte offset of the invalid sequence.
        offset: u64,
        /// Number of bytes in the string range.
        length: u64,
        /// Stable UTF-8 failure category.
        cause: InterfaceUtf8Failure,
    },
    /// Compression or framing failed for one bounded payload.
    Compression {
        /// Payload being encoded, framed, or decoded.
        context: InterfaceValidationContext,
        /// Exact compression or framing failure.
        cause: InterfaceCompressionFailure,
    },
    /// A digest could not be computed for structurally invalid retained bytes.
    DigestUnavailable {
        /// Region whose digest was requested.
        context: InterfaceValidationContext,
        /// Digest field being computed.
        field: InterfaceValidationField,
    },
    /// A validated bounded allocation could not be reserved by the host.
    AllocationUnavailable {
        /// Region whose decoded representation was being allocated.
        context: InterfaceValidationContext,
        /// Value or table being allocated.
        field: InterfaceValidationField,
        /// Requested allocation size in bytes.
        requested: u64,
    },
    /// Surface reconstruction retained its exact typed validation cause.
    SurfaceBuild {
        /// Exact surface construction failure.
        cause: Box<PackageInterfaceSurfaceBuildError>,
    },
    /// The exact artifact hash does not match the retained bytes.
    ArtifactHashMismatch {
        /// Hash declared by the artifact.
        expected: InterfaceArtifactHash,
        /// Hash computed from retained bytes.
        actual: InterfaceArtifactHash,
    },
    /// The semantic content hash does not match the retained section identities.
    ContentHashMismatch {
        /// Hash declared by the artifact.
        expected: InterfaceContentHash,
        /// Hash computed from retained content identities.
        actual: InterfaceContentHash,
    },
    /// One section payload does not match its declared checksum.
    SectionChecksumMismatch {
        /// Section whose payload checksum failed validation.
        section: InterfaceSectionTag,
        /// Checksum declared by the section directory.
        expected: InterfaceSectionHash,
        /// Checksum computed from retained bytes.
        actual: InterfaceSectionHash,
    },
    /// An unknown optional section payload does not match its declared checksum.
    UnknownSectionChecksumMismatch {
        /// Raw section tag from the directory.
        raw_tag: u32,
        /// Checksum declared by the section directory.
        expected: InterfaceSectionHash,
        /// Checksum computed from retained bytes.
        actual: InterfaceSectionHash,
    },
    /// A decoded section does not match its committed semantic content hash.
    SectionContentHashMismatch {
        /// Section whose decoded content failed validation.
        section: InterfaceSectionTag,
        /// Content hash declared by the section directory.
        expected: [u8; 32],
        /// Content hash computed from decoded bytes.
        actual: [u8; 32],
    },
    /// One implementation payload does not match its declared checksum.
    PayloadChecksumMismatch {
        /// Exact implementation directory entry whose payload failed validation.
        context: InterfaceValidationContext,
        /// Checksum declared by the implementation directory.
        expected: [u8; 32],
        /// Checksum computed from retained bytes.
        actual: [u8; 32],
    },
    /// One decoded implementation payload does not match its committed content hash.
    PayloadContentHashMismatch {
        /// Exact implementation directory entry whose decoded payload failed validation.
        context: InterfaceValidationContext,
        /// Content hash declared by the implementation directory.
        expected: [u8; 32],
        /// Content hash computed from decoded bytes.
        actual: [u8; 32],
    },
    /// A decoded specialization key disagrees with the selected cache identity.
    SpecializationKeyMismatch {
        /// Selected specialization identity.
        expected: [u8; 32],
        /// Specialization identity decoded from the implementation payload.
        actual: [u8; 32],
    },
    /// A package implementation artifact was selected under a different configuration.
    ImplementationConfigurationMismatch {
        /// Selected target, runtime, and panic configuration identity.
        expected: [u8; 32],
        /// Configuration identity retained by the implementation artifact.
        actual: [u8; 32],
    },
    /// A package implementation artifact belongs to a different package-interface surface.
    ImplementationInterfaceIdentityMismatch {
        /// Interface identity required by dependency selection.
        expected: Box<PackageInterfaceIdentity>,
        /// Interface identity retained by the implementation artifact.
        actual: Box<PackageInterfaceIdentity>,
    },
    /// One selected implementation dependency disagrees with the artifact dependency table.
    ImplementationDependencyMismatch {
        /// First mismatched dependency-table index.
        index: u64,
        /// Selected dependency, or `None` when the selected table ended first.
        expected: Option<Box<InterfaceDependency>>,
        /// Artifact dependency, or `None` when the artifact table ended first.
        actual: Option<Box<InterfaceDependency>>,
    },
    /// An externally controlled size exceeds its configured ceiling.
    ResourceLimitExceeded {
        /// Resource category that exceeded its ceiling.
        limit: InterfaceLimit,
        /// Externally supplied size or count.
        actual: u64,
        /// Configured maximum size or count.
        maximum: u64,
    },
}

impl InterfaceValidationError {
    /// Projects this validation failure into its exact locale-neutral payload without consuming it.
    pub fn diagnostic_failure(&self) -> bray_diagnostics::DiagnosticInterfaceValidationFailure {
        crate::presentation::diagnostic_failure(self)
    }

    /// Converts this validation failure into its exact locale-neutral payload.
    pub fn into_diagnostic_failure(self) -> bray_diagnostics::DiagnosticInterfaceValidationFailure {
        crate::presentation::diagnostic_failure(&self)
    }

    /// Converts this validation failure into a locale-neutral diagnostic.
    pub fn into_diagnostic(self, id: DiagnosticId) -> Diagnostic {
        crate::presentation::validation_diagnostic(self, id)
    }

    /// Converts this validation failure into a single-entry diagnostic bag.
    pub fn into_diagnostic_bag(self) -> DiagnosticBag {
        DiagnosticBag::single(self.into_diagnostic(DiagnosticId::new(0)))
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        DiagnosticArg, DiagnosticArgName, DiagnosticId, DiagnosticInterfaceLimit,
        DiagnosticInterfaceValidationFailure, DiagnosticKind, SeverityKind,
    };

    use super::{
        InterfaceMalformedCause, InterfaceValidationContext, InterfaceValidationError,
        InterfaceValidationField,
    };
    use crate::{
        InterfaceArtifactHash, InterfaceFormatRevision, InterfaceLanguageRevision, InterfaceLimit,
        InterfaceSectionHash, InterfaceSectionTag,
    };

    #[test]
    fn validation_failures_publish_exact_structured_diagnostic_kinds() {
        let cases = [
            (
                InterfaceValidationError::InvalidMagic { actual: [0; 8] },
                DiagnosticKind::InterfaceInvalidMagic,
            ),
            (
                InterfaceValidationError::UnsupportedFormatRevision {
                    actual: InterfaceFormatRevision::new(2),
                },
                DiagnosticKind::InterfaceUnsupportedFormatRevision,
            ),
            (
                InterfaceValidationError::UnsupportedLanguageRevision {
                    expected: InterfaceLanguageRevision::new(1),
                    actual: InterfaceLanguageRevision::new(2),
                },
                DiagnosticKind::InterfaceUnsupportedLanguageRevision,
            ),
            (
                InterfaceValidationError::UnsupportedByteOrder {
                    expected: crate::header::BYTE_ORDER_MARKER,
                    actual: 0,
                },
                DiagnosticKind::InterfaceUnsupportedEncoding,
            ),
            (
                InterfaceValidationError::Truncated {
                    context: InterfaceValidationContext::Header,
                    field: InterfaceValidationField::Magic,
                    offset: 0,
                    expected_length: 8,
                    actual_length: 7,
                },
                DiagnosticKind::InterfaceValidationFailed,
            ),
            (
                InterfaceValidationError::Malformed {
                    context: InterfaceValidationContext::Header,
                    cause: InterfaceMalformedCause::InvalidValue {
                        field: InterfaceValidationField::RequiredFlags,
                    },
                },
                DiagnosticKind::InterfaceValidationFailed,
            ),
            (
                InterfaceValidationError::ArtifactHashMismatch {
                    expected: InterfaceArtifactHash::from_bytes([0; 32]),
                    actual: InterfaceArtifactHash::from_bytes([1; 32]),
                },
                DiagnosticKind::InterfaceHashMismatch,
            ),
            (
                InterfaceValidationError::SectionChecksumMismatch {
                    section: InterfaceSectionTag::Strings,
                    expected: InterfaceSectionHash::from_bytes([0; 32]),
                    actual: InterfaceSectionHash::from_bytes([1; 32]),
                },
                DiagnosticKind::InterfaceSectionChecksumMismatch,
            ),
            (
                InterfaceValidationError::ResourceLimitExceeded {
                    limit: InterfaceLimit::FileSize,
                    actual: 2,
                    maximum: 1,
                },
                DiagnosticKind::InterfaceResourceLimitExceeded,
            ),
        ];

        for (error, expected_kind) in cases {
            let diagnostic = error.into_diagnostic(DiagnosticId::new(0));

            assert_eq!(diagnostic.kind(), expected_kind);
            assert_eq!(diagnostic.severity(), SeverityKind::Error);
            assert_eq!(diagnostic.args().len(), 1);

            assert_eq!(
                diagnostic.args()[0].name(),
                DiagnosticArgName::InterfaceValidationFailure
            );
        }
    }

    #[test]
    fn implementation_entry_limits_preserve_their_exact_resource_category() {
        let diagnostic = InterfaceValidationError::ResourceLimitExceeded {
            limit: InterfaceLimit::ImplementationEntryCount,
            actual: 2,
            maximum: 1,
        }
        .into_diagnostic(DiagnosticId::new(0));

        assert_eq!(
            diagnostic.args(),
            [DiagnosticArg::interface_validation_failure(
                DiagnosticInterfaceValidationFailure::ResourceLimitExceeded {
                    limit: DiagnosticInterfaceLimit::ImplementationEntryCount,
                    actual: 2,
                    maximum: 1,
                }
            )]
        );
    }
}
