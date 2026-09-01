use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::{Arc, OnceLock};

use crate::diagnostic::{
    InterfaceMalformedCause, InterfaceValidationContext, InterfaceValidationError,
    InterfaceValidationField,
};
use crate::encoding::{decode_zstd_frame, validate_zstd_frame};
use crate::framing::{
    CheckedRangeError, allocate_items, available_length, checked_range, directory_entry_context,
    directory_field, header_field, invalid_index, malformed, map_wire_error, overlapping_range,
    range_validation_error, ranges_overlap, usize_to_u64_saturating,
};
use crate::hash::{
    compute_artifact_hash, compute_content_hash, compute_section_content_hash, compute_section_hash,
};
use crate::header::{BYTE_ORDER_MARKER, CURRENT_FORMAT_REVISION, InterfaceHeader, MAGIC};
use crate::limits::{InterfaceLimit, InterfaceValidationLimits, InterfaceValidationPolicy};
use crate::section::{DirectoryEntry, InterfaceSectionTag, ValidatedInterfaceSection};
use crate::{InterfaceSectionCompatibility, InterfaceSectionEncoding, InterfaceSectionRevision};

pub(crate) fn is_strictly_sorted<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

/// Immutable package-interface bytes with an eagerly validated structural envelope.
#[derive(Clone, Debug)]
pub struct ValidatedPackageInterface {
    bytes: Arc<[u8]>,
    header: InterfaceHeader,
    directory: Arc<[DirectoryEntry]>,
    decoded_sections: Arc<[OnceLock<Result<Option<Arc<[u8]>>, InterfaceValidationError>>]>,
    limits: InterfaceValidationLimits,
}

impl PartialEq for ValidatedPackageInterface {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes
            && self.header == other.header
            && self.directory == other.directory
            && self.limits == other.limits
    }
}

impl Eq for ValidatedPackageInterface {}

impl Hash for ValidatedPackageInterface {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.bytes.hash(state);
        self.header.hash(state);
        self.directory.hash(state);
        self.limits.hash(state);
    }
}

impl ValidatedPackageInterface {
    /// Validates untrusted bytes under an exact compatibility and resource policy.
    pub fn try_new(
        bytes: impl Into<Arc<[u8]>>,
        policy: InterfaceValidationPolicy,
    ) -> Result<Self, InterfaceValidationError> {
        let bytes = bytes.into();

        validate_file_size(bytes.len(), policy)?;

        let decoded = InterfaceHeader::decode(&bytes).map_err(|error| {
            map_wire_error(
                InterfaceValidationContext::Header,
                header_field(error.offset()),
                error,
            )
        })?;

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
        .map_err(|error| {
            range_validation_error(
                InterfaceValidationContext::Directory,
                InterfaceValidationField::DirectoryLength,
                error,
            )
        })?;

        let directory = decode_directory(&bytes, directory_range, policy)?;

        validate_hashes(decoded.header, &directory, &bytes)?;

        let mut decoded_sections = allocate_items(
            InterfaceValidationContext::Directory,
            InterfaceValidationField::RecordCount,
            directory.len(),
        )?;

        decoded_sections.extend(std::iter::repeat_with(OnceLock::new).take(directory.len()));

        Ok(Self {
            bytes,
            header: decoded.header,
            directory: directory.into(),
            decoded_sections: decoded_sections.into(),
            limits: policy.limits(),
        })
    }

    /// Returns the validated fixed header.
    pub const fn header(&self) -> InterfaceHeader {
        self.header
    }

    /// Returns the exact validated artifact byte length.
    pub const fn byte_len(&self) -> u64 {
        self.header.declared_file_length()
    }

    /// Returns the total number of validated sections.
    pub fn section_count(&self) -> usize {
        self.directory.len()
    }

    /// Returns one validated section by its stable category.
    pub fn section(
        &self,
        tag: InterfaceSectionTag,
    ) -> Result<Option<ValidatedInterfaceSection<'_>>, InterfaceValidationError> {
        let Ok(index) = self
            .directory
            .binary_search_by_key(&tag.wire_value(), |entry| entry.raw_tag())
        else {
            return Ok(None);
        };

        let entry = self
            .directory
            .get(index)
            .ok_or_else(|| invalid_index(InterfaceValidationContext::Directory, index, 0))?;

        if entry.tag() != Some(tag) {
            return Ok(None);
        }

        self.section_by_index(index).map(Some)
    }

    /// Decodes every known section and returns views in canonical tag order.
    pub fn sections(&self) -> Result<Vec<ValidatedInterfaceSection<'_>>, InterfaceValidationError> {
        let mut sections = allocate_items(
            InterfaceValidationContext::Directory,
            InterfaceValidationField::RecordCount,
            self.directory.len(),
        )?;

        for (index, entry) in self.directory.iter().enumerate() {
            if entry.tag().is_some() {
                sections.push(self.section_by_index(index)?);
            }
        }

        Ok(sections)
    }

    fn sections_by_tag(
        &self,
        tags: &[InterfaceSectionTag],
    ) -> Result<Vec<ValidatedInterfaceSection<'_>>, InterfaceValidationError> {
        tags.iter()
            .map(|tag| {
                self.section(*tag)?.ok_or_else(|| {
                    malformed(
                        InterfaceValidationContext::Section(*tag),
                        InterfaceMalformedCause::Missing {
                            field: InterfaceValidationField::SectionTag,
                        },
                    )
                })
            })
            .collect()
    }

    fn section_by_index(
        &self,
        index: usize,
    ) -> Result<ValidatedInterfaceSection<'_>, InterfaceValidationError> {
        let entry = *self.directory.get(index).ok_or_else(|| {
            invalid_index(
                InterfaceValidationContext::Directory,
                index,
                self.directory.len(),
            )
        })?;

        let stored = entry.payload(&self.bytes).ok_or_else(|| {
            range_validation_error(
                directory_entry_context(index, entry.raw_tag()),
                InterfaceValidationField::EncodedLength,
                CheckedRangeError::OutOfBounds {
                    offset: entry.offset(),
                    length: entry.encoded_length(),
                    available: available_length(entry.offset(), self.bytes.len()),
                },
            )
        })?;

        let decoded = self
            .decoded_sections
            .get(index)
            .ok_or_else(|| {
                invalid_index(
                    InterfaceValidationContext::Directory,
                    index,
                    self.decoded_sections.len(),
                )
            })?
            .get_or_init(|| decode_and_verify_section(index, entry, stored));

        let bytes = decoded
            .as_ref()
            .map_err(Clone::clone)?
            .as_deref()
            .unwrap_or(stored);

        ValidatedInterfaceSection::new(entry, bytes).ok_or_else(|| {
            malformed(
                directory_entry_context(index, entry.raw_tag()),
                InterfaceMalformedCause::InvalidValue {
                    field: InterfaceValidationField::SectionEncoding,
                },
            )
        })
    }

    /// Decodes and validates the eager package identity and symbol-surface sections.
    pub fn decode_identity_surface(
        &self,
    ) -> Result<crate::PackageInterfaceSurface, InterfaceValidationError> {
        crate::surface::decode_surface(self, self.limits)
    }

    /// Decodes and validates semantics against an already decoded identity surface.
    pub fn decode_semantics(
        &self,
        surface: &crate::PackageInterfaceSurface,
    ) -> Result<crate::InterfaceSemantics, InterfaceValidationError> {
        let sections = self.sections_by_tag(crate::semantic::COMPLETE_SEMANTIC_SECTIONS)?;

        crate::decode_semantics(&sections, surface, self.limits)
    }

    /// Decodes the semantic dependency graph required by one symbol-owned record category.
    pub fn decode_semantic_graph(
        &self,
        surface: &crate::PackageInterfaceSurface,
        owner: bray_symbols::InterfaceSymbolId,
        kind: crate::InterfaceSemanticRecordKind,
    ) -> Result<crate::InterfaceSemantics, InterfaceValidationError> {
        if let Some(semantics) = self.decode_selected_semantic_graph(surface, owner, kind)? {
            return Ok(semantics);
        }

        let sections = self.sections_by_tag(crate::semantic::COMPLETE_SEMANTIC_SECTIONS)?;

        crate::semantic::decode_semantic_graph(&sections, surface, owner, kind, self.limits)
    }

    /// Decodes the independently addressable semantic dependency graph for one symbol-owned record.
    ///
    /// Returns `None` when the record category requires the complete semantic graph.
    pub fn decode_selected_semantic_graph(
        &self,
        surface: &crate::PackageInterfaceSurface,
        owner: bray_symbols::InterfaceSymbolId,
        kind: crate::InterfaceSemanticRecordKind,
    ) -> Result<Option<crate::InterfaceSemantics>, InterfaceValidationError> {
        let Some(tags) = crate::semantic::selected_semantic_sections(kind) else {
            return Ok(None);
        };

        let sections = self.sections_by_tag(tags)?;

        crate::semantic::decode_selected_semantic_graph(
            &sections,
            surface,
            owner,
            kind,
            self.limits,
        )
    }

    /// Decodes and validates the complete semantic closure of this interface.
    pub fn validate_complete(&self) -> Result<(), InterfaceValidationError> {
        let surface = self.decode_identity_surface()?;

        self.decode_semantics(&surface)?;

        Ok(())
    }

    pub(crate) const fn limits(&self) -> InterfaceValidationLimits {
        self.limits
    }
}

fn decode_and_verify_section(
    index: usize,
    entry: DirectoryEntry,
    stored: &[u8],
) -> Result<Option<Arc<[u8]>>, InterfaceValidationError> {
    let context = directory_entry_context(index, entry.raw_tag());
    let encoding = entry.encoding().ok_or_else(|| {
        malformed(
            context,
            InterfaceMalformedCause::InvalidDiscriminant {
                field: InterfaceValidationField::SectionEncoding,
                actual: u64::from(entry.raw_encoding()),
            },
        )
    })?;

    let decoded = match encoding {
        InterfaceSectionEncoding::Raw => None,
        InterfaceSectionEncoding::ZstdFrame => {
            Some(decode_zstd_frame(context, stored, entry.decoded_length())?)
        }
    };

    let bytes = decoded.as_deref().unwrap_or(stored);
    let tag = entry.tag().ok_or_else(|| {
        malformed(
            context,
            InterfaceMalformedCause::InvalidDiscriminant {
                field: InterfaceValidationField::SectionTag,
                actual: u64::from(entry.raw_tag()),
            },
        )
    })?;

    let actual = compute_section_content_hash(tag, bytes);

    if actual != entry.content_hash() {
        return Err(InterfaceValidationError::SectionContentHashMismatch {
            section: tag,
            expected: *entry.content_hash().as_bytes(),
            actual: *actual.as_bytes(),
        });
    }

    Ok(decoded)
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
        return Err(InterfaceValidationError::InvalidMagic { actual: magic });
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

    if byte_order_marker != BYTE_ORDER_MARKER {
        return Err(InterfaceValidationError::UnsupportedByteOrder {
            expected: BYTE_ORDER_MARKER,
            actual: byte_order_marker,
        });
    }

    if header.required_flags() != crate::InterfaceRequiredFlags::NONE {
        return Err(InterfaceValidationError::UnsupportedRequiredFlags {
            actual: header.required_flags(),
        });
    }

    let actual_length = usize_to_u64_saturating(bytes.len());

    if header.declared_file_length() > actual_length {
        return Err(InterfaceValidationError::Truncated {
            context: InterfaceValidationContext::Artifact,
            field: InterfaceValidationField::DeclaredFileLength,
            offset: actual_length,
            expected_length: header.declared_file_length() - actual_length,
            actual_length: 0,
        });
    }

    if header.declared_file_length() != actual_length {
        return Err(InterfaceValidationError::TrailingBytes {
            context: InterfaceValidationContext::Artifact,
            offset: header.declared_file_length(),
            count: actual_length - header.declared_file_length(),
        });
    }

    if !header
        .directory_length()
        .is_multiple_of(DirectoryEntry::WIRE_LENGTH)
    {
        return Err(malformed(
            InterfaceValidationContext::Header,
            InterfaceMalformedCause::InvalidAlignment {
                field: InterfaceValidationField::DirectoryLength,
                value: header.directory_length(),
                alignment: DirectoryEntry::WIRE_LENGTH,
            },
        ));
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

    let mut entries = allocate_items(
        InterfaceValidationContext::Directory,
        InterfaceValidationField::RecordCount,
        capacity,
    )?;

    let mut previous_tag = None;
    let mut previous_range = None;

    let mut decoded_allocation = 0_u64;

    for (index, chunk) in bytes[directory_range.clone()]
        .chunks_exact(DirectoryEntry::LENGTH)
        .enumerate()
    {
        let raw_tag = chunk
            .get(..4)
            .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
            .map(u32::from_le_bytes)
            .unwrap_or(0);

        let context = directory_entry_context(index, raw_tag);
        let decoded = DirectoryEntry::decode(chunk)
            .map_err(|error| map_wire_error(context, directory_field(error.offset()), error))?;
        let tag = InterfaceSectionTag::from_wire_value(decoded.raw_tag);

        validate_section_contract(context, tag, decoded)?;

        if previous_tag.is_some_and(|previous| decoded.raw_tag <= previous) {
            return Err(malformed(
                context,
                InterfaceMalformedCause::OrderingViolation {
                    field: InterfaceValidationField::SectionTag,
                    previous: u64::from(previous_tag.unwrap_or(decoded.raw_tag)),
                    actual: u64::from(decoded.raw_tag),
                },
            ));
        }

        policy
            .limits()
            .check(InterfaceLimit::RecordCount, decoded.record_count)?;

        decoded_allocation = decoded_allocation
            .checked_add(decoded.decoded_length)
            .ok_or_else(|| InterfaceValidationError::ResourceLimitExceeded {
                limit: InterfaceLimit::DecodedAllocation,
                actual: u64::MAX,
                maximum: policy.limits().maximum(InterfaceLimit::DecodedAllocation),
            })?;

        policy
            .limits()
            .check(InterfaceLimit::DecodedAllocation, decoded_allocation)?;

        let payload_range = checked_range(decoded.offset, decoded.encoded_length, bytes.len())
            .map_err(|error| {
                range_validation_error(context, InterfaceValidationField::EncodedLength, error)
            })?;

        let header_range = 0..InterfaceHeader::LENGTH;

        if ranges_overlap(&payload_range, &header_range) {
            return Err(overlapping_range(context, &payload_range, &header_range));
        }

        if ranges_overlap(&payload_range, &directory_range) {
            return Err(overlapping_range(context, &payload_range, &directory_range));
        }

        if let Some(previous_range) = &previous_range
            && ranges_overlap(&payload_range, previous_range)
        {
            return Err(overlapping_range(context, &payload_range, previous_range));
        }

        let entry = DirectoryEntry::from_decoded(decoded);

        if tag.is_some() && entry.encoding() == Some(InterfaceSectionEncoding::ZstdFrame) {
            let payload = bytes.get(payload_range.clone()).ok_or_else(|| {
                range_validation_error(
                    context,
                    InterfaceValidationField::EncodedLength,
                    CheckedRangeError::OutOfBounds {
                        offset: decoded.offset,
                        length: decoded.encoded_length,
                        available: available_length(decoded.offset, bytes.len()),
                    },
                )
            })?;

            validate_zstd_frame(context, payload, entry.decoded_length())?;
        }

        previous_tag = Some(decoded.raw_tag);
        previous_range = Some(payload_range);

        entries.push(entry);
    }

    Ok(entries)
}

fn validate_hashes(
    header: InterfaceHeader,
    directory: &[DirectoryEntry],
    bytes: &[u8],
) -> Result<(), InterfaceValidationError> {
    for (index, entry) in directory.iter().enumerate() {
        let context = directory_entry_context(index, entry.raw_tag());
        let payload = entry.payload(bytes).ok_or_else(|| {
            range_validation_error(
                context,
                InterfaceValidationField::EncodedLength,
                CheckedRangeError::OutOfBounds {
                    offset: entry.offset(),
                    length: entry.encoded_length(),
                    available: available_length(entry.offset(), bytes.len()),
                },
            )
        })?;

        let actual = compute_section_hash(entry, payload);

        if actual != entry.checksum() {
            return match entry.tag() {
                Some(section) => Err(InterfaceValidationError::SectionChecksumMismatch {
                    section,
                    expected: entry.checksum(),
                    actual,
                }),
                None => Err(InterfaceValidationError::UnknownSectionChecksumMismatch {
                    raw_tag: entry.raw_tag(),
                    expected: entry.checksum(),
                    actual,
                }),
            };
        }
    }

    let artifact_hash =
        compute_artifact_hash(bytes).ok_or(InterfaceValidationError::DigestUnavailable {
            context: InterfaceValidationContext::Artifact,
            field: InterfaceValidationField::ArtifactHash,
        })?;

    if artifact_hash != header.artifact_hash() {
        return Err(InterfaceValidationError::ArtifactHashMismatch {
            expected: header.artifact_hash(),
            actual: artifact_hash,
        });
    }

    let content_hash = compute_content_hash(
        &header,
        directory.iter().filter_map(|entry| {
            entry
                .tag()
                .map(|tag| (tag, entry.decoded_length(), entry.content_hash()))
        }),
    );

    if content_hash != header.content_hash() {
        return Err(InterfaceValidationError::ContentHashMismatch {
            expected: header.content_hash(),
            actual: content_hash,
        });
    }

    Ok(())
}

fn validate_section_contract(
    context: InterfaceValidationContext,
    tag: Option<InterfaceSectionTag>,
    decoded: crate::section::DecodedDirectoryEntry,
) -> Result<(), InterfaceValidationError> {
    let Some(compatibility) =
        InterfaceSectionCompatibility::from_wire_value(decoded.raw_compatibility)
    else {
        return Err(malformed(
            context,
            InterfaceMalformedCause::InvalidDiscriminant {
                field: InterfaceValidationField::SectionCompatibility,
                actual: u64::from(decoded.raw_compatibility),
            },
        ));
    };

    let Some(tag) = tag else {
        if compatibility.is_optional() {
            return Ok(());
        }

        return Err(malformed(
            context,
            InterfaceMalformedCause::InvalidDiscriminant {
                field: InterfaceValidationField::SectionTag,
                actual: u64::from(decoded.raw_tag),
            },
        ));
    };

    if compatibility != tag.compatibility() {
        return Err(malformed(
            context,
            InterfaceMalformedCause::ValueMismatch {
                field: InterfaceValidationField::SectionCompatibility,
                expected: u64::from(tag.compatibility().wire_value()),
                actual: u64::from(decoded.raw_compatibility),
            },
        ));
    }

    let encoding = InterfaceSectionEncoding::from_wire_value(decoded.raw_encoding);

    if decoded.section_revision != InterfaceSectionRevision::CURRENT {
        if compatibility.is_optional() {
            return Ok(());
        }

        return Err(malformed(
            context,
            InterfaceMalformedCause::ValueMismatch {
                field: InterfaceValidationField::SectionRevision,
                expected: u64::from(InterfaceSectionRevision::CURRENT.raw()),
                actual: u64::from(decoded.section_revision.raw()),
            },
        ));
    }

    if encoding.is_none() {
        if compatibility.is_optional() {
            return Ok(());
        }

        return Err(malformed(
            context,
            InterfaceMalformedCause::InvalidDiscriminant {
                field: InterfaceValidationField::SectionEncoding,
                actual: u64::from(decoded.raw_encoding),
            },
        ));
    }

    if matches!(encoding, Some(InterfaceSectionEncoding::Raw))
        && decoded.encoded_length != decoded.decoded_length
    {
        return Err(malformed(
            context,
            InterfaceMalformedCause::LengthMismatch {
                field: InterfaceValidationField::DecodedLength,
                expected: decoded.encoded_length,
                actual: decoded.decoded_length,
            },
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        DiagnosticArg, DiagnosticId, DiagnosticInterfaceLimit,
        DiagnosticInterfaceValidationFailure, DiagnosticKind,
    };
    use bray_messages::DiagnosticRenderer;
    use bray_symbols::PackageIdentity;

    use super::ValidatedPackageInterface;
    use crate::artifact::{EncodedArtifactSection, assemble_sections};
    use crate::diagnostic::{
        InterfaceMalformedCause, InterfaceValidationContext, InterfaceValidationError,
        InterfaceValidationField,
    };
    use crate::framing::header_field;
    use crate::hash::{compute_artifact_hash, compute_section_hash};
    use crate::header::{BYTE_ORDER_MARKER, InterfaceHeader, MAGIC};
    use crate::limits::{InterfaceLimit, InterfaceValidationLimits, InterfaceValidationPolicy};
    use crate::section::{DirectoryEntry, InterfaceSectionRevision, InterfaceSectionTag};
    use crate::test_support::{package_interface_export_bundle, package_version};
    use crate::{
        CURRENT_FORMAT_REVISION, InterfaceLanguageRevision, InterfaceProductIdentity,
        InterfaceProductKind, InterfaceSectionCompatibility, InterfaceSectionEncoding,
        PackageInterfaceIdentity,
    };

    const LANGUAGE_REVISION: InterfaceLanguageRevision = InterfaceLanguageRevision::new(7);

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

        let Some(strings) = interface
            .section(InterfaceSectionTag::Strings)
            .unwrap_or_else(|error| panic!("validated section must decode: {error:?}"))
        else {
            panic!("validated string section must exist");
        };

        assert_eq!(strings.tag(), InterfaceSectionTag::Strings);
        assert_eq!(strings.revision(), crate::InterfaceSectionRevision::CURRENT);
        assert_eq!(strings.encoding(), crate::InterfaceSectionEncoding::Raw);
        assert_eq!(strings.record_count(), 2);
        assert_eq!(strings.bytes(), b"alpha beta");

        assert_eq!(
            interface
                .sections()
                .unwrap_or_else(|error| panic!("validated sections must decode: {error:?}"))
                .len(),
            2
        );

        assert_eq!(interface.section(InterfaceSectionTag::Contracts), Ok(None));
    }

    #[test]
    fn every_truncated_header_boundary_is_rejected_without_panicking() {
        let bytes = artifact(&[]);

        for length in 0..InterfaceHeader::LENGTH {
            let error = ValidatedPackageInterface::try_new(&bytes[..length], policy())
                .expect_err("truncated header prefix must fail validation");

            let InterfaceValidationError::Truncated {
                context,
                field,
                offset,
                expected_length,
                actual_length,
            } = error
            else {
                panic!("unexpected result for prefix length {length}: {error:?}")
            };

            assert_eq!(context, InterfaceValidationContext::Header);
            assert_eq!(field, header_field(offset_usize(offset)));
            assert_eq!(offset + actual_length, wire_length(length));
            assert!(actual_length < expected_length);
        }
    }

    #[test]
    fn header_identity_and_compatibility_are_exact() {
        let bytes = artifact(&[]);
        let unsupported_revision = unsupported_format_revision();

        let unsupported_revision_byte = u8::try_from(unsupported_revision.raw())
            .unwrap_or_else(|error| panic!("test format revision must fit one byte: {error}"));

        let mut invalid_magic = MAGIC;
        invalid_magic[0] = 0;

        assert_mutation_error(
            &bytes,
            0,
            0,
            InterfaceValidationError::InvalidMagic {
                actual: invalid_magic,
            },
        );

        assert_mutation_error(
            &bytes,
            8,
            unsupported_revision_byte,
            InterfaceValidationError::UnsupportedFormatRevision {
                actual: unsupported_revision,
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

        assert_mutation_error(
            &bytes,
            12,
            0,
            InterfaceValidationError::UnsupportedByteOrder {
                expected: BYTE_ORDER_MARKER,
                actual: BYTE_ORDER_MARKER & !0xff,
            },
        );

        assert_mutation_error(
            &bytes,
            16,
            1,
            InterfaceValidationError::UnsupportedRequiredFlags {
                actual: crate::InterfaceRequiredFlags::from_bits(1),
            },
        );
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
            Err(InterfaceValidationError::Truncated {
                context: InterfaceValidationContext::Artifact,
                field: InterfaceValidationField::DeclaredFileLength,
                offset: wire_length(bytes.len()),
                expected_length: 1,
                actual_length: 0,
            })
        );

        let mut trailing = bytes.clone();

        write_u64(&mut trailing, 24, wire_length(bytes.len()) - 1);

        assert_eq!(
            ValidatedPackageInterface::try_new(trailing, policy()),
            Err(InterfaceValidationError::TrailingBytes {
                context: InterfaceValidationContext::Artifact,
                offset: wire_length(bytes.len()) - 1,
                count: 1,
            })
        );

        let mut invalid_directory_length = bytes.clone();

        write_u64(&mut invalid_directory_length, 40, 1);

        assert_eq!(
            ValidatedPackageInterface::try_new(invalid_directory_length, policy()),
            Err(InterfaceValidationError::Malformed {
                context: InterfaceValidationContext::Header,
                cause: InterfaceMalformedCause::InvalidAlignment {
                    field: InterfaceValidationField::DirectoryLength,
                    value: 1,
                    alignment: DirectoryEntry::WIRE_LENGTH,
                },
            })
        );

        let mut overflowing_directory = bytes;

        write_u64(&mut overflowing_directory, 32, u64::MAX);

        assert_eq!(
            ValidatedPackageInterface::try_new(overflowing_directory, policy()),
            Err(InterfaceValidationError::Malformed {
                context: InterfaceValidationContext::Directory,
                cause: InterfaceMalformedCause::RangeOverflow {
                    offset: u64::MAX,
                    length: DirectoryEntry::WIRE_LENGTH,
                },
            })
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
            Err(InterfaceValidationError::Malformed {
                context: InterfaceValidationContext::DirectoryEntry {
                    index: 0,
                    raw_tag: 99,
                },
                cause: InterfaceMalformedCause::InvalidDiscriminant {
                    field: InterfaceValidationField::SectionTag,
                    actual: 99,
                },
            })
        );

        let mut encoded_section = artifact(&[SectionFixture {
            tag: InterfaceSectionTag::Strings,
            record_count: 1,
            payload: b"a",
        }]);

        let directory = encoded_section.len() - DirectoryEntry::LENGTH;

        encoded_section[directory + 7] = u8::MAX;

        assert_eq!(
            ValidatedPackageInterface::try_new(encoded_section, policy()),
            Err(InterfaceValidationError::Malformed {
                context: InterfaceValidationContext::DirectoryEntry {
                    index: 0,
                    raw_tag: InterfaceSectionTag::Strings.wire_value(),
                },
                cause: InterfaceMalformedCause::InvalidDiscriminant {
                    field: InterfaceValidationField::SectionEncoding,
                    actual: u64::from(u8::MAX),
                },
            })
        );

        let mut revised_section = artifact(&[SectionFixture {
            tag: InterfaceSectionTag::Strings,
            record_count: 1,
            payload: b"a",
        }]);

        let directory = revised_section.len() - DirectoryEntry::LENGTH;

        write_u16(&mut revised_section, directory + 4, u16::MAX);

        assert_eq!(
            ValidatedPackageInterface::try_new(revised_section, policy()),
            Err(InterfaceValidationError::Malformed {
                context: InterfaceValidationContext::DirectoryEntry {
                    index: 0,
                    raw_tag: InterfaceSectionTag::Strings.wire_value(),
                },
                cause: InterfaceMalformedCause::ValueMismatch {
                    field: InterfaceValidationField::SectionRevision,
                    expected: u64::from(InterfaceSectionRevision::CURRENT.raw()),
                    actual: u64::from(u16::MAX),
                },
            })
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
            Err(InterfaceValidationError::Malformed {
                context: InterfaceValidationContext::DirectoryEntry {
                    index: 1,
                    raw_tag: InterfaceSectionTag::Strings.wire_value(),
                },
                cause: InterfaceMalformedCause::OrderingViolation {
                    field: InterfaceValidationField::SectionTag,
                    previous: u64::from(InterfaceSectionTag::Strings.wire_value()),
                    actual: u64::from(InterfaceSectionTag::Strings.wire_value()),
                },
            })
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
            Err(InterfaceValidationError::Malformed {
                context: InterfaceValidationContext::DirectoryEntry {
                    index: 1,
                    raw_tag: InterfaceSectionTag::Dependencies.wire_value(),
                },
                cause: InterfaceMalformedCause::RangeOverlap {
                    offset: wire_length(InterfaceHeader::LENGTH),
                    length: 2,
                    conflicting_offset: wire_length(InterfaceHeader::LENGTH),
                    conflicting_length: 2,
                },
            })
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
            Err(InterfaceValidationError::Malformed {
                context: InterfaceValidationContext::DirectoryEntry {
                    index: 0,
                    raw_tag: InterfaceSectionTag::Strings.wire_value(),
                },
                cause: InterfaceMalformedCause::RangeOverflow {
                    offset: u64::MAX,
                    length: 2,
                },
            })
        );
    }

    #[test]
    fn explicitly_optional_unknown_sections_are_validated_and_ignored() {
        let bytes = artifact(&[SectionFixture {
            tag: InterfaceSectionTag::SourceProvenance,
            record_count: 1,
            payload: b"future tooling data",
        }]);

        let content_hash = validate(bytes.clone()).header().content_hash();

        let extension = optional_extension(
            bytes.clone(),
            99,
            InterfaceSectionCompatibility::Discardable,
        );

        let interface = validate(extension.clone());

        assert_eq!(interface.header().content_hash(), content_hash);

        assert_eq!(
            interface
                .sections()
                .unwrap_or_else(|error| panic!(
                    "optional sections must remain skippable: {error:?}"
                ))
                .len(),
            0
        );

        assert_eq!(
            interface.section(InterfaceSectionTag::SourceProvenance),
            Ok(None)
        );

        let preserved = optional_extension(
            bytes.clone(),
            99,
            InterfaceSectionCompatibility::PreserveOpaque,
        );

        assert_eq!(validate(preserved).section_count(), 1);

        let future_provenance = optional_extension(
            bytes,
            InterfaceSectionTag::SourceProvenance.wire_value(),
            InterfaceSectionCompatibility::Discardable,
        );

        let interface = validate(future_provenance);

        assert_eq!(
            interface.section(InterfaceSectionTag::SourceProvenance),
            Ok(None)
        );

        let mut required = extension.clone();
        let directory = required.len() - DirectoryEntry::LENGTH;

        required[directory + 6] = 0;

        assert_eq!(
            ValidatedPackageInterface::try_new(required, policy()),
            Err(InterfaceValidationError::Malformed {
                context: InterfaceValidationContext::DirectoryEntry {
                    index: 0,
                    raw_tag: 99,
                },
                cause: InterfaceMalformedCause::InvalidDiscriminant {
                    field: InterfaceValidationField::SectionTag,
                    actual: 99,
                },
            })
        );

        let mut corrupted = extension;

        corrupted[InterfaceHeader::LENGTH] ^= 0xff;

        let error = ValidatedPackageInterface::try_new(corrupted, policy())
            .expect_err("corrupted optional section must fail its exact checksum");

        assert_unknown_checksum_mismatch(error, 99);
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
    fn compressed_decoded_lengths_are_bounded_before_frame_decoding() {
        let mut bytes = artifact(&[SectionFixture {
            tag: InterfaceSectionTag::Strings,
            record_count: 1,
            payload: &[0; 4096],
        }]);

        let directory = bytes.len() - DirectoryEntry::LENGTH;

        write_u64(&mut bytes, directory + 24, 4097);

        assert_limit_error(
            &bytes,
            InterfaceValidationLimits::default().with_decoded_allocation(4096),
            InterfaceLimit::DecodedAllocation,
            4097,
            4096,
        );
    }

    #[test]
    fn compressed_sections_are_decoded_once_on_first_request() {
        let bytes = artifact(&[SectionFixture {
            tag: InterfaceSectionTag::Strings,
            record_count: 1,
            payload: &[0; 4096],
        }]);

        let interface = validate(bytes);

        assert!(interface.decoded_sections[0].get().is_none());

        let first = interface
            .section(InterfaceSectionTag::Strings)
            .unwrap_or_else(|error| panic!("compressed section must decode: {error:?}"))
            .unwrap_or_else(|| panic!("compressed section must be present"));

        assert!(interface.decoded_sections[0].get().is_some());

        let second = interface
            .section(InterfaceSectionTag::Strings)
            .unwrap_or_else(|error| panic!("cached section must decode: {error:?}"))
            .unwrap_or_else(|| panic!("cached section must be present"));

        assert!(std::ptr::eq(first.bytes(), second.bytes()));
    }

    #[test]
    fn content_identity_is_verified_without_decoding_sections() {
        let mut bytes = artifact(&[SectionFixture {
            tag: InterfaceSectionTag::Strings,
            record_count: 1,
            payload: b"payload",
        }]);

        bytes[InterfaceHeader::CONTENT_HASH_OFFSET] ^= 0xff;

        rewrite_artifact_hash(&mut bytes);

        let error = ValidatedPackageInterface::try_new(bytes, policy())
            .expect_err("corrupted content identity must fail validation");

        assert!(matches!(
            error,
            InterfaceValidationError::ContentHashMismatch { .. }
        ));
    }

    #[test]
    fn requested_sections_must_match_their_committed_decoded_content() {
        let mut bytes = artifact(&[SectionFixture {
            tag: InterfaceSectionTag::Strings,
            record_count: 1,
            payload: b"payload",
        }]);

        bytes[InterfaceHeader::LENGTH] ^= 0xff;

        let directory = bytes.len() - DirectoryEntry::LENGTH;

        rewrite_section_checksum(&mut bytes, directory);
        rewrite_artifact_hash(&mut bytes);

        let interface = validate(bytes);

        assert!(interface.decoded_sections[0].get().is_none());

        let error = interface
            .section(InterfaceSectionTag::Strings)
            .expect_err("mutated decoded section must fail its content hash");

        assert!(matches!(
            error,
            InterfaceValidationError::SectionContentHashMismatch {
                section: InterfaceSectionTag::Strings,
                ..
            }
        ));

        assert!(interface.decoded_sections[0].get().is_some());
    }

    #[test]
    fn semantic_decoding_does_not_request_optional_provenance() {
        let bundle = package_interface_export_bundle();

        let mut sections = crate::surface::encode_surface(bundle.surface())
            .into_iter()
            .map(EncodedArtifactSection::from_surface)
            .chain(
                crate::semantic::encode_validated_semantics(bundle.semantics())
                    .into_iter()
                    .map(EncodedArtifactSection::from_semantic),
            )
            .collect::<Vec<_>>();

        let provenance = sections
            .iter_mut()
            .find(|section| section.tag() == InterfaceSectionTag::SourceProvenance)
            .unwrap_or_else(|| panic!("encoded semantics must contain provenance"));

        *provenance.payload_mut() = vec![0; 4096];

        sections.sort_by_key(EncodedArtifactSection::tag);

        let language_revision = bundle.language_revision();

        let bytes = assemble_sections(
            &sections,
            bundle.surface().identity().clone(),
            language_revision,
        )
        .map(|artifact| artifact.bytes().to_vec())
        .unwrap_or_else(|error| panic!("test artifact must encode: {error:?}"));

        let interface = ValidatedPackageInterface::try_new(
            bytes,
            InterfaceValidationPolicy::new(language_revision),
        )
        .unwrap_or_else(|error| panic!("valid test artifact was rejected: {error:?}"));

        let provenance = interface
            .directory
            .binary_search_by_key(
                &InterfaceSectionTag::SourceProvenance.wire_value(),
                |entry| entry.raw_tag(),
            )
            .unwrap_or_else(|_| panic!("provenance directory entry must exist"));

        assert_eq!(
            interface.directory[provenance].encoding(),
            Some(InterfaceSectionEncoding::ZstdFrame)
        );

        let surface = interface
            .decode_identity_surface()
            .unwrap_or_else(|error| panic!("identity surface must decode: {error:?}"));

        interface
            .decode_semantics(&surface)
            .unwrap_or_else(|error| panic!("semantics must decode: {error:?}"));

        assert!(interface.decoded_sections[provenance].get().is_none());
    }

    #[test]
    fn checksums_and_both_hashes_are_verified() {
        let bytes = artifact(&[SectionFixture {
            tag: InterfaceSectionTag::Strings,
            record_count: 1,
            payload: b"payload",
        }]);

        let mut corrupted_section = bytes.clone();
        corrupted_section[InterfaceHeader::LENGTH] = b'P';

        let error = ValidatedPackageInterface::try_new(corrupted_section, policy())
            .expect_err("mutated section bytes must fail their exact checksum");

        assert!(matches!(
            error,
            InterfaceValidationError::SectionChecksumMismatch {
                section: InterfaceSectionTag::Strings,
                ..
            }
        ));

        for offset in [
            InterfaceHeader::CONTENT_HASH_OFFSET,
            InterfaceHeader::ARTIFACT_HASH_OFFSET,
        ] {
            let mut corrupted_hash = bytes.clone();
            corrupted_hash[offset] = 0xff;

            let error = ValidatedPackageInterface::try_new(corrupted_hash, policy())
                .expect_err("mutated artifact identity must fail its exact hash");

            assert!(matches!(
                error,
                InterfaceValidationError::ArtifactHashMismatch { .. }
            ));
        }
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
            &[DiagnosticArg::interface_validation_failure(
                DiagnosticInterfaceValidationFailure::ResourceLimitExceeded {
                    limit: DiagnosticInterfaceLimit::RecordCount,
                    actual: 12,
                    maximum: 10,
                }
            )]
        );

        assert_eq!(
            DiagnosticRenderer::english().render(&diagnostic).message(),
            "package interface requires record count of 12, but the configured maximum is 10"
        );

        let section = InterfaceValidationError::SectionChecksumMismatch {
            section: InterfaceSectionTag::Contracts,
            expected: crate::InterfaceSectionHash::from_bytes([0; 32]),
            actual: crate::InterfaceSectionHash::from_bytes([1; 32]),
        }
        .into_diagnostic(DiagnosticId::new(5));

        let expected = format!(
            "package-interface contracts section declares checksum BLAKE3 {}, but its bytes have checksum BLAKE3 {}",
            "00".repeat(32),
            "01".repeat(32),
        );

        assert_eq!(
            DiagnosticRenderer::english().render(&section).message(),
            expected
        );

        let unsupported_revision = unsupported_format_revision();

        let revision = InterfaceValidationError::UnsupportedFormatRevision {
            actual: unsupported_revision,
        }
        .into_diagnostic(DiagnosticId::new(6));

        let expected = format!(
            "package interface uses format revision {}, but this compiler requires {}",
            unsupported_revision.raw(),
            CURRENT_FORMAT_REVISION.raw()
        );

        assert_eq!(
            DiagnosticRenderer::english().render(&revision).message(),
            expected
        );
    }

    #[test]
    fn validated_interfaces_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<ValidatedPackageInterface>();
    }

    fn artifact(sections: &[SectionFixture<'_>]) -> Vec<u8> {
        let sections = sections
            .iter()
            .map(|section| {
                EncodedArtifactSection::new(
                    section.tag,
                    section.record_count,
                    section.payload.to_vec(),
                )
            })
            .collect::<Vec<_>>();

        assemble_sections(&sections, interface_identity(), LANGUAGE_REVISION)
            .map(|artifact| artifact.bytes().to_vec())
            .unwrap_or_else(|error| panic!("test artifact must encode: {error:?}"))
    }

    fn optional_extension(
        mut bytes: Vec<u8>,
        raw_tag: u32,
        compatibility: InterfaceSectionCompatibility,
    ) -> Vec<u8> {
        let directory = bytes.len() - DirectoryEntry::LENGTH;

        write_u32(&mut bytes, directory, raw_tag);
        write_u16(&mut bytes, directory + 4, u16::MAX);

        bytes[directory + 6] = compatibility.wire_value();
        bytes[directory + 7] = u8::MAX;

        rewrite_section_checksum(&mut bytes, directory);
        rewrite_artifact_hash(&mut bytes);

        bytes
    }

    fn rewrite_section_checksum(bytes: &mut [u8], directory: usize) {
        let decoded = DirectoryEntry::decode(&bytes[directory..])
            .unwrap_or_else(|error| panic!("test directory must decode: {error:?}"));

        let entry = DirectoryEntry::from_decoded(decoded);

        let checksum = {
            let payload = entry
                .payload(bytes)
                .unwrap_or_else(|| panic!("test payload must be in bounds"));

            compute_section_hash(&entry, payload)
        };

        let start = directory + DirectoryEntry::CHECKSUM_OFFSET;
        let end = start + crate::InterfaceSectionHash::LENGTH;

        bytes[start..end].copy_from_slice(checksum.as_bytes());
    }

    fn rewrite_artifact_hash(bytes: &mut [u8]) {
        let artifact_hash = compute_artifact_hash(bytes)
            .unwrap_or_else(|| panic!("test artifact hash must compute"));

        bytes[InterfaceHeader::ARTIFACT_HASH_OFFSET..InterfaceHeader::ARTIFACT_HASH_OFFSET + 32]
            .copy_from_slice(artifact_hash.as_bytes());
    }

    fn interface_identity() -> PackageInterfaceIdentity {
        let Some(package) = PackageIdentity::try_new("example.package") else {
            panic!("test package identity must be valid");
        };

        let Some(product) = InterfaceProductIdentity::try_new("library") else {
            panic!("test product identity must be valid");
        };

        PackageInterfaceIdentity::try_new(
            package,
            package_version(),
            product,
            InterfaceProductKind::Library,
            "public-v1",
        )
        .unwrap_or_else(|| panic!("test package-interface identity must be valid"))
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

    fn unsupported_format_revision() -> crate::InterfaceFormatRevision {
        let raw = CURRENT_FORMAT_REVISION
            .raw()
            .checked_add(1)
            .unwrap_or_else(|| panic!("test format revision must have a successor"));

        crate::InterfaceFormatRevision::new(raw)
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

    fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
        bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
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

    fn offset_usize(value: u64) -> usize {
        match usize::try_from(value) {
            Ok(value) => value,
            Err(error) => panic!("test offset must fit the host: {error:?}"),
        }
    }

    fn assert_unknown_checksum_mismatch(error: InterfaceValidationError, raw_tag: u32) {
        let InterfaceValidationError::UnknownSectionChecksumMismatch {
            raw_tag: actual_tag,
            expected,
            actual,
        } = error
        else {
            panic!("expected unknown-section checksum mismatch, got {error:?}")
        };

        assert_eq!(actual_tag, raw_tag);
        assert_ne!(expected, actual);
    }
}
