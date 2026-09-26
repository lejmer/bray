use super::abi::runtime_function_type;
use bray_codegen::{
    CodegenFailure, CodegenInstance, CodegenRequest, CodegenSymbolKey, CodegenTarget,
};
use bray_runtime_interface::{NATIVE_PANIC_REPORT_FIELDS, ProtectedFrameOperation, RuntimeAbiRole, RuntimeAbiType};
use bray_target::{ObjectFormat, TargetArchitecture};
use inkwell::AddressSpace;
use inkwell::attributes::{Attribute, AttributeLoc};
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::{AnyType, BasicTypeEnum, FunctionType, StructType};
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

    match key {
        CodegenSymbolKey::Runtime(reference) => {
            runtime_indirect_result_type(context, target, reference.role())
        }
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

pub(super) fn uses_microsoft_x64_abi(target: &CodegenTarget) -> bool {
    target.machine().architecture() == TargetArchitecture::X86_64
        && target.machine().object_format() == ObjectFormat::Coff
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
    if let CodegenSymbolKey::Runtime(reference) = key {
        return super::abi::invoke_runtime(
            context,
            builder,
            target,
            reference.role(),
            function,
            arguments,
            name,
        );
    }

    let Some(result) = indirect_result_type(context, target, key) else {
        let value = builder
            .build_call(function, arguments, name)
            .map_err(CodegenFailure::backend_library)?
            .try_as_basic_value()
            .basic();

        let (Some(value), CodegenSymbolKey::ProtectedFrame { operation, .. }) = (value, key) else {
            return Ok(value);
        };

        let logical = match operation {
            ProtectedFrameOperation::StateDescription => frame_state_type(context).into(),
            _ => return Ok(Some(value)),
        };

        return crate::translation::reinterpret_value(
            context,
            builder,
            value,
            logical,
            value.get_type(),
            name,
        )
        .map(Some);
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
        .map_err(CodegenFailure::backend_library)?;

    if matches!(
        key,
        CodegenSymbolKey::ProtectedFrame {
            operation: ProtectedFrameOperation::MoveBeforeStart,
            ..
        }
    ) {
        call.add_attribute(
            AttributeLoc::Param(0),
            indirect_result_attribute(context, result)?,
        );
    }

    builder
        .build_load(result, storage, name)
        .map(Some)
        .map_err(CodegenFailure::backend_library)
}

pub(crate) fn indirect_result_attribute(
    context: &Context,
    result: BasicTypeEnum<'_>,
) -> Result<Attribute, CodegenFailure> {
    let kind = Attribute::get_named_enum_kind_id("sret");

    if kind == 0 {
        return Err(CodegenFailure::UnsupportedTarget);
    }

    Ok(context.create_type_attribute(kind, result.as_any_type_enum()))
}

pub(crate) fn runtime_indirect_result_type<'context>(
    context: &'context Context,
    target: &CodegenTarget,
    role: RuntimeAbiRole,
) -> Option<BasicTypeEnum<'context>> {
    let result = role.native_signature()?.result();

    // Product observations exceed the register-return limit on every supported native ABI.
    if matches!(
        result,
        RuntimeAbiType::ProductObservation
            | RuntimeAbiType::PanicReport
            | RuntimeAbiType::RunOutcome
            | RuntimeAbiType::FrameProgress
    ) || uses_microsoft_x64_abi(target) && aggregate_is_indirect(result)
    {
        runtime_value_type(context, target, result)
    } else {
        None
    }
}

pub(crate) fn frame_parameter_index(
    target: &CodegenTarget,
    operation: ProtectedFrameOperation,
    index: u32,
) -> u32 {
    index + u32::from(frame_result_is_indirect(target, operation))
}

pub(crate) fn return_frame_result<'context>(
    context: &'context Context,
    builder: &Builder<'context>,
    function: FunctionValue<'context>,
    target: &CodegenTarget,
    operation: ProtectedFrameOperation,
    result: BasicValueEnum<'context>,
) -> Result<(), CodegenFailure> {
    if frame_result_is_indirect(target, operation) {
        let destination = function
            .get_first_param()
            .and_then(|value| match value {
                BasicValueEnum::PointerValue(value) => Some(value),
                _ => None,
            })
            .expect("native ABI lowering requires an established mapping or value");

        builder
            .build_store(destination, result)
            .map_err(CodegenFailure::backend_library)?;

        builder
            .build_return(None)
            .map_err(CodegenFailure::backend_library)?;
    } else {
        let physical = function
            .get_type()
            .get_return_type()
            .expect("native ABI lowering requires an established mapping or value");

        let result = crate::translation::reinterpret_value(
            context,
            builder,
            result,
            physical,
            physical,
            "frame.result",
        )?;

        builder
            .build_return(Some(&result))
            .map_err(CodegenFailure::backend_library)?;
    }

    Ok(())
}

fn frame_result_is_indirect(_target: &CodegenTarget, operation: ProtectedFrameOperation) -> bool {
    matches!(
        operation,
        ProtectedFrameOperation::MoveBeforeStart
            | ProtectedFrameOperation::Resume
            | ProtectedFrameOperation::CancellationEntry
    )
}

pub(crate) fn return_frame_state(
    context: &Context,
    builder: &Builder<'_>,
    affinity: u64,
    lane_requirements: u64,
) -> Result<(), CodegenFailure> {
    let state = context
        .i64_type()
        .const_int(affinity | (lane_requirements << 32), false);

    builder
        .build_return(Some(&state))
        .map_err(CodegenFailure::backend_library)?;

    Ok(())
}

pub(crate) fn frame_operation_function<'context>(
    module: &Module<'context>,
    request: CodegenRequest<'_>,
    instance: &CodegenInstance,
    operation: ProtectedFrameOperation,
) -> FunctionValue<'context> {
    let frame = instance
        .protected_frame_identity()
        .expect("native ABI lowering requires an established mapping or value");

    let symbol = request
        .mappings()
        .symbol(&CodegenSymbolKey::ProtectedFrame { frame, operation })
        .expect("native ABI lowering requires an established mapping or value");

    module
        .get_function(symbol.name().as_str())
        .unwrap_or_else(|| panic!("protected-frame operation must be declared: {operation:?}"))
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
            panic_report_type(context).into(),
        ],
        false,
    )
}

pub(crate) fn panic_report_type(context: &Context) -> StructType<'_> {
    context.struct_type(&NATIVE_PANIC_REPORT_FIELDS.map(|field| panic_field_type(context, field)), false)
}

fn panic_field_type(context: &Context, field: RuntimeAbiType) -> BasicTypeEnum<'_> {
    match field {
        RuntimeAbiType::U32 => context.i32_type().into(),
        RuntimeAbiType::U64 | RuntimeAbiType::Usize => context.i64_type().into(),
        RuntimeAbiType::Pointer => context.ptr_type(AddressSpace::default()).into(),
        _ => panic!("panic report field has an unsupported native ABI type"),
    }
}

pub(crate) fn run_outcome_type<'context>(
    context: &'context Context,
    target: &CodegenTarget,
) -> StructType<'context> {
    context.struct_type(
        &[
            context.i32_type().into(),
            pointer_integer_type(context, target).into(),
            panic_report_type(context).into(),
        ],
        false,
    )
}

fn task_allocation_type(context: &Context) -> StructType<'_> {
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

fn root_start_type(context: &Context) -> StructType<'_> {
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

pub(super) fn frame_state_type(context: &Context) -> StructType<'_> {
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
        &std::array::from_fn::<_, 9, _>(|index| {
            panic_field_type(context, NATIVE_PANIC_REPORT_FIELDS[index])
        }),
        false,
    )
}

pub(crate) fn source_anchor_value(
    context: &Context,
    source: bray_runtime_abi::NativeSourceAnchor,
) -> StructValue<'_> {
    let package = source.package();

    let [word0, word1, word2, word3] = [0, 8, 16, 24].map(|offset| {
        let bytes = package[offset..offset + 8]
            .try_into()
            .expect("package digest has four words");

        context
            .i64_type()
            .const_int(u64::from_le_bytes(bytes), false)
            .into()
    });

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
        word0,
        word1,
        word2,
        word3,
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

pub(crate) fn declare_runtime_function<'context>(
    module: &Module<'context>,
    context: &'context Context,
    target: &CodegenTarget,
    role: RuntimeAbiRole,
) -> Result<FunctionValue<'context>, CodegenFailure> {
    let name = role
        .native_symbol()
        .unwrap_or_else(|| panic!("runtime declaration requires a native ABI role, got {role:?}"));

    let signature = runtime_function_type(context, target, role).unwrap_or_else(|| {
        panic!("runtime declaration requires a realizable native ABI for {role:?}")
    });

    let function = module
        .get_function(name)
        .unwrap_or_else(|| module.add_function(name, signature, None));

    for (location, attribute) in super::abi::runtime_attributes(context, target, role)? {
        function.add_attribute(location, attribute);
    }

    Ok(function)
}

pub(super) fn aggregate_is_indirect(kind: RuntimeAbiType) -> bool {
    matches!(
        kind,
        RuntimeAbiType::Configuration
            | RuntimeAbiType::RootStart
            | RuntimeAbiType::RunOutcome
            | RuntimeAbiType::TaskAllocation
            | RuntimeAbiType::InactiveFrame
            | RuntimeAbiType::FrameProgress
            | RuntimeAbiType::ProductObservation
    )
}

pub(super) fn runtime_value_type<'context>(
    context: &'context Context,
    target: &CodegenTarget,
    kind: RuntimeAbiType,
) -> Option<BasicTypeEnum<'context>> {
    let usize = pointer_integer_type(context, target);

    Some(match kind {
        RuntimeAbiType::Void | RuntimeAbiType::Never => return None,
        RuntimeAbiType::U8 => context.i8_type().into(),
        RuntimeAbiType::U32 => context.i32_type().into(),
        RuntimeAbiType::U64 => context.i64_type().into(),
        RuntimeAbiType::Usize => usize.into(),
        RuntimeAbiType::Pointer => context.ptr_type(AddressSpace::default()).into(),
        RuntimeAbiType::Configuration => runtime_configuration_type(context, target).into(),
        RuntimeAbiType::RootStart => root_start_type(context).into(),
        RuntimeAbiType::RunOutcome => run_outcome_type(context, target).into(),
        RuntimeAbiType::PanicReport => panic_report_type(context).into(),
        RuntimeAbiType::TaskAllocation => task_allocation_type(context).into(),
        RuntimeAbiType::InactiveFrame => inactive_frame_type(context).into(),
        RuntimeAbiType::FrameProgress => frame_progress_type(context).into(),
        RuntimeAbiType::LaneResult => context
            .struct_type(
                &[context.i32_type().into(), context.i32_type().into()],
                false,
            )
            .into(),
        RuntimeAbiType::ProductObservation => context
            .struct_type(
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
            .into(),
    })
}

pub(crate) fn frame_operation_type<'context>(
    context: &'context Context,
    target: &CodegenTarget,
    operation: ProtectedFrameOperation,
) -> FunctionType<'context> {
    let usize = pointer_integer_type(context, target);
    let pointer = context.ptr_type(AddressSpace::default());

    match operation {
        ProtectedFrameOperation::MoveBeforeStart
        | ProtectedFrameOperation::Resume
        | ProtectedFrameOperation::CancellationEntry => context
            .void_type()
            .fn_type(&[pointer.into(), usize.into()], false),
        ProtectedFrameOperation::StateDescription => context
            .i64_type()
            .fn_type(&[usize.into(), context.i32_type().into()], false),
        ProtectedFrameOperation::TaskBroadcast | ProtectedFrameOperation::Destruction => {
            context.void_type().fn_type(&[usize.into()], false)
        }
        ProtectedFrameOperation::LifecycleResolution => context.void_type().fn_type(
            &[pointer.into(), usize.into(), context.i32_type().into()],
            false,
        ),
        ProtectedFrameOperation::CompletionMove => context
            .void_type()
            .fn_type(&[usize.into(), usize.into()], false),
    }
}

#[cfg(test)]
mod tests {
    use bray_codegen::CodegenTarget;
    use bray_runtime_interface::{ProtectedFrameOperation, RuntimeAbiRole, RuntimeAbiType};
    use bray_target::NativeTarget;
    use inkwell::context::Context;

    #[test]
    fn panic_report_fields_keep_native_offsets() {
        let context = Context::create();
        let report = super::panic_report_type(&context);

        for native in NativeTarget::ALL {
            let target = CodegenTarget::for_native(native);

            let machine = crate::machine::LlvmTargetMachine::create(&target)
                .expect("native target must create an LLVM machine");

            let data = machine.target_data();

            assert_eq!(data.get_store_size(&report), 136);
            assert_eq!(data.get_abi_alignment(&report), 8);

            for (element, offset) in [(5, 24), (8, 48), (9, 56), (10, 64), (18, 128)] {
                assert_eq!(data.offset_of_element(&report, element), Some(offset));
            }
        }
    }

    #[test]
    fn every_native_catalog_role_has_a_declaration_on_every_native_target() {
        let context = Context::create();

        for native in NativeTarget::ALL {
            let target = CodegenTarget::for_native(native);

            for role in RuntimeAbiRole::ALL {
                let actual = super::runtime_function_type(&context, &target, role);

                assert_eq!(
                    actual.is_some(),
                    role.native_signature().is_some(),
                    "{native:?} {role:?}"
                );

                if let (Some(actual), Some(expected)) = (actual, role.native_signature()) {
                    let indirect =
                        super::runtime_indirect_result_type(&context, &target, role).is_some();

                    assert_eq!(
                        usize::try_from(actual.count_param_types()).expect("parameter count fits"),
                        expected
                            .parameters()
                            .iter()
                            .map(|kind| {
                                if target.machine().architecture()
                                    == bray_target::TargetArchitecture::X86_64
                                    && !super::uses_microsoft_x64_abi(&target)
                                    && matches!(
                                        kind,
                                        RuntimeAbiType::Configuration
                                            | RuntimeAbiType::InactiveFrame
                                    )
                                {
                                    2
                                } else {
                                    1
                                }
                            })
                            .sum::<usize>()
                            + usize::from(indirect),
                        "{native:?} {role:?}",
                    );
                }
            }
        }
    }

    #[test]
    fn native_byte_results_and_lane_records_keep_their_c_abi() {
        let context = Context::create();
        let target = CodegenTarget::for_native(NativeTarget::X86_64WindowsMsvc);

        let cancellation = super::runtime_function_type(
            &context,
            &target,
            RuntimeAbiRole::CurrentRunCancellationObservation,
        )
        .expect("native cancellation signature exists");

        let lane = super::runtime_function_type(
            &context,
            &target,
            RuntimeAbiRole::CompatibleLaneSelection,
        )
        .expect("native lane signature exists");

        assert_eq!(
            cancellation.get_return_type(),
            Some(context.i8_type().into())
        );

        assert_eq!(lane.get_return_type(), Some(context.i64_type().into()));

        assert_eq!(
            RuntimeAbiRole::SuspensionRegistration
                .native_signature()
                .expect("native signature exists")
                .result(),
            RuntimeAbiType::FrameProgress,
        );
    }

    #[test]
    fn product_host_observations_use_indirect_storage_on_every_target() {
        let context = Context::create();

        for native in NativeTarget::ALL {
            let target = CodegenTarget::for_native(native);
            let module = context.create_module("host.contract");

            let function = super::declare_runtime_function(
                &module,
                &context,
                &target,
                RuntimeAbiRole::ProductHostControl,
            )
            .expect("product host role has a native declaration");

            assert_eq!(function.get_type().get_return_type(), None, "{native:?}");
            assert_eq!(function.count_params(), 3, "{native:?}");

            assert!(
                function
                    .get_enum_attribute(
                        inkwell::attributes::AttributeLoc::Param(0),
                        inkwell::attributes::Attribute::get_named_enum_kind_id("sret"),
                    )
                    .is_some(),
                "{native:?}"
            );

            module.verify().expect("native declaration verifies");
        }
    }

    #[test]
    fn report_consumers_borrow_one_header_on_every_native_target() {
        let context = Context::create();

        for native in NativeTarget::ALL {
            let target = CodegenTarget::for_native(native);

            for role in [
                RuntimeAbiRole::PanicReporting,
                RuntimeAbiRole::PanicReportDestruction,
                RuntimeAbiRole::PanicPropagation,
            ] {
                let function = super::runtime_function_type(&context, &target, role)
                    .expect("report role has a native ABI");

                assert_eq!(
                    function.get_param_types(),
                    [context.ptr_type(inkwell::AddressSpace::default()).into()],
                    "{native:?} {role:?}"
                );
            }

            let report = super::runtime_function_type(
                &context,
                &target,
                RuntimeAbiRole::RootTerminalObservation,
            )
            .expect("terminal observation has a native ABI");

            assert_eq!(report.get_return_type(), None, "{native:?}");

            assert_eq!(
                report.get_param_types()[0],
                context.ptr_type(inkwell::AddressSpace::default()).into()
            );
        }
    }

    #[test]
    #[should_panic(expected = "runtime declaration requires a native ABI role, got FrameResume")]
    fn native_declarations_expose_compiler_owned_roles() {
        let context = Context::create();
        let module = context.create_module("compiler.role");
        let target = CodegenTarget::for_native(NativeTarget::X86_64WindowsMsvc);

        let _ = super::declare_runtime_function(
            &module,
            &context,
            &target,
            RuntimeAbiRole::FrameResume,
        );
    }

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

        assert_eq!(cancellation.count_param_types(), 2);
        assert_eq!(cancellation.get_return_type(), None);

        assert_eq!(state.count_param_types(), 2);

        assert_eq!(state.get_return_type(), Some(context.i64_type().into()));
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

        assert_eq!(panic.count_param_types(), 13);

        assert_eq!(
            panic.get_param_types(),
            [
                context.ptr_type(inkwell::AddressSpace::default()).into(),
                context.i32_type().into(),
                context.i32_type().into(),
                context.i32_type().into(),
                context.i32_type().into(),
                context.i32_type().into(),
                context.i64_type().into(),
                context.i64_type().into(),
                context.i64_type().into(),
                context.i64_type().into(),
                context.i64_type().into(),
                context.ptr_type(inkwell::AddressSpace::default()).into(),
                context.i64_type().into(),
            ]
        );
    }

    #[test]
    fn system_v_x64_native_abi_returns_small_records_in_registers_and_reports_indirectly() {
        let context = Context::create();
        let target = CodegenTarget::for_native(NativeTarget::X86_64LinuxGnu);

        let root = super::runtime_function_type(&context, &target, RuntimeAbiRole::RootExecution)
            .unwrap_or_else(|| panic!("root execution must have a native ABI"));

        assert_eq!(
            root.get_return_type(),
            Some(super::root_start_type(&context).into())
        );

        assert_eq!(root.count_param_types(), 3);

        let resume =
            super::frame_operation_type(&context, &target, ProtectedFrameOperation::Resume);

        assert_eq!(resume.get_return_type(), None);
        assert_eq!(resume.count_param_types(), 2);

        let panic = super::runtime_function_type(
            &context,
            &target,
            RuntimeAbiRole::PanicReportConstruction,
        )
        .unwrap_or_else(|| panic!("panic report construction must have a native ABI"));

        assert_eq!(panic.count_param_types(), 13);

        assert_eq!(
            panic.get_param_types(),
            [
                context.ptr_type(inkwell::AddressSpace::default()).into(),
                context.i32_type().into(),
                context.i32_type().into(),
                context.i32_type().into(),
                context.i32_type().into(),
                context.i32_type().into(),
                context.i64_type().into(),
                context.i64_type().into(),
                context.i64_type().into(),
                context.i64_type().into(),
                context.i64_type().into(),
                context.ptr_type(inkwell::AddressSpace::default()).into(),
                context.i64_type().into(),
            ]
        );
    }
}
