use crate::mapping::LlvmTypeMappings;
use crate::native::frame_operation_function;
use bray_codegen::{CodegenFailure, CodegenInstance, CodegenRequest, CodegenSymbolKey};
use bray_runtime_interface::ProtectedFrameOperation;
use inkwell::AddressSpace;
use inkwell::module::Module;
use inkwell::types::StructType;
use inkwell::values::BasicValueEnum;

use super::core::integer_pointer;

pub(super) fn translate_action_callbacks<'context>(
    module: &Module<'context>,
    request: CodegenRequest<'_>,
    instance: &CodegenInstance,
    context_type: StructType<'context>,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<(), CodegenFailure> {
    translate_failure_cleanup(module, request, instance, context_type, types)?;

    translate_completion_move(module, request, instance, context_type, types)?;
    translate_destruction(module, request, instance)?;

    Ok(())
}

fn translate_failure_cleanup<'context>(
    module: &Module<'context>,
    request: CodegenRequest<'_>,
    instance: &CodegenInstance,
    context_type: StructType<'context>,
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<(), CodegenFailure> {
    let broadcast = frame_operation_function(
        module,
        request,
        instance,
        ProtectedFrameOperation::TaskBroadcast,
    );

    let builder = types.context().create_builder();

    let block = types
        .context()
        .append_basic_block(broadcast, "frame.failure.broadcast");

    builder.position_at_end(block);

    let context = integer_pointer(&builder, broadcast, 0, types)?;

    let requested = builder
        .build_struct_gep(context_type, context, 2, "frame.failure.cancellation")
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_store(requested, types.context().i8_type().const_int(1, false))
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_return(None)
        .map_err(CodegenFailure::backend_library)?;

    let resolve = frame_operation_function(
        module,
        request,
        instance,
        ProtectedFrameOperation::LifecycleResolution,
    );

    let block = types
        .context()
        .append_basic_block(resolve, "frame.failure.resolve");

    let cleanup = types
        .context()
        .append_basic_block(resolve, "frame.failure.cleanup");

    let done = types
        .context()
        .append_basic_block(resolve, "frame.failure.done");

    builder.position_at_end(block);

    let exit = resolve
        .get_nth_param(2)
        .and_then(|value| match value {
            BasicValueEnum::IntValue(value) => Some(value),
            _ => None,
        })
        .expect("protected-frame translation requires an established mapping or value");

    let failed = builder
        .build_int_compare(
            inkwell::IntPredicate::EQ,
            exit,
            exit.get_type().const_int(3, false),
            "frame.runtime.failed",
        )
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_conditional_branch(failed, cleanup, done)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(cleanup);

    let resume =
        frame_operation_function(module, request, instance, ProtectedFrameOperation::Resume);

    let context = resolve
        .get_nth_param(1)
        .expect("protected-frame translation requires an established mapping or value");

    let frame = instance
        .protected_frame_identity()
        .expect("protected-frame translation requires an established mapping or value");

    let outcome = crate::native::invoke_function(
        types.context(),
        &builder,
        request.target(),
        &CodegenSymbolKey::ProtectedFrame {
            frame,
            operation: ProtectedFrameOperation::Resume,
        },
        resume,
        &[context.into()],
        "frame.failure.cleanup",
    )?
    .expect("protected-frame translation requires an established mapping or value");

    let destination = resolve
        .get_first_param()
        .and_then(|value| match value {
            BasicValueEnum::PointerValue(value) => Some(value),
            _ => None,
        })
        .expect("protected-frame translation requires an established mapping or value");

    builder
        .build_store(destination, outcome)
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_unconditional_branch(done)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(done);

    builder
        .build_return(None)
        .map_err(CodegenFailure::backend_library)?;

    Ok(())
}

fn translate_completion_move<'context>(
    module: &Module<'context>,
    request: CodegenRequest<'_>,
    instance: &CodegenInstance,
    context_type: StructType<'context>,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<(), CodegenFailure> {
    let function = frame_operation_function(
        module,
        request,
        instance,
        ProtectedFrameOperation::CompletionMove,
    );

    let descriptor = instance
        .mir()
        .frame_descriptor()
        .expect("protected-frame translation requires an established mapping or value");

    let builder = types.context().create_builder();

    let block = types
        .context()
        .append_basic_block(function, "frame.completion.move");

    builder.position_at_end(block);

    let context = integer_pointer(&builder, function, 0, types)?;
    let destination = integer_pointer(&builder, function, 1, types)?;

    let source = builder
        .build_struct_gep(context_type, context, 1, "frame.completion")
        .map_err(CodegenFailure::backend_library)?;

    let completion = builder
        .build_load(
            types.map(descriptor.result_type())?,
            source,
            "frame.completion.value",
        )
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_store(destination, completion)
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_return(None)
        .map_err(CodegenFailure::backend_library)?;

    Ok(())
}

fn translate_destruction(
    module: &Module<'_>,
    request: CodegenRequest<'_>,
    instance: &CodegenInstance,
) -> Result<(), CodegenFailure> {
    let function = frame_operation_function(
        module,
        request,
        instance,
        ProtectedFrameOperation::Destruction,
    );

    let context = module.get_context();
    let builder = context.create_builder();
    let block = context.append_basic_block(function, "frame.destroy");

    builder.position_at_end(block);

    let pointer = context.ptr_type(AddressSpace::default());

    let free = module.get_function("free").unwrap_or_else(|| {
        module.add_function(
            "free",
            context.void_type().fn_type(&[pointer.into()], false),
            None,
        )
    });

    let storage = function
        .get_first_param()
        .and_then(|value| match value {
            BasicValueEnum::IntValue(value) => Some(value),
            _ => None,
        })
        .expect("protected-frame translation requires an established mapping or value");

    let storage = builder
        .build_int_to_ptr(storage, pointer, "frame.storage")
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_call(free, &[storage.into()], "")
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_return(None)
        .map_err(CodegenFailure::backend_library)?;

    Ok(())
}
