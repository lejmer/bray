use inkwell::AddressSpace;
use inkwell::context::Context;
use inkwell::types::StructType;

use bray_codegen::CodegenTarget;

use super::core::pointer_integer_type;

pub(crate) fn frame_metadata_type<'context>(
    context: &'context Context,
    target: &CodegenTarget,
) -> StructType<'context> {
    let usize = pointer_integer_type(context, target);

    context.struct_type(
        &[
            context.i8_type().array_type(32).into(),
            context.i32_type().into(),
            usize.into(),
            usize.into(),
            usize.into(),
            usize.into(),
            context.ptr_type(AddressSpace::default()).into(),
        ],
        false,
    )
}

pub(crate) fn protected_frame_type<'context>(
    context: &'context Context,
    target: &CodegenTarget,
) -> StructType<'context> {
    let pointer = context.ptr_type(AddressSpace::default());

    context.struct_type(
        &[
            pointer_integer_type(context, target).into(),
            frame_metadata_type(context, target).into(),
            pointer.into(),
            pointer.into(),
            pointer.into(),
            pointer.into(),
            pointer.into(),
            pointer.into(),
        ],
        false,
    )
}
