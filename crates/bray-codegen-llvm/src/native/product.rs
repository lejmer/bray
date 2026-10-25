use bray_codegen::{CodegenFailure, CodegenTarget};
use bray_runtime_abi::{NativeProductHostStatus, NativeRuntimeStatus};
use bray_runtime_interface::RuntimeAbiRole;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::StructType;
use inkwell::values::{IntValue, PointerValue, StructValue};

pub(crate) fn product_observation_type<'context>(
    context: &'context Context,
    target: &CodegenTarget,
) -> StructType<'context> {
    let usize = super::pointer_integer_type(context, target);

    context.struct_type(
        &[
            context.i32_type().into(),
            context.i32_type().into(),
            usize.into(),
            usize.into(),
            usize.into(),
            usize.into(),
            usize.into(),
            usize.into(),
            context.i8_type().array_type(32).into(),
        ],
        false,
    )
}

pub(crate) fn invoke_product_control<'context>(
    context: &'context Context,
    module: &Module<'context>,
    builder: &Builder<'context>,
    target: &CodegenTarget,
    descriptor: PointerValue<'context>,
    operation: IntValue<'context>,
) -> Result<StructValue<'context>, CodegenFailure> {
    let usize = super::pointer_integer_type(context, target);
    let ty = product_observation_type(context, target);

    let failure = ty.const_named_struct(&[
        context.i32_type().const_int(5, false).into(),
        context.i32_type().const_int(4, false).into(),
        usize.const_zero().into(),
        usize.const_zero().into(),
        usize.const_zero().into(),
        usize.const_zero().into(),
        usize.const_zero().into(),
        usize.const_zero().into(),
        context.i8_type().array_type(32).const_zero().into(),
    ]);

    let destination = builder
        .build_alloca(ty, "product.observation")
        .map_err(CodegenFailure::backend_library)?;

    let key = builder
        .build_ptr_to_int(descriptor, usize, "product.key")
        .map_err(CodegenFailure::backend_library)?;

    let runtime = super::declare_runtime_function(
        module,
        context,
        target,
        RuntimeAbiRole::ProductHostControl,
    )?;

    let status = builder
        .build_call(
            runtime,
            &[key.into(), operation.into(), destination.into()],
            "product.status",
        )
        .map_err(CodegenFailure::backend_library)?
        .try_as_basic_value()
        .basic()
        .expect("product control returns its runtime boundary status")
        .into_int_value();

    let function = builder
        .get_insert_block()
        .and_then(|block| block.get_parent())
        .expect("product control is emitted inside its caller");

    let failed = context.append_basic_block(function, "product.failed");
    let completed = context.append_basic_block(function, "product.observed");

    let success = builder
        .build_int_compare(
            inkwell::IntPredicate::EQ,
            status,
            context.i32_type().const_zero(),
            "product.succeeded",
        )
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_conditional_branch(success, completed, failed)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(failed);

    let mut failure_status = context.i32_type().const_int(
        u64::from(NativeProductHostStatus::RUNTIME_FAILURE.code()),
        false,
    );

    for (native, host) in [
        (
            NativeRuntimeStatus::IDENTITY_EXHAUSTED,
            NativeProductHostStatus::IDENTITY_EXHAUSTED,
        ),
        (
            NativeRuntimeStatus::OUTGOING_STORAGE_UNAVAILABLE,
            NativeProductHostStatus::OUTGOING_STORAGE_UNAVAILABLE,
        ),
    ] {
        let matches = builder
            .build_int_compare(
                inkwell::IntPredicate::EQ,
                status,
                context
                    .i32_type()
                    .const_int(u64::from(native.code()), false),
                "product.failure.kind",
            )
            .map_err(CodegenFailure::backend_library)?;

        failure_status = builder
            .build_select(
                matches,
                context.i32_type().const_int(u64::from(host.code()), false),
                failure_status,
                "product.failure.status",
            )
            .map_err(CodegenFailure::backend_library)?
            .into_int_value();
    }

    let failure = builder
        .build_insert_value(failure, failure_status, 0, "product.failure")
        .map_err(CodegenFailure::backend_library)?
        .into_struct_value();

    builder
        .build_store(destination, failure)
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_unconditional_branch(completed)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(completed);

    Ok(builder
        .build_load(ty, destination, "product.observation.value")
        .map_err(CodegenFailure::backend_library)?
        .into_struct_value())
}
