use bray_symbols::{ConstantTermData, ConstantTermId, IntegerConstant, TargetSizedIntegerType};

use crate::{CheckerInfrastructureError, CheckerRequestContext, CheckerUnitView};

pub(super) fn array_length<C>(
    request: CheckerUnitView<'_, C>,
    length: usize,
) -> Result<ConstantTermId, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let length = u64::try_from(length)
        .map_err(|_| CheckerInfrastructureError::ConstantArrayLengthCapacityExceeded { length })?;

    request
        .semantic_values()
        .intern_constant_term(ConstantTermData::IntegerLiteral {
            ty: TargetSizedIntegerType::Usize,
            value: IntegerConstant::from_u64(length),
        })
        .map_err(CheckerInfrastructureError::SemanticValueStore)
}
