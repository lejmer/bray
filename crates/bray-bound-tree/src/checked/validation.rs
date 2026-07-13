use bray_symbols::AnonymousCallableSymbolId;

use super::{CheckedUnitBuildError, data::CheckedUnitData};
use crate::{BoundCallableBodyId, BoundExpressionId, BoundNodeKind};

pub(super) fn validate_anonymous_callable(
    data: &CheckedUnitData,
    callable: AnonymousCallableSymbolId,
) -> Result<(), CheckedUnitBuildError> {
    let expected = data.local_symbols().region();

    if callable.region() != expected {
        return Err(CheckedUnitBuildError::AnonymousCallableRegionMismatch {
            expected,
            actual: callable.region(),
        });
    }

    if data.local_symbols().anonymous_callable(callable).is_none() {
        return Err(CheckedUnitBuildError::MissingAnonymousCallable { callable });
    }

    Ok(())
}

pub(super) fn validate_callable_root(
    data: &CheckedUnitData,
    root: BoundCallableBodyId,
) -> Result<(), CheckedUnitBuildError> {
    if data.tree().callable_body(root).is_none() {
        return Err(CheckedUnitBuildError::MissingRoot {
            unit: data.unit(),
            kind: BoundNodeKind::CallableBody,
        });
    }

    Ok(())
}

pub(super) fn validate_expression_root(
    data: &CheckedUnitData,
    root: BoundExpressionId,
) -> Result<(), CheckedUnitBuildError> {
    if data.tree().expression(root).is_none() {
        return Err(CheckedUnitBuildError::MissingRoot {
            unit: data.unit(),
            kind: BoundNodeKind::Expression,
        });
    }

    Ok(())
}
