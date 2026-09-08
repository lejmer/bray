use bray_ir::MirOperand;
use bray_symbols::{
    ConstantValueData, ConstantValueKind, IntegerConstant, SemanticValueStore,
    SemanticValueStoreError, TypeId,
};

pub(crate) fn integer_constant(
    values: &SemanticValueStore,
    ty: TypeId,
    value: u64,
) -> Result<MirOperand, SemanticValueStoreError> {
    let value = values.intern_constant_value(ConstantValueData::new(
        ty,
        ConstantValueKind::Integer(IntegerConstant::from_u64(value)),
    ))?;

    Ok(MirOperand::Constant { value, ty })
}
