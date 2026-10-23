use bray_compiler_known::ImplementationHook;
use bray_ir::{
    MirBinaryOperator, MirImmediateValue, MirNumericConversionKind, MirOperand, MirOperationKind,
};
use bray_symbols::{
    ConstantValueData, ConstantValueKind, NumericTruncation, SemanticValueStore, TypeId,
};

pub(crate) fn can_lower_directly(
    values: &SemanticValueStore,
    callable: bray_symbols::CallableInstanceData,
    hook: ImplementationHook,
) -> bool {
    hook == ImplementationHook::NumericTruncate
        || values.substitution_is_concrete(callable.substitution())
}

pub(crate) fn lower_numeric_value<E>(
    values: &SemanticValueStore,
    hook: ImplementationHook,
    plan: Option<NumericTruncation>,
    input: Option<(MirOperand, TypeId)>,
    result: TypeId,
    mut emit: impl FnMut(MirOperationKind, TypeId) -> Result<MirOperand, E>,
    value_error: impl Fn(bray_symbols::SemanticValueStoreError) -> E,
) -> Result<MirOperand, E> {
    let boolean = |value| MirOperand::Immediate {
        value: MirImmediateValue::Boolean(value),
        ty: result,
    };

    if hook == ImplementationHook::NumericTruncationSupported {
        assert!(
            input.is_none(),
            "numeric type support query has no value arguments"
        );

        return Ok(boolean(plan.is_some()));
    }

    let Some((input, ty)) = input else {
        panic!("numeric value operation retains its one scalar input");
    };

    if hook == ImplementationHook::NumericTruncate {
        return emit(
            MirOperationKind::NumericConversion {
                kind: MirNumericConversionKind::Truncate,
                operand: input,
            },
            result,
        );
    }

    assert_eq!(hook, ImplementationHook::NumericTruncationDomain);

    let plan = plan.expect("checked numeric policy satisfies its supported scalar type constraint");

    let NumericTruncation::RealToInteger {
        minimum,
        minimum_inclusive,
        maximum,
        ..
    } = plan
    else {
        return Ok(boolean(true));
    };

    let bound = |bits| {
        values
            .intern_constant_value(ConstantValueData::new(ty, ConstantValueKind::Real(bits)))
            .map(|value| MirOperand::Constant { value, ty })
    };

    let minimum = bound(minimum).map_err(&value_error)?;
    let maximum = bound(maximum).map_err(value_error)?;

    let lower = emit(
        MirOperationKind::Binary {
            operator: if minimum_inclusive {
                MirBinaryOperator::GreaterThanOrEqual
            } else {
                MirBinaryOperator::GreaterThan
            },
            left: input.clone(),
            right: minimum,
        },
        result,
    )?;

    let upper = emit(
        MirOperationKind::Binary {
            operator: MirBinaryOperator::LessThan,
            left: input,
            right: maximum,
        },
        result,
    )?;

    emit(
        MirOperationKind::Binary {
            operator: MirBinaryOperator::BitwiseAnd,
            left: lower,
            right: upper,
        },
        result,
    )
}
