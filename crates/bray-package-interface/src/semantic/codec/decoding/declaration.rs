use super::common::{decode_tag, validate_record_count};
use crate::semantic::codec::common::{
    SemanticDecodeContext, map_wire_error, read_count, read_symbol_reference, read_u32,
};
use crate::semantic::codec::record::RecordTable;
use crate::semantic::model::{
    InterfaceCallableParameterDefault, InterfaceCallableReceiver, InterfaceCallableSignature,
    InterfaceDeclaredType, InterfaceGenericDeclaration, InterfacePredicateDefinition,
    InterfaceSemantics, InterfaceStorageMember, InterfaceStorageShape, InterfaceTypeId,
    InterfaceTypeRepresentation, InterfaceUnionStorageVariant, InterfaceUnionTag,
};
use crate::wire::WireReader;
use crate::{
    InterfaceLimit, InterfaceValidationError, InterfaceValidationLimits, ValidatedInterfaceSection,
};

pub(super) struct DeclarationRecordTables<'bytes> {
    pub(super) callable_signatures: RecordTable<'bytes>,
    pub(super) generic_declarations: RecordTable<'bytes>,
    pub(super) callable_parameter_defaults: RecordTable<'bytes>,
    pub(super) predicate_definitions: RecordTable<'bytes>,
    pub(super) declared_types: RecordTable<'bytes>,
    pub(super) type_representations: RecordTable<'bytes>,
}

pub(super) fn decode_declaration_tables<'bytes>(
    section: ValidatedInterfaceSection<'bytes>,
    context: &mut SemanticDecodeContext,
) -> Result<DeclarationRecordTables<'bytes>, InterfaceValidationError> {
    let mut reader = WireReader::new(section.bytes());
    let format_version = read_u32(&mut reader)?;

    if format_version != super::super::DECLARATION_SEMANTICS_FORMAT_VERSION {
        return Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Declaration,
        ));
    }

    let callable_signatures = RecordTable::read_from(&mut reader, context, section.tag())?;
    let generic_declarations = RecordTable::read_from(&mut reader, context, section.tag())?;
    let callable_parameter_defaults = RecordTable::read_from(&mut reader, context, section.tag())?;
    let predicate_definitions = RecordTable::read_from(&mut reader, context, section.tag())?;
    let declared_types = RecordTable::read_from(&mut reader, context, section.tag())?;
    let type_representations = RecordTable::read_from(&mut reader, context, section.tag())?;

    validate_record_count(
        section,
        [
            callable_signatures.len(),
            generic_declarations.len(),
            callable_parameter_defaults.len(),
            predicate_definitions.len(),
            declared_types.len(),
            type_representations.len(),
        ],
    )?;

    reader.finish().map_err(map_wire_error)?;

    Ok(DeclarationRecordTables {
        callable_signatures,
        generic_declarations,
        callable_parameter_defaults,
        predicate_definitions,
        declared_types,
        type_representations,
    })
}

pub(super) fn decode_declarations(
    section: ValidatedInterfaceSection<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
    semantics: &mut InterfaceSemantics,
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

    let predicate_definitions = tables
        .predicate_definitions
        .decode_all(context, decode_predicate_definition)?;

    let declared_types = tables
        .declared_types
        .decode_all(context, decode_declared_type)?;

    let type_representations = tables
        .type_representations
        .decode_all(context, |reader, context| {
            decode_type_representation(reader, limits, context)
        })?;

    semantics.callable_signatures = callable_signatures.into();
    semantics.generic_declarations = generic_declarations.into();
    semantics.callable_parameter_defaults = callable_parameter_defaults.into();
    semantics.predicate_definitions = predicate_definitions.into();
    semantics.declared_types = declared_types.into();
    semantics.type_representations = type_representations.into();

    Ok(())
}

pub(super) fn decode_declared_type(
    reader: &mut WireReader<'_>,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceDeclaredType, InterfaceValidationError> {
    let owner = read_symbol_reference(reader, context)?;
    let ty = InterfaceTypeId::new(read_u32(reader)?);

    Ok(InterfaceDeclaredType::new(owner, ty))
}

fn decode_type_representation(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceTypeRepresentation, InterfaceValidationError> {
    let owner = read_symbol_reference(reader, context)?;

    let layout = match read_u32(reader)? {
        1 => bray_symbols::DeclaredLayoutMode::Default,
        2 => bray_symbols::DeclaredLayoutMode::Stable,
        3 => bray_symbols::DeclaredLayoutMode::C,
        4 => bray_symbols::DeclaredLayoutMode::Transparent,
        _ => {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Declaration,
            ));
        }
    };

    let alignment = read_optional_u64(reader)?;
    let packing = read_optional_u64(reader)?;
    let opaque_size = read_optional_u64(reader)?;
    let incomplete = decode_bool(reader)?;
    let tagless_union = decode_bool(reader)?;

    let union_tag_type =
        crate::semantic::codec::common::read_optional_u32(reader)?.map(InterfaceTypeId::new);

    let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
    let mut union_tags = context.allocate_items(reader, count)?;

    for _ in 0..count {
        union_tags.push(InterfaceUnionTag::new(
            read_symbol_reference(reader, context)?,
            super::value::decode_integer(reader, limits)?,
        ));
    }

    let storage = match read_u32(reader)? {
        1 => {
            let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
            let mut members = context.allocate_items(reader, count)?;

            for _ in 0..count {
                members.push(InterfaceStorageMember::new(
                    read_optional_symbol_reference(reader, context)?,
                    InterfaceTypeId::new(read_u32(reader)?),
                ));
            }

            InterfaceStorageShape::Structure(members.into())
        }
        2 => {
            let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
            let mut variants = context.allocate_items(reader, count)?;

            for _ in 0..count {
                let variant = read_symbol_reference(reader, context)?;
                let member_count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
                let mut members = context.allocate_items(reader, member_count)?;

                for _ in 0..member_count {
                    members.push(InterfaceStorageMember::new(
                        read_optional_symbol_reference(reader, context)?,
                        InterfaceTypeId::new(read_u32(reader)?),
                    ));
                }

                variants.push(InterfaceUnionStorageVariant::new(variant, members));
            }

            InterfaceStorageShape::Union(variants.into())
        }
        _ => {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Declaration,
            ));
        }
    };

    let copy = match read_u32(reader)? {
        1 => bray_symbols::DeclaredCopyContract::Absent,
        2 => bray_symbols::DeclaredCopyContract::Unconditional,
        3 => bray_symbols::DeclaredCopyContract::Conditional,
        _ => {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Declaration,
            ));
        }
    };

    let copy_dependencies =
        crate::semantic::codec::common::read_symbol_references(reader, limits, context)?;

    let plain_storage = decode_bool(reader)?;
    let finite_size = decode_bool(reader)?;

    Ok(InterfaceTypeRepresentation::new(owner)
        .with_layout(layout, alignment, packing, union_tag_type)
        .with_opaque_storage(opaque_size, incomplete)
        .with_tagless_union(tagless_union)
        .with_union_tags(union_tags)
        .with_storage(storage)
        .with_copy(copy, copy_dependencies)
        .with_properties(plain_storage, finite_size))
}

fn read_optional_symbol_reference(
    reader: &mut WireReader<'_>,
    context: &mut SemanticDecodeContext,
) -> Result<Option<crate::InterfaceSymbolReference>, InterfaceValidationError> {
    match read_u32(reader)? {
        0 => Ok(None),
        1 => read_symbol_reference(reader, context).map(Some),
        _ => Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Declaration,
        )),
    }
}

fn read_optional_u64(reader: &mut WireReader<'_>) -> Result<Option<u64>, InterfaceValidationError> {
    match read_u32(reader)? {
        0 => Ok(None),
        1 => reader.read_u64().map(Some).map_err(map_wire_error),
        _ => Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Declaration,
        )),
    }
}

fn decode_bool(reader: &mut WireReader<'_>) -> Result<bool, InterfaceValidationError> {
    match read_u32(reader)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Declaration,
        )),
    }
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
        _ => {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Declaration,
            ));
        }
    };

    let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
    let mut parameters = context.allocate_items(reader, count)?;

    for _ in 0..count {
        parameters.push(read_symbol_reference(reader, context)?);
    }

    let result = InterfaceTypeId::new(read_u32(reader)?);
    let has_body = decode_bool(reader)?;

    Ok(
        InterfaceCallableSignature::new(owner, callable_type, receiver, parameters, result)
            .with_body(has_body),
    )
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
        _ => {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Declaration,
            ));
        }
    };

    Ok(InterfaceCallableParameterDefault::new(
        parameter, is_present,
    ))
}

pub(super) fn decode_predicate_definition(
    reader: &mut WireReader<'_>,
    context: &mut SemanticDecodeContext,
) -> Result<InterfacePredicateDefinition, InterfaceValidationError> {
    let owner = read_symbol_reference(reader, context)?;
    let state = decode_tag(read_u32(reader)?)?;

    Ok(InterfacePredicateDefinition::new(owner, state))
}
