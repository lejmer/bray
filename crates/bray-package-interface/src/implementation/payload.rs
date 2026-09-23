use std::sync::Arc;

use bray_base::NonEmptySharedStr;
use bray_codegen::{
    CodegenOptions, DebugInformationMode, OptimizationLevel, ReproducibilityLevel,
    RuntimeObservationMode, SizePreference,
};
use bray_symbols::{
    ForeignCallableDirection, InterfaceSymbolId, NativeSymbolBinding, NativeSymbolContract,
    NativeSymbolIdentity, NativeSymbolPresence, StaticStorageDuration,
};

use crate::wire::{WireEncoder, WireReader};
use crate::{
    InterfaceIntegerTarget, InterfaceLimit, InterfaceMalformedCause, InterfaceUtf8Failure,
    InterfaceValidationContext, InterfaceValidationError, InterfaceValidationField,
    InterfaceValidationLimits,
};

use super::codec::{decode_specialization_key, encode_specialization_key};
use super::{
    CURRENT_MIR_SCHEMA_REVISION, ImplementationMirSchemaRevision, InterfaceNativeBinding, InterfaceNativeBoundary,
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

    let mir_schema_revision = ImplementationMirSchemaRevision::new(
        reader
            .read_u16()
            .map_err(wire_error(InterfaceValidationField::SchemaRevision))?,
    );

    if mir_schema_revision != CURRENT_MIR_SCHEMA_REVISION {
        return Err(value_mismatch(
            InterfaceValidationField::SchemaRevision,
            u64::from(CURRENT_MIR_SCHEMA_REVISION.raw()),
            u64::from(mir_schema_revision.raw()),
        ));
    }

    let length = reader
        .read_u64()
        .map_err(wire_error(InterfaceValidationField::RecordLength))?;

    limits.check(InterfaceLimit::BlobLength, length)?;

    let length = usize::try_from(length)
        .map_err(|_| numeric_overflow(InterfaceValidationField::RecordLength, length))?;

    let payload = reader
        .read_bytes(length)
        .map_err(wire_error(InterfaceValidationField::RecordPayload))?;

    reader
        .finish()
        .map_err(wire_error(InterfaceValidationField::RecordPayload))?;

    InterfacePreSpecializedMir::new(key, mir_schema_revision, Arc::<[u8]>::from(payload)).ok_or(
        crate::implementation::invalid_value(crate::InterfaceValidationField::RecordPayload),
    )
}

pub(super) fn specialization_discriminator(
    key: &PackageImplementationSpecializationKey,
) -> [u8; 32] {
    key.cache_identity()
}

pub(super) fn encode_native_binding(binding: &InterfaceNativeBinding) -> Vec<u8> {
    let mut encoder = WireEncoder::new();

    encode_specialization_key(binding.key(), &mut encoder);
    encode_producer_options(binding.producer_options(), &mut encoder);
    encoder.write_bytes(&binding.unit());
    write_string(&mut encoder, binding.symbol());

    encoder.into_bytes()
}

pub(super) fn decode_native_binding(
    owner: InterfaceSymbolId,
    payload: &[u8],
    limits: InterfaceValidationLimits,
) -> Result<InterfaceNativeBinding, InterfaceValidationError> {
    let mut reader = WireReader::new(payload);
    let key = decode_specialization_key(&mut reader, limits)?;
    let producer_options = decode_producer_options(&mut reader)?;

    let unit = reader.read_array::<32>()
        .map_err(wire_error(InterfaceValidationField::Hash))?;

    let symbol = read_nonempty_string(&mut reader, limits)?;

    reader.finish()
        .map_err(wire_error(InterfaceValidationField::RecordPayload))?;

    Ok(InterfaceNativeBinding::new(owner, key, producer_options, unit, symbol))
}

fn encode_producer_options(options: CodegenOptions, encoder: &mut WireEncoder) {
    encoder.write_u8(match options.optimization() {
        OptimizationLevel::None => 0,
        OptimizationLevel::Basic => 1,
        OptimizationLevel::Full => 2,
    });

    encoder.write_u8(match options.size_preference() {
        SizePreference::None => 0,
        SizePreference::Size => 1,
        SizePreference::MinimumSize => 2,
    });

    encoder.write_u8(match options.debug_information() {
        DebugInformationMode::None => 0,
        DebugInformationMode::LineTables => 1,
        DebugInformationMode::Full => 2,
    });

    encoder.write_u8(match options.reproducibility() {
        ReproducibilityLevel::Semantic => 0,
        ReproducibilityLevel::ByteForByte => 1,
    });

    match options.runtime_observations() {
        RuntimeObservationMode::None => encoder.write_u8(0),
        RuntimeObservationMode::Memory => encoder.write_u8(1),
        RuntimeObservationMode::PerformanceInterval { inner_iterations } => {
            encoder.write_u8(2);
            encoder.write_u64(inner_iterations.get());
        }
    }
}

fn decode_producer_options(reader: &mut WireReader<'_>) -> Result<CodegenOptions, InterfaceValidationError> {
    let read = |reader: &mut WireReader<'_>| {
        reader.read_u8().map_err(wire_error(InterfaceValidationField::Discriminant))
    };

    let optimization = match read(reader)? {
        0 => OptimizationLevel::None,
        1 => OptimizationLevel::Basic,
        2 => OptimizationLevel::Full,
        actual => return Err(invalid_discriminant(actual)),
    };

    let size_preference = match read(reader)? {
        0 => SizePreference::None,
        1 => SizePreference::Size,
        2 => SizePreference::MinimumSize,
        actual => return Err(invalid_discriminant(actual)),
    };

    let debug_information = match read(reader)? {
        0 => DebugInformationMode::None,
        1 => DebugInformationMode::LineTables,
        2 => DebugInformationMode::Full,
        actual => return Err(invalid_discriminant(actual)),
    };

    let reproducibility = match read(reader)? {
        0 => ReproducibilityLevel::Semantic,
        1 => ReproducibilityLevel::ByteForByte,
        actual => return Err(invalid_discriminant(actual)),
    };

    let runtime_observations = match read(reader)? {
        0 => RuntimeObservationMode::None,
        1 => RuntimeObservationMode::Memory,
        2 => {
            let iterations = reader.read_u64()
                .map_err(wire_error(InterfaceValidationField::RecordPayload))?;

            let inner_iterations = std::num::NonZeroU64::new(iterations)
                .ok_or(crate::implementation::invalid_value(InterfaceValidationField::RecordPayload))?;

            RuntimeObservationMode::PerformanceInterval { inner_iterations }
        }
        actual => return Err(invalid_discriminant(actual)),
    };

    Ok(CodegenOptions::new(
        optimization, size_preference, debug_information, reproducibility, runtime_observations,
    ))
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

    let raw_direction = reader
        .read_u8()
        .map_err(wire_error(InterfaceValidationField::Discriminant))?;

    let direction = match raw_direction {
        0 => ForeignCallableDirection::Import,
        1 => ForeignCallableDirection::Export,
        _ => {
            return Err(invalid_discriminant(raw_direction));
        }
    };

    let raw_kind = reader
        .read_u8()
        .map_err(wire_error(InterfaceValidationField::Discriminant))?;

    let kind = match raw_kind {
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
        _ => {
            return Err(invalid_discriminant(raw_kind));
        }
    };

    let raw_identity = reader
        .read_u8()
        .map_err(wire_error(InterfaceValidationField::Discriminant))?;

    let identity = match raw_identity {
        0 => NativeSymbolIdentity::Name(read_nonempty_string(&mut reader, limits)?),
        1 => NativeSymbolIdentity::Ordinal(
            reader
                .read_u64()
                .map_err(wire_error(InterfaceValidationField::Ordinal))?,
        ),
        _ => {
            return Err(invalid_discriminant(raw_identity));
        }
    };

    let raw_version = reader
        .read_u8()
        .map_err(wire_error(InterfaceValidationField::Discriminant))?;

    let version = match raw_version {
        0 => None,
        1 => Some(read_nonempty_string(&mut reader, limits)?),
        _ => {
            return Err(invalid_discriminant(raw_version));
        }
    };

    let raw_binding = reader
        .read_u8()
        .map_err(wire_error(InterfaceValidationField::Discriminant))?;

    let binding = match raw_binding {
        0 => NativeSymbolBinding::Strong,
        1 => NativeSymbolBinding::Weak,
        _ => {
            return Err(invalid_discriminant(raw_binding));
        }
    };

    let raw_presence = reader
        .read_u8()
        .map_err(wire_error(InterfaceValidationField::Discriminant))?;

    let presence = match raw_presence {
        0 => NativeSymbolPresence::Required,
        1 => NativeSymbolPresence::Optional,
        _ => {
            return Err(invalid_discriminant(raw_presence));
        }
    };

    reader
        .finish()
        .map_err(wire_error(InterfaceValidationField::RecordPayload))?;

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
    let length = reader
        .read_u64()
        .map_err(wire_error(InterfaceValidationField::String))?;

    limits.check(InterfaceLimit::StringLength, length)?;

    let length = usize::try_from(length)
        .map_err(|_| numeric_overflow(InterfaceValidationField::String, length))?;

    let offset = reader.position();

    let bytes = reader
        .read_bytes(length)
        .map_err(wire_error(InterfaceValidationField::String))?;

    let value =
        std::str::from_utf8(bytes).map_err(|cause| InterfaceValidationError::InvalidUtf8 {
            context: InterfaceValidationContext::Artifact,
            field: InterfaceValidationField::String,
            offset: offset as u64,
            length: length as u64,
            cause: cause.error_len().map_or(
                InterfaceUtf8Failure::IncompleteSequence,
                |error_length| InterfaceUtf8Failure::InvalidSequence {
                    error_length: Some(error_length as u64),
                },
            ),
        })?;

    NonEmptySharedStr::try_new(value).ok_or(crate::implementation::invalid_value(
        crate::InterfaceValidationField::String,
    ))
}

const fn invalid_discriminant(actual: u8) -> InterfaceValidationError {
    InterfaceValidationError::Malformed {
        context: InterfaceValidationContext::Artifact,
        cause: InterfaceMalformedCause::InvalidDiscriminant {
            field: InterfaceValidationField::Discriminant,
            actual: actual as u64,
        },
    }
}

fn wire_error(
    field: InterfaceValidationField,
) -> impl FnOnce(crate::wire::WireDecodeError) -> InterfaceValidationError {
    move |error| crate::decode::map_wire_error(InterfaceValidationContext::Artifact, field, error)
}

const fn value_mismatch(
    field: InterfaceValidationField,
    expected: u64,
    actual: u64,
) -> InterfaceValidationError {
    InterfaceValidationError::Malformed {
        context: InterfaceValidationContext::Artifact,
        cause: InterfaceMalformedCause::ValueMismatch {
            field,
            expected,
            actual,
        },
    }
}

const fn numeric_overflow(field: InterfaceValidationField, value: u64) -> InterfaceValidationError {
    InterfaceValidationError::Malformed {
        context: InterfaceValidationContext::Artifact,
        cause: InterfaceMalformedCause::NumericOverflow {
            field,
            value,
            target: InterfaceIntegerTarget::Usize,
        },
    }
}
