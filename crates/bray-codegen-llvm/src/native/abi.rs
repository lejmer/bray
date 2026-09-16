use bray_codegen::{CodegenFailure, CodegenTarget};
use bray_runtime_interface::{RuntimeAbiRole, RuntimeAbiType};
use bray_target::{ObjectFormat, TargetArchitecture};
use inkwell::AddressSpace;
use inkwell::attributes::{Attribute, AttributeLoc};
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum, FunctionType};
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

    Some(register_type(context, target, signature.result(), false)?.fn_type(&parameters, false))
}

pub(crate) fn runtime_attributes(
    context: &Context,
    target: &CodegenTarget,
    role: RuntimeAbiRole,
) -> Result<Vec<(AttributeLoc, Attribute)>, CodegenFailure> {
    let mut attributes = Vec::new();

    if let Some(result) = runtime_indirect_result_type(context, target, role) {
        attributes.push((
            AttributeLoc::Param(0),
            indirect_result_attribute(context, result)?,
        ));
    }

    let machine = target.machine();

    let extends_bytes = machine.object_format() == ObjectFormat::MachO
        || machine.architecture() == TargetArchitecture::X86_64
            && machine.object_format() == ObjectFormat::Elf;

    if !extends_bytes {
        return Ok(attributes);
    }

    let signature = runtime_function_type(context, target, role).unwrap_or_else(|| {
        panic!("runtime attributes require a native ABI role, got {role:?}")
    });

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
    if kind == RuntimeAbiType::PanicReport
        || uses_microsoft_x64_abi(target) && aggregate_is_indirect(kind)
    {
        return Some(context.ptr_type(AddressSpace::default()).into());
    }

    register_type(context, target, kind, true)
}

fn expands_parameter(target: &CodegenTarget, kind: RuntimeAbiType) -> bool {
    target.machine().architecture() == TargetArchitecture::X86_64
        && !uses_microsoft_x64_abi(target)
        && aggregate_is_indirect(kind)
}

fn register_type<'context>(
    context: &'context Context,
    target: &CodegenTarget,
    kind: RuntimeAbiType,
    parameter: bool,
) -> Option<BasicTypeEnum<'context>> {
    if kind == RuntimeAbiType::LaneResult {
        return Some(context.i64_type().into());
    }

    if target.machine().architecture() == TargetArchitecture::Aarch64 && aggregate_is_indirect(kind)
    {
        // AAPCS64 returns integer records in x0/x1. Pointer parameters retain pointer types.
        return Some(if parameter && kind == RuntimeAbiType::InactiveFrame {
            context
                .ptr_type(AddressSpace::default())
                .array_type(2)
                .into()
        } else {
            context.i64_type().array_type(2).into()
        });
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

        if kind == RuntimeAbiType::PanicReport && value.is_pointer_value() {
            native_arguments.push(value.into());
        } else if kind == RuntimeAbiType::PanicReport
            || uses_microsoft_x64_abi(target) && aggregate_is_indirect(kind)
        {
            let storage =
                crate::translation::allocate_temporary(context, builder, value.get_type(), name)?;

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

#[cfg(test)]
mod tests {
    use super::{invoke_runtime, runtime_function_type};
    use crate::native::core::{declare_runtime_function, runtime_value_type};
    use bray_codegen::CodegenTarget;
    use bray_runtime_interface::RuntimeAbiRole;
    use bray_target::NativeTarget;
    use inkwell::context::Context;

    #[test]
    fn wrong_native_argument_count_exposes_the_role() {
        let context = Context::create();
        let target = CodegenTarget::for_native(NativeTarget::X86_64WindowsMsvc);
        let module = context.create_module("bad.runtime.call");
        let builder = context.create_builder();
        let wrapper = module.add_function("wrapper", context.void_type().fn_type(&[], false), None);

        builder.position_at_end(context.append_basic_block(wrapper, "entry"));

        let role = RuntimeAbiRole::CurrentNativeThreadIdentity;
        let function = declare_runtime_function(&module, &context, &target, role).unwrap();

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

        let message = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .expect("panic payload must be text");

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

                    return_frame_state(&context, &builder, 1, 2).unwrap();

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
                    .unwrap();

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
                }

                let result = invoke_function(
                    &context, &builder, &target, &key, function, &arguments, "callback",
                )
                .unwrap()
                .unwrap();

                assert_eq!(
                    result.get_type(),
                    logical.into(),
                    "{native:?} {operation:?}"
                );

                builder.build_return(None).unwrap();
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
                    .unwrap();

            let lane =
                runtime_function_type(&context, &target, RuntimeAbiRole::CompatibleLaneSelection)
                    .unwrap();

            let root =
                runtime_function_type(&context, &target, RuntimeAbiRole::RootExecution).unwrap();

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

                let function = declare_runtime_function(&module, &context, &target, role).unwrap();

                let arguments = signature
                    .parameters()
                    .iter()
                    .map(|kind| {
                        runtime_value_type(&context, &target, *kind)
                            .unwrap()
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
                .unwrap();

                assert_eq!(
                    value.map(|value| value.get_type()),
                    runtime_value_type(&context, &target, signature.result()),
                    "{native:?} {role:?}"
                );
            }

            builder.build_return(None).unwrap();

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

                let symbol = format!("@{}(", role.native_symbol().unwrap());

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
                    .unwrap();

                let call = ir
                    .lines()
                    .find(|line| line.contains(" call ") && line.contains(&symbol))
                    .unwrap();

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
            }
        }
    }
}
