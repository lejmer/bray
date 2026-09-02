use std::ops::Range;

use crate::diagnostic::{
    InterfaceMalformedCause, InterfaceValidationContext, InterfaceValidationError,
    InterfaceValidationField,
};
use crate::wire::WireDecodeError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CheckedRangeError {
    Overflow {
        offset: u64,
        length: u64,
    },
    OutOfBounds {
        offset: u64,
        length: u64,
        available: u64,
    },
}

pub(crate) fn checked_range(
    offset: u64,
    length: u64,
    file_length: usize,
) -> Result<Range<usize>, CheckedRangeError> {
    let end = offset
        .checked_add(length)
        .ok_or(CheckedRangeError::Overflow { offset, length })?;

    let start =
        usize::try_from(offset).map_err(|_| CheckedRangeError::Overflow { offset, length })?;

    let end = usize::try_from(end).map_err(|_| CheckedRangeError::Overflow { offset, length })?;

    if end > file_length {
        return Err(CheckedRangeError::OutOfBounds {
            offset,
            length,
            available: available_length(offset, file_length),
        });
    }

    Ok(start..end)
}

pub(crate) fn allocate_items<T>(
    context: InterfaceValidationContext,
    field: InterfaceValidationField,
    count: usize,
) -> Result<Vec<T>, InterfaceValidationError> {
    let mut values = Vec::new();

    values.try_reserve_exact(count).map_err(|_| {
        InterfaceValidationError::AllocationUnavailable {
            context,
            field,
            requested: usize_to_u64_saturating(count.saturating_mul(std::mem::size_of::<T>())),
        }
    })?;

    Ok(values)
}

pub(crate) const fn ranges_overlap(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

pub(crate) fn range_validation_error(
    context: InterfaceValidationContext,
    field: InterfaceValidationField,
    error: CheckedRangeError,
) -> InterfaceValidationError {
    match error {
        CheckedRangeError::Overflow { offset, length } => malformed(
            context,
            InterfaceMalformedCause::RangeOverflow { offset, length },
        ),
        CheckedRangeError::OutOfBounds {
            offset,
            length,
            available,
        } => InterfaceValidationError::Truncated {
            context,
            field,
            offset,
            expected_length: length,
            actual_length: available,
        },
    }
}

pub(crate) fn map_wire_error(
    context: InterfaceValidationContext,
    field: InterfaceValidationField,
    error: WireDecodeError,
) -> InterfaceValidationError {
    match error {
        WireDecodeError::Truncated {
            offset,
            expected_length,
            actual_length,
        } => InterfaceValidationError::Truncated {
            context,
            field,
            offset: usize_to_u64_saturating(offset),
            expected_length: usize_to_u64_saturating(expected_length),
            actual_length: usize_to_u64_saturating(actual_length),
        },
        WireDecodeError::TrailingBytes { offset, count } => {
            InterfaceValidationError::TrailingBytes {
                context,
                offset: usize_to_u64_saturating(offset),
                count: usize_to_u64_saturating(count),
            }
        }
    }
}

pub(crate) const fn malformed(
    context: InterfaceValidationContext,
    cause: InterfaceMalformedCause,
) -> InterfaceValidationError {
    InterfaceValidationError::Malformed { context, cause }
}

pub(crate) fn invalid_index(
    context: InterfaceValidationContext,
    index: usize,
    available: usize,
) -> InterfaceValidationError {
    malformed(
        context,
        InterfaceMalformedCause::InvalidReference {
            field: InterfaceValidationField::Index,
            index: usize_to_u64_saturating(index),
            available: usize_to_u64_saturating(available),
        },
    )
}

pub(crate) fn directory_entry_context(index: usize, raw_tag: u32) -> InterfaceValidationContext {
    InterfaceValidationContext::DirectoryEntry {
        index: usize_to_u64_saturating(index),
        raw_tag,
    }
}

pub(crate) fn available_length(offset: u64, file_length: usize) -> u64 {
    let Ok(start) = usize::try_from(offset) else {
        return 0;
    };

    usize_to_u64_saturating(file_length.saturating_sub(start))
}

pub(crate) fn overlapping_range(
    context: InterfaceValidationContext,
    range: &Range<usize>,
    conflicting: &Range<usize>,
) -> InterfaceValidationError {
    malformed(
        context,
        InterfaceMalformedCause::RangeOverlap {
            offset: usize_to_u64_saturating(range.start),
            length: usize_to_u64_saturating(range.len()),
            conflicting_offset: usize_to_u64_saturating(conflicting.start),
            conflicting_length: usize_to_u64_saturating(conflicting.len()),
        },
    )
}

pub(crate) const fn header_field(offset: usize) -> InterfaceValidationField {
    match offset {
        0..8 => InterfaceValidationField::Magic,
        8..10 => InterfaceValidationField::FormatRevision,
        10..12 => InterfaceValidationField::LanguageRevision,
        12..16 => InterfaceValidationField::ByteOrderMarker,
        16..24 => InterfaceValidationField::RequiredFlags,
        24..32 => InterfaceValidationField::DeclaredFileLength,
        32..40 => InterfaceValidationField::DirectoryOffset,
        40..48 => InterfaceValidationField::DirectoryLength,
        48..80 => InterfaceValidationField::ContentHash,
        _ => InterfaceValidationField::ArtifactHash,
    }
}

pub(crate) const fn directory_field(offset: usize) -> InterfaceValidationField {
    match offset {
        0..4 => InterfaceValidationField::SectionTag,
        4..6 => InterfaceValidationField::SectionRevision,
        6 => InterfaceValidationField::SectionCompatibility,
        7 => InterfaceValidationField::SectionEncoding,
        8..16 => InterfaceValidationField::SectionOffset,
        16..24 => InterfaceValidationField::EncodedLength,
        24..32 => InterfaceValidationField::DecodedLength,
        32..40 => InterfaceValidationField::RecordCount,
        40..72 => InterfaceValidationField::Hash,
        _ => InterfaceValidationField::ContentHash,
    }
}

pub(crate) fn usize_to_u64_saturating(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}
