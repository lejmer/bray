use crate::CheckerInfrastructureError;

pub(super) fn selection_ordinal_u32(value: usize) -> Result<u32, CheckerInfrastructureError> {
    match u32::try_from(value) {
        Ok(value) => Ok(value),
        Err(_) => Err(CheckerInfrastructureError::SelectionInputCapacityExceeded {
            count: value,
        }),
    }
}

pub(super) fn selection_ordinal_u64(value: usize) -> Result<u64, CheckerInfrastructureError> {
    match u64::try_from(value) {
        Ok(value) => Ok(value),
        Err(_) => Err(CheckerInfrastructureError::SelectionInputCapacityExceeded {
            count: value,
        }),
    }
}
