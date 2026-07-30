use bray_codegen::{CodegenSymbolKey, CodegenTarget};
use bray_runtime_interface::{ProtectedFrameOperation, RuntimeAbiRole};
use inkwell::AddressSpace;
use inkwell::context::Context;
use inkwell::types::{BasicMetadataTypeEnum, FunctionType, StructType};

pub(crate) fn symbol_function_type<'context>(
    context: &'context Context,
    target: &CodegenTarget,
    key: &CodegenSymbolKey,
) -> Option<FunctionType<'context>> {
    match key {
        CodegenSymbolKey::Runtime(reference) => {
            runtime_function_type(context, target, reference.role())
        }
        CodegenSymbolKey::ProtectedFrame { operation, .. } => {
            Some(frame_operation_type(context, target, *operation))
        }
        CodegenSymbolKey::Instance(_) => None,
    }
}

pub(crate) fn protected_frame_type<'context>(
    context: &'context Context,
    target: &CodegenTarget,
) -> StructType<'context> {
    let usize = pointer_integer_type(context, target);
    let pointer = context.ptr_type(AddressSpace::default());

    context.struct_type(
        &[
            usize.into(),
            context.i8_type().array_type(32).into(),
            context.i32_type().into(),
            usize.into(),
            usize.into(),
            usize.into(),
            usize.into(),
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

pub(crate) fn frame_progress_type(context: &Context) -> StructType<'_> {
    context.struct_type(
        &[
            context.i32_type().into(),
            context.i32_type().into(),
            context.i64_type().into(),
        ],
        false,
    )
}

pub(crate) fn run_outcome_type(context: &Context) -> StructType<'_> {
    context.struct_type(
        &[context.i32_type().into(), context.i64_type().into()],
        false,
    )
}

pub(crate) fn runtime_configuration_type<'context>(
    context: &'context Context,
    target: &CodegenTarget,
) -> StructType<'context> {
    let usize = pointer_integer_type(context, target);

    context.struct_type(&[usize.into(), usize.into()], false)
}

pub(crate) fn frame_state_type(context: &Context) -> StructType<'_> {
    context.struct_type(
        &[context.i32_type().into(), context.i32_type().into()],
        false,
    )
}

pub(crate) fn pointer_integer_type<'context>(
    context: &'context Context,
    target: &CodegenTarget,
) -> inkwell::types::IntType<'context> {
    match target.machine().pointer_width_bits().get() {
        32 => context.i32_type(),
        64 => context.i64_type(),
        _ => context.i64_type(),
    }
}

fn runtime_function_type<'context>(
    context: &'context Context,
    target: &CodegenTarget,
    role: RuntimeAbiRole,
) -> Option<FunctionType<'context>> {
    match role {
        RuntimeAbiRole::RootExecution => Some(
            run_outcome_type(context)
                .fn_type(
                    &[
                        protected_frame_type(context, target).into(),
                        runtime_configuration_type(context, target).into(),
                    ],
                    false,
                ),
        ),
        RuntimeAbiRole::StructuredShutdown => {
            Some(context.i32_type().fn_type(&[], false))
        }
        _ => None,
    }
}

pub(crate) fn frame_operation_type<'context>(
    context: &'context Context,
    target: &CodegenTarget,
    operation: ProtectedFrameOperation,
) -> FunctionType<'context> {
    let usize = pointer_integer_type(context, target);
    let parameters = |types: &[BasicMetadataTypeEnum<'context>]| types.to_vec();

    match operation {
        ProtectedFrameOperation::MoveBeforeStart => protected_frame_type(context, target)
            .fn_type(&parameters(&[usize.into()]), false),
        ProtectedFrameOperation::Resume => frame_progress_type(context).fn_type(
            &parameters(&[usize.into(), context.i8_type().into()]),
            false,
        ),
        ProtectedFrameOperation::CancellationEntry => frame_state_type(context).fn_type(
            &parameters(&[usize.into(), context.i32_type().into()]),
            false,
        ),
        ProtectedFrameOperation::TaskBroadcast
        | ProtectedFrameOperation::Destruction => context
            .void_type()
            .fn_type(&parameters(&[usize.into()]), false),
        ProtectedFrameOperation::LifecycleResolution => context.void_type().fn_type(
            &parameters(&[usize.into(), context.i32_type().into()]),
            false,
        ),
        ProtectedFrameOperation::CompletionMove => context.void_type().fn_type(
            &parameters(&[usize.into(), usize.into()]),
            false,
        ),
    }
}
