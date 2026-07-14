use crate::semantic::codec::common::{map_wire_error, read_count, read_u32};
use crate::tag::WireTag;
use crate::wire::WireReader;
use crate::{InterfaceLimit, InterfaceValidationError, ValidatedInterfaceSection};
use bray_symbols::RealConstantBits;
use std::sync::Arc;

pub(super) fn validate_record_count(
    section: ValidatedInterfaceSection<'_>,
    counts: impl IntoIterator<Item = usize>,
) -> Result<(), InterfaceValidationError> {
    let mut total = 0_u64;

    for count in counts {
        total = total
            .checked_add(u64::try_from(count).map_err(|_| InterfaceValidationError::Malformed)?)
            .ok_or(InterfaceValidationError::Malformed)?;
    }

    if total != section.record_count() {
        return Err(InterfaceValidationError::Malformed);
    }

    Ok(())
}

pub(super) fn read_ids<T>(
    reader: &mut WireReader<'_>,
    context: &mut crate::semantic::codec::common::SemanticDecodeContext,
    create: impl Fn(u32) -> T,
) -> Result<Arc<[T]>, InterfaceValidationError> {
    let count = read_count(reader, context.limits(), InterfaceLimit::RecordCount)?;
    let mut values = context.allocate_items(reader, count)?;

    for _ in 0..count {
        values.push(create(read_u32(reader)?));
    }

    Ok(values.into())
}

pub(super) fn decode_real(
    reader: &mut WireReader<'_>,
) -> Result<RealConstantBits, InterfaceValidationError> {
    match read_u32(reader)? {
        1 => Ok(RealConstantBits::Binary16(
            reader.read_u16().map_err(map_wire_error)?,
        )),
        2 => Ok(RealConstantBits::Binary32(read_u32(reader)?)),
        3 => Ok(RealConstantBits::Binary64(
            reader.read_u64().map_err(map_wire_error)?,
        )),
        4 => Ok(RealConstantBits::Binary128(
            reader.read_array().map_err(map_wire_error)?,
        )),
        _ => Err(InterfaceValidationError::Malformed),
    }
}

pub(super) fn decode_tag<T: WireTag>(value: u32) -> Result<T, InterfaceValidationError> {
    T::from_wire(value).ok_or(InterfaceValidationError::Malformed)
}
