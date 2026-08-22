use std::sync::Arc;

use bray_base::NonEmptySharedStr;
use bray_symbols::{
    ForeignCallableDirection, InterfaceSymbolId, NativeSymbolBinding, NativeSymbolContract,
    NativeSymbolIdentity, NativeSymbolPresence, StaticStorageDuration,
};

use crate::decode::map_wire_error;
use crate::wire::{WireEncoder, WireReader};
use crate::{InterfaceLimit, InterfaceValidationError, InterfaceValidationLimits};

use super::codec::{decode_specialization_key, encode_specialization_key};
use super::{
    CURRENT_MIR_SCHEMA_REVISION, ImplementationMirSchemaRevision, InterfaceNativeBoundary,
    InterfaceNativeBoundaryKind, InterfacePreSpecializedMir,
    PackageImplementationSpecializationKey,
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
) -> [u8; 32] {
    key.cache_identity()
}

pub(super) fn encode_native_boundary(boundary: &InterfaceNativeBoundary) -> Vec<u8> {
    let mut encoder = WireEncoder::new();

    let direction = match boundary.direction() {
        ForeignCallableDirection::Import => 0,
        ForeignCallableDirection::Export => 1,
    };

    encoder.write_u8(direction);

    let kind = match boundary.kind() {
        InterfaceNativeBoundaryKind::Callable => 0,
        InterfaceNativeBoundaryKind::Static {
            duration: StaticStorageDuration::Product,
            mutable: false,
        } => 1,
        InterfaceNativeBoundaryKind::Static {
            duration: StaticStorageDuration::Product,
            mutable: true,
        } => 2,
        InterfaceNativeBoundaryKind::Static {
            duration: StaticStorageDuration::ExactThread,
            mutable: false,
        } => 3,
        InterfaceNativeBoundaryKind::Static {
            duration: StaticStorageDuration::ExactThread,
            mutable: true,
        } => 4,
    };

    encoder.write_u8(kind);

    match boundary.symbol().identity() {
        NativeSymbolIdentity::Name(name) => {
            encoder.write_u8(0);
            write_string(&mut encoder, name.as_str());
        }
        NativeSymbolIdentity::Ordinal(ordinal) => {
            encoder.write_u8(1);
            encoder.write_u64(*ordinal);
        }
    }

    match boundary.symbol().version() {
        Some(version) => {
            encoder.write_u8(1);
            write_string(&mut encoder, version);
        }
        None => encoder.write_u8(0),
    }

    encoder.write_u8(match boundary.symbol().binding() {
        NativeSymbolBinding::Strong => 0,
        NativeSymbolBinding::Weak => 1,
    });

    encoder.write_u8(match boundary.symbol().presence() {
        NativeSymbolPresence::Required => 0,
        NativeSymbolPresence::Optional => 1,
    });

    encoder.into_bytes()
}

pub(super) fn decode_native_boundary(
    owner: InterfaceSymbolId,
    payload: &[u8],
    limits: InterfaceValidationLimits,
) -> Result<InterfaceNativeBoundary, InterfaceValidationError> {
    let mut reader = WireReader::new(payload);

    let direction = match reader.read_u8().map_err(map_wire_error)? {
        0 => ForeignCallableDirection::Import,
        1 => ForeignCallableDirection::Export,
        _ => return Err(InterfaceValidationError::Malformed),
    };

    let kind = match reader.read_u8().map_err(map_wire_error)? {
        0 => InterfaceNativeBoundaryKind::Callable,
        1 => InterfaceNativeBoundaryKind::Static {
            duration: StaticStorageDuration::Product,
            mutable: false,
        },
        2 => InterfaceNativeBoundaryKind::Static {
            duration: StaticStorageDuration::Product,
            mutable: true,
        },
        3 => InterfaceNativeBoundaryKind::Static {
            duration: StaticStorageDuration::ExactThread,
            mutable: false,
        },
        4 => InterfaceNativeBoundaryKind::Static {
            duration: StaticStorageDuration::ExactThread,
            mutable: true,
        },
        _ => return Err(InterfaceValidationError::Malformed),
    };

    let identity = match reader.read_u8().map_err(map_wire_error)? {
        0 => NativeSymbolIdentity::Name(read_nonempty_string(&mut reader, limits)?),
        1 => NativeSymbolIdentity::Ordinal(reader.read_u64().map_err(map_wire_error)?),
        _ => return Err(InterfaceValidationError::Malformed),
    };

    let version = match reader.read_u8().map_err(map_wire_error)? {
        0 => None,
        1 => Some(read_nonempty_string(&mut reader, limits)?),
        _ => return Err(InterfaceValidationError::Malformed),
    };

    let binding = match reader.read_u8().map_err(map_wire_error)? {
        0 => NativeSymbolBinding::Strong,
        1 => NativeSymbolBinding::Weak,
        _ => return Err(InterfaceValidationError::Malformed),
    };

    let presence = match reader.read_u8().map_err(map_wire_error)? {
        0 => NativeSymbolPresence::Required,
        1 => NativeSymbolPresence::Optional,
        _ => return Err(InterfaceValidationError::Malformed),
    };

    reader.finish().map_err(map_wire_error)?;

    let symbol = NativeSymbolContract::new(identity, version, binding, presence);

    Ok(InterfaceNativeBoundary::new(owner, direction, kind, symbol))
}

fn write_string(encoder: &mut WireEncoder, value: &str) {
    encoder.write_u64(u64::try_from(value.len()).unwrap_or(u64::MAX));
    encoder.write_bytes(value.as_bytes());
}

fn read_nonempty_string(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
) -> Result<NonEmptySharedStr, InterfaceValidationError> {
    let length = reader.read_u64().map_err(map_wire_error)?;

    limits.check(InterfaceLimit::StringLength, length)?;

    let length = usize::try_from(length).map_err(|_| InterfaceValidationError::Malformed)?;
    let bytes = reader.read_bytes(length).map_err(map_wire_error)?;
    let value = std::str::from_utf8(bytes).map_err(|_| InterfaceValidationError::Malformed)?;

    NonEmptySharedStr::try_new(value).ok_or(InterfaceValidationError::Malformed)
}
