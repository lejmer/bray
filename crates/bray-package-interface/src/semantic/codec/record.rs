use std::ops::Range;

use super::common::{SemanticDecodeContext, read_count, write_count};
use crate::decode::wire_error;
use crate::wire::{WireEncoder, WireReader};
use crate::{
    InterfaceIntegerTarget, InterfaceLimit, InterfaceMalformedCause, InterfaceSectionTag,
    InterfaceValidationContext, InterfaceValidationError, InterfaceValidationField,
};

pub(super) struct RecordTable<'bytes> {
    payload: &'bytes [u8],
    ranges: Vec<Range<usize>>,
    section: InterfaceSectionTag,
}

impl<'bytes> RecordTable<'bytes> {
    pub(super) fn read_from(
        reader: &mut WireReader<'bytes>,
        context: &mut SemanticDecodeContext,
        section: InterfaceSectionTag,
    ) -> Result<Self, InterfaceValidationError> {
        let validation = InterfaceValidationContext::Section(section);
        let count = read_count(reader, context.limits(), InterfaceLimit::RecordCount)?;

        let mut ranges = context.allocate_items(reader, count)?;
        let mut expected_offset = 0_usize;

        for index in 0..count {
            let record_context = InterfaceValidationContext::Record {
                section,
                index: index as u64,
            };

            let raw_offset = reader.read_u32().map_err(wire_error(
                record_context,
                InterfaceValidationField::RecordOffset,
            ))?;

            let offset = usize::try_from(raw_offset).map_err(|_| {
                numeric_overflow(
                    record_context,
                    InterfaceValidationField::RecordOffset,
                    u64::from(raw_offset),
                )
            })?;

            let raw_length = reader.read_u32().map_err(wire_error(
                record_context,
                InterfaceValidationField::RecordLength,
            ))?;

            let length = usize::try_from(raw_length).map_err(|_| {
                numeric_overflow(
                    record_context,
                    InterfaceValidationField::RecordLength,
                    u64::from(raw_length),
                )
            })?;

            if offset != expected_offset {
                return Err(InterfaceValidationError::Malformed {
                    context: record_context,
                    cause: InterfaceMalformedCause::OrderingViolation {
                        field: InterfaceValidationField::RecordOffset,
                        previous: expected_offset as u64,
                        actual: offset as u64,
                    },
                });
            }

            let end = offset
                .checked_add(length)
                .ok_or(InterfaceValidationError::Malformed {
                    context: record_context,
                    cause: InterfaceMalformedCause::RangeOverflow {
                        offset: offset as u64,
                        length: length as u64,
                    },
                })?;

            ranges.push(offset..end);

            expected_offset = end;
        }

        let payload = reader.read_bytes(expected_offset).map_err(wire_error(
            validation,
            InterfaceValidationField::RecordPayload,
        ))?;

        Ok(Self {
            payload,
            ranges,
            section,
        })
    }

    pub(super) fn len(&self) -> usize {
        self.ranges.len()
    }

    pub(super) fn record(&self, index: u32) -> Result<&'bytes [u8], InterfaceValidationError> {
        let validation = InterfaceValidationContext::Record {
            section: self.section,
            index: u64::from(index),
        };

        let index = usize::try_from(index).map_err(|_| {
            numeric_overflow(
                validation,
                InterfaceValidationField::Index,
                u64::from(index),
            )
        })?;

        let range = self
            .ranges
            .get(index)
            .ok_or(InterfaceValidationError::Malformed {
                context: validation,
                cause: InterfaceMalformedCause::InvalidReference {
                    field: InterfaceValidationField::Index,
                    index: index as u64,
                    available: self.ranges.len() as u64,
                },
            })?;

        self.payload
            .get(range.clone())
            .ok_or(InterfaceValidationError::Malformed {
                context: validation,
                cause: InterfaceMalformedCause::InvalidReference {
                    field: InterfaceValidationField::RecordPayload,
                    index: range.start as u64,
                    available: self.payload.len() as u64,
                },
            })
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
        let validation = InterfaceValidationContext::Record {
            section: self.section,
            index: u64::from(index),
        };

        let previous = context.replace_validation(validation);
        let mut reader = WireReader::new(self.record(index)?);
        let value = decode(&mut reader, context).map_err(|error| with_context(error, validation));
        context.replace_validation(previous);
        let value = value?;

        reader.finish().map_err(wire_error(
            validation,
            InterfaceValidationField::RecordPayload,
        ))?;

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
            let index = u32::try_from(index).map_err(|_| {
                numeric_overflow(
                    InterfaceValidationContext::Section(self.section),
                    InterfaceValidationField::Index,
                    index as u64,
                )
            })?;

            values.push(self.decode(index, context, &mut decode)?);
        }

        Ok(values)
    }
}

const fn numeric_overflow(
    context: InterfaceValidationContext,
    field: InterfaceValidationField,
    value: u64,
) -> InterfaceValidationError {
    InterfaceValidationError::Malformed {
        context,
        cause: InterfaceMalformedCause::NumericOverflow {
            field,
            value,
            target: InterfaceIntegerTarget::Usize,
        },
    }
}

fn with_context(
    error: InterfaceValidationError,
    context: InterfaceValidationContext,
) -> InterfaceValidationError {
    match error {
        InterfaceValidationError::Truncated {
            field,
            offset,
            expected_length,
            actual_length,
            ..
        } => InterfaceValidationError::Truncated {
            context,
            field,
            offset,
            expected_length,
            actual_length,
        },
        InterfaceValidationError::TrailingBytes { offset, count, .. } => {
            InterfaceValidationError::TrailingBytes {
                context,
                offset,
                count,
            }
        }
        InterfaceValidationError::Malformed { cause, .. } => {
            InterfaceValidationError::Malformed { context, cause }
        }
        InterfaceValidationError::InvalidUtf8 {
            field,
            offset,
            length,
            cause,
            ..
        } => InterfaceValidationError::InvalidUtf8 {
            context,
            field,
            offset,
            length,
            cause,
        },
        error => error,
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

        let table = RecordTable::read_from(
            &mut reader,
            &mut context,
            crate::InterfaceSectionTag::SemanticTypes,
        )
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
            RecordTable::read_from(
                &mut reader,
                &mut context,
                crate::InterfaceSectionTag::SemanticTypes,
            )
            .map(|_| ()),
            Err(InterfaceValidationError::Malformed {
                context: crate::InterfaceValidationContext::Record {
                    section: crate::InterfaceSectionTag::SemanticTypes,
                    index: 0,
                },
                cause: crate::InterfaceMalformedCause::OrderingViolation {
                    field: crate::InterfaceValidationField::RecordOffset,
                    previous: 0,
                    actual: 1,
                },
            })
        );

        let mut truncated = WireEncoder::new();

        truncated.write_u32(1);
        truncated.write_u32(0);
        truncated.write_u32(4);

        let mut reader = WireReader::new(truncated.bytes());
        let mut context = SemanticDecodeContext::new(InterfaceValidationLimits::default());

        assert_eq!(
            RecordTable::read_from(
                &mut reader,
                &mut context,
                crate::InterfaceSectionTag::SemanticTypes,
            )
            .map(|_| ()),
            Err(InterfaceValidationError::Truncated {
                context: crate::InterfaceValidationContext::Section(
                    crate::InterfaceSectionTag::SemanticTypes,
                ),
                field: crate::InterfaceValidationField::RecordPayload,
                offset: 12,
                expected_length: 4,
                actual_length: 0,
            })
        );
    }
}
