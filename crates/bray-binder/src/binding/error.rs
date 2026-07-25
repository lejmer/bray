use crate::{BinderFactError, unit::BoundUnitConstructionError};

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

impl From<BinderFactError> for BindingError {
    fn from(error: BinderFactError) -> Self {
        match error {
            BinderFactError::Cancelled => Self::Cancelled,
            BinderFactError::DependencyUnavailable => Self::DependencyUnavailable,
        }
    }
}

impl From<BoundUnitConstructionError> for BindingError {
    fn from(error: BoundUnitConstructionError) -> Self {
        Self::Construction(error)
    }
}

pub(crate) type BindingResult<T> = Result<T, BindingError>;
