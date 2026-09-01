use std::collections::BTreeSet;
use std::sync::{Arc, OnceLock};

use crate::decode::{DecodeBudget, wire_error};
use crate::implementation::artifact_decoding::{decode_directory_entry, decode_entry_payload};
use crate::implementation::codec::decode_identity;
use crate::implementation::hash::{compute_artifact_hash, compute_content_hash};
use crate::wire::WireReader;
use crate::{
    InterfaceContentHash, InterfaceLanguageRevision, InterfaceLimit, InterfaceValidationContext,
    InterfaceValidationError, InterfaceValidationField, InterfaceValidationLimits,
    PackageImplementationIdentity,
};

use super::{
    BYTE_ORDER_MARKER, DIRECTORY_ENTRY_LENGTH, HEADER_LENGTH, ImplementationDirectoryEntry,
    ImplementationPayloadKind, MAGIC, PackageImplementationArtifact, REQUIRED_FLAGS,
    executable_discriminator,
};

impl PackageImplementationArtifact {
    /// Validates canonical framing and identity without decoding unrelated body payloads.
    pub fn try_from_bytes(
        bytes: impl Into<Arc<[u8]>>,
        limits: InterfaceValidationLimits,
    ) -> Result<Self, InterfaceValidationError> {
        let bytes = bytes.into();

        limits.check(
            InterfaceLimit::FileSize,
            u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        )?;

        let header = bytes.get(..HEADER_LENGTH).unwrap_or(&bytes);

        let mut reader = WireReader::new(header);

        let actual_magic = reader.read_array::<8>().map_err(wire_error(
            InterfaceValidationContext::Header,
            InterfaceValidationField::Magic,
        ))?;

        if actual_magic != MAGIC {
            return Err(InterfaceValidationError::InvalidMagic {
                actual: actual_magic,
            });
        }

        let format_revision =
            crate::InterfaceFormatRevision::new(reader.read_u16().map_err(wire_error(
                InterfaceValidationContext::Header,
                InterfaceValidationField::FormatRevision,
            ))?);

        if format_revision != crate::CURRENT_FORMAT_REVISION {
            return Err(InterfaceValidationError::UnsupportedFormatRevision {
                actual: format_revision,
            });
        }

        let language_revision =
            InterfaceLanguageRevision::new(reader.read_u16().map_err(wire_error(
                InterfaceValidationContext::Header,
                InterfaceValidationField::LanguageRevision,
            ))?);

        let byte_order = reader.read_u32().map_err(wire_error(
            InterfaceValidationContext::Header,
            InterfaceValidationField::ByteOrderMarker,
        ))?;

        if byte_order != BYTE_ORDER_MARKER {
            return Err(InterfaceValidationError::UnsupportedByteOrder {
                expected: BYTE_ORDER_MARKER,
                actual: byte_order,
            });
        }

        let required_flags = reader.read_u64().map_err(wire_error(
            InterfaceValidationContext::Header,
            InterfaceValidationField::RequiredFlags,
        ))?;

        if required_flags != REQUIRED_FLAGS {
            return Err(InterfaceValidationError::UnsupportedRequiredFlags {
                actual: crate::InterfaceRequiredFlags::from_bits(required_flags),
            });
        }

        let declared_file_length = reader.read_u64().map_err(wire_error(
            InterfaceValidationContext::Header,
            InterfaceValidationField::DeclaredFileLength,
        ))?;

        let directory_offset = reader.read_u64().map_err(wire_error(
            InterfaceValidationContext::Header,
            InterfaceValidationField::DirectoryOffset,
        ))?;

        let directory_length = reader.read_u64().map_err(wire_error(
            InterfaceValidationContext::Header,
            InterfaceValidationField::DirectoryLength,
        ))?;

        let content_hash = reader.read_array::<32>().map_err(wire_error(
            InterfaceValidationContext::Header,
            InterfaceValidationField::ContentHash,
        ))?;

        let artifact_hash = reader.read_array::<32>().map_err(wire_error(
            InterfaceValidationContext::Header,
            InterfaceValidationField::ArtifactHash,
        ))?;

        reader.finish().map_err(wire_error(
            InterfaceValidationContext::Header,
            InterfaceValidationField::RecordPayload,
        ))?;

        let actual_file_length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);

        if declared_file_length != actual_file_length {
            return Err(InterfaceValidationError::Malformed {
                context: crate::InterfaceValidationContext::Header,
                cause: crate::InterfaceMalformedCause::LengthMismatch {
                    field: crate::InterfaceValidationField::DeclaredFileLength,
                    expected: declared_file_length,
                    actual: actual_file_length,
                },
            });
        }

        let actual_artifact_hash =
            compute_artifact_hash(&bytes).ok_or(InterfaceValidationError::DigestUnavailable {
                context: crate::InterfaceValidationContext::Artifact,
                field: crate::InterfaceValidationField::ArtifactHash,
            })?;

        if actual_artifact_hash != artifact_hash {
            return Err(InterfaceValidationError::ArtifactHashMismatch {
                expected: crate::InterfaceArtifactHash::from_bytes(artifact_hash),
                actual: crate::InterfaceArtifactHash::from_bytes(actual_artifact_hash),
            });
        }

        let directory_offset = usize::try_from(directory_offset).map_err(|_| {
            crate::implementation::invalid_value(crate::InterfaceValidationField::Value)
        })?;

        let directory_length = usize::try_from(directory_length).map_err(|_| {
            crate::implementation::invalid_value(crate::InterfaceValidationField::Value)
        })?;

        if directory_offset < HEADER_LENGTH
            || directory_offset.checked_add(directory_length) != Some(bytes.len())
            || directory_length % DIRECTORY_ENTRY_LENGTH != 0
        {
            return Err(crate::implementation::invalid_value(
                crate::InterfaceValidationField::Value,
            ));
        }

        let count = directory_length / DIRECTORY_ENTRY_LENGTH;

        limits.check(
            InterfaceLimit::ImplementationEntryCount,
            u64::try_from(count).unwrap_or(u64::MAX),
        )?;

        let directory_bytes =
            bytes
                .get(directory_offset..)
                .ok_or(InterfaceValidationError::Truncated {
                    context: crate::InterfaceValidationContext::Directory,
                    field: crate::InterfaceValidationField::DirectoryLength,
                    offset: directory_offset as u64,
                    expected_length: directory_length as u64,
                    actual_length: bytes.len().saturating_sub(directory_offset) as u64,
                })?;

        let mut directory_reader = WireReader::new(directory_bytes);
        let mut budget = DecodeBudget::new(limits);

        let mut directory = budget.allocate_items(
            &directory_reader,
            crate::InterfaceValidationContext::Directory,
            crate::InterfaceValidationField::EntryKind,
            count,
        )?;

        let mut expected_offset = HEADER_LENGTH;
        let mut decoded_total = 0_u64;

        for index in 0..count {
            let entry = decode_directory_entry(
                &mut directory_reader,
                &bytes,
                directory_offset,
                expected_offset,
                index as u64,
                limits,
            )?;

            decoded_total = decoded_total.checked_add(entry.decoded_length).ok_or(
                crate::implementation::invalid_value(crate::InterfaceValidationField::Value),
            )?;

            limits.check(InterfaceLimit::DecodedAllocation, decoded_total)?;

            if directory
                .last()
                .is_some_and(|previous: &ImplementationDirectoryEntry| {
                    (previous.owner, previous.raw_kind, previous.discriminator)
                        >= (entry.owner, entry.raw_kind, entry.discriminator)
                })
            {
                return Err(crate::implementation::invalid_value(
                    crate::InterfaceValidationField::Value,
                ));
            }

            expected_offset = entry.payload.end;
            directory.push(entry);
        }

        directory_reader.finish().map_err(wire_error(
            InterfaceValidationContext::Directory,
            InterfaceValidationField::DirectoryLength,
        ))?;

        if expected_offset != directory_offset {
            return Err(InterfaceValidationError::Malformed {
                context: crate::InterfaceValidationContext::Directory,
                cause: crate::InterfaceMalformedCause::LengthMismatch {
                    field: crate::InterfaceValidationField::DirectoryOffset,
                    expected: directory_offset as u64,
                    actual: expected_offset as u64,
                },
            });
        }

        let actual_content_hash = compute_content_hash(language_revision, &directory);

        if actual_content_hash != content_hash {
            return Err(InterfaceValidationError::ContentHashMismatch {
                expected: InterfaceContentHash::from_bytes(content_hash),
                actual: InterfaceContentHash::from_bytes(actual_content_hash),
            });
        }

        validate_encoded_executable_template_families(&directory)?;

        let identity =
            decode_implementation_identity(&bytes, &directory, language_revision, limits)?;

        let decoded = (0..directory.len()).map(|_| OnceLock::new()).collect();

        Ok(Self {
            bytes,
            identity,
            content_hash,
            artifact_hash,
            directory: directory.into(),
            decoded,
            limits,
        })
    }
}

fn decode_implementation_identity(
    bytes: &[u8],
    directory: &[ImplementationDirectoryEntry],
    language_revision: InterfaceLanguageRevision,
    limits: InterfaceValidationLimits,
) -> Result<PackageImplementationIdentity, InterfaceValidationError> {
    let identity_entries = directory
        .iter()
        .filter(|entry| entry.kind == Some(ImplementationPayloadKind::Identity))
        .collect::<Vec<_>>();

    let [identity_entry] = identity_entries.as_slice() else {
        return Err(crate::implementation::invalid_value(
            crate::InterfaceValidationField::Value,
        ));
    };

    let identity_payload = decode_entry_payload(bytes, identity_entry, limits)?;
    let identity = decode_identity(&identity_payload, limits)?;

    if identity.language_revision() != language_revision {
        return Err(crate::implementation::invalid_value(
            crate::InterfaceValidationField::Value,
        ));
    }

    Ok(identity)
}

fn validate_encoded_executable_template_families(
    directory: &[ImplementationDirectoryEntry],
) -> Result<(), InterfaceValidationError> {
    let mut entries = directory
        .iter()
        .filter(|entry| entry.kind == Some(ImplementationPayloadKind::ExecutableTemplate))
        .peekable();

    let mut platform_services = BTreeSet::new();

    while let Some(first) = entries.next() {
        let owner = first.owner;
        let family_size = first.family_size;

        for expected in 0..family_size {
            let entry = if expected == 0 {
                first
            } else {
                entries.next().ok_or(crate::implementation::invalid_value(
                    crate::InterfaceValidationField::Value,
                ))?
            };

            if entry.owner != owner
                || entry.family_size != family_size
                || entry.discriminator != executable_discriminator(expected)
                || (expected != 0 && entry.platform_service.is_some())
            {
                return Err(crate::implementation::invalid_value(
                    crate::InterfaceValidationField::Value,
                ));
            }

            if let Some(role) = entry.platform_service
                && !platform_services.insert(role)
            {
                return Err(crate::implementation::invalid_value(
                    crate::InterfaceValidationField::Value,
                ));
            }
        }

        if entries.peek().is_some_and(|entry| entry.owner == owner) {
            return Err(crate::implementation::invalid_value(
                crate::InterfaceValidationField::Value,
            ));
        }
    }

    Ok(())
}
