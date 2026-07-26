use std::ops::Range;

use super::common::{SemanticDecodeContext, map_wire_error, read_count, write_count};
use crate::wire::{WireEncoder, WireReader};
use crate::{InterfaceLimit, InterfaceValidationError};

pub(super) struct RecordTable<'bytes> {
    payload: &'bytes [u8],
    ranges: Vec<Range<usize>>,
}

impl<'bytes> RecordTable<'bytes> {
    pub(super) fn read_from(
        reader: &mut WireReader<'bytes>,
        context: &mut SemanticDecodeContext,
    ) -> Result<Self, InterfaceValidationError> {
        let count = read_count(reader, context.limits(), InterfaceLimit::RecordCount)?;

        let mut ranges = context.allocate_items(reader, count)?;
        let mut expected_offset = 0_usize;

        for _ in 0..count {
            let offset = usize::try_from(reader.read_u32().map_err(map_wire_error)?)
                .map_err(|_| InterfaceValidationError::Malformed)?;

            let length = usize::try_from(reader.read_u32().map_err(map_wire_error)?)
                .map_err(|_| InterfaceValidationError::Malformed)?;

            if offset != expected_offset {
                return Err(InterfaceValidationError::Malformed);
            }

            let end = offset
                .checked_add(length)
                .ok_or(InterfaceValidationError::Malformed)?;

            ranges.push(offset..end);

            expected_offset = end;
        }

        let payload = reader.read_bytes(expected_offset).map_err(map_wire_error)?;

        Ok(Self { payload, ranges })
    }

    pub(super) fn len(&self) -> usize {
        self.ranges.len()
    }

    pub(super) fn record(&self, index: u32) -> Result<&'bytes [u8], InterfaceValidationError> {
        let index = usize::try_from(index).map_err(|_| InterfaceValidationError::Malformed)?;

        let range = self
            .ranges
            .get(index)
            .ok_or(InterfaceValidationError::Malformed)?;

        self.payload
            .get(range.clone())
            .ok_or(InterfaceValidationError::Malformed)
    }

    pub(super) fn decode<T>(
        &self,
        index: u32,
        context: &mut SemanticDecodeContext,
        decode: impl FnOnce(
            &mut WireReader<'bytes>,
            &mut SemanticDecodeContext,
        ) -> Result<T, InterfaceValidationError>,
    ) -> Result<T, InterfaceValidationError> {
        let mut reader = WireReader::new(self.record(index)?);
        let value = decode(&mut reader, context)?;

        reader.finish().map_err(map_wire_error)?;

        Ok(value)
    }

    pub(super) fn decode_all<T>(
        &self,
        context: &mut SemanticDecodeContext,
        mut decode: impl FnMut(
            &mut WireReader<'bytes>,
            &mut SemanticDecodeContext,
        ) -> Result<T, InterfaceValidationError>,
    ) -> Result<Vec<T>, InterfaceValidationError> {
        let reader = WireReader::new(self.payload);
        let mut values = context.allocate_items(&reader, self.len())?;

        for index in 0..self.len() {
            let index = u32::try_from(index).map_err(|_| InterfaceValidationError::Malformed)?;

            values.push(self.decode(index, context, &mut decode)?);
        }

        Ok(values)
    }
}

pub(super) fn encode_record_table<T>(
    encoder: &mut WireEncoder,
    records: &[T],
    mut encode: impl FnMut(&mut WireEncoder, &T),
) {
    let records = records
        .iter()
        .map(|record| {
            let mut encoder = WireEncoder::new();

            encode(&mut encoder, record);

            encoder.into_bytes()
        })
        .collect::<Vec<_>>();

    write_count(encoder, records.len());

    let mut offset = 0_u32;

    for record in &records {
        let length = u32::try_from(record.len())
            .unwrap_or_else(|_| unreachable!("validated semantic records fit the wire format"));

        encoder.write_u32(offset);
        encoder.write_u32(length);

        offset = offset
            .checked_add(length)
            .unwrap_or_else(|| unreachable!("validated semantic tables fit the wire format"));
    }

    for record in records {
        encoder.write_bytes(&record);
    }
}

#[cfg(test)]
mod tests {
    use super::{RecordTable, encode_record_table};
    use crate::semantic::codec::common::SemanticDecodeContext;
    use crate::wire::{WireEncoder, WireReader};
    use crate::{InterfaceValidationError, InterfaceValidationLimits};

    #[test]
    fn record_tables_preserve_canonical_random_access() {
        let mut encoder = WireEncoder::new();

        encode_record_table(&mut encoder, &[11_u32, 22_u32], |encoder, value| {
            encoder.write_u32(*value);
        });

        let mut reader = WireReader::new(encoder.bytes());
        let mut context = SemanticDecodeContext::new(InterfaceValidationLimits::default());

        let table = RecordTable::read_from(&mut reader, &mut context)
            .unwrap_or_else(|error| panic!("record table must decode: {error:?}"));

        assert_eq!(table.len(), 2);
        assert_eq!(table.record(0), Ok(&11_u32.to_le_bytes()[..]));
        assert_eq!(table.record(1), Ok(&22_u32.to_le_bytes()[..]));
        assert_eq!(reader.finish(), Ok(()));
    }

    #[test]
    fn record_tables_reject_noncanonical_offsets_and_truncated_payloads() {
        let mut noncanonical = WireEncoder::new();

        noncanonical.write_u32(1);
        noncanonical.write_u32(1);
        noncanonical.write_u32(0);

        let mut reader = WireReader::new(noncanonical.bytes());
        let mut context = SemanticDecodeContext::new(InterfaceValidationLimits::default());

        assert_eq!(
            RecordTable::read_from(&mut reader, &mut context).map(|_| ()),
            Err(InterfaceValidationError::Malformed)
        );

        let mut truncated = WireEncoder::new();

        truncated.write_u32(1);
        truncated.write_u32(0);
        truncated.write_u32(4);

        let mut reader = WireReader::new(truncated.bytes());
        let mut context = SemanticDecodeContext::new(InterfaceValidationLimits::default());

        assert_eq!(
            RecordTable::read_from(&mut reader, &mut context).map(|_| ()),
            Err(InterfaceValidationError::Truncated)
        );
    }
}
