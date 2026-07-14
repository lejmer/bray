use super::common::validate_record_count;
use crate::semantic::codec::common::{
    SemanticDecodeContext, map_wire_error, read_count, read_external_key, read_optional_u32,
    read_u32,
};
use crate::semantic::model::{
    InterfaceCheckedTemplateId, InterfaceSemanticFacts, InterfaceSupportEntity,
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
    facts: &mut InterfaceSemanticFacts,
) -> Result<(), InterfaceValidationError> {
    let mut reader = WireReader::new(section.bytes());
    let count = read_count(&mut reader, limits, InterfaceLimit::RecordCount)?;

    validate_record_count(section, [count])?;

    let mut entities = Vec::with_capacity(count);

    for _ in 0..count {
        entities.push(match read_u32(&mut reader)? {
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
            _ => return Err(InterfaceValidationError::Malformed),
        });
    }

    reader.finish().map_err(map_wire_error)?;

    facts.support_entities = entities.into();

    Ok(())
}
