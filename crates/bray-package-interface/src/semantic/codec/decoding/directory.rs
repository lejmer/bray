use crate::semantic::codec::common::{
    SemanticDecodeContext, map_wire_error, read_symbol_reference, read_u32,
};
use crate::tag::WireTag;
use crate::validation::is_strictly_sorted;
use crate::wire::WireReader;
use crate::{
    InterfaceLimit, InterfaceSectionTag, InterfaceSemanticRecord, InterfaceSemanticRecordKind,
    InterfaceValidationError, InterfaceValidationLimits, ValidatedInterfaceSection,
};
use std::sync::Arc;

pub(crate) fn decode_semantic_directory(
    section: ValidatedInterfaceSection<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<Arc<[InterfaceSemanticRecord]>, InterfaceValidationError> {
    let count = usize::try_from(section.record_count()).map_err(|_| {
        crate::semantic::codec::invalid_value(crate::InterfaceValidationField::Reference)
    })?;

    limits.check(InterfaceLimit::RecordCount, section.record_count())?;

    let mut reader = WireReader::new(section.bytes());
    let mut entries = context.allocate_items(&reader, count)?;

    for _ in 0..count {
        let owner = read_symbol_reference(&mut reader, context)?;

        let kind = InterfaceSemanticRecordKind::from_wire(read_u32(&mut reader)?).ok_or(
            crate::semantic::codec::invalid_value(crate::InterfaceValidationField::Reference),
        )?;

        let section = InterfaceSectionTag::from_wire_value(read_u32(&mut reader)?).ok_or(
            crate::semantic::codec::invalid_value(crate::InterfaceValidationField::Reference),
        )?;

        let record = read_u32(&mut reader)?;

        entries.push(InterfaceSemanticRecord {
            owner,
            kind,
            section,
            record,
        });
    }

    reader.finish().map_err(map_wire_error)?;

    if !is_strictly_sorted(&entries) {
        return Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Reference,
        ));
    }

    Ok(entries.into())
}
