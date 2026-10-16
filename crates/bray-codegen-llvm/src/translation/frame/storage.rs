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
    activation: PointerValue<'context>,
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
        RuntimeAbiRole::OutgoingActivation,
    )?;

    let record = builder
        .build_call(
            claim,
            &[descriptor.as_pointer_value().into(), lease.into()],
            "",
        )
        .map_err(CodegenFailure::backend_library)?
        .try_as_basic_value()
        .basic()
        .expect("admitted frame construction claims its report record")
        .into_int_value();

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

    let header = header_pointer(builder, context_type, storage)?;
    let header_type = crate::native::activation::header_type(context, request.target());

    for (field, value) in [
        (0, inkwell::values::BasicValueEnum::from(value)),
        (1, activation.into()),
        (2, address.into()),
    ] {
        let destination = builder
            .build_struct_gep(header_type, header, field, "frame.activation.field")
            .map_err(CodegenFailure::backend_library)?;

        builder
            .build_store(destination, value)
            .map_err(CodegenFailure::backend_library)?;
    }

    // The same bundle admits frame bytes and one primary report before capture
    // transfer. Keep the claimed record with its inactive owner through dispatch.
    let outgoing = builder
        .build_struct_gep(header_type, header, 16, "frame.outgoing")
        .map_err(CodegenFailure::backend_library)?;

    let records_type = header_type
        .get_field_type_at_index(16)
        .expect("activation header owns its outgoing records")
        .into_struct_type();

    for (field, value) in [record, record, record.get_type().const_int(1, false)]
        .into_iter()
        .enumerate()
    {
        let destination = builder
            .build_struct_gep(
                records_type,
                outgoing,
                u32::try_from(field).expect("report records have three fields"),
                "frame.outgoing.field",
            )
            .map_err(CodegenFailure::backend_library)?;

        builder
            .build_store(destination, value)
            .map_err(CodegenFailure::backend_library)?;
    }

    Ok(storage)
}

pub(super) fn publish<'context>(
    module: &Module<'context>,
    request: CodegenRequest<'_>,
    context_type: StructType<'context>,
    storage: PointerValue<'context>,
    activation: PointerValue<'context>,
    builder: &Builder<'context>,
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<(), CodegenFailure> {
    let context = types.context();
    let lease_type = crate::mapping::cleanup::lease_type(context, request.target());
    let header = header_pointer(builder, context_type, storage)?;

    let lease = builder
        .build_load(lease_type, header, "frame.publication.lease")
        .map_err(CodegenFailure::backend_library)?;

    let token_type = crate::native::inactive_frame_type(context, request.target());

    let token = builder
        .build_insert_value(token_type.const_zero(), lease, 0, "frame.publication.token")
        .map_err(CodegenFailure::backend_library)?;

    let token = builder
        .build_insert_value(token, activation, 1, "frame.publication.descriptor")
        .map_err(CodegenFailure::backend_library)?;

    let slot =
        crate::translation::allocate_temporary(context, builder, token_type, "frame.publication")?;

    builder
        .build_store(slot, token)
        .map_err(CodegenFailure::backend_library)?;

    let publish = crate::native::declare_runtime_function(
        module,
        context,
        request.target(),
        RuntimeAbiRole::ActivationPublication,
    )?;

    let status = builder
        .build_call(publish, &[slot.into()], "frame.published")
        .map_err(CodegenFailure::backend_library)?
        .try_as_basic_value()
        .basic()
        .expect("activation publication returns its status");

    crate::translation::require_runtime_success(
        module,
        context,
        builder,
        status,
        "frame.publication",
    )
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

    let header = header_pointer(&builder, context_type, storage)?;

    let retained = builder
        .build_struct_gep(
            crate::native::activation::header_type(context, request.target()),
            header,
            0,
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

pub(crate) fn header_pointer<'context>(
    builder: &Builder<'context>,
    context_type: StructType<'context>,
    storage: PointerValue<'context>,
) -> Result<PointerValue<'context>, CodegenFailure> {
    builder
        .build_struct_gep(
            context_type,
            storage,
            context_type.count_fields() - 1,
            "frame.activation",
        )
        .map_err(CodegenFailure::backend_library)
}
