use std::str;
use std::sync::Arc;

use bray_runtime_interface::BinarySymbolName;
use bray_symbols::{ForeignCallableDirection, InterfaceSymbolId};

use crate::decode::map_wire_error;
use crate::wire::{WireEncoder, WireReader};
use crate::{InterfaceLimit, InterfaceValidationError, InterfaceValidationLimits};

use super::codec::{decode_specialization_key, encode_specialization_key};
use super::{
    CURRENT_MIR_SCHEMA_REVISION, ImplementationMirSchemaRevision, InterfaceNativeBoundary,
    InterfacePreSpecializedMir, PackageImplementationSpecializationKey,
};

pub(super) fn encode_pre_specialized_mir(mir: &InterfacePreSpecializedMir) -> Vec<u8> {
    let mut encoder = WireEncoder::new();

    encode_specialization_key(mir.key(), &mut encoder);
    encoder.write_u16(mir.mir_schema_revision().raw());
    encoder.write_u64(u64::try_from(mir.payload().len()).unwrap_or(u64::MAX));
    encoder.write_bytes(mir.payload());

    encoder.into_bytes()
}

pub(super) fn decode_pre_specialized_mir(
    payload: &[u8],
    limits: InterfaceValidationLimits,
) -> Result<InterfacePreSpecializedMir, InterfaceValidationError> {
    let mut reader = WireReader::new(payload);
    let key = decode_specialization_key(&mut reader, limits)?;
    let mir_schema_revision =
        ImplementationMirSchemaRevision::new(reader.read_u16().map_err(map_wire_error)?);

    if mir_schema_revision != CURRENT_MIR_SCHEMA_REVISION {
        return Err(InterfaceValidationError::Malformed);
    }
    let length = reader.read_u64().map_err(map_wire_error)?;

    limits.check(InterfaceLimit::BlobLength, length)?;

    let length = usize::try_from(length).map_err(|_| InterfaceValidationError::Malformed)?;
    let payload = reader.read_bytes(length).map_err(map_wire_error)?;

    reader.finish().map_err(map_wire_error)?;

    InterfacePreSpecializedMir::new(key, mir_schema_revision, Arc::<[u8]>::from(payload))
        .ok_or(InterfaceValidationError::Malformed)
}

pub(super) fn specialization_discriminator(
    key: &PackageImplementationSpecializationKey,
) -> u32 {
    let identity = key.cache_identity();

    u32::from_le_bytes([identity[0], identity[1], identity[2], identity[3]])
}

pub(super) fn encode_native_boundary(boundary: &InterfaceNativeBoundary) -> Vec<u8> {
    let mut payload = Vec::with_capacity(boundary.symbol().as_str().len().saturating_add(1));

    let direction = match boundary.direction() {
        ForeignCallableDirection::Import => 0,
        ForeignCallableDirection::Export => 1,
    };

    payload.push(direction);
    payload.extend_from_slice(boundary.symbol().as_str().as_bytes());

    payload
}

pub(super) fn decode_native_boundary(
    owner: InterfaceSymbolId,
    payload: &[u8],
    limits: InterfaceValidationLimits,
) -> Result<InterfaceNativeBoundary, InterfaceValidationError> {
    let Some((&direction, symbol)) = payload.split_first() else {
        return Err(InterfaceValidationError::Malformed);
    };

    limits.check(
        InterfaceLimit::StringLength,
        u64::try_from(symbol.len()).unwrap_or(u64::MAX),
    )?;

    let direction = match direction {
        0 => ForeignCallableDirection::Import,
        1 => ForeignCallableDirection::Export,
        _ => return Err(InterfaceValidationError::Malformed),
    };

    let symbol = str::from_utf8(symbol).map_err(|_| InterfaceValidationError::Malformed)?;

    let symbol =
        BinarySymbolName::try_new(symbol.to_owned()).ok_or(InterfaceValidationError::Malformed)?;

    Ok(InterfaceNativeBoundary::new(owner, direction, symbol))
}
