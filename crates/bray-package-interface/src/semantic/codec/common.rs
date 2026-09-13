use std::str;
use std::sync::Arc;

use bray_symbols::{
    ExternalSymbolKey, SymbolKey, SymbolKeyData, SymbolKind, SymbolOrdinal, SynthesizedSymbolKey,
    SynthesizedSymbolRole,
};

use crate::decode::{
    DecodeBudget, read_optional_u32 as decode_optional_u32, read_u32 as decode_u32, wire_error,
};
pub(super) use crate::external_key::write_external_key;
use crate::tag::WireTag;
use crate::wire::{WireEncoder, WireReader};
use crate::{
    DependencyInterfaceId, InterfaceIntegerTarget, InterfaceLimit, InterfaceMalformedCause,
    InterfaceSectionTag, InterfaceSymbolReference, InterfaceUtf8Failure,
    InterfaceValidationContext, InterfaceValidationError, InterfaceValidationField,
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
    let shape = read_u32(reader)?;

    match shape {
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
                .ok_or_else(|| invalid_value(InterfaceValidationField::Reference))?;

            Ok(InterfaceSymbolReference::CompilerKnown(reference))
        }
        _ => Err(invalid_discriminant(
            InterfaceValidationField::Discriminant,
            shape,
        )),
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
        return Err(malformed(InterfaceMalformedCause::Missing {
            field: InterfaceValidationField::Identity,
        }));
    }

    let mut key = None;

    for index in 0..count {
        let shape = read_u32(reader)?;

        key = Some(match shape {
            1 if index == 0 => {
                let declaration = bray_compiler_known::CompilerKnownDeclarationKey::try_new(
                    read_string(reader, context)?,
                )
                .ok_or_else(|| invalid_value(InterfaceValidationField::Declaration))?;

                let kind_raw = read_u32(reader)?;

                let kind = SymbolKind::from_wire(kind_raw).ok_or_else(|| {
                    invalid_discriminant(InterfaceValidationField::SymbolKind, kind_raw)
                })?;

                SymbolKey::compiler_known_declaration(declaration, kind)
                    .ok_or_else(|| invalid_value(InterfaceValidationField::Identity))?
            }
            2 if index > 0 => {
                let subject = key.ok_or_else(|| {
                    malformed(InterfaceMalformedCause::Missing {
                        field: InterfaceValidationField::Subject,
                    })
                })?;

                let role_raw = read_u32(reader)?;

                let role = SynthesizedSymbolRole::from_wire(role_raw).ok_or_else(|| {
                    invalid_discriminant(InterfaceValidationField::Role, role_raw)
                })?;

                let ordinal = read_optional_u32(reader)?.map(SymbolOrdinal::new);

                let synthesized = SynthesizedSymbolKey::try_new(role, subject, ordinal)
                    .ok_or_else(|| invalid_value(InterfaceValidationField::Identity))?;

                SymbolKey::synthesized(synthesized)
            }
            _ => {
                return Err(invalid_discriminant(
                    InterfaceValidationField::Discriminant,
                    shape,
                ));
            }
        });
    }

    key.ok_or_else(|| {
        malformed(InterfaceMalformedCause::Missing {
            field: InterfaceValidationField::Identity,
        })
    })
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
    validation: InterfaceValidationContext,
}

impl SemanticDecodeContext {
    pub(crate) const fn new(limits: InterfaceValidationLimits) -> Self {
        Self {
            budget: DecodeBudget::new(limits),
            validation: InterfaceValidationContext::Artifact,
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
        self.budget.allocate_items(
            reader,
            self.validation,
            InterfaceValidationField::Value,
            count,
        )
    }

    pub(crate) fn allocate_derived_items<T>(
        &mut self,
        count: usize,
    ) -> Result<Vec<T>, InterfaceValidationError> {
        self.budget
            .allocate_derived_items(self.validation, InterfaceValidationField::Value, count)
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

    pub(crate) const fn validation(&self) -> InterfaceValidationContext {
        self.validation
    }

    pub(crate) fn replace_validation(
        &mut self,
        validation: InterfaceValidationContext,
    ) -> InterfaceValidationContext {
        std::mem::replace(&mut self.validation, validation)
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

    let offset = reader.position();

    let bytes = reader.read_bytes(length).map_err(wire_error(
        context.validation(),
        InterfaceValidationField::String,
    ))?;

    let value = str::from_utf8(bytes).map_err(|cause| InterfaceValidationError::InvalidUtf8 {
        context: context.validation(),
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

    usize::try_from(value).map_err(|_| {
        malformed(InterfaceMalformedCause::NumericOverflow {
            field: InterfaceValidationField::RecordCount,
            value: u64::from(value),
            target: InterfaceIntegerTarget::Usize,
        })
    })
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

pub(super) fn read_u32(reader: &mut WireReader<'_>) -> Result<u32, InterfaceValidationError> {
    decode_u32(
        reader,
        InterfaceValidationContext::Section(InterfaceSectionTag::SemanticRecordDirectory),
        InterfaceValidationField::Value,
    )
}

pub(super) fn read_optional_u32(
    reader: &mut WireReader<'_>,
) -> Result<Option<u32>, InterfaceValidationError> {
    decode_optional_u32(
        reader,
        InterfaceValidationContext::Section(InterfaceSectionTag::SemanticRecordDirectory),
        InterfaceValidationField::Value,
    )
}

pub(super) fn map_wire_error(error: crate::wire::WireDecodeError) -> InterfaceValidationError {
    crate::decode::map_wire_error(
        InterfaceValidationContext::Section(InterfaceSectionTag::SemanticRecordDirectory),
        InterfaceValidationField::RecordPayload,
        error,
    )
}

pub(super) const fn malformed(cause: InterfaceMalformedCause) -> InterfaceValidationError {
    InterfaceValidationError::Malformed {
        context: InterfaceValidationContext::Section(InterfaceSectionTag::SemanticRecordDirectory),
        cause,
    }
}

pub(in crate::semantic) const fn invalid_value(
    field: InterfaceValidationField,
) -> InterfaceValidationError {
    malformed(InterfaceMalformedCause::InvalidValue { field })
}

pub(in crate::semantic) const fn invalid_discriminant(
    field: InterfaceValidationField,
    actual: u32,
) -> InterfaceValidationError {
    malformed(InterfaceMalformedCause::InvalidDiscriminant {
        field,
        actual: actual as u64,
    })
}
