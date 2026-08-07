use bray_codegen::CodegenFailure;
use bray_ir::MirStorageId;

pub(crate) fn frame_storage_field_index(storage: MirStorageId) -> Result<u32, CodegenFailure> {
    storage
        .slot()
        .checked_add(3)
        .ok_or(CodegenFailure::ResourceExhausted)
}
