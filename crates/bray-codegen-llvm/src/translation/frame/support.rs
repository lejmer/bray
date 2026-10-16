use bray_codegen::CodegenFailure;
use bray_ir::MirStorageId;

pub(crate) fn frame_storage_field_index(storage: MirStorageId) -> Result<u32, CodegenFailure> {
    storage
        .slot()
        .checked_add(3)
        .ok_or(CodegenFailure::ResourceExhausted)
}

pub(crate) fn terminal_state<'context>(
    context: &'context inkwell::context::Context,
    descriptor: &bray_ir::MirFrameDescriptor,
) -> Result<inkwell::values::IntValue<'context>, CodegenFailure> {
    let terminal: u32 =
        crate::conversion::resource_limit(descriptor.states().len(), "frame terminal state")?;

    Ok(context.i32_type().const_int(u64::from(terminal), false))
}

pub(crate) fn cancellation_requested<'context>(
    context: &'context inkwell::context::Context,
    builder: &inkwell::builder::Builder<'context>,
    context_type: inkwell::types::StructType<'context>,
    storage: inkwell::values::PointerValue<'context>,
) -> Result<inkwell::values::IntValue<'context>, CodegenFailure> {
    let pointer = builder
        .build_struct_gep(context_type, storage, 2, "frame.cancellation.pointer")
        .map_err(CodegenFailure::backend_library)?;

    let requested = builder
        .build_load(context.i8_type(), pointer, "frame.cancellation")
        .map_err(CodegenFailure::backend_library)?
        .into_int_value();

    crate::translation::unit::nonzero_integer(builder, requested, "frame.cancellation.requested")
}
