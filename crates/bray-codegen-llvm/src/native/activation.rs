use bray_codegen::{CodegenFailure, CodegenTarget};
use inkwell::values::{BasicValueEnum, PointerValue};
use inkwell::{AddressSpace, builder::Builder, context::Context, types::StructType};

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
            word.into(),
            super::frame_progress_type(context).into(),
            context.i32_type().into(),
        ],
        false,
    )
}

/// Transfers the published frame lease through the native inactive-frame ABI.
pub(crate) fn inactive_token<'context>(
    context: &'context Context,
    target: &CodegenTarget,
    builder: &Builder<'context>,
    frame: PointerValue<'context>,
    descriptor: PointerValue<'context>,
) -> Result<BasicValueEnum<'context>, CodegenFailure> {
    let word = super::pointer_integer_type(context, target);

    let header_offset = builder
        .build_struct_gep(
            descriptor_type(context, target),
            descriptor,
            1,
            "frame.header.offset.slot",
        )
        .map_err(CodegenFailure::backend_library)?;

    let offset = builder
        .build_load(word, header_offset, "frame.header.offset")
        .map_err(CodegenFailure::backend_library)?
        .into_int_value();

    let address = builder
        .build_ptr_to_int(frame, word, "frame.address")
        .map_err(CodegenFailure::backend_library)?;

    let address = builder
        .build_int_add(address, offset, "frame.header.address")
        .map_err(CodegenFailure::backend_library)?;

    let header = builder
        .build_int_to_ptr(address, frame.get_type(), "frame.header")
        .map_err(CodegenFailure::backend_library)?;

    let lease = builder
        .build_load(
            crate::mapping::cleanup::lease_type(context, target),
            header,
            "frame.lease",
        )
        .map_err(CodegenFailure::backend_library)?;

    let token = builder
        .build_insert_value(
            super::inactive_frame_type(context, target).const_zero(),
            lease,
            0,
            "frame.token.lease",
        )
        .map_err(CodegenFailure::backend_library)?;

    let token = builder
        .build_insert_value(token, descriptor, 1, "frame.token.descriptor")
        .map_err(CodegenFailure::backend_library)?;

    Ok(token.into_struct_value().into())
}

pub(crate) fn lane_bits(requirements: &[bray_runtime_interface::ExecutionLaneRequirement]) -> u32 {
    requirements.iter().fold(0, |bits, lane| {
        bits | match lane {
            bray_runtime_interface::ExecutionLaneRequirement::Blocking => {
                bray_runtime_abi::NativeLaneRequirements::BLOCKING.bits()
            }
            bray_runtime_interface::ExecutionLaneRequirement::Compute => {
                bray_runtime_abi::NativeLaneRequirements::COMPUTE.bits()
            }
            bray_runtime_interface::ExecutionLaneRequirement::MainThread => {
                bray_runtime_abi::NativeLaneRequirements::MAIN_THREAD.bits()
            }
        }
    })
}
