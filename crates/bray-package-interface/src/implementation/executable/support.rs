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

/// Interface tables that executable templates reference by slot, in wire order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TemplateTable {
    Type,
    ConstantValue,
    ConstantTerm,
    Substitution,
    TraitApplication,
    ImplementationInstance,
    DependencyContract,
}

impl TemplateTable {
    pub(crate) const COUNT: usize = 7;

    pub(crate) const fn index(self) -> usize {
        self as usize
    }
}

/// Interface slots that one executable template references, so a consumer can
/// decode only their closure. Each table lists strictly increasing slots.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExecutableTemplateReferences {
    pub(crate) tables: [Vec<u32>; TemplateTable::COUNT],
}

impl ExecutableTemplateReferences {
    pub(super) fn write(&self, encoder: &mut WireEncoder) {
        for table in &self.tables {
            write_count(encoder, table.len());

            for slot in table {
                encoder.write_u32(*slot);
            }
        }
    }

    pub(super) fn read(
        reader: &mut WireReader<'_>,
        limits: InterfaceValidationLimits,
    ) -> Result<Self, InterfaceValidationError> {
        let mut references = Self::default();

        for table in &mut references.tables {
            let count = read_count(reader, limits)?;

            for _ in 0..count {
                table.push(read_u32(reader)?);
            }

            if !table.windows(2).all(|pair| pair[0] < pair[1]) {
                return Err(crate::implementation::invalid_value(
                    crate::InterfaceValidationField::Value,
                ));
            }
        }

        Ok(references)
    }

    pub(crate) fn slots(&self, table: TemplateTable) -> &[u32] {
        &self.tables[table.index()]
    }
}

/// Maps a template's interface slots into the graph selected for that template.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExecutableTemplateSlots {
    pub(crate) tables: [std::collections::BTreeMap<u32, u32>; TemplateTable::COUNT],
}

impl ExecutableTemplateSlots {
    pub(crate) fn slot(&self, table: TemplateTable, slot: u32) -> Option<u32> {
        self.tables[table.index()].get(&slot).copied()
    }
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
    use super::{
        ExecutableTemplateReferences, FORMAT_VERSION, TemplateTable, read_bool, read_count,
        read_optional,
    };
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
    fn template_references_round_trip_and_reject_unordered_slots() {
        let mut references = ExecutableTemplateReferences::default();

        references.tables[TemplateTable::Type.index()] = vec![0, 4, 9];
        references.tables[TemplateTable::DependencyContract.index()] = vec![2];

        let mut encoder = WireEncoder::new();

        references.write(&mut encoder);

        let mut reader = WireReader::new(encoder.bytes());
        let limits = InterfaceValidationLimits::default();

        assert_eq!(
            ExecutableTemplateReferences::read(&mut reader, limits),
            Ok(references.clone())
        );

        // Each table is canonical, so a repeated slot is malformed.
        references.tables[TemplateTable::Type.index()] = vec![4, 4];

        let mut encoder = WireEncoder::new();

        references.write(&mut encoder);

        let mut reader = WireReader::new(encoder.bytes());

        assert!(ExecutableTemplateReferences::read(&mut reader, limits).is_err());
    }

    #[test]
    fn current_executable_template_format_is_version_one() {
        assert_eq!(FORMAT_VERSION, 1);
    }
}
