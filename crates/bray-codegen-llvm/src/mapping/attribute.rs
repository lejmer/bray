use bray_codegen::{CodegenFailure, CodegenValueAttribute};
use inkwell::attributes::Attribute;
use inkwell::types::AnyType;

use super::LlvmTypeMappings;

pub(crate) fn enum_attribute(
    name: &str,
    value: u64,
    types: &LlvmTypeMappings<'_, '_>,
) -> Result<Attribute, CodegenFailure> {
    let kind = attribute_kind(name)?;

    Ok(types.context().create_enum_attribute(kind, value))
}

pub(crate) fn type_attribute(
    name: &str,
    pointee: bray_symbols::TypeId,
    types: &mut LlvmTypeMappings<'_, '_>,
) -> Result<Attribute, CodegenFailure> {
    let kind = attribute_kind(name)?;

    let pointee = types.map(pointee)?.as_any_type_enum();

    Ok(types.context().create_type_attribute(kind, pointee))
}

fn attribute_kind(name: &str) -> Result<u32, CodegenFailure> {
    let kind = Attribute::get_named_enum_kind_id(name);

    (kind != 0)
        .then_some(kind)
        .ok_or(CodegenFailure::UnsupportedTarget)
}

pub(crate) const fn value_attribute_name(attribute: CodegenValueAttribute) -> &'static str {
    match attribute {
        CodegenValueAttribute::InRegister => "inreg",
        CodegenValueAttribute::NoAlias => "noalias",
        CodegenValueAttribute::NonNull => "nonnull",
        CodegenValueAttribute::NoUndef => "noundef",
    }
}
