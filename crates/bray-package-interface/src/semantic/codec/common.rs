use std::str;
use std::sync::Arc;

use bray_symbols::{
    ExternalDeclarationIdentity, ExternalSymbolKey, ExternalSymbolKeyData, ModulePathKey,
    PackageIdentity, SymbolKind, SymbolName, SymbolOrdinal, SynthesizedSymbolRole,
};

use crate::decode::DecodeBudget;
use crate::tag::WireTag;
use crate::wire::{WireDecodeError, WireEncoder, WireReader};
use crate::{
    DependencyInterfaceId, InterfaceLimit, InterfaceSymbolReference, InterfaceValidationError,
    InterfaceValidationLimits,
};

pub(super) fn write_symbol_reference(
    encoder: &mut WireEncoder,
    reference: &InterfaceSymbolReference,
) {
    match reference {
        InterfaceSymbolReference::Local(id) => {
            encoder.write_u32(1);
            encoder.write_u32(id.raw());
        }
        InterfaceSymbolReference::Dependency { dependency, key } => {
            encoder.write_u32(2);
            encoder.write_u32(dependency.raw());
            write_external_key(encoder, key);
        }
    }
}

pub(super) fn read_symbol_reference(
    reader: &mut WireReader<'_>,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceSymbolReference, InterfaceValidationError> {
    match read_u32(reader)? {
        1 => Ok(InterfaceSymbolReference::Local(
            bray_symbols::InterfaceSymbolId::new(read_u32(reader)?),
        )),
        2 => Ok(InterfaceSymbolReference::Dependency {
            dependency: DependencyInterfaceId::new(read_u32(reader)?),
            key: read_external_key(reader, context)?,
        }),
        _ => Err(InterfaceValidationError::Malformed),
    }
}

fn write_external_key(encoder: &mut WireEncoder, key: &ExternalSymbolKey) {
    let mut components = Vec::new();
    let mut current = Some(key);

    while let Some(key) = current {
        components.push(key);
        current = match key.data() {
            ExternalSymbolKeyData::Package(_) => None,
            ExternalSymbolKeyData::Module { package, .. } => Some(package),
            ExternalSymbolKeyData::Declaration { owner, .. }
            | ExternalSymbolKeyData::Synthesized { owner, .. } => Some(owner),
        };
    }

    components.reverse();
    write_count(encoder, components.len());

    for component in components {
        encoder.write_u32(component.kind().to_wire());

        match component.data() {
            ExternalSymbolKeyData::Package(package) => {
                encoder.write_u32(1);
                write_string(encoder, package.as_str());
            }
            ExternalSymbolKeyData::Module { path, .. } => {
                encoder.write_u32(2);
                write_count(encoder, path.segments().len());

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

fn read_external_key(
    reader: &mut WireReader<'_>,
    context: &mut SemanticDecodeContext,
) -> Result<ExternalSymbolKey, InterfaceValidationError> {
    let limits = context.limits();
    let count = read_count(reader, limits, InterfaceLimit::ExternalReferenceCount)?;

    context.charge_external_reference(count)?;

    if count == 0 {
        return Err(InterfaceValidationError::Malformed);
    }

    let mut key = None;

    for _ in 0..count {
        let kind =
            SymbolKind::from_wire(read_u32(reader)?).ok_or(InterfaceValidationError::Malformed)?;
        let shape = read_u32(reader)?;

        key = Some(match shape {
            1 if key.is_none() && kind == SymbolKind::Package => {
                let package = PackageIdentity::try_new(read_string(reader, limits)?)
                    .ok_or(InterfaceValidationError::Malformed)?;

                ExternalSymbolKey::package(package)
            }
            2 if kind == SymbolKind::Module => {
                let owner = key.ok_or(InterfaceValidationError::Malformed)?;
                let segment_count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
                let mut segments = Vec::with_capacity(segment_count);

                for _ in 0..segment_count {
                    segments.push(read_string(reader, limits)?);
                }

                let path =
                    ModulePathKey::try_new(segments).ok_or(InterfaceValidationError::Malformed)?;

                ExternalSymbolKey::module(owner, path).ok_or(InterfaceValidationError::Malformed)?
            }
            3 => {
                let owner = key.ok_or(InterfaceValidationError::Malformed)?;

                match read_u32(reader)? {
                    1 => {
                        let name = SymbolName::try_new(read_string(reader, limits)?)
                            .ok_or(InterfaceValidationError::Malformed)?;

                        ExternalSymbolKey::named(owner, kind, name)
                    }
                    2 => ExternalSymbolKey::ordinal(
                        owner,
                        kind,
                        SymbolOrdinal::new(read_u32(reader)?),
                    ),
                    _ => return Err(InterfaceValidationError::Malformed),
                }
                .ok_or(InterfaceValidationError::Malformed)?
            }
            4 => {
                let owner = key.ok_or(InterfaceValidationError::Malformed)?;
                let role = SynthesizedSymbolRole::from_wire(read_u32(reader)?)
                    .ok_or(InterfaceValidationError::Malformed)?;
                let ordinal = read_optional_u32(reader)?.map(SymbolOrdinal::new);
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

pub(super) struct SemanticDecodeContext {
    budget: DecodeBudget,
}

impl SemanticDecodeContext {
    pub(super) const fn new(limits: InterfaceValidationLimits) -> Self {
        Self {
            budget: DecodeBudget::new(limits),
        }
    }

    pub(super) const fn limits(&self) -> InterfaceValidationLimits {
        self.budget.limits()
    }

    fn charge_external_reference(
        &mut self,
        component_count: usize,
    ) -> Result<(), InterfaceValidationError> {
        self.budget.charge_external_reference(component_count)
    }
}

pub(super) fn write_string(encoder: &mut WireEncoder, value: &str) {
    write_count(encoder, value.len());
    encoder.write_bytes(value.as_bytes());
}

pub(super) fn read_string(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
) -> Result<Arc<str>, InterfaceValidationError> {
    let length = read_count(reader, limits, InterfaceLimit::StringLength)?;
    let bytes = reader.read_bytes(length).map_err(map_wire_error)?;
    let value = str::from_utf8(bytes).map_err(|_| InterfaceValidationError::Malformed)?;

    Ok(Arc::from(value))
}

pub(super) fn write_count(encoder: &mut WireEncoder, count: usize) {
    let value = match u32::try_from(count) {
        Ok(value) => value,
        Err(_) => unreachable!("validated interface counts fit the wire representation"),
    };

    encoder.write_u32(value);
}

pub(super) fn read_count(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    limit: InterfaceLimit,
) -> Result<usize, InterfaceValidationError> {
    let value = read_u32(reader)?;

    limits.check(limit, u64::from(value))?;

    usize::try_from(value).map_err(|_| InterfaceValidationError::Malformed)
}

pub(super) fn write_optional_u32(encoder: &mut WireEncoder, value: Option<u32>) {
    match value {
        Some(value) => {
            encoder.write_u32(1);
            encoder.write_u32(value);
        }
        None => encoder.write_u32(0),
    }
}

pub(super) fn read_optional_u32(
    reader: &mut WireReader<'_>,
) -> Result<Option<u32>, InterfaceValidationError> {
    match read_u32(reader)? {
        0 => Ok(None),
        1 => Ok(Some(read_u32(reader)?)),
        _ => Err(InterfaceValidationError::Malformed),
    }
}

pub(super) fn read_u32(reader: &mut WireReader<'_>) -> Result<u32, InterfaceValidationError> {
    reader.read_u32().map_err(map_wire_error)
}

pub(super) const fn map_wire_error(error: WireDecodeError) -> InterfaceValidationError {
    match error {
        WireDecodeError::Truncated => InterfaceValidationError::Truncated,
        WireDecodeError::TrailingBytes => InterfaceValidationError::Malformed,
    }
}
