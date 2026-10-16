use bray_codegen::CodegenTarget;
use inkwell::{AddressSpace, context::Context, types::StructType};

pub(crate) fn descriptor_type<'context>(
    context: &'context Context,
    target: &CodegenTarget,
) -> StructType<'context> {
    let word = super::pointer_integer_type(context, target);
    let pointer = context.ptr_type(AddressSpace::default());

    context.struct_type(
        &[
            crate::mapping::cleanup::descriptor_type(context, target).into(),
            word.into(),
            word.into(),
            word.into(),
            word.into(),
            context.i32_type().into(),
            pointer.into(),
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

pub(crate) fn header_type<'context>(
    context: &'context Context,
    target: &CodegenTarget,
) -> StructType<'context> {
    let word = super::pointer_integer_type(context, target);
    let records = context.struct_type(&[word.into(), word.into(), word.into()], false);

    context.struct_type(
        &[
            crate::mapping::cleanup::lease_type(context, target).into(),
            context.ptr_type(AddressSpace::default()).into(),
            word.into(),
            word.into(),
            word.into(),
            word.into(),
            word.into(),
            context.i64_type().into(),
            context.i64_type().into(),
            context.i32_type().into(),
            context.i32_type().into(),
            context.i32_type().into(),
            context.i32_type().into(),
            context.i32_type().into(),
            super::frame_progress_type(context).into(),
            records.into(),
            records.into(),
        ],
        false,
    )
}
