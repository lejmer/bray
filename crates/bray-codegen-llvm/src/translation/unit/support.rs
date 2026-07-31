use bray_codegen::{
    CodegenCallableSignature, CodegenFailure, CodegenHelperMapping, CodegenParameterMapping,
    CodegenResultMapping,
};
use bray_ir::{MirBinaryOperator, MirHelperReference};
use bray_symbols::{IntegerConstant, IntegerSign, RealConstantBits};
use inkwell::builder::{Builder, BuilderError};
use inkwell::values::{AggregateValueEnum, BasicValueEnum, IntValue, PointerValue};
use inkwell::{FloatPredicate, IntPredicate};

pub(super) fn llvm<T>(result: Result<T, BuilderError>) -> Result<T, CodegenFailure> {
    result.map_err(|_| CodegenFailure::BackendLibrary)
}

pub(super) fn next_helper<'mapping>(
    helpers: &mut impl Iterator<Item = &'mapping CodegenHelperMapping>,
    expected: &MirHelperReference,
) -> Result<&'mapping CodegenHelperMapping, CodegenFailure> {
    let helper = helpers
        .next()
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    if helper.reference() != expected {
        return Err(CodegenFailure::GeneratedModuleInvariant);
    }

    Ok(helper)
}

pub(super) fn parameter_type(
    signature: &CodegenCallableSignature,
    parameter: usize,
) -> Result<bray_symbols::TypeId, CodegenFailure> {
    match signature.parameters().get(parameter) {
        Some(CodegenParameterMapping::Direct { ty, .. }) => Ok(*ty),
        Some(CodegenParameterMapping::Indirect { pointee, .. }) => Ok(*pointee),
        Some(CodegenParameterMapping::Ignore) | None => {
            Err(CodegenFailure::GeneratedModuleInvariant)
        }
    }
}

pub(super) fn result_type(
    signature: &CodegenCallableSignature,
) -> Result<bray_symbols::TypeId, CodegenFailure> {
    match signature.result() {
        CodegenResultMapping::Direct { ty, .. } => Ok(*ty),
        CodegenResultMapping::Indirect { pointee, .. } => Ok(*pointee),
        CodegenResultMapping::Void => Err(CodegenFailure::GeneratedModuleInvariant),
    }
}

pub(super) fn insert_value<'context>(
    builder: &Builder<'context>,
    aggregate: BasicValueEnum<'context>,
    value: BasicValueEnum<'context>,
    index: usize,
) -> Result<BasicValueEnum<'context>, CodegenFailure> {
    let index = u32::try_from(index).map_err(|_| CodegenFailure::ResourceExhausted)?;

    let value = match aggregate {
        BasicValueEnum::ArrayValue(aggregate) => {
            llvm(builder.build_insert_value(aggregate, value, index, "aggregate.element"))?
        }
        BasicValueEnum::StructValue(aggregate) => {
            llvm(builder.build_insert_value(aggregate, value, index, "aggregate.field"))?
        }
        _ => return Err(CodegenFailure::GeneratedModuleInvariant),
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
        _ => Err(CodegenFailure::GeneratedModuleInvariant),
    }
}

pub(super) fn aggregate_value_length(value: BasicValueEnum<'_>) -> Result<u32, CodegenFailure> {
    match value {
        BasicValueEnum::ArrayValue(value) => Ok(value.get_type().len()),
        BasicValueEnum::StructValue(value) => Ok(value.get_type().count_fields()),
        _ => Err(CodegenFailure::GeneratedModuleInvariant),
    }
}

pub(super) fn aggregate_element(
    mappings: &bray_codegen::CodegenMappings,
    fields: &[bray_codegen::CodegenFieldLayout],
    semantic_index: usize,
) -> Result<u32, CodegenFailure> {
    physical_aggregate_element(fields, semantic_index, |field| {
        mappings
            .ty(field.ty())
            .and_then(|mapping| mapping.layout().map(|layout| layout.size()))
    })
}

fn physical_aggregate_element(
    fields: &[bray_codegen::CodegenFieldLayout],
    semantic_index: usize,
    field_size: impl Fn(&bray_codegen::CodegenFieldLayout) -> Option<u64>,
) -> Result<u32, CodegenFailure> {
    if semantic_index >= fields.len() {
        return Err(CodegenFailure::GeneratedModuleInvariant);
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

        let field_size = field_size(field).ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        previous_end = field
            .offset_bytes()
            .checked_add(field_size)
            .ok_or(CodegenFailure::ResourceExhausted)?;
    }

    Err(CodegenFailure::GeneratedModuleInvariant)
}

pub(super) fn aggregate_value_element(
    mappings: &bray_codegen::CodegenMappings,
    fields: &[bray_codegen::CodegenFieldLayout],
    semantic_index: usize,
) -> Result<usize, CodegenFailure> {
    usize::try_from(aggregate_element(mappings, fields, semantic_index)?)
        .map_err(|_| CodegenFailure::ResourceExhausted)
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

pub(super) fn integer_constant<'context>(
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

pub(super) const fn real_width(bits: RealConstantBits) -> u32 {
    match bits {
        RealConstantBits::Binary16(_) => 16,
        RealConstantBits::Binary32(_) => 32,
        RealConstantBits::Binary64(_) => 64,
        RealConstantBits::Binary128(_) => 128,
    }
}

pub(super) fn real_words(bits: RealConstantBits) -> Vec<u64> {
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

pub(super) const fn pointer_value(value: BasicValueEnum<'_>) -> Option<PointerValue<'_>> {
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

pub(super) const fn int_value(value: BasicValueEnum<'_>) -> Option<inkwell::values::IntValue<'_>> {
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
    use bray_codegen::{CodegenFailure, CodegenFieldLayout};
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

        assert_eq!(physical_aggregate_element(&adjacent, 1, |_| Some(4)), Ok(1));

        assert_eq!(physical_aggregate_element(&padded, 1, |_| Some(4)), Ok(2));

        assert_eq!(
            physical_aggregate_element(&adjacent, 2, |_| Some(4)),
            Err(CodegenFailure::GeneratedModuleInvariant)
        );
    }
}
