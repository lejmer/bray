use bray_symbols::{
    ExternalDeclarationIdentity, ExternalSymbolKey, ExternalSymbolKeyData, ModulePathKey,
    PackageIdentity, SymbolKind, SymbolName, SymbolOrdinal, SynthesizedSymbolRole,
};

use crate::decode::{DecodeBudget, map_wire_error};
use crate::tag::WireTag;
use crate::wire::{WireEncoder, WireReader};
use crate::{InterfaceLimit, InterfaceValidationError};

pub(crate) fn write_external_key(encoder: &mut WireEncoder, key: &ExternalSymbolKey) {
    let mut components = Vec::new();
    let mut current = Some(key);

    while let Some(component) = current {
        components.push(component);
        current = component.owner();
    }

    encoder.write_u32(checked_u32(components.len()));

    for component in components.into_iter().rev() {
        encoder.write_u32(component.kind().to_wire());

        match component.data() {
            ExternalSymbolKeyData::Package(package) => {
                encoder.write_u32(1);
                write_string(encoder, package.as_str());
            }
            ExternalSymbolKeyData::Module { path, .. } => {
                encoder.write_u32(2);
                encoder.write_u32(checked_u32(path.segments().len()));

                for segment in path.segments() {
                    write_string(encoder, segment);
                }
            }
            ExternalSymbolKeyData::Declaration { identity, .. } => {
                encoder.write_u32(3);

                match identity {
                    ExternalDeclarationIdentity::Name(name) => {
                        encoder.write_u32(1);
                        write_string(encoder, name.as_str());
                    }
                    ExternalDeclarationIdentity::Ordinal(ordinal) => {
                        encoder.write_u32(2);
                        encoder.write_u32(ordinal.raw());
                    }
                }
            }
            ExternalSymbolKeyData::Synthesized { role, ordinal, .. } => {
                encoder.write_u32(4);
                encoder.write_u32(role.to_wire());
                write_optional_u32(encoder, ordinal.map(SymbolOrdinal::raw));
            }
        }
    }
}

pub(crate) fn read_external_key(
    reader: &mut WireReader<'_>,
    budget: &mut DecodeBudget,
) -> Result<ExternalSymbolKey, InterfaceValidationError> {
    let count = read_count(reader, budget, InterfaceLimit::ExternalReferenceCount)?;

    if count == 0 {
        return Err(InterfaceValidationError::Malformed);
    }

    budget.charge_external_reference(count)?;

    let mut key = None;

    for _ in 0..count {
        let kind = SymbolKind::from_wire(reader.read_u32().map_err(map_wire_error)?)
            .ok_or(InterfaceValidationError::Malformed)?;

        let shape = reader.read_u32().map_err(map_wire_error)?;

        key = Some(match shape {
            1 if key.is_none() && kind == SymbolKind::Package => {
                let package = PackageIdentity::try_new(read_string(reader, budget.limits())?)
                    .ok_or(InterfaceValidationError::Malformed)?;

                ExternalSymbolKey::package(package)
            }
            2 if kind == SymbolKind::Module => {
                let owner = key.take().ok_or(InterfaceValidationError::Malformed)?;
                let segment_count = read_count(reader, budget, InterfaceLimit::RecordCount)?;

                let mut segments = budget.allocate_items_with_minimum(reader, segment_count, 5)?;

                for _ in 0..segment_count {
                    segments.push(read_string(reader, budget.limits())?);
                }

                let path =
                    ModulePathKey::try_new(segments).ok_or(InterfaceValidationError::Malformed)?;

                ExternalSymbolKey::module(owner, path).ok_or(InterfaceValidationError::Malformed)?
            }
            3 => {
                let owner = key.take().ok_or(InterfaceValidationError::Malformed)?;

                match reader.read_u32().map_err(map_wire_error)? {
                    1 => {
                        let name = SymbolName::try_new(read_string(reader, budget.limits())?)
                            .ok_or(InterfaceValidationError::Malformed)?;

                        ExternalSymbolKey::named(owner, kind, name)
                    }
                    2 => ExternalSymbolKey::ordinal(
                        owner,
                        kind,
                        SymbolOrdinal::new(reader.read_u32().map_err(map_wire_error)?),
                    ),
                    _ => return Err(InterfaceValidationError::Malformed),
                }
                .ok_or(InterfaceValidationError::Malformed)?
            }
            4 => {
                let owner = key.take().ok_or(InterfaceValidationError::Malformed)?;

                let role =
                    SynthesizedSymbolRole::from_wire(reader.read_u32().map_err(map_wire_error)?)
                        .ok_or(InterfaceValidationError::Malformed)?;

                let ordinal = match reader.read_u32().map_err(map_wire_error)? {
                    0 => None,
                    1 => Some(SymbolOrdinal::new(
                        reader.read_u32().map_err(map_wire_error)?,
                    )),
                    _ => return Err(InterfaceValidationError::Malformed),
                };

                let key = ExternalSymbolKey::synthesized(owner, role, ordinal)
                    .ok_or(InterfaceValidationError::Malformed)?;

                if key.kind() != kind {
                    return Err(InterfaceValidationError::Malformed);
                }

                key
            }
            _ => return Err(InterfaceValidationError::Malformed),
        });
    }

    key.ok_or(InterfaceValidationError::Malformed)
}

fn read_count(
    reader: &mut WireReader<'_>,
    budget: &DecodeBudget,
    limit: InterfaceLimit,
) -> Result<usize, InterfaceValidationError> {
    let count = reader.read_u32().map_err(map_wire_error)?;

    budget.limits().check(limit, u64::from(count))?;

    usize::try_from(count).map_err(|_| InterfaceValidationError::Malformed)
}

fn write_optional_u32(encoder: &mut WireEncoder, value: Option<u32>) {
    match value {
        Some(value) => {
            encoder.write_u32(1);
            encoder.write_u32(value);
        }
        None => encoder.write_u32(0),
    }
}

fn write_string(encoder: &mut WireEncoder, value: &str) {
    encoder.write_u32(checked_u32(value.len()));
    encoder.write_bytes(value.as_bytes());
}

fn read_string(
    reader: &mut WireReader<'_>,
    limits: crate::InterfaceValidationLimits,
) -> Result<std::sync::Arc<str>, InterfaceValidationError> {
    let length = reader.read_u32().map_err(map_wire_error)?;

    limits.check(InterfaceLimit::StringLength, u64::from(length))?;

    let length = usize::try_from(length).map_err(|_| InterfaceValidationError::Malformed)?;
    let bytes = reader.read_bytes(length).map_err(map_wire_error)?;
    let value = std::str::from_utf8(bytes).map_err(|_| InterfaceValidationError::Malformed)?;

    if value.is_empty() {
        return Err(InterfaceValidationError::Malformed);
    }

    Ok(std::sync::Arc::from(value))
}

fn checked_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
