use bray_codegen::CodegenFailure;
use bray_ir::MirStorageId;

pub(crate) fn frame_storage_field_index(storage: MirStorageId) -> Result<u32, CodegenFailure> {
    storage
        .slot()
        .checked_add(3)
        .and_then(|index| u32::try_from(index).ok())
        .ok_or(CodegenFailure::ResourceExhausted)
}
