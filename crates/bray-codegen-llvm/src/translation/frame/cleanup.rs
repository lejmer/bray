use crate::mapping::LlvmTypeMappings;
use crate::native::frame_operation_function;
use bray_codegen::{CodegenFailure, CodegenInstance, CodegenRequest, CodegenSymbolKey};
use bray_runtime_interface::ProtectedFrameOperation;
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
    super::storage::translate_destruction(module, request, instance, context_type, types)?;

    Ok(())
}

fn translate_failure_cleanup<'context>(
    module: &Module<'context>,
    request: CodegenRequest<'_>,
    instance: &CodegenInstance,
    context_type: StructType<'context>,
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<(), CodegenFailure> {
    let builder = types.context().create_builder();

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

    let context = integer_pointer(&builder, resolve, 1, types)?;

    let state = builder
        .build_struct_gep(context_type, context, 0, "frame.cleanup.state")
        .map_err(CodegenFailure::backend_library)?;

    let state = builder
        .build_load(types.context().i32_type(), state, "frame.cleanup.current")
        .map_err(CodegenFailure::backend_library)?
        .into_int_value();

    let terminal = super::support::terminal_state(
        types.context(),
        instance
            .mir()
            .frame_descriptor()
            .expect("protected frame has its description"),
    )?;

    let pending = builder
        .build_int_compare(
            inkwell::IntPredicate::NE,
            state,
            terminal,
            "frame.cleanup.pending",
        )
        .map_err(CodegenFailure::backend_library)?;

    let failed = builder
        .build_and(failed, pending, "frame.cleanup.required")
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_conditional_branch(failed, cleanup, done)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(cleanup);

    // Abandonment resolves initialized state without entering an inactive body.
    // The selected cancellation entry also preserves a suspended cleanup result.
    let cancel = frame_operation_function(
        module,
        request,
        instance,
        ProtectedFrameOperation::CancellationEntry,
    );

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
            operation: ProtectedFrameOperation::CancellationEntry,
        },
        cancel,
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

    let state = builder
        .build_struct_gep(context_type, context, 0, "frame.consumed.state")
        .map_err(CodegenFailure::backend_library)?;

    let terminal = super::support::terminal_state(types.context(), descriptor)?;

    builder
        .build_store(state, terminal)
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_return(None)
        .map_err(CodegenFailure::backend_library)?;

    Ok(())
}
