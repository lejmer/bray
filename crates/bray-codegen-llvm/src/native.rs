use bray_codegen::{
    CodegenFailure, CodegenInstance, CodegenRequest, CodegenSymbolKey, CodegenTarget,
};
use bray_runtime_interface::{ProtectedFrameOperation, RuntimeAbiRole};
use bray_target::{ObjectFormat, TargetArchitecture};
use inkwell::AddressSpace;
use inkwell::attributes::{Attribute, AttributeLoc};
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::{AnyType, BasicMetadataTypeEnum, BasicTypeEnum, FunctionType, StructType};
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue, StructValue};

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

pub(crate) fn indirect_result_type<'context>(
    context: &'context Context,
    target: &CodegenTarget,
    key: &CodegenSymbolKey,
) -> Option<BasicTypeEnum<'context>> {
    if matches!(
        key,
        CodegenSymbolKey::ProtectedFrame {
            operation: ProtectedFrameOperation::MoveBeforeStart,
            ..
        }
    ) {
        return Some(protected_frame_type(context, target).into());
    }

    if !uses_microsoft_x64_abi(target) {
        return None;
    }

    match key {
        CodegenSymbolKey::Runtime(reference) => match reference.role() {
            RuntimeAbiRole::RootExecution => Some(root_start_type(context).into()),
            RuntimeAbiRole::SynchronousRootExecution
            | RuntimeAbiRole::ForeignCallbackExecution
            | RuntimeAbiRole::RootTerminalObservation
            | RuntimeAbiRole::JoinRegistration => Some(run_outcome_type(context, target).into()),
            RuntimeAbiRole::TaskAllocation => Some(task_allocation_type(context).into()),
            RuntimeAbiRole::TaskObservationCreation => Some(inactive_frame_type(context).into()),
            _ => None,
        },
        CodegenSymbolKey::ProtectedFrame { operation, .. } => {
            frame_result_is_indirect(target, *operation).then(|| match operation {
                ProtectedFrameOperation::Resume | ProtectedFrameOperation::CancellationEntry => {
                    frame_progress_type(context).into()
                }
                ProtectedFrameOperation::MoveBeforeStart => {
                    unreachable!("move-before-start results are handled for every native ABI")
                }
                _ => unreachable!("only aggregate frame results use indirect storage"),
            })
        }
        CodegenSymbolKey::Instance(_) => None,
    }
}

pub(crate) fn uses_microsoft_x64_abi(target: &CodegenTarget) -> bool {
    target.machine().architecture() == TargetArchitecture::X86_64
        && target.machine().object_format() == ObjectFormat::Coff
}

pub(crate) fn uses_indirect_argument(
    target: &CodegenTarget,
    role: RuntimeAbiRole,
    index: usize,
) -> bool {
    uses_microsoft_x64_abi(target)
        && matches!(
            (role, index),
            (RuntimeAbiRole::RootExecution, 1) | (RuntimeAbiRole::AwaitedFrameComposition, 0)
        )
}

pub(crate) fn invoke_function<'context>(
    context: &'context Context,
    builder: &Builder<'context>,
    target: &CodegenTarget,
    key: &CodegenSymbolKey,
    function: FunctionValue<'context>,
    arguments: &[BasicMetadataValueEnum<'context>],
    name: &str,
) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
    let Some(result) = indirect_result_type(context, target, key) else {
        return builder
            .build_call(function, arguments, name)
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)
            .map(|call| call.try_as_basic_value().basic());
    };

    let storage = crate::translation::allocate_temporary(
        context,
        builder,
        result,
        &format!("{name}.result"),
    )?;

    let arguments = std::iter::once(storage.into())
        .chain(arguments.iter().copied())
        .collect::<Vec<_>>();

    let call = builder
        .build_call(function, &arguments, name)
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    let kind = Attribute::get_named_enum_kind_id("sret");

    if kind == 0 {
        return Err(CodegenFailure::UnsupportedTarget);
    }

    call.add_attribute(
        AttributeLoc::Param(0),
        context.create_type_attribute(kind, result.as_any_type_enum()),
    );

    builder
        .build_load(result, storage, name)
        .map(Some)
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)
}

pub(crate) fn frame_parameter_index(
    target: &CodegenTarget,
    operation: ProtectedFrameOperation,
    index: u32,
) -> u32 {
    index + u32::from(frame_result_is_indirect(target, operation))
}

pub(crate) fn return_frame_result(
    builder: &Builder<'_>,
    function: FunctionValue<'_>,
    target: &CodegenTarget,
    operation: ProtectedFrameOperation,
    result: BasicValueEnum<'_>,
) -> Result<(), CodegenFailure> {
    if frame_result_is_indirect(target, operation) {
        let destination = function
            .get_first_param()
            .and_then(|value| match value {
                BasicValueEnum::PointerValue(value) => Some(value),
                _ => None,
            })
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        builder
            .build_store(destination, result)
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

        builder
            .build_return(None)
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;
    } else {
        builder
            .build_return(Some(&result))
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;
    }

    Ok(())
}

fn frame_result_is_indirect(target: &CodegenTarget, operation: ProtectedFrameOperation) -> bool {
    operation == ProtectedFrameOperation::MoveBeforeStart
        || uses_microsoft_x64_abi(target)
            && matches!(
                operation,
                ProtectedFrameOperation::Resume | ProtectedFrameOperation::CancellationEntry
            )
}

pub(crate) fn return_frame_state(
    context: &Context,
    builder: &Builder<'_>,
    target: &CodegenTarget,
    affinity: u64,
    lane_requirements: u64,
) -> Result<(), CodegenFailure> {
    if uses_microsoft_x64_abi(target) {
        let state = context
            .i64_type()
            .const_int(affinity | (lane_requirements << 32), false);

        builder
            .build_return(Some(&state))
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;
    } else {
        let state = frame_state_type(context).const_named_struct(&[
            context.i32_type().const_int(affinity, false).into(),
            context
                .i32_type()
                .const_int(lane_requirements, false)
                .into(),
        ]);

        builder
            .build_return(Some(&state))
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;
    }

    Ok(())
}

pub(crate) fn frame_operation_function<'context>(
    module: &Module<'context>,
    request: CodegenRequest<'_>,
    instance: &CodegenInstance,
    operation: ProtectedFrameOperation,
) -> Result<FunctionValue<'context>, CodegenFailure> {
    let frame = instance
        .protected_frame_identity()
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let symbol = request
        .mappings()
        .symbol(&CodegenSymbolKey::ProtectedFrame { frame, operation })
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    module
        .get_function(symbol.name().as_str())
        .ok_or(CodegenFailure::GeneratedModuleInvariant)
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

pub(crate) fn run_outcome_type<'context>(
    context: &'context Context,
    target: &CodegenTarget,
) -> StructType<'context> {
    context.struct_type(
        &[
            context.i32_type().into(),
            pointer_integer_type(context, target).into(),
        ],
        false,
    )
}

pub(crate) fn task_allocation_type(context: &Context) -> StructType<'_> {
    context.struct_type(
        &[context.i32_type().into(), context.i64_type().into()],
        false,
    )
}

pub(crate) fn run_result_layout_type<'context>(
    context: &'context Context,
    target: &CodegenTarget,
) -> StructType<'context> {
    let usize = pointer_integer_type(context, target);

    context.struct_type(
        &[
            usize.into(),
            usize.into(),
            usize.into(),
            context.i64_type().into(),
            usize.into(),
            usize.into(),
            usize.into(),
            context.i64_type().into(),
            usize.into(),
            context.i64_type().into(),
        ],
        false,
    )
}

pub(crate) fn root_start_type(context: &Context) -> StructType<'_> {
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

pub(crate) fn source_anchor_type(context: &Context) -> StructType<'_> {
    context.struct_type(
        &[
            context.i32_type().into(),
            context.i32_type().into(),
            context.i32_type().into(),
            context.i32_type().into(),
            context.i64_type().into(),
        ],
        false,
    )
}

pub(crate) fn source_anchor_value(
    context: &Context,
    source: bray_runtime_abi::NativeSourceAnchor,
) -> StructValue<'_> {
    source_anchor_type(context).const_named_struct(&[
        context
            .i32_type()
            .const_int(u64::from(source.is_available()), false)
            .into(),
        context
            .i32_type()
            .const_int(u64::from(source.source()), false)
            .into(),
        context
            .i32_type()
            .const_int(u64::from(source.start()), false)
            .into(),
        context
            .i32_type()
            .const_int(u64::from(source.end()), false)
            .into(),
        context.i64_type().const_int(source.version(), false).into(),
    ])
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
    if uses_microsoft_x64_abi(target) {
        let pointer = context.ptr_type(AddressSpace::default());
        let usize = pointer_integer_type(context, target);

        match role {
            RuntimeAbiRole::RootExecution => {
                return Some(context.void_type().fn_type(
                    &[
                        pointer.into(),
                        pointer_integer_type(context, target).into(),
                        pointer.into(),
                    ],
                    false,
                ));
            }
            RuntimeAbiRole::SynchronousRootExecution | RuntimeAbiRole::ForeignCallbackExecution => {
                return Some(context.void_type().fn_type(
                    &[
                        pointer.into(),
                        pointer.into(),
                        pointer_integer_type(context, target).into(),
                    ],
                    false,
                ));
            }
            RuntimeAbiRole::RootTerminalObservation => {
                return Some(
                    context
                        .void_type()
                        .fn_type(&[pointer.into(), context.i64_type().into()], false),
                );
            }
            RuntimeAbiRole::TaskAllocation => {
                return Some(context.void_type().fn_type(&[pointer.into()], false));
            }
            RuntimeAbiRole::TaskStart => {
                return Some(
                    context
                        .i32_type()
                        .fn_type(&[context.i64_type().into(), pointer.into()], false),
                );
            }
            RuntimeAbiRole::TaskObservationCreation => {
                return Some(context.void_type().fn_type(
                    &[
                        pointer.into(),
                        context.i64_type().into(),
                        context.i8_type().into(),
                        pointer.into(),
                        pointer.into(),
                        pointer.into(),
                    ],
                    false,
                ));
            }
            RuntimeAbiRole::TaskResolution => {
                return Some(context.i32_type().fn_type(
                    &[context.i64_type().into(), pointer.into(), pointer.into()],
                    false,
                ));
            }
            RuntimeAbiRole::JoinRegistration => {
                return Some(context.void_type().fn_type(
                    &[
                        pointer.into(),
                        context.i64_type().into(),
                        pointer.into(),
                        usize.into(),
                    ],
                    false,
                ));
            }
            RuntimeAbiRole::PanicReportConstruction => {
                return Some(pointer_integer_type(context, target).fn_type(
                    &[
                        context.i32_type().into(),
                        context.i32_type().into(),
                        context.i32_type().into(),
                        context.i32_type().into(),
                        context.i32_type().into(),
                        context.i64_type().into(),
                        pointer.into(),
                        usize.into(),
                    ],
                    false,
                ));
            }
            RuntimeAbiRole::AwaitedFrameComposition => {
                return Some(context.void_type().fn_type(&[pointer.into()], false));
            }
            _ => {}
        }
    }

    match role {
        RuntimeAbiRole::RuntimeInitialization => {
            let capacity = pointer_integer_type(context, target);

            Some(
                context
                    .i32_type()
                    .fn_type(&[capacity.into(), capacity.into()], false),
            )
        }
        RuntimeAbiRole::RootExecution => Some(root_start_type(context).fn_type(
            &[
                pointer_integer_type(context, target).into(),
                runtime_configuration_type(context, target).into(),
            ],
            false,
        )),
        RuntimeAbiRole::TaskAllocation => Some(task_allocation_type(context).fn_type(&[], false)),
        RuntimeAbiRole::TaskStart => Some(context.i32_type().fn_type(
            &[
                context.i64_type().into(),
                inactive_frame_type(context).into(),
            ],
            false,
        )),
        RuntimeAbiRole::TaskObservationCreation => Some(inactive_frame_type(context).fn_type(
            &[
                context.i64_type().into(),
                context.i8_type().into(),
                context.ptr_type(AddressSpace::default()).into(),
                context.ptr_type(AddressSpace::default()).into(),
                context.ptr_type(AddressSpace::default()).into(),
            ],
            false,
        )),
        RuntimeAbiRole::TaskResolution => Some(context.i32_type().fn_type(
            &[
                context.i64_type().into(),
                context.ptr_type(AddressSpace::default()).into(),
                context.ptr_type(AddressSpace::default()).into(),
            ],
            false,
        )),
        RuntimeAbiRole::JoinRegistration => Some(run_outcome_type(context, target).fn_type(
            &[
                context.i64_type().into(),
                context.ptr_type(AddressSpace::default()).into(),
                pointer_integer_type(context, target).into(),
            ],
            false,
        )),
        RuntimeAbiRole::TaskCancellationRequest | RuntimeAbiRole::TaskDestruction => Some(
            context
                .i32_type()
                .fn_type(&[context.i64_type().into()], false),
        ),
        RuntimeAbiRole::NativeThreadExecution => {
            let address = pointer_integer_type(context, target);
            let pointer = context.ptr_type(AddressSpace::default());

            Some(context.i32_type().fn_type(
                &[
                    pointer.into(),
                    address.into(),
                    pointer.into(),
                    address.into(),
                    pointer.into(),
                ],
                false,
            ))
        }
        RuntimeAbiRole::TaskEventCreation => {
            Some(pointer_integer_type(context, target).fn_type(&[], false))
        }
        RuntimeAbiRole::TaskEventSignal | RuntimeAbiRole::TaskEventDestruction => Some(
            context
                .i32_type()
                .fn_type(&[pointer_integer_type(context, target).into()], false),
        ),
        RuntimeAbiRole::CurrentNativeThreadIdentity | RuntimeAbiRole::MainNativeThreadIdentity => {
            Some(context.i64_type().fn_type(&[], false))
        }
        RuntimeAbiRole::NativeThreadPanicReportRecovery => {
            let address = pointer_integer_type(context, target);

            Some(address.fn_type(&[address.into()], false))
        }
        RuntimeAbiRole::SynchronousRootExecution | RuntimeAbiRole::ForeignCallbackExecution => {
            Some(run_outcome_type(context, target).fn_type(
                &[
                    context.ptr_type(AddressSpace::default()).into(),
                    pointer_integer_type(context, target).into(),
                ],
                false,
            ))
        }
        RuntimeAbiRole::RootCancellationRequest => Some(
            context
                .i32_type()
                .fn_type(&[context.i64_type().into()], false),
        ),
        RuntimeAbiRole::RootTerminalObservation => {
            Some(run_outcome_type(context, target).fn_type(&[context.i64_type().into()], false))
        }
        RuntimeAbiRole::RootCompletionResolution => Some(
            context
                .i32_type()
                .fn_type(&[context.i64_type().into()], false),
        ),
        RuntimeAbiRole::PanicReporting | RuntimeAbiRole::PanicReportDestruction => Some(
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
        RuntimeAbiRole::TestEntrySelection => Some(
            context
                .i8_type()
                .fn_type(&[context.i32_type().into()], false),
        ),
        RuntimeAbiRole::PanicReportConstruction => {
            Some(panic_report_construction_type(context, target))
        }
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

fn panic_report_construction_type<'context>(
    context: &'context Context,
    target: &CodegenTarget,
) -> FunctionType<'context> {
    pointer_integer_type(context, target).fn_type(
        &[
            context.i32_type().into(),
            context.i32_type().into(),
            context.i32_type().into(),
            context.i32_type().into(),
            context.i32_type().into(),
            context.i64_type().into(),
            context.ptr_type(AddressSpace::default()).into(),
            pointer_integer_type(context, target).into(),
        ],
        false,
    )
}

pub(crate) fn frame_operation_type<'context>(
    context: &'context Context,
    target: &CodegenTarget,
    operation: ProtectedFrameOperation,
) -> FunctionType<'context> {
    let usize = pointer_integer_type(context, target);
    let pointer = context.ptr_type(AddressSpace::default());
    let parameters = |types: &[BasicMetadataTypeEnum<'context>]| types.to_vec();

    if operation == ProtectedFrameOperation::MoveBeforeStart {
        return context
            .void_type()
            .fn_type(&parameters(&[pointer.into(), usize.into()]), false);
    }

    if uses_microsoft_x64_abi(target) {
        return match operation {
            ProtectedFrameOperation::MoveBeforeStart => {
                unreachable!("move-before-start uses indirect results on every native ABI")
            }
            ProtectedFrameOperation::StateDescription => context.i64_type().fn_type(
                &parameters(&[usize.into(), context.i32_type().into()]),
                false,
            ),
            ProtectedFrameOperation::Resume | ProtectedFrameOperation::CancellationEntry => context
                .void_type()
                .fn_type(&parameters(&[pointer.into(), usize.into()]), false),
            ProtectedFrameOperation::TaskBroadcast | ProtectedFrameOperation::Destruction => {
                context
                    .void_type()
                    .fn_type(&parameters(&[usize.into()]), false)
            }
            ProtectedFrameOperation::LifecycleResolution => context.void_type().fn_type(
                &parameters(&[usize.into(), context.i32_type().into()]),
                false,
            ),
            ProtectedFrameOperation::CompletionMove => context
                .void_type()
                .fn_type(&parameters(&[usize.into(), usize.into()]), false),
        };
    }

    match operation {
        ProtectedFrameOperation::MoveBeforeStart => {
            unreachable!("move-before-start uses indirect results on every native ABI")
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
    use bray_codegen::CodegenTarget;
    use bray_runtime_interface::{ProtectedFrameOperation, RuntimeAbiRole};
    use bray_target::NativeTarget;
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

    #[test]
    fn frame_moves_use_indirect_results_on_every_native_target() {
        let context = Context::create();

        for native_target in NativeTarget::ALL {
            let target = CodegenTarget::for_native(native_target);

            let operation = super::frame_operation_type(
                &context,
                &target,
                ProtectedFrameOperation::MoveBeforeStart,
            );

            assert_eq!(operation.get_return_type(), None, "{native_target:?}");
            assert_eq!(operation.count_param_types(), 2, "{native_target:?}");

            assert!(
                super::frame_result_is_indirect(&target, ProtectedFrameOperation::MoveBeforeStart),
                "{native_target:?}"
            );
        }
    }

    #[test]
    fn microsoft_x64_native_abi_uses_indirect_aggregate_results_and_arguments() {
        let context = Context::create();
        let target = CodegenTarget::for_native(NativeTarget::X86_64WindowsMsvc);

        let root = super::runtime_function_type(&context, &target, RuntimeAbiRole::RootExecution)
            .unwrap_or_else(|| panic!("root execution must have a native ABI"));

        assert_eq!(root.get_return_type(), None);
        assert_eq!(root.count_param_types(), 3);

        let resume =
            super::frame_operation_type(&context, &target, ProtectedFrameOperation::Resume);

        assert_eq!(resume.get_return_type(), None);
        assert_eq!(resume.count_param_types(), 2);

        let state = super::frame_operation_type(
            &context,
            &target,
            ProtectedFrameOperation::StateDescription,
        );

        assert_eq!(state.get_return_type(), Some(context.i64_type().into()));
        assert_eq!(state.count_param_types(), 2);

        let panic = super::runtime_function_type(
            &context,
            &target,
            RuntimeAbiRole::PanicReportConstruction,
        )
        .unwrap_or_else(|| panic!("panic report construction must have a native ABI"));

        assert_eq!(panic.count_param_types(), 8);

        assert_eq!(
            panic.get_param_types(),
            [
                context.i32_type().into(),
                context.i32_type().into(),
                context.i32_type().into(),
                context.i32_type().into(),
                context.i32_type().into(),
                context.i64_type().into(),
                context.ptr_type(inkwell::AddressSpace::default()).into(),
                context.i64_type().into(),
            ]
        );
    }

    #[test]
    fn system_v_x64_native_abi_keeps_register_aggregate_results() {
        let context = Context::create();
        let target = CodegenTarget::for_native(NativeTarget::X86_64LinuxGnu);

        let root = super::runtime_function_type(&context, &target, RuntimeAbiRole::RootExecution)
            .unwrap_or_else(|| panic!("root execution must have a native ABI"));

        assert_eq!(
            root.get_return_type(),
            Some(super::root_start_type(&context).into())
        );

        assert_eq!(root.count_param_types(), 2);

        let resume =
            super::frame_operation_type(&context, &target, ProtectedFrameOperation::Resume);

        assert_eq!(
            resume.get_return_type(),
            Some(super::frame_progress_type(&context).into())
        );

        assert_eq!(resume.count_param_types(), 1);

        let panic = super::runtime_function_type(
            &context,
            &target,
            RuntimeAbiRole::PanicReportConstruction,
        )
        .unwrap_or_else(|| panic!("panic report construction must have a native ABI"));

        assert_eq!(panic.count_param_types(), 8);

        assert_eq!(
            panic.get_param_types(),
            [
                context.i32_type().into(),
                context.i32_type().into(),
                context.i32_type().into(),
                context.i32_type().into(),
                context.i32_type().into(),
                context.i64_type().into(),
                context.ptr_type(inkwell::AddressSpace::default()).into(),
                context.i64_type().into(),
            ]
        );
    }
}
