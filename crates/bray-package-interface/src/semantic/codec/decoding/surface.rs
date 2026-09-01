use super::common::{decode_tag, validate_record_count};
use crate::semantic::codec::coherence::coherence_record_indexes;
use crate::semantic::codec::common::{
    SemanticDecodeContext, map_wire_error, read_count, read_optional_u32, read_string,
    read_symbol_reference, read_u32,
};
use crate::semantic::codec::record::RecordTable;
use crate::semantic::model::{
    InterfaceAbiDependency, InterfaceCoherenceRecord, InterfaceConstantValueId,
    InterfaceImplementationRecord, InterfaceRuntimeRequirement, InterfaceSemantics,
    InterfaceSourceProvenance, InterfaceTargetPropertyDependency, InterfaceTraitApplicationId,
    InterfaceTypeId,
};
use crate::tag::WireTag;
use crate::wire::WireReader;
use crate::{
    InterfaceLimit, InterfaceValidationError, InterfaceValidationLimits, ValidatedInterfaceSection,
};
use bray_runtime_interface::{
    ExecutionLaneRequirement, PanicAbiIdentity, ProtectedAsyncFrameId, ProtectedFrameAbiVersions,
    RuntimeAbiVersion, RuntimeCapability, RuntimeRequirements,
};
use bray_target::TargetIdentity;

pub(super) struct ImplementationRecordTables<'bytes> {
    pub(super) implementations: RecordTable<'bytes>,
    pub(super) coherence: RecordTable<'bytes>,
}

pub(super) struct DecodedImplementationRecord {
    pub(super) implementation: InterfaceImplementationRecord,
    pub(super) coherence: Vec<u32>,
}

pub(super) fn decode_implementation_tables<'bytes>(
    section: ValidatedInterfaceSection<'bytes>,
    context: &mut SemanticDecodeContext,
) -> Result<ImplementationRecordTables<'bytes>, InterfaceValidationError> {
    let mut reader = WireReader::new(section.bytes());
    let implementations = RecordTable::read_from(&mut reader, context, section.tag())?;
    let coherence = RecordTable::read_from(&mut reader, context, section.tag())?;

    validate_record_count(section, [implementations.len(), coherence.len()])?;

    reader.finish().map_err(map_wire_error)?;

    Ok(ImplementationRecordTables {
        implementations,
        coherence,
    })
}

pub(super) fn decode_implementations(
    section: ValidatedInterfaceSection<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
    semantics: &mut InterfaceSemantics,
) -> Result<(), InterfaceValidationError> {
    let tables = decode_implementation_tables(section, context)?;

    let implementations = tables
        .implementations
        .decode_all(context, |reader, context| {
            decode_implementation_record(reader, limits, context)
        })?;

    let coherence = tables.coherence.decode_all(context, |reader, context| {
        decode_coherence_record(reader, limits, context)
    })?;

    let coherence_by_implementation = coherence_record_indexes(&coherence).ok_or(
        crate::semantic::codec::invalid_value(crate::InterfaceValidationField::Type),
    )?;

    for implementation in &implementations {
        let expected = coherence_by_implementation
            .get(&implementation.implementation.implementation)
            .map_or(&[][..], Vec::as_slice);

        if implementation.coherence != expected {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Type,
            ));
        }
    }

    semantics.implementations = implementations
        .into_iter()
        .map(|implementation| implementation.implementation)
        .collect();

    semantics.coherence = coherence.into();

    Ok(())
}

pub(super) fn decode_implementation_record(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<DecodedImplementationRecord, InterfaceValidationError> {
    let implementation = InterfaceImplementationRecord::new(
        read_symbol_reference(reader, context)?,
        InterfaceTypeId::new(read_u32(reader)?),
        read_optional_u32(reader)?.map(InterfaceTraitApplicationId::new),
    );

    let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
    let mut coherence = context.allocate_items(reader, count)?;

    for _ in 0..count {
        coherence.push(read_u32(reader)?);
    }

    if !coherence.windows(2).all(|pair| pair[0] < pair[1]) {
        return Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Type,
        ));
    }

    Ok(DecodedImplementationRecord {
        implementation,
        coherence,
    })
}

pub(super) fn decode_coherence_record(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceCoherenceRecord, InterfaceValidationError> {
    let subject = InterfaceTypeId::new(read_u32(reader)?);
    let trait_application = InterfaceTraitApplicationId::new(read_u32(reader)?);

    let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
    let mut implementations = context.allocate_items(reader, count)?;

    for _ in 0..count {
        implementations.push(read_symbol_reference(reader, context)?);
    }

    Ok(InterfaceCoherenceRecord::new(
        subject,
        trait_application,
        implementations,
    ))
}
pub(super) struct TargetRecordTables<'bytes> {
    pub(super) targets: RecordTable<'bytes>,
    pub(super) abis: RecordTable<'bytes>,
    pub(super) runtimes: RecordTable<'bytes>,
}

pub(super) fn decode_target_tables<'bytes>(
    section: ValidatedInterfaceSection<'bytes>,
    context: &mut SemanticDecodeContext,
) -> Result<TargetRecordTables<'bytes>, InterfaceValidationError> {
    let mut reader = WireReader::new(section.bytes());

    let targets = RecordTable::read_from(&mut reader, context, section.tag())?;
    let abis = RecordTable::read_from(&mut reader, context, section.tag())?;
    let runtimes = RecordTable::read_from(&mut reader, context, section.tag())?;

    validate_record_count(section, [targets.len(), abis.len(), runtimes.len()])?;
    reader.finish().map_err(map_wire_error)?;

    Ok(TargetRecordTables {
        targets,
        abis,
        runtimes,
    })
}

pub(super) fn decode_target_dependencies(
    section: ValidatedInterfaceSection<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
    semantics: &mut InterfaceSemantics,
) -> Result<(), InterfaceValidationError> {
    let tables = decode_target_tables(section, context)?;

    let targets = tables.targets.decode_all(context, decode_target_record)?;
    let abis = tables.abis.decode_all(context, decode_abi_record)?;

    let runtimes = tables.runtimes.decode_all(context, |reader, context| {
        decode_runtime_record(reader, limits, context)
    })?;

    semantics.target_dependencies = targets.into();
    semantics.abi_dependencies = abis.into();
    semantics.runtime_requirements = runtimes.into();

    Ok(())
}

pub(super) fn decode_target_record(
    reader: &mut WireReader<'_>,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceTargetPropertyDependency, InterfaceValidationError> {
    Ok(InterfaceTargetPropertyDependency::new(
        read_symbol_reference(reader, context)?,
        read_symbol_reference(reader, context)?,
        InterfaceConstantValueId::new(read_u32(reader)?),
    ))
}

pub(super) fn decode_abi_record(
    reader: &mut WireReader<'_>,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceAbiDependency, InterfaceValidationError> {
    Ok(InterfaceAbiDependency::new(
        read_symbol_reference(reader, context)?,
        decode_tag(read_u32(reader)?)?,
    ))
}

pub(super) fn decode_runtime_record(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<InterfaceRuntimeRequirement, InterfaceValidationError> {
    let owner = read_symbol_reference(reader, context)?;
    let frames = read_frames(reader, limits, context)?;

    let abi_version = read_version(reader)?;

    let frame_abi = match read_presence(reader)? {
        true => Some(ProtectedFrameAbiVersions::new(
            read_version(reader)?,
            read_version(reader)?,
            read_version(reader)?,
            read_version(reader)?,
            read_version(reader)?,
        )),
        false => None,
    };

    let target = TargetIdentity::try_new(read_string(reader, context)?).ok_or(
        crate::semantic::codec::invalid_value(crate::InterfaceValidationField::Type),
    )?;

    let panic_abi = PanicAbiIdentity::try_new(read_string(reader, context)?).ok_or(
        crate::semantic::codec::invalid_value(crate::InterfaceValidationField::Type),
    )?;

    let capabilities = read_tags::<RuntimeCapability>(reader, limits, context)?;
    let lanes = read_tags::<ExecutionLaneRequirement>(reader, limits, context)?;

    Ok(InterfaceRuntimeRequirement::new(
        owner,
        frames,
        RuntimeRequirements::new(
            None,
            abi_version,
            frame_abi,
            target,
            panic_abi,
            [],
            capabilities,
            lanes,
        ),
    ))
}

fn read_frames(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<Vec<ProtectedAsyncFrameId>, InterfaceValidationError> {
    let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
    let mut frames = context.allocate_items(reader, count)?;

    for _ in 0..count {
        let bytes = reader.read_bytes(32).map_err(map_wire_error)?;

        let digest = <[u8; 32]>::try_from(bytes).map_err(|_| {
            crate::semantic::codec::invalid_value(crate::InterfaceValidationField::Type)
        })?;

        frames.push(ProtectedAsyncFrameId::new(digest));
    }

    if !frames.windows(2).all(|pair| pair[0] < pair[1]) {
        return Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Type,
        ));
    }

    Ok(frames)
}

fn read_presence(reader: &mut WireReader<'_>) -> Result<bool, InterfaceValidationError> {
    match read_u32(reader)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Type,
        )),
    }
}

fn read_version(
    reader: &mut WireReader<'_>,
) -> Result<RuntimeAbiVersion, InterfaceValidationError> {
    let major = u16::try_from(read_u32(reader)?).map_err(|_| {
        crate::semantic::codec::invalid_value(crate::InterfaceValidationField::Type)
    })?;

    let minor = u16::try_from(read_u32(reader)?).map_err(|_| {
        crate::semantic::codec::invalid_value(crate::InterfaceValidationField::Type)
    })?;

    Ok(RuntimeAbiVersion::new(major, minor))
}

fn read_tags<T: WireTag + Ord>(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<Vec<T>, InterfaceValidationError> {
    let count = read_count(reader, limits, InterfaceLimit::RecordCount)?;
    let mut values = context.allocate_items(reader, count)?;

    for _ in 0..count {
        values.push(decode_tag(read_u32(reader)?)?);
    }

    if !values.windows(2).all(|pair| pair[0] < pair[1]) {
        return Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Type,
        ));
    }

    Ok(values)
}

pub(super) fn decode_provenance(
    section: ValidatedInterfaceSection<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
    semantics: &mut InterfaceSemantics,
) -> Result<(), InterfaceValidationError> {
    let count = usize::try_from(section.record_count()).map_err(|_| {
        crate::semantic::codec::invalid_value(crate::InterfaceValidationField::Type)
    })?;

    limits.check(InterfaceLimit::RecordCount, section.record_count())?;

    let mut reader = WireReader::new(section.bytes());
    let mut values = context.allocate_items(&reader, count)?;

    for _ in 0..count {
        let symbol = read_symbol_reference(&mut reader, context)?;
        let document = read_string(&mut reader, context)?;

        let start = read_u32(&mut reader)?;
        let end = read_u32(&mut reader)?;

        let value = InterfaceSourceProvenance::try_new(symbol, document, start, end).ok_or(
            crate::semantic::codec::invalid_value(crate::InterfaceValidationField::Type),
        )?;

        values.push(value);
    }

    reader.finish().map_err(map_wire_error)?;

    semantics.provenance = values.into();

    Ok(())
}
