use crate::wire::{WireDecodeError, WireReader};
use crate::{
    InterfaceLimit, InterfaceMalformedCause, InterfaceValidationContext, InterfaceValidationError,
    InterfaceValidationField, InterfaceValidationLimits,
};

pub(crate) fn read_optional_u32(
    reader: &mut WireReader<'_>,
    context: InterfaceValidationContext,
    field: InterfaceValidationField,
) -> Result<Option<u32>, InterfaceValidationError> {
    match read_u32(reader, context, field)? {
        0 => Ok(None),
        1 => Ok(Some(read_u32(reader, context, field)?)),
        actual => Err(InterfaceValidationError::Malformed {
            context,
            cause: InterfaceMalformedCause::InvalidDiscriminant {
                field,
                actual: u64::from(actual),
            },
        }),
    }
}

pub(crate) fn read_u32(
    reader: &mut WireReader<'_>,
    context: InterfaceValidationContext,
    field: InterfaceValidationField,
) -> Result<u32, InterfaceValidationError> {
    reader.read_u32().map_err(wire_error(context, field))
}

pub(crate) fn wire_error(
    context: InterfaceValidationContext,
    field: InterfaceValidationField,
) -> impl FnOnce(WireDecodeError) -> InterfaceValidationError {
    move |error| map_wire_error(context, field, error)
}

pub(crate) const fn map_wire_error(
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
            offset: offset as u64,
            expected_length: expected_length as u64,
            actual_length: actual_length as u64,
        },
        WireDecodeError::TrailingBytes { offset, count } => {
            InterfaceValidationError::TrailingBytes {
                context,
                offset: offset as u64,
                count: count as u64,
            }
        }
    }
}

pub(crate) struct DecodeBudget {
    limits: InterfaceValidationLimits,
    allocated: u64,
    external_references: u64,
}

impl DecodeBudget {
    pub(crate) const fn new(limits: InterfaceValidationLimits) -> Self {
        Self {
            limits,
            allocated: 0,
            external_references: 0,
        }
    }

    pub(crate) const fn limits(&self) -> InterfaceValidationLimits {
        self.limits
    }

    pub(crate) fn allocate_items<T>(
        &mut self,
        reader: &WireReader<'_>,
        context: InterfaceValidationContext,
        field: InterfaceValidationField,
        count: usize,
    ) -> Result<Vec<T>, InterfaceValidationError> {
        self.allocate_items_with_minimum(reader, context, field, count, std::mem::size_of::<u32>())
    }

    pub(crate) fn allocate_items_with_minimum<T>(
        &mut self,
        reader: &WireReader<'_>,
        context: InterfaceValidationContext,
        field: InterfaceValidationField,
        count: usize,
        minimum_item_wire_bytes: usize,
    ) -> Result<Vec<T>, InterfaceValidationError> {
        let expected_length = count.saturating_mul(minimum_item_wire_bytes);

        if expected_length > reader.remaining() {
            return Err(InterfaceValidationError::Truncated {
                context,
                field,
                offset: reader.position() as u64,
                expected_length: expected_length as u64,
                actual_length: reader.remaining() as u64,
            });
        }

        self.allocate_derived_items(context, field, count)
    }

    pub(crate) fn allocate_derived_items<T>(
        &mut self,
        context: InterfaceValidationContext,
        field: InterfaceValidationField,
        count: usize,
    ) -> Result<Vec<T>, InterfaceValidationError> {
        self.charge_items::<T>(count)?;

        crate::framing::allocate_items(context, field, count)
    }

    pub(crate) fn charge_items<T>(&mut self, count: usize) -> Result<(), InterfaceValidationError> {
        self.charge(count.saturating_mul(std::mem::size_of::<T>()))
    }

    pub(crate) fn charge(&mut self, bytes: usize) -> Result<(), InterfaceValidationError> {
        let bytes = u64::try_from(bytes).unwrap_or(u64::MAX);

        self.allocated = self.allocated.saturating_add(bytes);

        self.limits
            .check(InterfaceLimit::DecodedAllocation, self.allocated)
    }

    pub(crate) fn charge_external_reference(
        &mut self,
        component_count: usize,
    ) -> Result<(), InterfaceValidationError> {
        let count = u64::try_from(component_count).unwrap_or(u64::MAX);

        self.external_references = self.external_references.saturating_add(count);

        self.limits.check(
            InterfaceLimit::ExternalReferenceCount,
            self.external_references,
        )
    }
}
