use bray_codegen::{CodegenFailure, CodegenTarget};
use bray_runtime_interface::{RuntimeAbiRole, RuntimeAbiType};
use bray_target::{ObjectFormat, TargetArchitecture};
use inkwell::AddressSpace;
use inkwell::attributes::{Attribute, AttributeLoc};
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::types::{AnyType, BasicMetadataTypeEnum, BasicType, BasicTypeEnum, FunctionType};
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue};

use super::core::{
    aggregate_is_indirect, indirect_result_attribute, runtime_indirect_result_type,
    runtime_value_type, uses_microsoft_x64_abi,
};

pub(super) fn runtime_function_type<'context>(
    context: &'context Context,
    target: &CodegenTarget,
    role: RuntimeAbiRole,
) -> Option<FunctionType<'context>> {
    let signature = role.native_signature()?;
    let indirect = runtime_indirect_result_type(context, target, role).is_some();
    let mut parameters = Vec::new();

    if indirect {
        parameters.push(context.ptr_type(AddressSpace::default()).into());
    }

    for kind in signature.parameters() {
        let physical = parameter_type(context, target, *kind)?;

        if expands_parameter(target, *kind) {
            parameters.extend(
                physical
                    .into_struct_type()
                    .get_field_types()
                    .into_iter()
                    .map(BasicMetadataTypeEnum::from),
            );
        } else {
            parameters.push(physical.into());
        }
    }

    if indirect
        || matches!(
            signature.result(),
            RuntimeAbiType::Void | RuntimeAbiType::Never
        )
    {
        return Some(context.void_type().fn_type(&parameters, false));
    }

    Some(register_type(context, target, signature.result())?.fn_type(&parameters, false))
}

pub(crate) fn runtime_attributes(
    context: &Context,
    target: &CodegenTarget,
    role: RuntimeAbiRole,
) -> Result<Vec<(AttributeLoc, Attribute)>, CodegenFailure> {
    let mut attributes = Vec::new();

    if role == RuntimeAbiRole::PanicReportConstruction {
        attributes.push((
            AttributeLoc::Function,
            crate::mapping::enum_attribute("cold", 0, context)?,
        ));
    }

    if let Some(result) = runtime_indirect_result_type(context, target, role) {
        attributes.push((
            AttributeLoc::Param(0),
            indirect_result_attribute(context, result)?,
        ));
    }

    if target.machine().architecture() == TargetArchitecture::X86_64
        && !uses_microsoft_x64_abi(target)
    {
        let signature = role
            .native_signature()
            .expect("runtime role has a native signature");

        let mut index = u32::from(runtime_indirect_result_type(context, target, role).is_some());

        for kind in signature.parameters() {
            if matches!(
                kind,
                RuntimeAbiType::InactiveFrame | RuntimeAbiType::PanicReport
            ) {
                let value = runtime_value_type(context, target, *kind)
                    .expect("indirect aggregate parameter has a native layout");

                attributes.push((
                    AttributeLoc::Param(index),
                    context.create_type_attribute(
                        Attribute::get_named_enum_kind_id("byval"),
                        value.as_any_type_enum(),
                    ),
                ));

                attributes.push((
                    AttributeLoc::Param(index),
                    crate::mapping::enum_attribute("align", 8, context)?,
                ));
            }

            index += if expands_parameter(target, *kind) {
                2
            } else {
                1
            };
        }
    }

    let machine = target.machine();

    let extends_bytes = machine.object_format() == ObjectFormat::MachO
        || machine.architecture() == TargetArchitecture::X86_64
            && machine.object_format() == ObjectFormat::Elf;

    if !extends_bytes {
        return Ok(attributes);
    }

    let signature = runtime_function_type(context, target, role)
        .unwrap_or_else(|| panic!("runtime attributes require a native ABI role, got {role:?}"));

    let zero_extend = crate::mapping::enum_attribute("zeroext", 0, context)?;

    for (index, parameter) in (0..).zip(signature.get_param_types()) {
        if parameter == context.i8_type().into() {
            attributes.push((AttributeLoc::Param(index), zero_extend));
        }
    }

    if signature.get_return_type() == Some(context.i8_type().into()) {
        attributes.push((AttributeLoc::Return, zero_extend));
    }

    Ok(attributes)
}

fn parameter_type<'context>(
    context: &'context Context,
    target: &CodegenTarget,
    kind: RuntimeAbiType,
) -> Option<BasicTypeEnum<'context>> {
    if parameter_is_indirect(target, kind) {
        return Some(context.ptr_type(AddressSpace::default()).into());
    }

    register_type(context, target, kind)
}

fn parameter_is_indirect(target: &CodegenTarget, kind: RuntimeAbiType) -> bool {
    matches!(
        kind,
        RuntimeAbiType::PanicReport | RuntimeAbiType::InactiveFrame
    ) || uses_microsoft_x64_abi(target) && aggregate_is_indirect(kind)
}

fn expands_parameter(target: &CodegenTarget, kind: RuntimeAbiType) -> bool {
    target.machine().architecture() == TargetArchitecture::X86_64
        && !uses_microsoft_x64_abi(target)
        && kind != RuntimeAbiType::InactiveFrame
        && aggregate_is_indirect(kind)
}

fn register_type<'context>(
    context: &'context Context,
    target: &CodegenTarget,
    kind: RuntimeAbiType,
) -> Option<BasicTypeEnum<'context>> {
    if kind == RuntimeAbiType::LaneResult {
        return Some(context.i64_type().into());
    }

    if target.machine().architecture() == TargetArchitecture::Aarch64 && aggregate_is_indirect(kind)
    {
        return Some(context.i64_type().array_type(2).into());
    }

    runtime_value_type(context, target, kind)
}

pub(super) fn invoke_runtime<'context>(
    context: &'context Context,
    builder: &Builder<'context>,
    target: &CodegenTarget,
    role: RuntimeAbiRole,
    function: FunctionValue<'context>,
    arguments: &[BasicMetadataValueEnum<'context>],
    name: &str,
) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
    let signature = role
        .native_signature()
        .unwrap_or_else(|| panic!("runtime invocation requires a native ABI role, got {role:?}"));

    assert_eq!(
        signature.parameters().len(),
        arguments.len(),
        "runtime invocation argument count must match the native ABI for {role:?}: expected {}, actual {}",
        signature.parameters().len(),
        arguments.len()
    );

    let result = runtime_indirect_result_type(context, target, role);

    let storage = result
        .map(|result| crate::translation::allocate_temporary(context, builder, result, name))
        .transpose()?;

    let completed = if role == RuntimeAbiRole::PanicReportSuppression {
        let report = super::core::panic_report_type(context);
        let mut owned = context.bool_type().const_zero();

        for argument in arguments {
            let value = BasicValueEnum::try_from(*argument)
                .expect("owned report arguments retain their native representation");

            let present = if let BasicValueEnum::PointerValue(pointer) = value {
                crate::translation::report_slot_has_ownership(context, builder, pointer)?
            } else {
                crate::translation::report_has_ownership(context, builder, |index| {
                    builder
                        .build_extract_value(value.into_struct_value(), index, "report.ownership")
                        .map_err(CodegenFailure::backend_library)
                })?
            };

            owned = builder
                .build_or(owned, present, "reports.have_ownership")
                .map_err(CodegenFailure::backend_library)?;
        }

        let parent = builder
            .get_insert_block()
            .and_then(|block| block.get_parent())
            .expect("report suppression belongs to a generated function");

        let call = context.append_basic_block(parent, "report.suppress.owned");
        let empty = context.append_basic_block(parent, "report.suppress.empty");
        let completed = context.append_basic_block(parent, "report.suppress.completed");

        builder
            .build_conditional_branch(owned, call, empty)
            .map_err(CodegenFailure::backend_library)?;

        builder.position_at_end(empty);

        // Empty reports transfer no ownership. Preserve the native operation's canonical
        // slot clearing while avoiding argument copies, temporary outcomes and native calls.
        for argument in arguments {
            if let BasicMetadataValueEnum::PointerValue(pointer) = argument {
                builder
                    .build_store(*pointer, report.const_zero())
                    .map_err(CodegenFailure::backend_library)?;
            }
        }

        builder
            .build_store(
                storage.expect("native reports use an indirect result"),
                report.const_zero(),
            )
            .map_err(CodegenFailure::backend_library)?;

        builder
            .build_unconditional_branch(completed)
            .map_err(CodegenFailure::backend_library)?;

        builder.position_at_end(call);

        Some(completed)
    } else {
        None
    };

    let mut native_arguments = storage.into_iter().map(Into::into).collect::<Vec<_>>();

    for (kind, argument) in signature
        .parameters()
        .iter()
        .copied()
        .zip(arguments.iter().copied())
    {
        let value = BasicValueEnum::try_from(argument).unwrap_or_else(|_| {
            panic!("runtime argument for {role:?} must be a basic LLVM value, got {argument:?}")
        });

        let physical = parameter_type(context, target, kind)
            .expect("native ABI lowering requires an established mapping or value");

        if kind == RuntimeAbiType::PanicReportPointer && value.is_pointer_value() {
            native_arguments.push(value.into());
        } else if parameter_is_indirect(target, kind) || kind == RuntimeAbiType::PanicReportPointer
        {
            let name = format!("{name}.argument");

            let storage = if uses_microsoft_x64_abi(target) && parameter_is_indirect(target, kind) {
                // Win64 indirect value arguments require 16-byte temporary alignment,
                // even when the record's fields only require native-word alignment.
                crate::translation::allocate_aligned_temporary(
                    context,
                    builder,
                    value.get_type(),
                    16,
                    &name,
                )?
            } else {
                crate::translation::allocate_temporary(context, builder, value.get_type(), &name)?
            };

            builder
                .build_store(storage, value)
                .map_err(CodegenFailure::backend_library)?;

            native_arguments.push(storage.into());
        } else {
            let value = crate::translation::reinterpret_value(
                context, builder, value, physical, physical, name,
            )?;

            if expands_parameter(target, kind) {
                for index in 0..value.into_struct_value().get_type().count_fields() {
                    native_arguments.push(
                        builder
                            .build_extract_value(value.into_struct_value(), index, name)
                            .map_err(CodegenFailure::backend_library)?
                            .into(),
                    );
                }
            } else {
                native_arguments.push(value.into());
            }
        }
    }

    let call = builder
        .build_call(function, &native_arguments, name)
        .map_err(CodegenFailure::backend_library)?;

    for (location, attribute) in runtime_attributes(context, target, role)? {
        call.add_attribute(location, attribute);
    }

    if let Some(completed) = completed {
        builder
            .build_unconditional_branch(completed)
            .map_err(CodegenFailure::backend_library)?;

        builder.position_at_end(completed);
    }

    if let (Some(result), Some(storage)) = (result, storage) {
        return builder
            .build_load(result, storage, name)
            .map(Some)
            .map_err(CodegenFailure::backend_library);
    }

    let Some(value) = call.try_as_basic_value().basic() else {
        return Ok(None);
    };

    let logical = runtime_value_type(context, target, signature.result())
        .expect("native ABI lowering requires an established mapping or value");

    crate::translation::reinterpret_value(context, builder, value, logical, value.get_type(), name)
        .map(Some)
}

pub(crate) fn call_void_intrinsic<'context>(
    module: &inkwell::module::Module<'context>,
    context: &'context Context,
    builder: &Builder<'context>,
    name: &str,
) -> Result<(), CodegenFailure> {
    let function = module.get_function(name).unwrap_or_else(|| {
        module.add_function(name, context.void_type().fn_type(&[], false), None)
    });

    builder
        .build_call(function, &[], "")
        .map_err(CodegenFailure::backend_library)?;

    Ok(())
}

pub(crate) fn declare_callback<'context>(
    module: &inkwell::module::Module<'context>,
    name: &str,
    block_name: &str,
    ty: FunctionType<'context>,
    target: &CodegenTarget,
) -> FunctionValue<'context> {
    let callback = module.add_function(name, ty, None);

    callback.set_linkage(inkwell::module::Linkage::WeakODR);

    callback
        .as_global_value()
        .set_visibility(inkwell::GlobalVisibility::Hidden);

    crate::comdat::attach_any(
        module,
        callback.as_global_value(),
        name,
        target.machine().object_format(),
    );

    module
        .get_context()
        .append_basic_block(callback, block_name);

    callback
}

#[cfg(test)]
mod tests {
    use super::{invoke_runtime, runtime_function_type};
    use crate::native::core::{declare_runtime_function, runtime_value_type};
    use bray_codegen::CodegenTarget;
    use bray_runtime_interface::RuntimeAbiRole;
    use bray_target::NativeTarget;
    use inkwell::context::Context;

    #[test]
    fn panic_construction_is_cold_without_cooling_ordinary_report_merges() {
        for native in NativeTarget::ALL {
            let context = Context::create();
            let module = context.create_module("failure.attributes");

            let target = CodegenTarget::for_native(native);

            let constructor = declare_runtime_function(
                &module,
                &context,
                &target,
                RuntimeAbiRole::PanicReportConstruction,
            )
            .expect("panic construction has a native declaration");

            let merge = declare_runtime_function(
                &module,
                &context,
                &target,
                RuntimeAbiRole::PanicReportSuppression,
            )
            .expect("report suppression has a native declaration");

            let cold = inkwell::attributes::Attribute::get_named_enum_kind_id("cold");

            assert!(
                constructor
                    .get_enum_attribute(inkwell::attributes::AttributeLoc::Function, cold)
                    .is_some()
            );

            assert!(
                merge
                    .get_enum_attribute(inkwell::attributes::AttributeLoc::Function, cold)
                    .is_none()
            );

            let caller =
                module.add_function("construct", context.void_type().fn_type(&[], false), None);

            let builder = context.create_builder();

            builder.position_at_end(context.append_basic_block(caller, "entry"));

            let arguments = RuntimeAbiRole::PanicReportConstruction
                .native_signature()
                .expect("native signature")
                .parameters()
                .iter()
                .map(|kind| {
                    runtime_value_type(&context, &target, *kind)
                        .expect("native parameter type")
                        .const_zero()
                        .into()
                })
                .collect::<Vec<_>>();

            invoke_runtime(
                &context,
                &builder,
                &target,
                RuntimeAbiRole::PanicReportConstruction,
                constructor,
                &arguments,
                "report",
            )
            .expect("cold native invocation retains its ABI");

            builder.build_return(None).expect("native caller returns");

            module
                .verify()
                .expect("cold declarations and calls verify on every native ABI");
        }
    }

    #[test]
    fn empty_report_suppression_skips_only_unowned_native_transfers() {
        use inkwell::types::BasicTypeEnum;
        use inkwell::values::AnyValue;

        let context = Context::create();
        let report = crate::native::panic_report_type(&context);
        let role = RuntimeAbiRole::PanicReportSuppression;

        for native in NativeTarget::ALL {
            let target = CodegenTarget::for_native(native);

            let machine = crate::machine::LlvmTargetMachine::create(&target)
                .expect("native report target creates an LLVM machine");

            for owner in [None, Some(10), Some(16), Some(19), Some(20)] {
                for owning_argument in 0..2 {
                    for pointer_arguments in [false, true] {
                        let module = context.create_module("report.suppression");

                        module.set_data_layout(&machine.target_data().get_data_layout());

                        let builder = context.create_builder();

                        let wrapper =
                            module.add_function("wrapper", report.fn_type(&[], false), None);

                        builder.position_at_end(context.append_basic_block(wrapper, "entry"));

                        let mut arguments = Vec::new();

                        for index in 0..2 {
                            // Non-owning header fields still need canonical empty results.
                            let mut value = builder
                                .build_insert_value(
                                    report.const_zero(),
                                    context.i32_type().const_int(1, false),
                                    0,
                                    "source",
                                )
                                .expect("report source field exists")
                                .into_struct_value();

                            if index == owning_argument
                                && let Some(field) = owner
                            {
                                let nonzero: inkwell::values::BasicValueEnum<'_> = match report
                                    .get_field_type_at_index(field)
                                    .expect("ownership field exists")
                                {
                                    BasicTypeEnum::IntType(integer) => {
                                        integer.const_int(1, false).into()
                                    }
                                    BasicTypeEnum::PointerType(pointer) => context
                                        .i64_type()
                                        .const_int(1, false)
                                        .const_to_pointer(pointer)
                                        .into(),
                                    _ => panic!("native ownership fields are scalar values"),
                                };

                                value = builder
                                    .build_insert_value(value, nonzero, field, "owner")
                                    .expect("report ownership field exists")
                                    .into_struct_value();
                            }

                            if pointer_arguments {
                                let slot = builder
                                    .build_alloca(report, "report.slot")
                                    .expect("report has stack storage");

                                builder
                                    .build_store(slot, value)
                                    .expect("report slot is writable");

                                arguments.push(slot.into());
                            } else {
                                arguments.push(value.into());
                            }
                        }

                        let function = declare_runtime_function(&module, &context, &target, role)
                            .expect("native suppression has a declared ABI");

                        let value = invoke_runtime(
                            &context, &builder, &target, role, function, &arguments, "suppress",
                        )
                        .expect("owned report suppression lowers")
                        .expect("report suppression returns a value");

                        builder
                            .build_return(Some(&value))
                            .expect("wrapper returns its logical report");

                        module
                            .verify()
                            .expect("native report transfer is valid LLVM");

                        machine
                            .run_passes(&module, "default<O2>")
                            .expect("native report transfer optimizes");

                        let body = wrapper.print_to_string().to_string();

                        assert_eq!(
                            body.contains("@bray_runtime_panic_report_suppression"),
                            owner.is_some(),
                            "{native:?} {owner:?} {pointer_arguments}: {body}"
                        );

                        if owner.is_none() {
                            assert!(body.contains("zeroinitializer"), "{body}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn empty_report_suppression_clears_both_borrowed_native_slots() {
        use inkwell::values::AnyValue;

        let context = Context::create();
        let target = CodegenTarget::for_native(NativeTarget::X86_64LinuxGnu);
        let module = context.create_module("report.suppression.slots");
        let builder = context.create_builder();
        let pointer = context.ptr_type(inkwell::AddressSpace::default());

        let wrapper = module.add_function(
            "wrapper",
            context
                .void_type()
                .fn_type(&[pointer.into(), pointer.into()], false),
            None,
        );

        builder.position_at_end(context.append_basic_block(wrapper, "entry"));

        let arguments = wrapper.get_param_iter().map(Into::into).collect::<Vec<_>>();
        let role = RuntimeAbiRole::PanicReportSuppression;

        let function = declare_runtime_function(&module, &context, &target, role)
            .expect("native suppression has a declared ABI");

        invoke_runtime(
            &context, &builder, &target, role, function, &arguments, "suppress",
        )
        .expect("native slots preserve report ownership");

        builder
            .build_return(None)
            .expect("slot wrapper returns normally");

        module
            .verify()
            .expect("native report slots form valid LLVM");

        let body = wrapper.print_to_string().to_string();

        for slot in [0, 1] {
            assert!(
                body.contains(&format!("zeroinitializer, ptr %{slot},")),
                "{body}"
            );
        }
    }

    #[test]
    fn wrong_native_argument_count_exposes_the_role() {
        let context = Context::create();
        let target = CodegenTarget::for_native(NativeTarget::X86_64WindowsMsvc);
        let module = context.create_module("bad.runtime.call");
        let builder = context.create_builder();
        let wrapper = module.add_function("wrapper", context.void_type().fn_type(&[], false), None);

        builder.position_at_end(context.append_basic_block(wrapper, "entry"));

        let role = RuntimeAbiRole::CurrentNativeThreadIdentity;

        let function = declare_runtime_function(&module, &context, &target, role)
            .expect("fixture `declare_runtime_function` call must succeed");

        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = invoke_runtime(
                &context,
                &builder,
                &target,
                role,
                function,
                &[context.i32_type().const_zero().into()],
                "bad.call",
            );
        }))
        .expect_err("wrong native argument count must panic");

        let message = bray_testing::panic_payload_text(panic.as_ref());

        assert!(message.contains("CurrentNativeThreadIdentity"));
        assert!(message.contains("expected 0"));
        assert!(message.contains("actual 1"));
    }

    #[test]
    fn protected_callbacks_pack_returns_and_restore_logical_records_on_every_target() {
        use crate::native::core::{
            frame_operation_type, frame_progress_type, frame_state_type, invoke_function,
            return_frame_result, return_frame_state,
        };

        use bray_codegen::CodegenSymbolKey;
        use bray_runtime_interface::{ProtectedAsyncFrameId, ProtectedFrameOperation};

        let context = Context::create();

        for native in NativeTarget::ALL {
            let target = CodegenTarget::for_native(native);
            let module = context.create_module("frame.callbacks");
            let builder = context.create_builder();

            for operation in [
                ProtectedFrameOperation::Resume,
                ProtectedFrameOperation::CancellationEntry,
                ProtectedFrameOperation::TaskBroadcast,
                ProtectedFrameOperation::StateDescription,
            ] {
                let name = format!("{operation:?}");

                let function = module.add_function(
                    &name,
                    frame_operation_type(&context, &target, operation),
                    None,
                );

                builder.position_at_end(context.append_basic_block(function, "entry"));

                let logical = if operation == ProtectedFrameOperation::StateDescription {
                    assert_eq!(
                        function.get_type().get_return_type(),
                        Some(context.i64_type().into())
                    );

                    return_frame_state(&context, &builder, 1, 2)
                        .expect("fixture `return_frame_state` call must succeed");

                    frame_state_type(&context)
                } else {
                    assert_eq!(function.get_type().get_return_type(), None);

                    let logical = frame_progress_type(&context);

                    let value = logical.const_named_struct(&[
                        context.i32_type().const_int(1, false).into(),
                        context.i32_type().const_int(2, false).into(),
                        context.i64_type().const_int(3, false).into(),
                        crate::native::panic_report_type(&context)
                            .const_zero()
                            .into(),
                    ]);

                    return_frame_result(
                        &context,
                        &builder,
                        function,
                        &target,
                        operation,
                        value.into(),
                    )
                    .expect("fixture `return_frame_result` call must succeed");

                    logical
                };

                let caller = module.add_function(
                    &format!("{name}.caller"),
                    context.void_type().fn_type(&[], false),
                    None,
                );

                builder.position_at_end(context.append_basic_block(caller, "entry"));

                let key = CodegenSymbolKey::ProtectedFrame {
                    frame: ProtectedAsyncFrameId::new([0; 32]),
                    operation,
                };

                let mut arguments = vec![context.i64_type().const_zero().into()];

                if operation == ProtectedFrameOperation::StateDescription {
                    arguments.push(context.i32_type().const_zero().into());
                    arguments.push(context.i32_type().const_zero().into());
                    arguments.push(context.i64_type().const_zero().into());
                }

                let result = invoke_function(
                    &context, &builder, &target, &key, function, &arguments, "callback",
                )
                .expect("fixture `invoke_function` call must succeed")
                .expect("successful fixture query must contain its expected result");

                assert_eq!(
                    result.get_type(),
                    logical.into(),
                    "{native:?} {operation:?}"
                );

                builder
                    .build_return(None)
                    .expect("fixture `build_return` call must succeed");
            }

            let resolution = frame_operation_type(
                &context,
                &target,
                ProtectedFrameOperation::LifecycleResolution,
            );

            assert_eq!(resolution.get_return_type(), None);

            assert_eq!(
                resolution.get_param_types(),
                [
                    context.ptr_type(inkwell::AddressSpace::default()).into(),
                    context.i64_type().into(),
                    context.i32_type().into(),
                ]
            );

            module
                .verify()
                .unwrap_or_else(|error| panic!("{native:?}: {error}"));
        }
    }

    #[test]
    fn aggregate_register_shapes_match_clang_for_every_native_abi() {
        let context = Context::create();

        for native in NativeTarget::ALL {
            let target = CodegenTarget::for_native(native);

            let progress =
                runtime_function_type(&context, &target, RuntimeAbiRole::SuspensionRegistration)
                    .expect("fixture `runtime_function_type` call must succeed");

            let lane =
                runtime_function_type(&context, &target, RuntimeAbiRole::CompatibleLaneSelection)
                    .expect("fixture `runtime_function_type` call must succeed");

            let root = runtime_function_type(&context, &target, RuntimeAbiRole::RootExecution)
                .expect("fixture `runtime_function_type` call must succeed");

            let expected_root_parameters = match native {
                NativeTarget::X86_64WindowsMsvc => vec![
                    context.ptr_type(inkwell::AddressSpace::default()).into(),
                    context.i64_type().into(),
                    context.ptr_type(inkwell::AddressSpace::default()).into(),
                ],
                NativeTarget::X86_64LinuxGnu | NativeTarget::X86_64MacOs => {
                    vec![context.i64_type().into(); 3]
                }
                NativeTarget::Aarch64LinuxGnu
                | NativeTarget::Aarch64WindowsMsvc
                | NativeTarget::Aarch64MacOs => vec![
                    context.i64_type().into(),
                    context.i64_type().array_type(2).into(),
                ],
            };

            assert_eq!(progress.get_return_type(), None, "{native:?}");

            assert_eq!(
                lane.get_return_type(),
                Some(context.i64_type().into()),
                "{native:?}"
            );

            assert_eq!(
                root.get_param_types(),
                expected_root_parameters,
                "{native:?}"
            );
        }
    }

    #[test]
    fn every_native_call_accepts_and_returns_logical_values_on_every_target() {
        let context = Context::create();

        for native in NativeTarget::ALL {
            let target = CodegenTarget::for_native(native);
            let module = context.create_module("runtime.calls");

            let wrapper =
                module.add_function("wrapper", context.void_type().fn_type(&[], false), None);

            let builder = context.create_builder();

            builder.position_at_end(context.append_basic_block(wrapper, "entry"));

            for role in RuntimeAbiRole::ALL {
                let Some(signature) = role.native_signature() else {
                    continue;
                };

                let function = declare_runtime_function(&module, &context, &target, role)
                    .expect("fixture `declare_runtime_function` call must succeed");

                let arguments = signature
                    .parameters()
                    .iter()
                    .map(|kind| {
                        runtime_value_type(&context, &target, *kind)
                            .expect("fixture `runtime_value_type` call must succeed")
                            .const_zero()
                            .into()
                    })
                    .collect::<Vec<_>>();

                let value = invoke_runtime(
                    &context,
                    &builder,
                    &target,
                    role,
                    function,
                    &arguments,
                    role.as_str(),
                )
                .expect("fixture `invoke_runtime` call must succeed");

                assert_eq!(
                    value.map(|value| value.get_type()),
                    runtime_value_type(&context, &target, signature.result()),
                    "{native:?} {role:?}"
                );
            }

            builder
                .build_return(None)
                .expect("fixture `build_return` call must succeed");

            module
                .verify()
                .unwrap_or_else(|error| panic!("{native:?}: {error}"));

            let extends_bytes = matches!(
                native,
                NativeTarget::X86_64LinuxGnu
                    | NativeTarget::X86_64MacOs
                    | NativeTarget::Aarch64MacOs
            );

            let ir = module.print_to_string().to_string();

            for role in RuntimeAbiRole::ALL {
                let Some(signature) = role.native_signature() else {
                    continue;
                };

                let symbol = format!(
                    "@{}(",
                    role.native_symbol()
                        .expect("fixture runtime role must have a native symbol")
                );

                if native == NativeTarget::X86_64WindowsMsvc {
                    let name = format!("%{}.argument", role.as_str());

                    let arguments = ir
                        .lines()
                        .filter(|line| {
                            line.trim_start().starts_with(&name) && line.contains(" = alloca ")
                        })
                        .collect::<Vec<_>>();

                    let expected = signature
                        .parameters()
                        .iter()
                        .filter(|kind| super::parameter_is_indirect(&target, **kind))
                        .count();

                    assert_eq!(arguments.len(), expected, "{native:?} {role:?}");

                    assert!(
                        arguments.iter().all(|line| line.ends_with("align 16")),
                        "{native:?} {role:?}: {arguments:?}"
                    );
                }

                let byte_count = signature
                    .parameters()
                    .iter()
                    .filter(|kind| **kind == bray_runtime_interface::RuntimeAbiType::U8)
                    .count()
                    + usize::from(signature.result() == bray_runtime_interface::RuntimeAbiType::U8);

                let expected = if extends_bytes { byte_count } else { 0 };

                let declaration = ir
                    .lines()
                    .find(|line| line.starts_with("declare ") && line.contains(&symbol))
                    .expect("fixture lookup must find its expected value");

                let call = ir
                    .lines()
                    .find(|line| line.contains(" call ") && line.contains(&symbol))
                    .expect("fixture lookup must find its expected value");

                assert_eq!(
                    declaration.matches("zeroext").count(),
                    expected,
                    "{native:?} {role:?} declaration"
                );

                assert_eq!(
                    call.matches("zeroext").count(),
                    expected,
                    "{native:?} {role:?} call"
                );

                let indirect_values = if matches!(
                    native,
                    NativeTarget::X86_64LinuxGnu | NativeTarget::X86_64MacOs
                ) {
                    signature
                        .parameters()
                        .iter()
                        .filter(|kind| {
                            matches!(
                                kind,
                                bray_runtime_interface::RuntimeAbiType::InactiveFrame
                                    | bray_runtime_interface::RuntimeAbiType::PanicReport
                            )
                        })
                        .count()
                } else {
                    0
                };

                for (kind, instruction) in [("declaration", declaration), ("call", call)] {
                    assert_eq!(
                        instruction.matches("byval(").count(),
                        indirect_values,
                        "{native:?} {role:?} {kind}"
                    );
                }
            }
        }
    }
}
