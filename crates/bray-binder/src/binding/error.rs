use crate::{BindingQueryError, unit::BoundUnitConstructionError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BindingError {
    Cancelled,
    DependencyUnavailable,
    IdentityCapacityExceeded,
    RollbackFailed,
    TransactionContextMismatch,
    ControlTargetMismatch,
    UnsupportedSyntax,
    Construction(BoundUnitConstructionError),
}

impl From<BindingQueryError> for BindingError {
    fn from(error: BindingQueryError) -> Self {
        match error {
            BindingQueryError::Cancelled => Self::Cancelled,
            BindingQueryError::DependencyUnavailable => Self::DependencyUnavailable,
        }
    }
}

impl From<BoundUnitConstructionError> for BindingError {
    fn from(error: BoundUnitConstructionError) -> Self {
        Self::Construction(error)
    }
}

pub(crate) type BindingResult<T> = Result<T, BindingError>;
