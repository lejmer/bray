use crate::{BindingQueryError, unit::BoundUnitConstructionError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BindingError<Upstream = std::convert::Infallible> {
    Cancelled,
    CheckerInfrastructure(bray_checker::CheckerInfrastructureError),
    Upstream(Upstream),
    DependencyUnavailable,
    IdentityCapacityExceeded,
    RollbackFailed,
    TransactionContextMismatch,
    ControlTargetMismatch,
    UnsupportedSyntax,
    Construction(BoundUnitConstructionError),
}

impl<Upstream> From<BindingQueryError<Upstream>> for BindingError<Upstream> {
    fn from(error: BindingQueryError<Upstream>) -> Self {
        match error {
            BindingQueryError::Cancelled => Self::Cancelled,
            BindingQueryError::CheckerInfrastructure(error) => Self::CheckerInfrastructure(error),
            BindingQueryError::Upstream(error) => Self::Upstream(error),
            BindingQueryError::DependencyUnavailable => Self::DependencyUnavailable,
        }
    }
}

impl<Upstream> From<BoundUnitConstructionError> for BindingError<Upstream> {
    fn from(error: BoundUnitConstructionError) -> Self {
        Self::Construction(error)
    }
}

pub(crate) type BindingResult<T, Upstream = std::convert::Infallible> =
    Result<T, BindingError<Upstream>>;
