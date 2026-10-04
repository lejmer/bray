use std::collections::BTreeSet;
use std::sync::{Arc, OnceLock};

use crate::decode::{DecodeBudget, wire_error};
use crate::implementation::codec::decode_identity;
use crate::implementation::hash::{compute_content_hash, compute_metadata_hash};
use crate::wire::WireReader;
use crate::{
    InterfaceContentHash, InterfaceLanguageRevision, InterfaceLimit, InterfaceValidationContext,
    InterfaceValidationError, InterfaceValidationField, InterfaceValidationLimits,
    PackageImplementationIdentity,
};

use super::decoding::{decode_directory_entry, decode_entry_payload};
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
        Self::from_storage(
            super::storage::ImplementationStorage::memory(bytes.into()),
            limits,
        )
    }

    /// Opens bounded metadata and keeps the same file handle for authenticated payload reads.
    pub fn try_open(
        path: &std::path::Path,
        limits: InterfaceValidationLimits,
    ) -> Result<Self, InterfaceValidationError> {
        Self::from_storage(
            super::storage::ImplementationStorage::open(path, limits)?,
            limits,
        )
    }

    fn from_storage(
        storage: super::storage::ImplementationStorage,
        limits: InterfaceValidationLimits,
    ) -> Result<Self, InterfaceValidationError> {
        limits.check(InterfaceLimit::ImplementationFileSize, storage.len() as u64)?;

        let header = storage.read(0..HEADER_LENGTH.min(storage.len()))?;

        let mut reader = WireReader::new(&header);

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

        let actual_file_length = storage.len() as u64;

        if declared_file_length != actual_file_length {
            return Err(InterfaceValidationError::Malformed {
                context: InterfaceValidationContext::Header,
                cause: crate::InterfaceMalformedCause::LengthMismatch {
                    field: InterfaceValidationField::DeclaredFileLength,
                    expected: declared_file_length,
                    actual: actual_file_length,
                },
            });
        }

        let directory_offset = usize::try_from(directory_offset)
            .map_err(|_| crate::implementation::invalid_value(InterfaceValidationField::Value))?;

        let directory_length = usize::try_from(directory_length)
            .map_err(|_| crate::implementation::invalid_value(InterfaceValidationField::Value))?;

        if directory_offset < HEADER_LENGTH
            || directory_offset.checked_add(directory_length) != Some(storage.len())
            || directory_length % DIRECTORY_ENTRY_LENGTH != 0
        {
            return Err(crate::implementation::invalid_value(
                InterfaceValidationField::Value,
            ));
        }

        let count = directory_length / DIRECTORY_ENTRY_LENGTH;

        limits.check(
            InterfaceLimit::ImplementationEntryCount,
            u64::try_from(count).unwrap_or(u64::MAX),
        )?;

        limits.check(InterfaceLimit::DecodedAllocation, directory_length as u64)?;

        let directory_bytes = storage.read(directory_offset..storage.len())?;

        let actual_artifact_hash = compute_metadata_hash(&header, &directory_bytes).ok_or(
            InterfaceValidationError::DigestUnavailable {
                context: InterfaceValidationContext::Artifact,
                field: InterfaceValidationField::ArtifactHash,
            },
        )?;

        if actual_artifact_hash != artifact_hash {
            return Err(InterfaceValidationError::ArtifactHashMismatch {
                expected: crate::InterfaceArtifactHash::from_bytes(artifact_hash),
                actual: crate::InterfaceArtifactHash::from_bytes(actual_artifact_hash),
            });
        }

        let mut directory_reader = WireReader::new(&directory_bytes);
        let mut budget = DecodeBudget::new(limits);

        let mut directory = budget.allocate_items(
            &directory_reader,
            InterfaceValidationContext::Directory,
            InterfaceValidationField::EntryKind,
            count,
        )?;

        let mut expected_offset = HEADER_LENGTH;

        for index in 0..count {
            let entry = decode_directory_entry(
                &mut directory_reader,
                directory_offset,
                expected_offset,
                index as u64,
                limits,
            )?;

            if directory
                .last()
                .is_some_and(|previous: &ImplementationDirectoryEntry| {
                    (previous.owner, previous.raw_kind, previous.discriminator)
                        >= (entry.owner, entry.raw_kind, entry.discriminator)
                })
            {
                return Err(crate::implementation::invalid_value(
                    InterfaceValidationField::Value,
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
                context: InterfaceValidationContext::Directory,
                cause: crate::InterfaceMalformedCause::LengthMismatch {
                    field: InterfaceValidationField::DirectoryOffset,
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
            decode_implementation_identity(&storage, &directory, language_revision, &mut budget)?;

        let mut decoded = budget.allocate_derived_items(
            InterfaceValidationContext::Directory,
            InterfaceValidationField::RecordPayload,
            directory.len(),
        )?;

        decoded.extend((0..directory.len()).map(|_| OnceLock::new()));

        Ok(Self {
            storage: Arc::new(storage),
            identity,
            content_hash,
            artifact_hash,
            directory: directory.into(),
            decoded: decoded.into(),
            native_indexes: Arc::new(std::array::from_fn(|_| OnceLock::new())),
            allocation: Arc::new(std::sync::Mutex::new(budget)),
            limits,
        })
    }
}

fn decode_implementation_identity(
    storage: &super::storage::ImplementationStorage,
    directory: &[ImplementationDirectoryEntry],
    language_revision: InterfaceLanguageRevision,
    budget: &mut DecodeBudget,
) -> Result<PackageImplementationIdentity, InterfaceValidationError> {
    let identity_entries = directory
        .iter()
        .filter(|entry| entry.kind == Some(ImplementationPayloadKind::Identity))
        .collect::<Vec<_>>();

    let [identity_entry] = identity_entries.as_slice() else {
        return Err(crate::implementation::invalid_value(
            InterfaceValidationField::Value,
        ));
    };

    budget.charge(usize::try_from(identity_entry.decoded_length).unwrap_or(usize::MAX))?;

    let limits = budget.limits();

    let identity_payload = decode_entry_payload(
        storage.read(identity_entry.payload.clone())?,
        identity_entry,
        limits,
    )?;

    storage.authenticated(
        identity_entry.payload.len(),
        identity_entry.decoded_length,
        identity_entry.encoding == crate::InterfaceSectionEncoding::ZstdFrame,
    );

    let identity = decode_identity(&identity_payload, limits)?;

    if identity.language_revision() != language_revision {
        return Err(crate::implementation::invalid_value(
            InterfaceValidationField::Value,
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
                    InterfaceValidationField::Value,
                ))?
            };

            if entry.owner != owner
                || entry.family_size != family_size
                || entry.discriminator != executable_discriminator(expected)
                || (expected != 0 && entry.platform_service.is_some())
            {
                return Err(crate::implementation::invalid_value(
                    InterfaceValidationField::Value,
                ));
            }

            if let Some(role) = entry.platform_service
                && !platform_services.insert(role)
            {
                return Err(crate::implementation::invalid_value(
                    InterfaceValidationField::Value,
                ));
            }
        }

        if entries.peek().is_some_and(|entry| entry.owner == owner) {
            return Err(crate::implementation::invalid_value(
                InterfaceValidationField::Value,
            ));
        }
    }

    Ok(())
}
