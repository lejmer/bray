use bray_compiler_known::ImplementationHook;
use bray_ir::{
    MirBinaryOperator, MirBlockId, MirImmediateValue, MirNumericConversionKind, MirOperand,
    MirOperationKind, MirSourceAnchor, MirUnitBuilder,
};
use bray_symbols::{
    ConstantValueData, ConstantValueKind, NumericTruncation, SemanticValueStore, TypeId,
};

use crate::LoweringError;

pub(crate) fn can_lower_directly(
    values: &SemanticValueStore,
    callable: bray_symbols::CallableInstanceData,
    hook: ImplementationHook,
) -> bool {
    hook == ImplementationHook::NumericTruncate
        || values.substitution_is_concrete(callable.substitution())
}

pub(crate) fn lower_numeric_value(
    builder: &mut MirUnitBuilder,
    values: &SemanticValueStore,
    block: MirBlockId,
    source: &MirSourceAnchor,
    hook: ImplementationHook,
    plan: Option<NumericTruncation>,
    inputs: &[MirOperand],
    result: TypeId,
) -> Result<MirOperand, LoweringError> {
    let boolean = |value| MirOperand::Immediate {
        value: MirImmediateValue::Boolean(value),
        ty: result,
    };

    if hook == ImplementationHook::NumericTruncationSupported {
        assert!(
            inputs.is_empty(),
            "numeric type support query has no value arguments"
        );

        return Ok(boolean(plan.is_some()));
    }

    let [input] = inputs else {
        panic!("numeric value operation retains its one scalar input");
    };

    if hook == ImplementationHook::NumericTruncate {
        return push_numeric_value(
            builder,
            block,
            source,
            MirOperationKind::NumericConversion {
                kind: MirNumericConversionKind::Truncate,
                operand: input.clone(),
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

    let ty = builder.operand_type(input);

    let bound = |bits| {
        values
            .intern_constant_value(ConstantValueData::new(ty, ConstantValueKind::Real(bits)))
            .map(|value| MirOperand::Constant { value, ty })
    };

    let minimum = bound(minimum)?;
    let maximum = bound(maximum)?;

    let lower = push_numeric_value(
        builder,
        block,
        source,
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

    let upper = push_numeric_value(
        builder,
        block,
        source,
        MirOperationKind::Binary {
            operator: MirBinaryOperator::LessThan,
            left: input.clone(),
            right: maximum,
        },
        result,
    )?;

    push_numeric_value(
        builder,
        block,
        source,
        MirOperationKind::Binary {
            operator: MirBinaryOperator::BitwiseAnd,
            left: lower,
            right: upper,
        },
        result,
    )
}

fn push_numeric_value(
    builder: &mut MirUnitBuilder,
    block: MirBlockId,
    source: &MirSourceAnchor,
    kind: MirOperationKind,
    ty: TypeId,
) -> Result<MirOperand, LoweringError> {
    // Every generated scalar operation retains the same source identity independently.
    let commit = builder.push_operation(block, source.clone(), kind, Some(ty))?;

    Ok(MirOperand::Value(
        commit
            .result()
            .expect("numeric value operation has its typed result"),
    ))
}
