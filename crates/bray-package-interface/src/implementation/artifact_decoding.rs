use std::sync::Arc;

use bray_symbols::InterfaceSymbolId;

use crate::decode::wire_error;
use crate::wire::WireReader;
use crate::{
    InterfaceIntegerTarget, InterfaceLimit, InterfaceMalformedCause, InterfaceValidationContext,
    InterfaceValidationError, InterfaceValidationField, InterfaceValidationLimits,
};

use super::artifact::{ImplementationDirectoryEntry, ImplementationPayloadKind};
use super::hash::{compute_payload_content_hash, compute_payload_hash};

pub(super) fn decode_directory_entry(
    reader: &mut WireReader<'_>,
    bytes: &[u8],
    directory_offset: usize,
    expected_offset: usize,
    index: u64,
    limits: InterfaceValidationLimits,
) -> Result<ImplementationDirectoryEntry, InterfaceValidationError> {
    let provisional_context =
        InterfaceValidationContext::ImplementationEntry { index, raw_kind: 0 };
    let owner = InterfaceSymbolId::new(reader.read_u32().map_err(wire_error(
        provisional_context,
        InterfaceValidationField::Owner,
    ))?);
    let raw_kind = reader.read_u8().map_err(wire_error(
        provisional_context,
        InterfaceValidationField::EntryKind,
    ))?;
    let context = InterfaceValidationContext::ImplementationEntry { index, raw_kind };

    let compatibility =
        crate::InterfaceSectionCompatibility::from_wire_value(reader.read_u8().map_err(
            wire_error(context, InterfaceValidationField::SectionCompatibility),
        )?)
        .ok_or_else(|| invalid_value(context, InterfaceValidationField::SectionCompatibility))?;

    let raw_encoding = reader
        .read_u8()
        .map_err(wire_error(context, InterfaceValidationField::EntryEncoding))?;

    let reserved = reader
        .read_u8()
        .map_err(wire_error(context, InterfaceValidationField::Value))?;
    if reserved != 0 {
        return Err(value_mismatch(
            context,
            InterfaceValidationField::Value,
            0,
            u64::from(reserved),
        ));
    }

    let section_revision = crate::InterfaceSectionRevision::new(reader.read_u16().map_err(
        wire_error(context, InterfaceValidationField::SectionRevision),
    )?);

    let reserved = reader
        .read_u16()
        .map_err(wire_error(context, InterfaceValidationField::Value))?;
    if reserved != 0 {
        return Err(value_mismatch(
            context,
            InterfaceValidationField::Value,
            0,
            u64::from(reserved),
        ));
    }

    let discriminator = reader
        .read_array::<32>()
        .map_err(wire_error(context, InterfaceValidationField::Discriminant))?;
    let family_size = reader
        .read_u32()
        .map_err(wire_error(context, InterfaceValidationField::RecordCount))?;

    let platform_service = match reader
        .read_u32()
        .map_err(wire_error(context, InterfaceValidationField::Role))?
    {
        0 => None,
        id => Some(
            bray_runtime_interface::PlatformServiceRole::from_id(id)
                .ok_or_else(|| invalid_value(context, InterfaceValidationField::Role))?,
        ),
    };

    let offset = reader
        .read_u64()
        .map_err(wire_error(context, InterfaceValidationField::EntryOffset))?;
    let encoded_length = reader
        .read_u64()
        .map_err(wire_error(context, InterfaceValidationField::EncodedLength))?;
    let decoded_length = reader
        .read_u64()
        .map_err(wire_error(context, InterfaceValidationField::DecodedLength))?;
    let record_count = reader
        .read_u64()
        .map_err(wire_error(context, InterfaceValidationField::RecordCount))?;
    let checksum = reader
        .read_array::<32>()
        .map_err(wire_error(context, InterfaceValidationField::Hash))?;
    let content_hash = reader
        .read_array::<32>()
        .map_err(wire_error(context, InterfaceValidationField::ContentHash))?;

    limits.check(InterfaceLimit::BlobLength, encoded_length)?;
    limits.check(InterfaceLimit::BlobLength, decoded_length)?;
    limits.check(InterfaceLimit::RecordCount, record_count)?;

    let offset = usize::try_from(offset)
        .map_err(|_| numeric_overflow(context, InterfaceValidationField::EntryOffset, offset))?;

    let encoded_length = usize::try_from(encoded_length).map_err(|_| {
        numeric_overflow(
            context,
            InterfaceValidationField::EncodedLength,
            encoded_length,
        )
    })?;

    let end = offset
        .checked_add(encoded_length)
        .ok_or(InterfaceValidationError::Malformed {
            context,
            cause: InterfaceMalformedCause::RangeOverflow {
                offset: offset as u64,
                length: encoded_length as u64,
            },
        })?;

    if offset != expected_offset {
        return Err(InterfaceValidationError::Malformed {
            context,
            cause: InterfaceMalformedCause::OrderingViolation {
                field: InterfaceValidationField::EntryOffset,
                previous: expected_offset as u64,
                actual: offset as u64,
            },
        });
    }

    if end > directory_offset {
        return Err(InterfaceValidationError::Truncated {
            context,
            field: InterfaceValidationField::RecordPayload,
            offset: offset as u64,
            expected_length: encoded_length as u64,
            actual_length: directory_offset.saturating_sub(offset) as u64,
        });
    }

    let kind = ImplementationPayloadKind::from_raw(raw_kind);

    if kind.is_some() {
        if compatibility != crate::InterfaceSectionCompatibility::Required
            || section_revision != crate::InterfaceSectionRevision::CURRENT
            || record_count != 1
        {
            return Err(invalid_value(context, InterfaceValidationField::Value));
        }
    } else if compatibility == crate::InterfaceSectionCompatibility::Required {
        return Err(invalid_value(
            context,
            InterfaceValidationField::SectionCompatibility,
        ));
    }

    validate_payload_address(
        kind,
        owner,
        discriminator,
        family_size,
        platform_service,
        limits,
    )?;

    let encoding =
        crate::InterfaceSectionEncoding::from_wire_value(raw_encoding).ok_or_else(|| {
            invalid_discriminant(
                context,
                InterfaceValidationField::EntryEncoding,
                u64::from(raw_encoding),
            )
        })?;

    let payload = bytes
        .get(offset..end)
        .ok_or(InterfaceValidationError::Truncated {
            context,
            field: InterfaceValidationField::RecordPayload,
            offset: offset as u64,
            expected_length: encoded_length as u64,
            actual_length: bytes.len().saturating_sub(offset) as u64,
        })?;

    if encoding == crate::InterfaceSectionEncoding::Raw {
        if encoded_length != usize::try_from(decoded_length).unwrap_or(usize::MAX) {
            return Err(InterfaceValidationError::Malformed {
                context,
                cause: InterfaceMalformedCause::LengthMismatch {
                    field: InterfaceValidationField::DecodedLength,
                    expected: decoded_length,
                    actual: encoded_length as u64,
                },
            });
        }
    } else {
        crate::encoding::validate_zstd_frame(context, payload, decoded_length)?;
    }

    let entry = ImplementationDirectoryEntry {
        index,
        owner,
        raw_kind,
        kind,
        compatibility,
        encoding,
        discriminator,
        family_size,
        platform_service,
        decoded_length,
        record_count,
        checksum,
        content_hash,
        payload: offset..end,
    };

    let actual_checksum = compute_payload_hash(&entry, payload);
    if actual_checksum != checksum {
        return Err(InterfaceValidationError::PayloadChecksumMismatch {
            context,
            expected: checksum,
            actual: actual_checksum,
        });
    }

    Ok(entry)
}

pub(super) fn decode_entry_payload(
    bytes: &[u8],
    entry: &ImplementationDirectoryEntry,
    limits: InterfaceValidationLimits,
) -> Result<Arc<[u8]>, InterfaceValidationError> {
    let context = InterfaceValidationContext::ImplementationEntry {
        index: entry.index,
        raw_kind: entry.raw_kind,
    };
    limits.check(InterfaceLimit::DecodedAllocation, entry.decoded_length)?;

    let encoded = bytes
        .get(entry.payload.clone())
        .ok_or_else(|| invalid_value(context, InterfaceValidationField::RecordPayload))?;

    let decoded = match entry.encoding {
        crate::InterfaceSectionEncoding::Raw => Arc::from(encoded),
        crate::InterfaceSectionEncoding::ZstdFrame => {
            crate::encoding::decode_zstd_frame(context, encoded, entry.decoded_length)?
        }
    };

    let content_hash =
        compute_payload_content_hash(entry.owner, entry.raw_kind, entry.discriminator, &decoded);

    if content_hash != entry.content_hash {
        return Err(InterfaceValidationError::PayloadContentHashMismatch {
            context,
            expected: entry.content_hash,
            actual: content_hash,
        });
    }

    Ok(decoded)
}

const fn invalid_value(
    context: InterfaceValidationContext,
    field: InterfaceValidationField,
) -> InterfaceValidationError {
    InterfaceValidationError::Malformed {
        context,
        cause: InterfaceMalformedCause::InvalidValue { field },
    }
}

const fn invalid_discriminant(
    context: InterfaceValidationContext,
    field: InterfaceValidationField,
    actual: u64,
) -> InterfaceValidationError {
    InterfaceValidationError::Malformed {
        context,
        cause: InterfaceMalformedCause::InvalidDiscriminant { field, actual },
    }
}

const fn value_mismatch(
    context: InterfaceValidationContext,
    field: InterfaceValidationField,
    expected: u64,
    actual: u64,
) -> InterfaceValidationError {
    InterfaceValidationError::Malformed {
        context,
        cause: InterfaceMalformedCause::ValueMismatch {
            field,
            expected,
            actual,
        },
    }
}

const fn numeric_overflow(
    context: InterfaceValidationContext,
    field: InterfaceValidationField,
    value: u64,
) -> InterfaceValidationError {
    InterfaceValidationError::Malformed {
        context,
        cause: InterfaceMalformedCause::NumericOverflow {
            field,
            value,
            target: InterfaceIntegerTarget::Usize,
        },
    }
}

fn validate_payload_address(
    kind: Option<ImplementationPayloadKind>,
    owner: InterfaceSymbolId,
    discriminator: [u8; 32],
    family_size: u32,
    platform_service: Option<bray_runtime_interface::PlatformServiceRole>,
    limits: InterfaceValidationLimits,
) -> Result<(), InterfaceValidationError> {
    match kind {
        Some(ImplementationPayloadKind::ExecutableTemplate) => {
            limits.check(InterfaceLimit::RecordCount, u64::from(family_size))?;

            let raw = u32::from_le_bytes([
                discriminator[0],
                discriminator[1],
                discriminator[2],
                discriminator[3],
            ]);

            if family_size == 0
                || raw >= family_size
                || discriminator[4..].iter().any(|byte| *byte != 0)
            {
                return Err(crate::implementation::invalid_value(
                    crate::InterfaceValidationField::Value,
                ));
            }
        }
        Some(ImplementationPayloadKind::Identity) => {
            if owner.raw() != 0
                || discriminator != [0; 32]
                || family_size != 0
                || platform_service.is_some()
            {
                return Err(crate::implementation::invalid_value(
                    crate::InterfaceValidationField::Value,
                ));
            }
        }
        Some(ImplementationPayloadKind::ConstantCallableBody)
        | Some(ImplementationPayloadKind::NativeBoundary) => {
            if discriminator != [0; 32] || family_size != 0 || platform_service.is_some() {
                return Err(crate::implementation::invalid_value(
                    crate::InterfaceValidationField::Value,
                ));
            }
        }
        Some(ImplementationPayloadKind::PreSpecializedMir) => {
            if owner.raw() != 0 || family_size != 0 || platform_service.is_some() {
                return Err(crate::implementation::invalid_value(
                    crate::InterfaceValidationField::Value,
                ));
            }
        }
        None => {}
    }

    Ok(())
}
