use crate::unit::BoundUnitConstructionError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BindingError {
    Cancelled,
    IdentityCapacityExceeded,
    RollbackFailed,
    ControlTargetMismatch,
    UnsupportedSyntax,
    Construction(BoundUnitConstructionError),
}

impl From<BoundUnitConstructionError> for BindingError {
    fn from(error: BoundUnitConstructionError) -> Self {
        Self::Construction(error)
    }
}

pub(crate) type BindingResult<T> = Result<T, BindingError>;
