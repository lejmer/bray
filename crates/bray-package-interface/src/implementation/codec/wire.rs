use std::str;
use std::sync::Arc;

use bray_symbols::PackageIdentity;

use crate::decode::DecodeBudget;
use crate::wire::{WireEncoder, WireReader};
use crate::{
    InterfaceContentHash, InterfaceDependency, InterfaceLimit, InterfaceMalformedCause,
    InterfaceProductIdentity, InterfaceProductKind, InterfaceUtf8Failure,
    InterfaceValidationContext, InterfaceValidationError, InterfaceValidationField,
    InterfaceValidationLimits,
};

use super::super::{
    ImplementationSpecializationArgument, ImplementationSpecializationArgumentKind,
};

pub(super) fn write_arguments(
    encoder: &mut WireEncoder,
    arguments: &[ImplementationSpecializationArgument],
) {
    encoder.write_u32(checked_u32(arguments.len()));

    for argument in arguments {
        encoder.write_u8(match argument.kind() {
            ImplementationSpecializationArgumentKind::Type => 0,
            ImplementationSpecializationArgumentKind::Constant => 1,
        });

        encoder.write_bytes(&argument.identity());
    }
}

pub(super) fn read_arguments(
    reader: &mut WireReader<'_>,
    budget: &mut DecodeBudget,
) -> Result<Vec<ImplementationSpecializationArgument>, InterfaceValidationError> {
    let count = reader
        .read_u32()
        .map_err(wire_error(InterfaceValidationField::RecordCount))?;

    budget
        .limits()
        .check(InterfaceLimit::RecordCount, u64::from(count))?;

    let count = usize::try_from(count).map_err(|_| {
        crate::implementation::invalid_value(crate::InterfaceValidationField::RecordCount)
    })?;

    let mut arguments = budget.allocate_items_with_minimum(
        reader,
        InterfaceValidationContext::Artifact,
        InterfaceValidationField::Argument,
        count,
        33,
    )?;

    for _ in 0..count {
        let kind = match reader
            .read_u8()
            .map_err(wire_error(InterfaceValidationField::Discriminant))?
        {
            0 => ImplementationSpecializationArgumentKind::Type,
            1 => ImplementationSpecializationArgumentKind::Constant,
            actual => {
                return Err(invalid_discriminant(
                    InterfaceValidationField::Discriminant,
                    u64::from(actual),
                ));
            }
        };

        let identity = reader
            .read_array::<32>()
            .map_err(wire_error(InterfaceValidationField::Identity))?;

        arguments.push(ImplementationSpecializationArgument::new(kind, identity));
    }

    Ok(arguments)
}

pub(super) fn write_dependency(encoder: &mut WireEncoder, dependency: &InterfaceDependency) {
    write_string(encoder, dependency.package().as_str());
    write_string(encoder, dependency.product().as_str());
    encoder.write_bytes(dependency.content_hash().as_bytes());
}

pub(super) fn read_dependency(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
) -> Result<InterfaceDependency, InterfaceValidationError> {
    let package = PackageIdentity::try_new(read_string(reader, limits)?).ok_or(
        crate::implementation::invalid_value(crate::InterfaceValidationField::PackageName),
    )?;

    let product = InterfaceProductIdentity::try_new(read_string(reader, limits)?).ok_or(
        crate::implementation::invalid_value(crate::InterfaceValidationField::ProductName),
    )?;

    let content_hash = InterfaceContentHash::from_bytes(
        reader
            .read_array::<32>()
            .map_err(wire_error(InterfaceValidationField::ContentHash))?,
    );

    Ok(InterfaceDependency::new(package, product, content_hash))
}

pub(super) fn write_string(encoder: &mut WireEncoder, value: &str) {
    encoder.write_u32(checked_u32(value.len()));
    encoder.write_bytes(value.as_bytes());
}

pub(super) fn read_string(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
) -> Result<Arc<str>, InterfaceValidationError> {
    let length = reader
        .read_u32()
        .map_err(wire_error(InterfaceValidationField::String))?;

    limits.check(InterfaceLimit::StringLength, u64::from(length))?;

    let length = usize::try_from(length).map_err(|_| {
        crate::implementation::invalid_value(crate::InterfaceValidationField::String)
    })?;

    let offset = reader.position();

    let bytes = reader
        .read_bytes(length)
        .map_err(wire_error(InterfaceValidationField::String))?;

    let value = str::from_utf8(bytes).map_err(|cause| InterfaceValidationError::InvalidUtf8 {
        context: InterfaceValidationContext::Artifact,
        field: InterfaceValidationField::String,
        offset: offset as u64,
        length: length as u64,
        cause: cause
            .error_len()
            .map_or(InterfaceUtf8Failure::IncompleteSequence, |error_length| {
                InterfaceUtf8Failure::InvalidSequence {
                    error_length: Some(error_length as u64),
                }
            }),
    })?;

    if value.is_empty() {
        return Err(crate::implementation::invalid_value(
            crate::InterfaceValidationField::String,
        ));
    }

    Ok(Arc::from(value))
}

pub(super) const fn product_kind_to_wire(kind: InterfaceProductKind) -> u32 {
    match kind {
        InterfaceProductKind::Library => 0,
        InterfaceProductKind::Executable => 1,
        InterfaceProductKind::Test => 2,
    }
}

pub(super) const fn product_kind_from_wire(
    raw: u32,
) -> Result<InterfaceProductKind, InterfaceValidationError> {
    match raw {
        0 => Ok(InterfaceProductKind::Library),
        1 => Ok(InterfaceProductKind::Executable),
        2 => Ok(InterfaceProductKind::Test),
        _ => Err(invalid_discriminant(
            InterfaceValidationField::Discriminant,
            raw as u64,
        )),
    }
}

pub(super) fn checked_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

pub(crate) const fn invalid_value(field: InterfaceValidationField) -> InterfaceValidationError {
    InterfaceValidationError::Malformed {
        context: InterfaceValidationContext::Artifact,
        cause: InterfaceMalformedCause::InvalidValue { field },
    }
}

pub(super) const fn invalid_discriminant(
    field: InterfaceValidationField,
    actual: u64,
) -> InterfaceValidationError {
    InterfaceValidationError::Malformed {
        context: InterfaceValidationContext::Artifact,
        cause: InterfaceMalformedCause::InvalidDiscriminant { field, actual },
    }
}

pub(crate) fn map_wire_error(error: crate::wire::WireDecodeError) -> InterfaceValidationError {
    crate::decode::map_wire_error(
        InterfaceValidationContext::Artifact,
        InterfaceValidationField::Value,
        error,
    )
}

pub(super) fn wire_error(
    field: InterfaceValidationField,
) -> impl FnOnce(crate::wire::WireDecodeError) -> InterfaceValidationError {
    move |error| crate::decode::map_wire_error(InterfaceValidationContext::Artifact, field, error)
}
