use crate::semantic::codec::common::{
    SemanticDecodeContext, map_wire_error, read_symbol_reference, read_u32,
};
use crate::tag::WireTag;
use crate::wire::WireReader;
use crate::{
    InterfaceLimit, InterfaceSectionTag, InterfaceSemanticFactEntry, InterfaceSemanticFactKind,
    InterfaceValidationError, InterfaceValidationLimits, ValidatedInterfaceSection,
};
use std::sync::Arc;

pub(super) fn decode_fact_directory(
    section: ValidatedInterfaceSection<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<Arc<[InterfaceSemanticFactEntry]>, InterfaceValidationError> {
    let count =
        usize::try_from(section.record_count()).map_err(|_| InterfaceValidationError::Malformed)?;

    limits.check(InterfaceLimit::RecordCount, section.record_count())?;

    let mut reader = WireReader::new(section.bytes());
    let mut entries = Vec::with_capacity(count);

    for _ in 0..count {
        let owner = read_symbol_reference(&mut reader, context)?;
        let kind = InterfaceSemanticFactKind::from_wire(read_u32(&mut reader)?)
            .ok_or(InterfaceValidationError::Malformed)?;
        let section = InterfaceSectionTag::from_wire_value(read_u32(&mut reader)?)
            .ok_or(InterfaceValidationError::Malformed)?;
        let record = read_u32(&mut reader)?;

        entries.push(InterfaceSemanticFactEntry {
            owner,
            kind,
            section,
            record,
        });
    }

    reader.finish().map_err(map_wire_error)?;

    if !entries.windows(2).all(|pair| pair[0] < pair[1]) {
        return Err(InterfaceValidationError::Malformed);
    }

    Ok(entries.into())
}
