use crate::implementation::map_wire_error;
use crate::wire::{WireEncoder, WireReader};
use crate::{InterfaceLimit, InterfaceValidationError, InterfaceValidationLimits};

pub(super) const FORMAT_VERSION: u32 = 1;

pub(super) fn write_bool(encoder: &mut WireEncoder, value: bool) {
    encoder.write_u32(u32::from(value));
}

pub(super) fn read_bool(reader: &mut WireReader<'_>) -> Result<bool, InterfaceValidationError> {
    let raw = read_u32(reader)?;

    match raw {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(invalid_discriminant(raw)),
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
    let raw = read_u32(reader)?;

    match raw {
        0 => Ok(None),
        1 => read(reader).map(Some),
        _ => Err(invalid_discriminant(raw)),
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

    usize::try_from(count)
        .map_err(|_| crate::implementation::invalid_value(crate::InterfaceValidationField::Value))
}

pub(super) fn read_u32(reader: &mut WireReader<'_>) -> Result<u32, InterfaceValidationError> {
    reader.read_u32().map_err(map_wire_error)
}

const fn invalid_discriminant(actual: u32) -> InterfaceValidationError {
    InterfaceValidationError::Malformed {
        context: crate::InterfaceValidationContext::Artifact,
        cause: crate::InterfaceMalformedCause::InvalidDiscriminant {
            field: crate::InterfaceValidationField::Value,
            actual: actual as u64,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{FORMAT_VERSION, read_bool, read_count, read_optional};
    use crate::wire::{WireEncoder, WireReader};
    use crate::{
        InterfaceLimit, InterfaceMalformedCause, InterfaceValidationContext,
        InterfaceValidationError, InterfaceValidationField, InterfaceValidationLimits,
    };

    #[test]
    fn executable_template_markers_preserve_invalid_discriminants() {
        let expected = |raw: u32| InterfaceValidationError::Malformed {
            context: InterfaceValidationContext::Artifact,
            cause: InterfaceMalformedCause::InvalidDiscriminant {
                field: InterfaceValidationField::Value,
                actual: u64::from(raw),
            },
        };

        let mut boolean = WireEncoder::new();
        boolean.write_u32(2);
        let mut boolean_reader = WireReader::new(boolean.bytes());

        assert_eq!(read_bool(&mut boolean_reader), Err(expected(2)));

        let mut optional = WireEncoder::new();
        optional.write_u32(3);
        let mut optional_reader = WireReader::new(optional.bytes());

        assert_eq!(
            read_optional(&mut optional_reader, |_| {
                Ok::<_, InterfaceValidationError>(())
            }),
            Err(expected(3))
        );
    }

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
        assert_eq!(FORMAT_VERSION, 1);
    }
}
