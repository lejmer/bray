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
            pointer.into(),
        ],
        false,
    )
}

pub(crate) fn inactive_frame_type(context: &Context) -> StructType<'_> {
    let pointer = context.ptr_type(AddressSpace::default());

    context.struct_type(&[pointer.into(), pointer.into()], false)
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

pub(crate) fn string_view_type<'context>(
    context: &'context Context,
    target: &CodegenTarget,
) -> StructType<'context> {
    context.struct_type(
        &[
            context.ptr_type(AddressSpace::default()).into(),
            pointer_integer_type(context, target).into(),
        ],
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
            context
                .struct_type(
                    &[context.i32_type().into(), context.i64_type().into()],
                    false,
                )
                .fn_type(
                    &[
                        pointer_integer_type(context, target).into(),
                        runtime_configuration_type(context, target).into(),
                    ],
                    false,
                ),
        ),
        RuntimeAbiRole::TaskStart => Some(context.i32_type().fn_type(
            &[
                context.i64_type().into(),
                pointer_integer_type(context, target).into(),
            ],
            false,
        )),
        RuntimeAbiRole::SynchronousRootExecution => Some(run_outcome_type(context).fn_type(
            &[
                context.ptr_type(AddressSpace::default()).into(),
                pointer_integer_type(context, target).into(),
            ],
            false,
        )),
        RuntimeAbiRole::RootCancellationRequest => Some(
            context
                .i32_type()
                .fn_type(&[context.i64_type().into()], false),
        ),
        RuntimeAbiRole::RootTerminalObservation => {
            Some(run_outcome_type(context).fn_type(&[context.i64_type().into()], false))
        }
        RuntimeAbiRole::RootCompletionResolution => Some(
            context
                .i32_type()
                .fn_type(&[context.i64_type().into()], false),
        ),
        RuntimeAbiRole::PanicReporting => Some(
            context
                .i32_type()
                .fn_type(&[pointer_integer_type(context, target).into()], false),
        ),
        RuntimeAbiRole::EntryFailureReporting => Some(context.i32_type().fn_type(
            &[
                pointer_integer_type(context, target).into(),
                pointer_integer_type(context, target).into(),
            ],
            false,
        )),
        RuntimeAbiRole::PanicReportConstruction => Some(
            pointer_integer_type(context, target)
                .fn_type(&[string_view_type(context, target).into()], false),
        ),
        RuntimeAbiRole::PanicPropagation => Some(
            context
                .void_type()
                .fn_type(&[pointer_integer_type(context, target).into()], false),
        ),
        RuntimeAbiRole::AwaitedFrameComposition => Some(
            context
                .void_type()
                .fn_type(&[inactive_frame_type(context).into()], false),
        ),
        RuntimeAbiRole::FrameCompletionMove => {
            Some(pointer_integer_type(context, target).fn_type(&[], false))
        }
        RuntimeAbiRole::CleanupIncidentReporting | RuntimeAbiRole::StructuredShutdown => {
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
        ProtectedFrameOperation::MoveBeforeStart => {
            protected_frame_type(context, target).fn_type(&parameters(&[usize.into()]), false)
        }
        ProtectedFrameOperation::StateDescription => frame_state_type(context).fn_type(
            &parameters(&[usize.into(), context.i32_type().into()]),
            false,
        ),
        ProtectedFrameOperation::Resume | ProtectedFrameOperation::CancellationEntry => {
            frame_progress_type(context).fn_type(&parameters(&[usize.into()]), false)
        }
        ProtectedFrameOperation::TaskBroadcast | ProtectedFrameOperation::Destruction => context
            .void_type()
            .fn_type(&parameters(&[usize.into()]), false),
        ProtectedFrameOperation::LifecycleResolution => context.void_type().fn_type(
            &parameters(&[usize.into(), context.i32_type().into()]),
            false,
        ),
        ProtectedFrameOperation::CompletionMove => context
            .void_type()
            .fn_type(&parameters(&[usize.into(), usize.into()]), false),
    }
}

#[cfg(test)]
mod tests {
    use bray_runtime_interface::{ProtectedFrameOperation, RuntimeAbiRole};
    use inkwell::context::Context;

    #[test]
    fn cancellation_entry_has_its_exact_progress_callback_abi() {
        let context = Context::create();
        let target = bray_codegen::test_support::codegen_target();

        let cancellation = super::frame_operation_type(
            &context,
            &target,
            ProtectedFrameOperation::CancellationEntry,
        );

        let state = super::frame_operation_type(
            &context,
            &target,
            ProtectedFrameOperation::StateDescription,
        );

        assert_eq!(cancellation.count_param_types(), 1);

        assert_eq!(
            cancellation.get_return_type(),
            Some(super::frame_progress_type(&context).into())
        );

        assert_eq!(state.count_param_types(), 2);

        assert_eq!(
            state.get_return_type(),
            Some(super::frame_state_type(&context).into())
        );
    }

    #[test]
    fn root_execution_transfers_a_pointer_sized_frame_address() {
        let context = Context::create();
        let target = bray_codegen::test_support::codegen_target();

        let root = super::runtime_function_type(&context, &target, RuntimeAbiRole::RootExecution)
            .unwrap_or_else(|| panic!("root execution must have a native ABI"));

        let parameters = root.get_param_types();

        assert_eq!(
            parameters.first().copied(),
            Some(super::pointer_integer_type(&context, &target).into())
        );

        assert_ne!(
            parameters.first().copied(),
            Some(super::protected_frame_type(&context, &target).into())
        );
    }
}
