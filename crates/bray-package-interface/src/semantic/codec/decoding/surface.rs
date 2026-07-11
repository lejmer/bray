use super::common::{decode_tag, validate_record_count};
use crate::semantic::codec::common::{
    SemanticDecodeContext, map_wire_error, read_count, read_optional_u32, read_string,
    read_symbol_reference, read_u32,
};
use crate::semantic::model::{
    InterfaceAbiDependency, InterfaceCoherenceRecord, InterfaceConstantValueId,
    InterfaceImplementationRecord, InterfaceSemanticFacts, InterfaceSourceProvenance,
    InterfaceTargetFactDependency, InterfaceTraitApplicationId, InterfaceTypeId,
};
use crate::wire::WireReader;
use crate::{
    InterfaceLimit, InterfaceValidationError, InterfaceValidationLimits, ValidatedInterfaceSection,
};

pub(super) fn decode_implementations(
    section: ValidatedInterfaceSection<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
    facts: &mut InterfaceSemanticFacts,
) -> Result<(), InterfaceValidationError> {
    let mut reader = WireReader::new(section.bytes());
    let implementation_count = read_count(&mut reader, limits, InterfaceLimit::RecordCount)?;
    let coherence_count = read_count(&mut reader, limits, InterfaceLimit::RecordCount)?;

    validate_record_count(section, [implementation_count, coherence_count])?;

    let mut implementations = Vec::with_capacity(implementation_count);

    for _ in 0..implementation_count {
        implementations.push(InterfaceImplementationRecord::new(
            read_symbol_reference(&mut reader, context)?,
            InterfaceTypeId::new(read_u32(&mut reader)?),
            read_optional_u32(&mut reader)?.map(InterfaceTraitApplicationId::new),
        ));
    }

    let mut coherence = Vec::with_capacity(coherence_count);

    for _ in 0..coherence_count {
        let subject = InterfaceTypeId::new(read_u32(&mut reader)?);
        let trait_application = InterfaceTraitApplicationId::new(read_u32(&mut reader)?);
        let count = read_count(&mut reader, limits, InterfaceLimit::RecordCount)?;
        let mut candidates = Vec::with_capacity(count);

        for _ in 0..count {
            candidates.push(read_symbol_reference(&mut reader, context)?);
        }

        coherence.push(InterfaceCoherenceRecord::new(
            subject,
            trait_application,
            candidates,
        ));
    }

    reader.finish().map_err(map_wire_error)?;

    facts.implementations = implementations.into();
    facts.coherence = coherence.into();

    Ok(())
}

pub(super) fn decode_target_dependencies(
    section: ValidatedInterfaceSection<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
    facts: &mut InterfaceSemanticFacts,
) -> Result<(), InterfaceValidationError> {
    let mut reader = WireReader::new(section.bytes());
    let target_count = read_count(&mut reader, limits, InterfaceLimit::RecordCount)?;
    let abi_count = read_count(&mut reader, limits, InterfaceLimit::RecordCount)?;

    validate_record_count(section, [target_count, abi_count])?;

    let mut targets = Vec::with_capacity(target_count);

    for _ in 0..target_count {
        targets.push(InterfaceTargetFactDependency::new(
            read_symbol_reference(&mut reader, context)?,
            InterfaceConstantValueId::new(read_u32(&mut reader)?),
        ));
    }

    let mut abis = Vec::with_capacity(abi_count);

    for _ in 0..abi_count {
        abis.push(InterfaceAbiDependency::new(
            read_symbol_reference(&mut reader, context)?,
            decode_tag(read_u32(&mut reader)?)?,
        ));
    }

    reader.finish().map_err(map_wire_error)?;

    facts.target_dependencies = targets.into();
    facts.abi_dependencies = abis.into();

    Ok(())
}

pub(super) fn decode_provenance(
    section: ValidatedInterfaceSection<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
    facts: &mut InterfaceSemanticFacts,
) -> Result<(), InterfaceValidationError> {
    let count =
        usize::try_from(section.record_count()).map_err(|_| InterfaceValidationError::Malformed)?;

    limits.check(InterfaceLimit::RecordCount, section.record_count())?;

    let mut reader = WireReader::new(section.bytes());
    let mut values = Vec::with_capacity(count);

    for _ in 0..count {
        let symbol = read_symbol_reference(&mut reader, context)?;
        let document = read_string(&mut reader, limits)?;
        let start = read_u32(&mut reader)?;
        let end = read_u32(&mut reader)?;

        let value = InterfaceSourceProvenance::try_new(symbol, document, start, end)
            .ok_or(InterfaceValidationError::Malformed)?;

        values.push(value);
    }

    reader.finish().map_err(map_wire_error)?;

    facts.provenance = values.into();

    Ok(())
}
