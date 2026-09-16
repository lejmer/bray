use bray_codegen::{
    CodegenCallableSignature, CodegenFailure, CodegenHelperMapping, CodegenParameterMapping,
    CodegenResultMapping,
};
use bray_ir::{MirBinaryOperator, MirHelperReference};
use bray_runtime_abi::NativeRunState;
use bray_symbols::{IntegerConstant, IntegerSign, RealConstantBits};
use inkwell::builder::{Builder, BuilderError};
use inkwell::values::{AggregateValueEnum, BasicValueEnum, IntValue, PointerValue};
use inkwell::{FloatPredicate, IntPredicate};

pub(super) fn llvm<T>(result: Result<T, BuilderError>) -> Result<T, CodegenFailure> {
    result.map_err(CodegenFailure::backend_library)
}

pub(super) fn nonzero_integer<'context>(
    builder: &Builder<'context>,
    value: IntValue<'context>,
    name: &str,
) -> Result<IntValue<'context>, CodegenFailure> {
    llvm(builder.build_int_compare(IntPredicate::NE, value, value.get_type().const_zero(), name))
}

pub(super) fn next_helper<'mapping>(
    helpers: &mut impl Iterator<Item = &'mapping CodegenHelperMapping>,
    expected: &MirHelperReference,
) -> &'mapping CodegenHelperMapping {
    let helper = helpers
        .next()
        .unwrap_or_else(|| panic!("checked MIR is missing helper {expected:?}"));

    if helper.reference() != expected {
        panic!(
            "checked MIR expected helper {expected:?}, got {:?}",
            helper.reference()
        );
    }

    helper
}

pub(super) fn parameter_type(
    signature: &CodegenCallableSignature,
    parameter: usize,
) -> bray_symbols::TypeId {
    match signature.parameters().get(parameter) {
        Some(CodegenParameterMapping::Direct { ty, .. }) => *ty,
        Some(CodegenParameterMapping::Indirect { pointee, .. }) => *pointee,
        Some(CodegenParameterMapping::Ignore) | None => {
            panic!(
                "callable signature has no represented type for parameter {parameter}: {signature:?}"
            )
        }
    }
}

pub(super) fn result_type(
    signature: &CodegenCallableSignature,
) -> bray_symbols::TypeId {
    match signature.result() {
        CodegenResultMapping::Direct { ty, .. } => *ty,
        CodegenResultMapping::Indirect { pointee, .. } => *pointee,
        CodegenResultMapping::Void => {
            panic!("void callable signature has no represented result type: {signature:?}")
        }
    }
}

pub(super) fn insert_value<'context>(
    builder: &Builder<'context>,
    aggregate: BasicValueEnum<'context>,
    value: BasicValueEnum<'context>,
    index: usize,
) -> Result<BasicValueEnum<'context>, CodegenFailure> {
    let index = crate::conversion::resource_limit(index, "aggregate_field_index")?;

    let value = match aggregate {
        BasicValueEnum::ArrayValue(aggregate) => {
            llvm(builder.build_insert_value(aggregate, value, index, "aggregate.element"))?
        }
        BasicValueEnum::StructValue(aggregate) => {
            llvm(builder.build_insert_value(aggregate, value, index, "aggregate.field"))?
        }
        unexpected => panic!("checked MIR translation violated an established compiler contract: {unexpected:?}"),
    };

    match value {
        AggregateValueEnum::ArrayValue(value) => Ok(value.into()),
        AggregateValueEnum::StructValue(value) => Ok(value.into()),
    }
}

pub(super) fn extract_value<'context>(
    builder: &Builder<'context>,
    aggregate: BasicValueEnum<'context>,
    index: u32,
) -> Result<BasicValueEnum<'context>, CodegenFailure> {
    match aggregate {
        BasicValueEnum::ArrayValue(aggregate) => {
            llvm(builder.build_extract_value(aggregate, index, "projection.value"))
        }
        BasicValueEnum::StructValue(aggregate) => {
            llvm(builder.build_extract_value(aggregate, index, "projection.value"))
        }
        unexpected => panic!("checked MIR translation violated an established compiler contract: {unexpected:?}"),
    }
}

pub(crate) fn native_run_outcome<'context>(
    builder: &Builder<'context>,
    outcome: BasicValueEnum<'context>,
) -> Result<(IntValue<'context>, IntValue<'context>), CodegenFailure> {
    let state = extract_value(builder, outcome, 0)
        .map(|value| int_value(value).expect("checked MIR translation requires an established mapping or value"))?;

    let payload = extract_value(builder, outcome, 1)
        .map(|value| int_value(value).expect("checked MIR translation requires an established mapping or value"))?;

    Ok((state, payload))
}

pub(super) fn native_run_outcome_value<'context>(
    context: &'context inkwell::context::Context,
    builder: &Builder<'context>,
    target: &bray_codegen::CodegenTarget,
    state: NativeRunState,
    payload: BasicValueEnum<'context>,
) -> Result<BasicValueEnum<'context>, CodegenFailure> {
    let outcome = crate::native::run_outcome_type(context, target)
        .const_zero()
        .into();

    let payload_index = if state == NativeRunState::PANICKED {
        2
    } else {
        1
    };

    let state = context
        .i32_type()
        .const_int(u64::from(state.code()), false)
        .into();

    let outcome = insert_value(builder, outcome, state, 0)?;

    insert_value(builder, outcome, payload, payload_index)
}

pub(crate) fn native_run_state_is<'context>(
    builder: &Builder<'context>,
    state: IntValue<'context>,
    expected: NativeRunState,
    name: &str,
) -> Result<IntValue<'context>, CodegenFailure> {
    llvm(
        builder.build_int_compare(
            IntPredicate::EQ,
            state,
            state
                .get_type()
                .const_int(u64::from(expected.code()), false),
            name,
        ),
    )
}

pub(super) fn aggregate_value_length(value: BasicValueEnum<'_>) -> u32 {
    match value {
        BasicValueEnum::ArrayValue(value) => value.get_type().len(),
        BasicValueEnum::StructValue(value) => value.get_type().count_fields(),
        unexpected => panic!("checked MIR translation violated an established compiler contract: {unexpected:?}"),
    }
}

pub(super) fn physical_aggregate_element(
    fields: &[bray_codegen::CodegenFieldLayout],
    semantic_index: usize,
    field_size: impl Fn(&bray_codegen::CodegenFieldLayout) -> Result<u64, CodegenFailure>,
) -> Result<u32, CodegenFailure> {
    if semantic_index >= fields.len() {
        panic!(
            "aggregate semantic field index {semantic_index} exceeds field count {}",
            fields.len()
        );
    }

    let mut element = 0_u32;
    let mut previous_end = 0_u64;

    for (index, field) in fields.iter().enumerate() {
        if field.offset_bytes() > previous_end {
            element = element
                .checked_add(1)
                .ok_or(CodegenFailure::ResourceExhausted)?;
        }

        if index == semantic_index {
            return Ok(element);
        }

        element = element
            .checked_add(1)
            .ok_or(CodegenFailure::ResourceExhausted)?;

        let field_size = field_size(field)?;

        previous_end = field
            .offset_bytes()
            .checked_add(field_size)
            .ok_or(CodegenFailure::ResourceExhausted)?;
    }

    panic!("checked MIR translation violated an established compiler contract")
}

pub(super) fn integer_words(magnitude: &[u8]) -> Vec<u64> {
    let mut words: Vec<_> = magnitude
        .rchunks(8)
        .map(|chunk| {
            chunk
                .iter()
                .fold(0_u64, |word, byte| (word << 8) | u64::from(*byte))
        })
        .collect();

    if words.is_empty() {
        words.push(0);
    }

    words
}

pub(crate) fn integer_constant<'context>(
    ty: inkwell::types::IntType<'context>,
    value: &IntegerConstant,
) -> IntValue<'context> {
    let integer = ty.const_int_arbitrary_precision(&integer_words(value.magnitude()));

    if value.sign() == IntegerSign::Negative {
        integer.const_neg()
    } else {
        integer
    }
}

pub(crate) const fn real_width(bits: RealConstantBits) -> u32 {
    match bits {
        RealConstantBits::Binary16(_) => 16,
        RealConstantBits::Binary32(_) => 32,
        RealConstantBits::Binary64(_) => 64,
        RealConstantBits::Binary128(_) => 128,
    }
}

pub(crate) fn real_words(bits: RealConstantBits) -> Vec<u64> {
    match bits {
        RealConstantBits::Binary16(bits) => vec![u64::from(bits)],
        RealConstantBits::Binary32(bits) => vec![u64::from(bits)],
        RealConstantBits::Binary64(bits) => vec![bits],
        RealConstantBits::Binary128(bits) => {
            let high = u64::from_be_bytes([
                bits[0], bits[1], bits[2], bits[3], bits[4], bits[5], bits[6], bits[7],
            ]);

            let low = u64::from_be_bytes([
                bits[8], bits[9], bits[10], bits[11], bits[12], bits[13], bits[14], bits[15],
            ]);

            vec![low, high]
        }
    }
}

pub(crate) const fn pointer_value(value: BasicValueEnum<'_>) -> Option<PointerValue<'_>> {
    match value {
        BasicValueEnum::PointerValue(value) => Some(value),
        BasicValueEnum::ArrayValue(_)
        | BasicValueEnum::IntValue(_)
        | BasicValueEnum::FloatValue(_)
        | BasicValueEnum::StructValue(_)
        | BasicValueEnum::VectorValue(_)
        | BasicValueEnum::ScalableVectorValue(_) => None,
    }
}

pub(super) const fn int_value(value: BasicValueEnum<'_>) -> Option<IntValue<'_>> {
    match value {
        BasicValueEnum::IntValue(value) => Some(value),
        BasicValueEnum::ArrayValue(_)
        | BasicValueEnum::FloatValue(_)
        | BasicValueEnum::PointerValue(_)
        | BasicValueEnum::StructValue(_)
        | BasicValueEnum::VectorValue(_)
        | BasicValueEnum::ScalableVectorValue(_) => None,
    }
}

pub(super) const fn integer_predicate(operator: MirBinaryOperator, signed: bool) -> IntPredicate {
    match operator {
        MirBinaryOperator::Equal => IntPredicate::EQ,
        MirBinaryOperator::NotEqual => IntPredicate::NE,
        MirBinaryOperator::LessThan if signed => IntPredicate::SLT,
        MirBinaryOperator::LessThan => IntPredicate::ULT,
        MirBinaryOperator::LessThanOrEqual if signed => IntPredicate::SLE,
        MirBinaryOperator::LessThanOrEqual => IntPredicate::ULE,
        MirBinaryOperator::GreaterThan if signed => IntPredicate::SGT,
        MirBinaryOperator::GreaterThan => IntPredicate::UGT,
        MirBinaryOperator::GreaterThanOrEqual if signed => IntPredicate::SGE,
        MirBinaryOperator::GreaterThanOrEqual => IntPredicate::UGE,
        MirBinaryOperator::Add
        | MirBinaryOperator::Subtract
        | MirBinaryOperator::Multiply
        | MirBinaryOperator::Divide
        | MirBinaryOperator::Remainder
        | MirBinaryOperator::BitwiseAnd
        | MirBinaryOperator::BitwiseOr
        | MirBinaryOperator::BitwiseXor
        | MirBinaryOperator::ShiftLeft
        | MirBinaryOperator::ShiftRight => IntPredicate::EQ,
    }
}

pub(super) const fn float_predicate(operator: MirBinaryOperator) -> FloatPredicate {
    match operator {
        MirBinaryOperator::Equal => FloatPredicate::OEQ,
        MirBinaryOperator::NotEqual => FloatPredicate::ONE,
        MirBinaryOperator::LessThan => FloatPredicate::OLT,
        MirBinaryOperator::LessThanOrEqual => FloatPredicate::OLE,
        MirBinaryOperator::GreaterThan => FloatPredicate::OGT,
        MirBinaryOperator::GreaterThanOrEqual => FloatPredicate::OGE,
        MirBinaryOperator::Add
        | MirBinaryOperator::Subtract
        | MirBinaryOperator::Multiply
        | MirBinaryOperator::Divide
        | MirBinaryOperator::Remainder
        | MirBinaryOperator::BitwiseAnd
        | MirBinaryOperator::BitwiseOr
        | MirBinaryOperator::BitwiseXor
        | MirBinaryOperator::ShiftLeft
        | MirBinaryOperator::ShiftRight => FloatPredicate::PredicateFalse,
    }
}

#[cfg(test)]
mod tests {
    use bray_codegen::CodegenFieldLayout;
    use bray_symbols::{SemanticValueStore, TypeData};

    use super::{integer_words, physical_aggregate_element};

    #[test]
    fn zero_integer_constants_supply_one_llvm_word() {
        assert_eq!(integer_words(&[]), [0]);
    }

    #[test]
    fn aggregate_elements_account_for_field_size_before_padding() {
        let Ok(store) = SemanticValueStore::try_new() else {
            panic!("test semantic value store must be available");
        };

        let Ok(ty) = store.intern_type(TypeData::Error) else {
            panic!("test type must intern");
        };

        let adjacent = [
            CodegenFieldLayout::new(None, ty, 0),
            CodegenFieldLayout::new(None, ty, 4),
        ];

        let padded = [
            CodegenFieldLayout::new(None, ty, 0),
            CodegenFieldLayout::new(None, ty, 8),
        ];

        assert_eq!(physical_aggregate_element(&adjacent, 1, |_| Ok(4)), Ok(1));

        assert_eq!(physical_aggregate_element(&padded, 1, |_| Ok(4)), Ok(2));

        let panic = std::panic::catch_unwind(|| {
            physical_aggregate_element(&adjacent, 2, |_| Ok(4))
        })
        .expect_err("out-of-range aggregate field must panic");

        let message = bray_testing::panic_payload_text(panic.as_ref());

        assert!(message.contains("index 2"));
        assert!(message.contains("field count 2"));
    }
}
