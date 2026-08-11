use std::sync::Arc;

use bray_symbols::InterfaceSymbolId;

use crate::decode::map_wire_error;
use crate::wire::WireReader;
use crate::{InterfaceLimit, InterfaceValidationError, InterfaceValidationLimits};

use super::artifact::{ImplementationDirectoryEntry, ImplementationPayloadKind};
use super::hash::{compute_payload_content_hash, compute_payload_hash};

pub(super) fn decode_directory_entry(
    reader: &mut WireReader<'_>,
    bytes: &[u8],
    directory_offset: usize,
    expected_offset: usize,
    limits: InterfaceValidationLimits,
) -> Result<ImplementationDirectoryEntry, InterfaceValidationError> {
    let owner = InterfaceSymbolId::new(reader.read_u32().map_err(map_wire_error)?);
    let raw_kind = reader.read_u8().map_err(map_wire_error)?;

    let compatibility = crate::InterfaceSectionCompatibility::from_wire_value(
        reader.read_u8().map_err(map_wire_error)?,
    )
    .ok_or(InterfaceValidationError::Malformed)?;

    let raw_encoding = reader.read_u8().map_err(map_wire_error)?;

    if reader.read_u8().map_err(map_wire_error)? != 0 {
        return Err(InterfaceValidationError::Malformed);
    }

    let section_revision =
        crate::InterfaceSectionRevision::new(reader.read_u16().map_err(map_wire_error)?);

    if reader.read_u16().map_err(map_wire_error)? != 0 {
        return Err(InterfaceValidationError::Malformed);
    }

    let discriminator = reader.read_array::<32>().map_err(map_wire_error)?;
    let family_size = reader.read_u32().map_err(map_wire_error)?;

    if reader.read_u32().map_err(map_wire_error)? != 0 {
        return Err(InterfaceValidationError::Malformed);
    }

    let offset = reader.read_u64().map_err(map_wire_error)?;
    let encoded_length = reader.read_u64().map_err(map_wire_error)?;
    let decoded_length = reader.read_u64().map_err(map_wire_error)?;
    let record_count = reader.read_u64().map_err(map_wire_error)?;
    let checksum = reader.read_array::<32>().map_err(map_wire_error)?;
    let content_hash = reader.read_array::<32>().map_err(map_wire_error)?;

    limits.check(InterfaceLimit::BlobLength, encoded_length)?;
    limits.check(InterfaceLimit::BlobLength, decoded_length)?;
    limits.check(InterfaceLimit::RecordCount, record_count)?;

    let offset = usize::try_from(offset).map_err(|_| InterfaceValidationError::Malformed)?;
    let encoded_length =
        usize::try_from(encoded_length).map_err(|_| InterfaceValidationError::Malformed)?;

    let end = offset
        .checked_add(encoded_length)
        .ok_or(InterfaceValidationError::Malformed)?;

    if offset != expected_offset || end > directory_offset {
        return Err(InterfaceValidationError::Malformed);
    }

    let kind = ImplementationPayloadKind::from_raw(raw_kind);

    if kind.is_some() {
        if compatibility != crate::InterfaceSectionCompatibility::Required
            || section_revision != crate::InterfaceSectionRevision::CURRENT
            || record_count != 1
        {
            return Err(InterfaceValidationError::Malformed);
        }
    } else if compatibility == crate::InterfaceSectionCompatibility::Required {
        return Err(InterfaceValidationError::Malformed);
    }

    validate_payload_address(kind, owner, discriminator, family_size, limits)?;

    let encoding = crate::InterfaceSectionEncoding::from_wire_value(raw_encoding)
        .ok_or(InterfaceValidationError::Malformed)?;

    let payload = bytes
        .get(offset..end)
        .ok_or(InterfaceValidationError::Truncated)?;

    if encoding == crate::InterfaceSectionEncoding::Raw {
        if encoded_length != usize::try_from(decoded_length).unwrap_or(usize::MAX) {
            return Err(InterfaceValidationError::Malformed);
        }
    } else {
        crate::encoding::validate_zstd_frame(payload, decoded_length)?;
    }

    let entry = ImplementationDirectoryEntry {
        owner,
        raw_kind,
        kind,
        compatibility,
        encoding,
        discriminator,
        family_size,
        decoded_length,
        record_count,
        checksum,
        content_hash,
        payload: offset..end,
    };

    if compute_payload_hash(&entry, payload) != checksum {
        return Err(InterfaceValidationError::HashMismatch);
    }

    Ok(entry)
}

pub(super) fn decode_entry_payload(
    bytes: &[u8],
    entry: &ImplementationDirectoryEntry,
    limits: InterfaceValidationLimits,
) -> Result<Arc<[u8]>, InterfaceValidationError> {
    limits.check(InterfaceLimit::DecodedAllocation, entry.decoded_length)?;

    let encoded = bytes
        .get(entry.payload.clone())
        .ok_or(InterfaceValidationError::Malformed)?;

    let decoded = match entry.encoding {
        crate::InterfaceSectionEncoding::Raw => Arc::from(encoded),
        crate::InterfaceSectionEncoding::ZstdFrame => {
            crate::encoding::decode_zstd_frame(encoded, entry.decoded_length)?
        }
    };

    let content_hash =
        compute_payload_content_hash(entry.owner, entry.raw_kind, entry.discriminator, &decoded);

    if content_hash != entry.content_hash {
        return Err(InterfaceValidationError::HashMismatch);
    }

    Ok(decoded)
}

fn validate_payload_address(
    kind: Option<ImplementationPayloadKind>,
    owner: InterfaceSymbolId,
    discriminator: [u8; 32],
    family_size: u32,
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
                return Err(InterfaceValidationError::Malformed);
            }
        }
        Some(ImplementationPayloadKind::Identity) => {
            if owner.raw() != 0 || discriminator != [0; 32] || family_size != 0 {
                return Err(InterfaceValidationError::Malformed);
            }
        }
        Some(ImplementationPayloadKind::ConstantCallableBody)
        | Some(ImplementationPayloadKind::NativeBoundary) => {
            if discriminator != [0; 32] || family_size != 0 {
                return Err(InterfaceValidationError::Malformed);
            }
        }
        Some(ImplementationPayloadKind::PreSpecializedMir) => {
            if owner.raw() != 0 || family_size != 0 {
                return Err(InterfaceValidationError::Malformed);
            }
        }
        None => {}
    }

    Ok(())
}
