use crate::decode::map_wire_error;
use crate::wire::{WireEncoder, WireReader};
use crate::{InterfaceLimit, InterfaceValidationError, InterfaceValidationLimits};

pub(super) const FORMAT_VERSION: u32 = 2;

pub(super) fn write_bool(encoder: &mut WireEncoder, value: bool) {
    encoder.write_u32(u32::from(value));
}

pub(super) fn read_bool(reader: &mut WireReader<'_>) -> Result<bool, InterfaceValidationError> {
    match read_u32(reader)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(InterfaceValidationError::Malformed),
    }
}

pub(super) fn write_optional<T>(
    encoder: &mut WireEncoder,
    value: Option<T>,
    write: impl FnOnce(&mut WireEncoder, T),
) {
    match value {
        Some(value) => {
            encoder.write_u32(1);
            write(encoder, value);
        }
        None => encoder.write_u32(0),
    }
}

pub(super) fn read_optional<T>(
    reader: &mut WireReader<'_>,
    read: impl FnOnce(&mut WireReader<'_>) -> Result<T, InterfaceValidationError>,
) -> Result<Option<T>, InterfaceValidationError> {
    match read_u32(reader)? {
        0 => Ok(None),
        1 => read(reader).map(Some),
        _ => Err(InterfaceValidationError::Malformed),
    }
}

pub(super) fn write_count(encoder: &mut WireEncoder, count: usize) {
    encoder.write_count(count);
}

pub(super) fn read_count(
    reader: &mut WireReader<'_>,
    limits: InterfaceValidationLimits,
) -> Result<usize, InterfaceValidationError> {
    let count = read_u32(reader)?;

    limits.check(InterfaceLimit::TemplateGraphSize, u64::from(count))?;

    usize::try_from(count).map_err(|_| InterfaceValidationError::Malformed)
}

pub(super) fn read_u32(reader: &mut WireReader<'_>) -> Result<u32, InterfaceValidationError> {
    reader.read_u32().map_err(map_wire_error)
}

#[cfg(test)]
mod tests {
    use super::{FORMAT_VERSION, read_count};
    use crate::wire::{WireEncoder, WireReader};
    use crate::{InterfaceLimit, InterfaceValidationError, InterfaceValidationLimits};

    #[test]
    fn executable_template_counts_obey_graph_limits_before_allocation() {
        let mut encoder = WireEncoder::new();

        encoder.write_u32(2);

        let mut reader = WireReader::new(encoder.bytes());
        let limits = InterfaceValidationLimits::default().with_template_graph_size(1);

        assert_eq!(
            read_count(&mut reader, limits),
            Err(InterfaceValidationError::ResourceLimitExceeded {
                limit: InterfaceLimit::TemplateGraphSize,
                actual: 2,
                maximum: 1,
            })
        );
    }

    #[test]
    fn current_executable_template_format_is_version_two() {
        assert_eq!(FORMAT_VERSION, 2);
    }
}
