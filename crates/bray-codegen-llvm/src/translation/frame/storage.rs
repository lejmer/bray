use bray_codegen::{CodegenFailure, CodegenInstance, CodegenRequest};
use bray_runtime_interface::{ProtectedFrameOperation, RuntimeAbiRole};
use inkwell::{
    AddressSpace, builder::Builder, module::Module, types::StructType, values::PointerValue,
};

use crate::mapping::LlvmTypeMappings;

pub(super) fn claim<'context>(
    module: &Module<'context>,
    request: CodegenRequest<'_>,
    instance: &CodegenInstance,
    context_type: StructType<'context>,
    builder: &Builder<'context>,
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<PointerValue<'context>, CodegenFailure> {
    let context = types.context();
    let lease_type = crate::mapping::cleanup::lease_type(context, request.target());

    let lease =
        crate::translation::allocate_temporary(context, builder, lease_type, "frame.lease")?;

    let descriptor =
        crate::mapping::cleanup::storage_descriptor(module, request.mappings(), instance.key());

    let claim = crate::native::declare_runtime_function(
        module,
        context,
        request.target(),
        RuntimeAbiRole::CleanupStorageClaim,
    )?;

    builder
        .build_call(
            claim,
            &[descriptor.as_pointer_value().into(), lease.into()],
            "",
        )
        .map_err(CodegenFailure::backend_library)?;

    let value = builder
        .build_load(lease_type, lease, "frame.lease.value")
        .map_err(CodegenFailure::backend_library)?
        .into_struct_value();

    let address = builder
        .build_extract_value(value, 4, "frame.lease.address")
        .map_err(CodegenFailure::backend_library)?
        .into_int_value();

    let storage = builder
        .build_int_to_ptr(
            address,
            context.ptr_type(AddressSpace::default()),
            "frame.storage",
        )
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_store(storage, context_type.const_zero())
        .map_err(CodegenFailure::backend_library)?;

    let retained = builder
        .build_struct_gep(
            context_type,
            storage,
            context_type.count_fields() - 1,
            "frame.backing",
        )
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_store(retained, value)
        .map_err(CodegenFailure::backend_library)?;

    Ok(storage)
}

pub(super) fn translate_destruction<'context>(
    module: &Module<'context>,
    request: CodegenRequest<'_>,
    instance: &CodegenInstance,
    context_type: StructType<'context>,
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<(), CodegenFailure> {
    let function = crate::native::frame_operation_function(
        module,
        request,
        instance,
        ProtectedFrameOperation::Destruction,
    );

    let context = types.context();
    let builder = context.create_builder();
    let block = context.append_basic_block(function, "frame.destroy");

    builder.position_at_end(block);

    let storage = super::core::integer_pointer(&builder, function, 0, types)?;

    let retained = builder
        .build_struct_gep(
            context_type,
            storage,
            context_type.count_fields() - 1,
            "frame.backing",
        )
        .map_err(CodegenFailure::backend_library)?;

    let lease_type = crate::mapping::cleanup::lease_type(context, request.target());

    let value = builder
        .build_load(lease_type, retained, "frame.lease.value")
        .map_err(CodegenFailure::backend_library)?;

    // Retirement writes its empty lease after making the slot reusable. Keep the
    // destination outside that slot so a concurrent admission cannot race the write.
    let lease =
        crate::translation::allocate_temporary(context, &builder, lease_type, "frame.release")?;

    builder
        .build_store(lease, value)
        .map_err(CodegenFailure::backend_library)?;

    let release = crate::native::declare_runtime_function(
        module,
        context,
        request.target(),
        RuntimeAbiRole::CleanupStorageRelease,
    )?;

    let status = builder
        .build_call(release, &[lease.into()], "frame.released")
        .map_err(CodegenFailure::backend_library)?
        .try_as_basic_value()
        .basic()
        .expect("storage release returns its status");

    crate::translation::require_runtime_success(
        module,
        context,
        &builder,
        status,
        "frame.release",
    )?;

    builder
        .build_return(None)
        .map_err(CodegenFailure::backend_library)?;

    Ok(())
}
