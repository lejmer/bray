use super::common::validate_record_count;
use crate::semantic::codec::common::{
    SemanticDecodeContext, map_wire_error, read_count, read_external_key, read_optional_u32,
    read_u32,
};
use crate::semantic::model::{
    InterfaceCheckedTemplateId, InterfaceSemantics, InterfaceSupportEntity,
    InterfaceSupportImplementation, InterfaceTraitApplicationId, InterfaceTypeId,
};
use crate::wire::WireReader;
use crate::{
    InterfaceLimit, InterfaceValidationError, InterfaceValidationLimits, ValidatedInterfaceSection,
};

pub(super) fn decode_support_graph(
    section: ValidatedInterfaceSection<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
    semantics: &mut InterfaceSemantics,
) -> Result<(), InterfaceValidationError> {
    let mut reader = WireReader::new(section.bytes());
    let count = read_count(&mut reader, limits, InterfaceLimit::RecordCount)?;

    validate_record_count(section, [count])?;

    let mut entities = context.allocate_items(&reader, count)?;

    for _ in 0..count {
        let raw = read_u32(&mut reader)?;

        entities.push(match raw {
            1 => InterfaceSupportEntity::CheckedTemplate(InterfaceCheckedTemplateId::new(
                read_u32(&mut reader)?,
            )),
            2 => InterfaceSupportEntity::Declaration(read_external_key(&mut reader, context)?),
            3 => {
                let declaration = read_external_key(&mut reader, context)?;

                InterfaceSupportEntity::Implementation(InterfaceSupportImplementation::new(
                    declaration,
                    InterfaceTypeId::new(read_u32(&mut reader)?),
                    read_optional_u32(&mut reader)?.map(InterfaceTraitApplicationId::new),
                ))
            }
            _ => {
                return Err(crate::semantic::codec::invalid_discriminant(
                    crate::InterfaceValidationField::Support,
                    raw,
                ));
            }
        });
    }

    reader.finish().map_err(map_wire_error)?;

    semantics.support_entities = entities.into();

    Ok(())
}
