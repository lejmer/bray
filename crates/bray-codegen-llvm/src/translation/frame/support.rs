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
