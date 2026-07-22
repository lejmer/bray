use super::common::{decode_tag, validate_record_count};
use crate::semantic::codec::common::{
    SemanticDecodeContext, map_wire_error, read_count, read_symbol_reference, read_u32,
};
use crate::semantic::codec::record::RecordTable;
use crate::semantic::model::{
    InterfaceCallableParameterDefault, InterfaceCallableReceiver, InterfaceCallableSignature,
    InterfaceGenericDeclaration, InterfaceSemanticFacts, InterfaceTypeId,
};
use crate::wire::WireReader;
use crate::{
    InterfaceLimit, InterfaceValidationError, InterfaceValidationLimits, ValidatedInterfaceSection,
};

pub(super) struct DeclarationRecordTables<'bytes> {
    pub(super) callable_signatures: RecordTable<'bytes>,
    pub(super) generic_declarations: RecordTable<'bytes>,
    pub(super) callable_parameter_defaults: RecordTable<'bytes>,
}

pub(super) fn decode_declaration_tables<'bytes>(
    section: ValidatedInterfaceSection<'bytes>,
    context: &mut SemanticDecodeContext,
) -> Result<DeclarationRecordTables<'bytes>, InterfaceValidationError> {
    let mut reader = WireReader::new(section.bytes());
    let revision = read_u32(&mut reader)?;

    if revision != super::super::DECLARATION_FACT_SECTION_REVISION {
        return Err(InterfaceValidationError::Malformed);
    }

    let callable_signatures = RecordTable::read_from(&mut reader, context)?;
    let generic_declarations = RecordTable::read_from(&mut reader, context)?;
    let callable_parameter_defaults = RecordTable::read_from(&mut reader, context)?;

    validate_record_count(
        section,
        [
            callable_signatures.len(),
            generic_declarations.len(),
            callable_parameter_defaults.len(),
        ],
    )?;

    reader.finish().map_err(map_wire_error)?;

    Ok(DeclarationRecordTables {
        callable_signatures,
        generic_declarations,
        callable_parameter_defaults,
    })
}

pub(super) fn decode_declarations(
    section: ValidatedInterfaceSection<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
    facts: &mut InterfaceSemanticFacts,
) -> Result<(), InterfaceValidationError> {
    let tables = decode_declaration_tables(section, context)?;

    let callable_signatures = tables
        .callable_signatures
        .decode_all(context, |reader, context| {
            decode_callable_signature(reader, limits, context)
        })?;

    let generic_declarations = tables
        .generic_declarations
        .decode_all(context, |reader, context| {
            decode_generic_declaration(reader, limits, context)
        })?;

    let callable_parameter_defaults = tables
        .callable_parameter_defaults
        .decode_all(context, decode_callable_parameter_default)?;

    facts.callable_signatures = callable_signatures.into();
    facts.generic_declarations = generic_declarations.into();
    facts.callable_parameter_defaults = callable_parameter_defaults.into();

    Ok(())
}

pub(super) fn decode_callable_signature(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceCallableSignature, InterfaceValidationError> {
    let owner = read_symbol_reference(reader, context)?;
    let callable_type = InterfaceTypeId::new(read_u32(reader)?);

    let receiver = match read_u32(reader)? {
        0 => None,
        1 => Some(InterfaceCallableReceiver::new(
            read_symbol_reference(reader, context)?,
            InterfaceTypeId::new(read_u32(reader)?),
            decode_tag(read_u32(reader)?)?,
        )),
        _ => return Err(InterfaceValidationError::Malformed),
    };

    let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
    let mut parameters = context.allocate_items(reader, count)?;

    for _ in 0..count {
        parameters.push(read_symbol_reference(reader, context)?);
    }

    let result = InterfaceTypeId::new(read_u32(reader)?);

    Ok(InterfaceCallableSignature::new(
        owner,
        callable_type,
        receiver,
        parameters,
        result,
    ))
}

pub(super) fn decode_generic_declaration(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceGenericDeclaration, InterfaceValidationError> {
    let owner = read_symbol_reference(reader, context)?;
    let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
    let mut parameters = context.allocate_items(reader, count)?;

    for _ in 0..count {
        parameters.push(read_symbol_reference(reader, context)?);
    }

    Ok(InterfaceGenericDeclaration::new(owner, parameters))
}

pub(super) fn decode_callable_parameter_default(
    reader: &mut WireReader<'_>,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceCallableParameterDefault, InterfaceValidationError> {
    let parameter = read_symbol_reference(reader, context)?;

    let is_present = match read_u32(reader)? {
        0 => false,
        1 => true,
        _ => return Err(InterfaceValidationError::Malformed),
    };

    Ok(InterfaceCallableParameterDefault::new(
        parameter, is_present,
    ))
}
