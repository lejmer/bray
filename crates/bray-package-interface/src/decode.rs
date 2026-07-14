use crate::wire::{WireDecodeError, WireReader};
use crate::{InterfaceLimit, InterfaceValidationError, InterfaceValidationLimits};

pub(crate) fn read_optional_u32(
    reader: &mut WireReader<'_>,
) -> Result<Option<u32>, InterfaceValidationError> {
    match read_u32(reader)? {
        0 => Ok(None),
        1 => Ok(Some(read_u32(reader)?)),
        _ => Err(InterfaceValidationError::Malformed),
    }
}

pub(crate) fn read_u32(reader: &mut WireReader<'_>) -> Result<u32, InterfaceValidationError> {
    reader.read_u32().map_err(map_wire_error)
}

pub(crate) const fn map_wire_error(error: WireDecodeError) -> InterfaceValidationError {
    match error {
        WireDecodeError::Truncated => InterfaceValidationError::Truncated,
        WireDecodeError::TrailingBytes => InterfaceValidationError::Malformed,
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
        count: usize,
    ) -> Result<Vec<T>, InterfaceValidationError> {
        const MINIMUM_ITEM_WIRE_BYTES: usize = std::mem::size_of::<u32>();

        if count.saturating_mul(MINIMUM_ITEM_WIRE_BYTES) > reader.remaining() {
            return Err(InterfaceValidationError::Truncated);
        }

        self.charge_items::<T>(count)?;

        let mut values = Vec::new();

        values
            .try_reserve_exact(count)
            .map_err(|_| InterfaceValidationError::Malformed)?;

        Ok(values)
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
