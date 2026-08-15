use std::str;
use std::sync::Arc;

use bray_symbols::{
    ExternalSymbolKey, SymbolKey, SymbolKeyData, SymbolKind, SymbolOrdinal, SynthesizedSymbolKey,
    SynthesizedSymbolRole,
};

use crate::decode::DecodeBudget;
pub(super) use crate::decode::{map_wire_error, read_optional_u32, read_u32};
pub(super) use crate::external_key::write_external_key;
use crate::tag::WireTag;
use crate::wire::{WireEncoder, WireReader};
use crate::{
    DependencyInterfaceId, InterfaceLimit, InterfaceSymbolReference, InterfaceValidationError,
    InterfaceValidationLimits,
};

pub(in crate::semantic) const DECLARATION_SEMANTICS_FORMAT_VERSION: u32 = 1;
pub(in crate::semantic) const DECLARATION_TEMPLATE_FORMAT_VERSION: u32 = 1;

pub(crate) fn write_symbol_reference(
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
        InterfaceSymbolReference::CompilerKnown(reference) => {
            encoder.write_u32(3);
            write_compiler_known_key(encoder, reference.key());
        }
    }
}

pub(crate) fn read_symbol_reference(
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
        3 => {
            let key = read_compiler_known_key(reader, context)?;

            let reference = crate::CompilerKnownSymbolReference::try_new(key)
                .ok_or(InterfaceValidationError::Malformed)?;

            Ok(InterfaceSymbolReference::CompilerKnown(reference))
        }
        _ => Err(InterfaceValidationError::Malformed),
    }
}

fn write_compiler_known_key(encoder: &mut WireEncoder, key: &SymbolKey) {
    let mut components = Vec::new();
    let mut current = key;

    loop {
        components.push(current);

        match current.data() {
            SymbolKeyData::Synthesized(key) => current = key.subject(),
            SymbolKeyData::CompilerKnownDeclaration { .. } => break,
            _ => unreachable!("validated compiler-known references have catalog roots"),
        }
    }

    components.reverse();
    write_count(encoder, components.len());

    for component in components {
        match component.data() {
            SymbolKeyData::CompilerKnownDeclaration { key, kind } => {
                encoder.write_u32(1);
                write_string(encoder, key.as_str());
                encoder.write_u32(kind.to_wire());
            }
            SymbolKeyData::Synthesized(key) => {
                encoder.write_u32(2);
                encoder.write_u32(key.role().to_wire());
                write_optional_u32(encoder, key.ordinal().map(SymbolOrdinal::raw));
            }
            _ => unreachable!("validated compiler-known references contain known components"),
        }
    }
}

fn read_compiler_known_key(
    reader: &mut WireReader<'_>,
    context: &mut SemanticDecodeContext,
) -> Result<SymbolKey, InterfaceValidationError> {
    let count = read_count(
        reader,
        context.limits(),
        InterfaceLimit::ExternalReferenceCount,
    )?;

    context.charge_external_reference(count)?;

    if count == 0 {
        return Err(InterfaceValidationError::Malformed);
    }

    let mut key = None;

    for index in 0..count {
        key = Some(match read_u32(reader)? {
            1 if index == 0 => {
                let declaration = bray_compiler_known::CompilerKnownDeclarationKey::try_new(
                    read_string(reader, context)?,
                )
                .ok_or(InterfaceValidationError::Malformed)?;

                let kind = SymbolKind::from_wire(read_u32(reader)?)
                    .ok_or(InterfaceValidationError::Malformed)?;

                SymbolKey::compiler_known_declaration(declaration, kind)
                    .ok_or(InterfaceValidationError::Malformed)?
            }
            2 if index > 0 => {
                let subject = key.ok_or(InterfaceValidationError::Malformed)?;

                let role = SynthesizedSymbolRole::from_wire(read_u32(reader)?)
                    .ok_or(InterfaceValidationError::Malformed)?;

                let ordinal = read_optional_u32(reader)?.map(SymbolOrdinal::new);

                let synthesized = SynthesizedSymbolKey::try_new(role, subject, ordinal)
                    .ok_or(InterfaceValidationError::Malformed)?;

                SymbolKey::synthesized(synthesized)
            }
            _ => return Err(InterfaceValidationError::Malformed),
        });
    }

    key.ok_or(InterfaceValidationError::Malformed)
}

pub(super) fn write_symbol_references(
    encoder: &mut WireEncoder,
    references: &[InterfaceSymbolReference],
) {
    write_count(encoder, references.len());

    for reference in references {
        write_symbol_reference(encoder, reference);
    }
}

pub(super) fn read_symbol_references(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<Vec<InterfaceSymbolReference>, InterfaceValidationError> {
    let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
    let mut references = context.allocate_items(reader, count)?;

    for _ in 0..count {
        references.push(read_symbol_reference(reader, context)?);
    }

    Ok(references)
}

pub(super) fn read_external_key(
    reader: &mut WireReader<'_>,
    context: &mut SemanticDecodeContext,
) -> Result<ExternalSymbolKey, InterfaceValidationError> {
    crate::external_key::read_external_key(reader, &mut context.budget)
}

pub(crate) struct SemanticDecodeContext {
    budget: DecodeBudget,
}

impl SemanticDecodeContext {
    pub(crate) const fn new(limits: InterfaceValidationLimits) -> Self {
        Self {
            budget: DecodeBudget::new(limits),
        }
    }

    pub(crate) const fn limits(&self) -> InterfaceValidationLimits {
        self.budget.limits()
    }

    pub(crate) fn allocate_items<T>(
        &mut self,
        reader: &WireReader<'_>,
        count: usize,
    ) -> Result<Vec<T>, InterfaceValidationError> {
        self.budget.allocate_items(reader, count)
    }

    pub(crate) fn allocate_derived_items<T>(
        &mut self,
        count: usize,
    ) -> Result<Vec<T>, InterfaceValidationError> {
        self.budget.allocate_derived_items(count)
    }

    pub(crate) fn charge_items<T>(&mut self, count: usize) -> Result<(), InterfaceValidationError> {
        self.budget.charge_items::<T>(count)
    }

    fn charge_external_reference(
        &mut self,
        component_count: usize,
    ) -> Result<(), InterfaceValidationError> {
        self.budget.charge_external_reference(component_count)
    }

    fn charge(&mut self, bytes: usize) -> Result<(), InterfaceValidationError> {
        self.budget.charge(bytes)
    }
}

pub(super) fn write_string(encoder: &mut WireEncoder, value: &str) {
    write_count(encoder, value.len());
    encoder.write_bytes(value.as_bytes());
}

pub(super) fn read_string(
    reader: &mut WireReader<'_>,
    context: &mut SemanticDecodeContext,
) -> Result<Arc<str>, InterfaceValidationError> {
    let length = read_count(reader, context.limits(), InterfaceLimit::StringLength)?;

    context.charge(length)?;

    let bytes = reader.read_bytes(length).map_err(map_wire_error)?;
    let value = str::from_utf8(bytes).map_err(|_| InterfaceValidationError::Malformed)?;

    Ok(Arc::from(value))
}

pub(super) fn write_count(encoder: &mut WireEncoder, count: usize) {
    encoder.write_count(count);
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
