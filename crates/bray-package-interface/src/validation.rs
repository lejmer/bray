use std::ops::Range;
use std::sync::Arc;

use crate::diagnostic::InterfaceValidationError;
use crate::hash::{compute_artifact_hash, compute_content_hash, compute_section_hash};
use crate::header::{BYTE_ORDER_MARKER, CURRENT_FORMAT_REVISION, InterfaceHeader, MAGIC};
use crate::limits::{InterfaceLimit, InterfaceValidationLimits, InterfaceValidationPolicy};
use crate::section::{DirectoryEntry, InterfaceSectionTag, ValidatedInterfaceSection};
use crate::wire::WireDecodeError;

/// Immutable package-interface bytes with an eagerly validated structural envelope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedPackageInterface {
    bytes: Arc<[u8]>,
    header: InterfaceHeader,
    directory: Arc<[DirectoryEntry]>,
    limits: InterfaceValidationLimits,
}

impl ValidatedPackageInterface {
    /// Validates untrusted bytes under an exact compatibility and resource policy.
    pub fn try_new(
        bytes: impl Into<Arc<[u8]>>,
        policy: InterfaceValidationPolicy,
    ) -> Result<Self, InterfaceValidationError> {
        let bytes = bytes.into();

        validate_file_size(bytes.len(), policy)?;

        let decoded = InterfaceHeader::decode(&bytes).map_err(map_wire_error)?;
        validate_header(
            decoded.magic,
            decoded.byte_order_marker,
            decoded.header,
            &bytes,
            policy,
        )?;

        let directory_range = checked_range(
            decoded.header.directory_offset(),
            decoded.header.directory_length(),
            bytes.len(),
        )
        .map_err(range_validation_error)?;
        let directory = decode_directory(&bytes, directory_range, policy)?;

        validate_hashes(decoded.header, &directory, &bytes)?;

        Ok(Self {
            bytes,
            header: decoded.header,
            directory: directory.into(),
            limits: policy.limits(),
        })
    }

    /// Returns the validated fixed header.
    pub const fn header(&self) -> InterfaceHeader {
        self.header
    }

    /// Returns one validated section by its stable category.
    pub fn section(&self, tag: InterfaceSectionTag) -> Option<ValidatedInterfaceSection<'_>> {
        let index = self
            .directory
            .binary_search_by_key(&tag, |entry| entry.tag())
            .ok()?;
        let entry = *self.directory.get(index)?;

        ValidatedInterfaceSection::new(entry, &self.bytes)
    }

    /// Iterates over validated sections in canonical tag order.
    pub fn sections(&self) -> impl DoubleEndedIterator<Item = ValidatedInterfaceSection<'_>> + '_ {
        self.directory
            .iter()
            .filter_map(|entry| ValidatedInterfaceSection::new(*entry, &self.bytes))
    }

    /// Decodes and validates the eager package identity and symbol-surface sections.
    pub fn decode_identity_surface(
        &self,
    ) -> Result<crate::PackageInterfaceSurface, InterfaceValidationError> {
        crate::surface::decode_surface(self, self.limits)
    }
}

fn validate_file_size(
    byte_length: usize,
    policy: InterfaceValidationPolicy,
) -> Result<(), InterfaceValidationError> {
    let actual = usize_to_u64_saturating(byte_length);

    policy.limits().check(InterfaceLimit::FileSize, actual)
}

fn validate_header(
    magic: [u8; 8],
    byte_order_marker: u32,
    header: InterfaceHeader,
    bytes: &[u8],
    policy: InterfaceValidationPolicy,
) -> Result<(), InterfaceValidationError> {
    if magic != MAGIC {
        return Err(InterfaceValidationError::InvalidMagic);
    }

    if header.format_revision() != CURRENT_FORMAT_REVISION {
        return Err(InterfaceValidationError::UnsupportedFormatRevision {
            actual: header.format_revision(),
        });
    }

    if header.language_revision() != policy.language_revision() {
        return Err(InterfaceValidationError::UnsupportedLanguageRevision {
            expected: policy.language_revision(),
            actual: header.language_revision(),
        });
    }

    if byte_order_marker != BYTE_ORDER_MARKER
        || header.required_flags() != crate::InterfaceRequiredFlags::NONE
    {
        return Err(InterfaceValidationError::UnsupportedEncoding);
    }

    let actual_length = usize_to_u64_saturating(bytes.len());
    if header.declared_file_length() > actual_length {
        return Err(InterfaceValidationError::Truncated);
    }

    if header.declared_file_length() != actual_length {
        return Err(InterfaceValidationError::Malformed);
    }

    if !header
        .directory_length()
        .is_multiple_of(DirectoryEntry::WIRE_LENGTH)
    {
        return Err(InterfaceValidationError::Malformed);
    }

    Ok(())
}

fn decode_directory(
    bytes: &[u8],
    directory_range: Range<usize>,
    policy: InterfaceValidationPolicy,
) -> Result<Vec<DirectoryEntry>, InterfaceValidationError> {
    let directory_length = usize_to_u64_saturating(directory_range.len());
    let section_count = directory_length / DirectoryEntry::WIRE_LENGTH;

    policy
        .limits()
        .check(InterfaceLimit::SectionCount, section_count)?;

    let capacity = usize::try_from(section_count).map_err(|_| {
        InterfaceValidationError::ResourceLimitExceeded {
            limit: InterfaceLimit::SectionCount,
            actual: section_count,
            maximum: policy.limits().maximum(InterfaceLimit::SectionCount),
        }
    })?;
    let mut entries = Vec::with_capacity(capacity);
    let mut previous_tag = None;
    let mut previous_end = InterfaceHeader::LENGTH;
    let mut decoded_allocation = 0_u64;

    for chunk in bytes[directory_range.clone()].chunks_exact(DirectoryEntry::LENGTH) {
        let decoded = DirectoryEntry::decode(chunk).map_err(map_wire_error)?;

        let Some(tag) = InterfaceSectionTag::from_wire_value(decoded.raw_tag) else {
            return Err(InterfaceValidationError::Malformed);
        };

        if decoded.encoding_flags != 0
            || previous_tag.is_some_and(|previous| decoded.raw_tag <= previous)
        {
            return Err(InterfaceValidationError::Malformed);
        }

        policy
            .limits()
            .check(InterfaceLimit::RecordCount, decoded.record_count)?;

        decoded_allocation = decoded_allocation
            .checked_add(decoded.length)
            .ok_or_else(|| InterfaceValidationError::ResourceLimitExceeded {
                limit: InterfaceLimit::DecodedAllocation,
                actual: u64::MAX,
                maximum: policy.limits().maximum(InterfaceLimit::DecodedAllocation),
            })?;

        policy
            .limits()
            .check(InterfaceLimit::DecodedAllocation, decoded_allocation)?;

        let payload_range = checked_range(decoded.offset, decoded.length, bytes.len())
            .map_err(range_validation_error)?;

        if payload_range.start < InterfaceHeader::LENGTH
            || ranges_overlap(&payload_range, &directory_range)
            || payload_range.start < previous_end
        {
            return Err(InterfaceValidationError::Malformed);
        }

        let entry = DirectoryEntry::from_decoded(decoded, tag);
        previous_tag = Some(decoded.raw_tag);
        previous_end = payload_range.end;
        entries.push(entry);
    }

    Ok(entries)
}

fn validate_hashes(
    header: InterfaceHeader,
    directory: &[DirectoryEntry],
    bytes: &[u8],
) -> Result<(), InterfaceValidationError> {
    for entry in directory {
        let payload = entry
            .payload(bytes)
            .ok_or(InterfaceValidationError::Malformed)?;

        if compute_section_hash(entry, payload) != entry.checksum() {
            return Err(InterfaceValidationError::SectionChecksumMismatch {
                section: entry.tag(),
            });
        }
    }

    let content_hash = compute_content_hash(&header, directory, bytes)
        .ok_or(InterfaceValidationError::Malformed)?;
    let artifact_hash = compute_artifact_hash(bytes).ok_or(InterfaceValidationError::Malformed)?;

    if content_hash != header.content_hash() || artifact_hash != header.artifact_hash() {
        return Err(InterfaceValidationError::HashMismatch);
    }

    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CheckedRangeError {
    Overflow,
    OutOfBounds,
}

fn checked_range(
    offset: u64,
    length: u64,
    file_length: usize,
) -> Result<Range<usize>, CheckedRangeError> {
    let end = offset
        .checked_add(length)
        .ok_or(CheckedRangeError::Overflow)?;
    let start = usize::try_from(offset).map_err(|_| CheckedRangeError::Overflow)?;
    let end = usize::try_from(end).map_err(|_| CheckedRangeError::Overflow)?;

    if end > file_length {
        return Err(CheckedRangeError::OutOfBounds);
    }

    Ok(start..end)
}

const fn ranges_overlap(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

fn usize_to_u64_saturating(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

const fn range_validation_error(error: CheckedRangeError) -> InterfaceValidationError {
    match error {
        CheckedRangeError::Overflow => InterfaceValidationError::Malformed,
        CheckedRangeError::OutOfBounds => InterfaceValidationError::Truncated,
    }
}

const fn map_wire_error(error: WireDecodeError) -> InterfaceValidationError {
    match error {
        WireDecodeError::Truncated => InterfaceValidationError::Truncated,
        WireDecodeError::TrailingBytes => InterfaceValidationError::Malformed,
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticId,
        DiagnosticInterfaceLimit, DiagnosticKind,
    };
    use bray_messages::DiagnosticRenderer;

    use super::ValidatedPackageInterface;
    use crate::diagnostic::InterfaceValidationError;
    use crate::hash::{compute_artifact_hash, compute_content_hash, compute_section_hash};
    use crate::header::{BYTE_ORDER_MARKER, InterfaceHeader, MAGIC};
    use crate::limits::{InterfaceLimit, InterfaceValidationLimits, InterfaceValidationPolicy};
    use crate::section::{DirectoryEntry, InterfaceSectionTag};
    use crate::wire::WireEncoder;
    use crate::{CURRENT_FORMAT_REVISION, InterfaceLanguageRevision};

    const LANGUAGE_REVISION: InterfaceLanguageRevision = InterfaceLanguageRevision::new(7);
    const CONTENT_HASH_OFFSET: usize = 48;
    const ARTIFACT_HASH_OFFSET: usize = 80;

    struct SectionFixture<'bytes> {
        tag: InterfaceSectionTag,
        record_count: u64,
        payload: &'bytes [u8],
    }

    #[test]
    fn valid_artifacts_publish_immutable_section_views() {
        let bytes = artifact(&[
            SectionFixture {
                tag: InterfaceSectionTag::Strings,
                record_count: 2,
                payload: b"alpha beta",
            },
            SectionFixture {
                tag: InterfaceSectionTag::SourceProvenance,
                record_count: 1,
                payload: b"source.bray",
            },
        ]);

        let interface = validate(bytes);

        assert_eq!(
            interface.header().format_revision(),
            CURRENT_FORMAT_REVISION
        );
        assert_eq!(interface.header().language_revision(), LANGUAGE_REVISION);

        let Some(strings) = interface.section(InterfaceSectionTag::Strings) else {
            panic!("validated string section must exist");
        };

        assert_eq!(strings.tag(), InterfaceSectionTag::Strings);
        assert_eq!(strings.record_count(), 2);
        assert_eq!(strings.bytes(), b"alpha beta");
        assert_eq!(interface.sections().count(), 2);
        assert_eq!(interface.section(InterfaceSectionTag::Contracts), None);
    }

    #[test]
    fn every_truncated_header_boundary_is_rejected_without_panicking() {
        let bytes = artifact(&[]);

        for length in 0..InterfaceHeader::LENGTH {
            assert_eq!(
                ValidatedPackageInterface::try_new(&bytes[..length], policy()),
                Err(InterfaceValidationError::Truncated),
                "unexpected result for prefix length {length}"
            );
        }
    }

    #[test]
    fn header_identity_and_compatibility_are_exact() {
        let bytes = artifact(&[]);

        assert_mutation_error(&bytes, 0, 0, InterfaceValidationError::InvalidMagic);
        assert_mutation_error(
            &bytes,
            8,
            2,
            InterfaceValidationError::UnsupportedFormatRevision {
                actual: crate::InterfaceFormatRevision::new(2),
            },
        );
        assert_mutation_error(
            &bytes,
            10,
            8,
            InterfaceValidationError::UnsupportedLanguageRevision {
                expected: LANGUAGE_REVISION,
                actual: InterfaceLanguageRevision::new(8),
            },
        );
        assert_mutation_error(&bytes, 12, 0, InterfaceValidationError::UnsupportedEncoding);
        assert_mutation_error(&bytes, 16, 1, InterfaceValidationError::UnsupportedEncoding);
    }

    #[test]
    fn declared_lengths_and_range_arithmetic_are_checked() {
        let bytes = artifact(&[SectionFixture {
            tag: InterfaceSectionTag::Strings,
            record_count: 1,
            payload: b"a",
        }]);

        let mut truncated = bytes.clone();
        write_u64(&mut truncated, 24, wire_length(bytes.len()) + 1);
        assert_eq!(
            ValidatedPackageInterface::try_new(truncated, policy()),
            Err(InterfaceValidationError::Truncated)
        );

        let mut trailing = bytes.clone();
        write_u64(&mut trailing, 24, wire_length(bytes.len()) - 1);
        assert_eq!(
            ValidatedPackageInterface::try_new(trailing, policy()),
            Err(InterfaceValidationError::Malformed)
        );

        let mut invalid_directory_length = bytes.clone();
        write_u64(&mut invalid_directory_length, 40, 1);
        assert_eq!(
            ValidatedPackageInterface::try_new(invalid_directory_length, policy()),
            Err(InterfaceValidationError::Malformed)
        );

        let mut overflowing_directory = bytes;
        write_u64(&mut overflowing_directory, 32, u64::MAX);
        assert_eq!(
            ValidatedPackageInterface::try_new(overflowing_directory, policy()),
            Err(InterfaceValidationError::Malformed)
        );
    }

    #[test]
    fn directory_tags_ordering_and_ranges_are_validated_before_hashes() {
        let mut unknown_tag = artifact(&[SectionFixture {
            tag: InterfaceSectionTag::Strings,
            record_count: 1,
            payload: b"a",
        }]);
        let directory = unknown_tag.len() - DirectoryEntry::LENGTH;
        write_u32(&mut unknown_tag, directory, 99);
        assert_eq!(
            ValidatedPackageInterface::try_new(unknown_tag, policy()),
            Err(InterfaceValidationError::Malformed)
        );

        let mut encoded_section = artifact(&[SectionFixture {
            tag: InterfaceSectionTag::Strings,
            record_count: 1,
            payload: b"a",
        }]);
        let directory = encoded_section.len() - DirectoryEntry::LENGTH;
        write_u32(&mut encoded_section, directory + 4, 1);
        assert_eq!(
            ValidatedPackageInterface::try_new(encoded_section, policy()),
            Err(InterfaceValidationError::Malformed)
        );

        let mut duplicate = artifact(&[
            SectionFixture {
                tag: InterfaceSectionTag::Strings,
                record_count: 1,
                payload: b"a",
            },
            SectionFixture {
                tag: InterfaceSectionTag::Dependencies,
                record_count: 1,
                payload: b"b",
            },
        ]);
        let directory = duplicate.len() - 2 * DirectoryEntry::LENGTH;
        write_u32(
            &mut duplicate,
            directory + DirectoryEntry::LENGTH,
            InterfaceSectionTag::Strings.wire_value(),
        );
        assert_eq!(
            ValidatedPackageInterface::try_new(duplicate, policy()),
            Err(InterfaceValidationError::Malformed)
        );

        let mut overlap = artifact(&[
            SectionFixture {
                tag: InterfaceSectionTag::Strings,
                record_count: 1,
                payload: b"aa",
            },
            SectionFixture {
                tag: InterfaceSectionTag::Dependencies,
                record_count: 1,
                payload: b"bb",
            },
        ]);
        let directory = overlap.len() - 2 * DirectoryEntry::LENGTH;
        write_u64(
            &mut overlap,
            directory + DirectoryEntry::LENGTH + 8,
            wire_length(InterfaceHeader::LENGTH),
        );
        assert_eq!(
            ValidatedPackageInterface::try_new(overlap, policy()),
            Err(InterfaceValidationError::Malformed)
        );

        let mut overflow = artifact(&[SectionFixture {
            tag: InterfaceSectionTag::Strings,
            record_count: 1,
            payload: b"aa",
        }]);
        let directory = overflow.len() - DirectoryEntry::LENGTH;
        write_u64(&mut overflow, directory + 8, u64::MAX);
        assert_eq!(
            ValidatedPackageInterface::try_new(overflow, policy()),
            Err(InterfaceValidationError::Malformed)
        );
    }

    #[test]
    fn file_section_record_and_allocation_limits_are_enforced() {
        let bytes = artifact(&[SectionFixture {
            tag: InterfaceSectionTag::Strings,
            record_count: 3,
            payload: b"payload",
        }]);

        assert_limit_error(
            &bytes,
            InterfaceValidationLimits::default().with_file_size(wire_length(bytes.len()) - 1),
            InterfaceLimit::FileSize,
            wire_length(bytes.len()),
            wire_length(bytes.len()) - 1,
        );
        assert_limit_error(
            &bytes,
            InterfaceValidationLimits::default().with_section_count(0),
            InterfaceLimit::SectionCount,
            1,
            0,
        );
        assert_limit_error(
            &bytes,
            InterfaceValidationLimits::default().with_records_per_section(2),
            InterfaceLimit::RecordCount,
            3,
            2,
        );
        assert_limit_error(
            &bytes,
            InterfaceValidationLimits::default().with_decoded_allocation(6),
            InterfaceLimit::DecodedAllocation,
            7,
            6,
        );
    }

    #[test]
    fn checksums_and_both_hashes_are_verified() {
        let bytes = artifact(&[SectionFixture {
            tag: InterfaceSectionTag::Strings,
            record_count: 1,
            payload: b"payload",
        }]);

        assert_mutation_error(
            &bytes,
            InterfaceHeader::LENGTH,
            b'P',
            InterfaceValidationError::SectionChecksumMismatch {
                section: InterfaceSectionTag::Strings,
            },
        );
        assert_mutation_error(
            &bytes,
            CONTENT_HASH_OFFSET,
            0xff,
            InterfaceValidationError::HashMismatch,
        );
        assert_mutation_error(
            &bytes,
            ARTIFACT_HASH_OFFSET,
            0xff,
            InterfaceValidationError::HashMismatch,
        );
    }

    #[test]
    fn provenance_changes_preserve_content_hash_but_change_artifact_hash() {
        let first = validate(artifact(&[SectionFixture {
            tag: InterfaceSectionTag::SourceProvenance,
            record_count: 1,
            payload: b"first.bray",
        }]));
        let second = validate(artifact(&[SectionFixture {
            tag: InterfaceSectionTag::SourceProvenance,
            record_count: 1,
            payload: b"other.bray",
        }]));

        assert_eq!(
            first.header().content_hash(),
            second.header().content_hash()
        );
        assert_ne!(
            first.header().artifact_hash(),
            second.header().artifact_hash()
        );
    }

    #[test]
    fn validation_errors_convert_to_localized_structured_diagnostics() {
        let error = InterfaceValidationError::ResourceLimitExceeded {
            limit: InterfaceLimit::RecordCount,
            actual: 12,
            maximum: 10,
        };
        let diagnostic = error.into_diagnostic(DiagnosticId::new(4));

        assert_eq!(diagnostic.id(), DiagnosticId::new(4));
        assert_eq!(
            diagnostic.kind(),
            DiagnosticKind::InterfaceResourceLimitExceeded
        );
        assert_eq!(
            diagnostic.args(),
            &[
                DiagnosticArg::new(
                    DiagnosticArgName::InterfaceLimit,
                    DiagnosticArgValue::InterfaceLimit(DiagnosticInterfaceLimit::RecordCount),
                ),
                DiagnosticArg::new(
                    DiagnosticArgName::ActualCount,
                    DiagnosticArgValue::Count(12),
                ),
                DiagnosticArg::new(
                    DiagnosticArgName::MaximumCount,
                    DiagnosticArgValue::Count(10),
                ),
            ]
        );
        assert_eq!(
            DiagnosticRenderer::english().render(&diagnostic).message(),
            "package interface exceeds the configured record count limit: 12; maximum 10"
        );

        let section = InterfaceValidationError::SectionChecksumMismatch {
            section: InterfaceSectionTag::Contracts,
        }
        .into_diagnostic(DiagnosticId::new(5));
        assert_eq!(
            DiagnosticRenderer::english().render(&section).message(),
            "package-interface section checksum does not match for contracts"
        );

        let revision = InterfaceValidationError::UnsupportedFormatRevision {
            actual: crate::InterfaceFormatRevision::new(9),
        }
        .into_diagnostic(DiagnosticId::new(6));
        assert_eq!(
            DiagnosticRenderer::english().render(&revision).message(),
            "unsupported package-interface format revision 9; expected 1"
        );
    }

    #[test]
    fn validated_interfaces_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<ValidatedPackageInterface>();
    }

    fn artifact(sections: &[SectionFixture<'_>]) -> Vec<u8> {
        let payload_length = sections
            .iter()
            .map(|section| section.payload.len())
            .sum::<usize>();
        let directory_offset = InterfaceHeader::LENGTH + payload_length;
        let directory_length = sections.len() * DirectoryEntry::LENGTH;
        let file_length = directory_offset + directory_length;

        let mut encoder = WireEncoder::new();
        encoder.write_bytes(&MAGIC);
        encoder.write_u16(CURRENT_FORMAT_REVISION.raw());
        encoder.write_u16(LANGUAGE_REVISION.raw());
        encoder.write_u32(BYTE_ORDER_MARKER);
        encoder.write_u64(0);
        encoder.write_u64(wire_length(file_length));
        encoder.write_u64(wire_length(directory_offset));
        encoder.write_u64(wire_length(directory_length));
        encoder.write_bytes(&[0; 32]);
        encoder.write_bytes(&[0; 32]);

        let mut entries = Vec::with_capacity(sections.len());
        let mut payload_offset = InterfaceHeader::LENGTH;
        for section in sections {
            let entry_without_checksum = DirectoryEntry::for_test(
                section.tag,
                wire_length(payload_offset),
                wire_length(section.payload.len()),
                section.record_count,
                crate::InterfaceSectionHash::from_bytes([0; 32]),
            );
            let checksum = compute_section_hash(&entry_without_checksum, section.payload);
            entries.push(DirectoryEntry::for_test(
                section.tag,
                wire_length(payload_offset),
                wire_length(section.payload.len()),
                section.record_count,
                checksum,
            ));
            encoder.write_bytes(section.payload);
            payload_offset += section.payload.len();
        }

        for entry in &entries {
            encoder.write_u32(entry.tag().wire_value());
            encoder.write_u32(0);
            encoder.write_u64(entry.offset());
            encoder.write_u64(entry.length());
            encoder.write_u64(entry.record_count());
            encoder.write_bytes(entry.checksum().as_bytes());
        }

        let mut bytes = encoder.into_bytes();
        let decoded = match InterfaceHeader::decode(&bytes) {
            Ok(decoded) => decoded,
            Err(error) => panic!("test header must decode: {error:?}"),
        };
        let content_hash = match compute_content_hash(&decoded.header, &entries, &bytes) {
            Some(hash) => hash,
            None => panic!("test content hash inputs must be valid"),
        };
        bytes[CONTENT_HASH_OFFSET..CONTENT_HASH_OFFSET + 32]
            .copy_from_slice(content_hash.as_bytes());
        let artifact_hash = match compute_artifact_hash(&bytes) {
            Some(hash) => hash,
            None => panic!("test artifact must include the artifact-hash field"),
        };
        bytes[ARTIFACT_HASH_OFFSET..ARTIFACT_HASH_OFFSET + 32]
            .copy_from_slice(artifact_hash.as_bytes());

        bytes
    }

    fn validate(bytes: Vec<u8>) -> ValidatedPackageInterface {
        match ValidatedPackageInterface::try_new(bytes, policy()) {
            Ok(interface) => interface,
            Err(error) => panic!("valid test artifact was rejected: {error:?}"),
        }
    }

    const fn policy() -> InterfaceValidationPolicy {
        InterfaceValidationPolicy::new(LANGUAGE_REVISION)
    }

    fn assert_mutation_error(
        bytes: &[u8],
        offset: usize,
        value: u8,
        expected: InterfaceValidationError,
    ) {
        let mut mutated = bytes.to_vec();
        mutated[offset] = value;

        assert_eq!(
            ValidatedPackageInterface::try_new(mutated, policy()),
            Err(expected)
        );
    }

    fn assert_limit_error(
        bytes: &[u8],
        limits: InterfaceValidationLimits,
        limit: InterfaceLimit,
        actual: u64,
        maximum: u64,
    ) {
        assert_eq!(
            ValidatedPackageInterface::try_new(bytes, policy().with_limits(limits)),
            Err(InterfaceValidationError::ResourceLimitExceeded {
                limit,
                actual,
                maximum,
            })
        );
    }

    fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn write_u64(bytes: &mut [u8], offset: usize, value: u64) {
        bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }

    fn wire_length(value: usize) -> u64 {
        match u64::try_from(value) {
            Ok(value) => value,
            Err(error) => panic!("test length must fit the wire format: {error:?}"),
        }
    }
}
