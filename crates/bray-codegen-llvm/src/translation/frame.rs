use crate::mapping::LlvmTypeMappings;
use crate::translation::unit::UnitTranslator;
use bray_codegen::{
    CodegenFailure, CodegenInstance, CodegenParameterMapping, CodegenRequest,
    CodegenResultMapping, CodegenSymbolKey,
};
use bray_ir::MirStorageKind;
use bray_runtime_interface::ProtectedFrameOperation;
use inkwell::AddressSpace;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::StructType;
use inkwell::values::{BasicValueEnum, FunctionValue, PointerValue};

pub(crate) fn translate_protected_instance<'context, 'module, 'request>(
    context: &'context Context,
    module: &'module Module<'context>,
    request: CodegenRequest<'request>,
    instance: &'request CodegenInstance,
    types: &mut LlvmTypeMappings<'context, 'request>,
) -> Result<(), CodegenFailure> {
    if request.target().machine().architecture()
        != bray_target::TargetArchitecture::X86_64
        || request.target().machine().object_format()
            != bray_target::ObjectFormat::Elf
    {
        return Err(CodegenFailure::UnsupportedTarget);
    }

    let descriptor = instance
        .mir()
        .frame_descriptor()
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let context_type = frame_context_type(context, instance, types)?;

    translate_constructor(module, request, instance, context_type, types)?;
    translate_frame_adapter(module, request, instance, context_type, types)?;
    translate_state_callback(context, module, request, instance)?;
    translate_action_callbacks(module, request, instance, context_type, types)?;

    let resume = frame_operation_function(
        module,
        request,
        instance,
        ProtectedFrameOperation::Resume,
    )?;

    UnitTranslator::for_frame_resume(
        context,
        module,
        request,
        instance,
        resume,
        context_type,
        types,
    )?
    .translate()?;

    if descriptor.states().is_empty() {
        return Err(CodegenFailure::GeneratedModuleInvariant);
    }

    Ok(())
}

fn frame_context_type<'context>(
    context: &'context Context,
    instance: &CodegenInstance,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<StructType<'context>, CodegenFailure> {
    let descriptor = instance
        .mir()
        .frame_descriptor()
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let mut fields = Vec::with_capacity(instance.mir().storages().len() + 2);
    fields.push(context.i32_type().into());
    fields.push(types.map(descriptor.result_type())?);

    for (_, storage) in instance.mir().storages_with_ids() {
        fields.push(types.map(storage.ty())?);
    }

    Ok(context.struct_type(&fields, false))
}

fn translate_constructor<'context>(
    module: &Module<'context>,
    request: CodegenRequest<'_>,
    instance: &CodegenInstance,
    context_type: StructType<'context>,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<(), CodegenFailure> {
    let symbol = request
        .mappings()
        .instance_symbol(instance.key())
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let function = module
        .get_function(symbol.name().as_str())
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let CodegenResultMapping::Direct { .. } = symbol.signature().result() else {
        return Err(CodegenFailure::GeneratedModuleInvariant);
    };

    let context = types.context();
    let builder = context.create_builder();
    let block = context.append_basic_block(function, "frame.create");
    builder.position_at_end(block);

    let pointer = context.ptr_type(AddressSpace::default());
    let usize = crate::native::pointer_integer_type(types.context(), request.target());
    let calloc_type = pointer.fn_type(&[usize.into(), usize.into()], false);

    let calloc = module
        .get_function("calloc")
        .unwrap_or_else(|| module.add_function("calloc", calloc_type, None));

    let size = types.target_data().get_store_size(&context_type);

    let call = builder
        .build_call(
            calloc,
            &[
                usize.const_int(1, false).into(),
                usize.const_int(size, false).into(),
            ],
            "frame.storage",
        )
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    let storage = call
        .try_as_basic_value()
        .basic()
        .and_then(pointer_value)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    initialize_parameters(
        &builder,
        function,
        symbol.signature(),
        instance,
        context_type,
        storage,
    )?;

    builder
        .build_return(Some(&storage))
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    Ok(())
}

fn initialize_parameters(
    builder: &inkwell::builder::Builder<'_>,
    function: FunctionValue<'_>,
    signature: &bray_codegen::CodegenCallableSignature,
    instance: &CodegenInstance,
    context_type: StructType<'_>,
    context: PointerValue<'_>,
) -> Result<(), CodegenFailure> {
    let storages = instance
        .mir()
        .storages_with_ids()
        .filter(|(_, storage)| storage.kind() == MirStorageKind::Parameter);

    let mut parameter_index = 0_u32;

    for ((storage, _), mapping) in storages.zip(signature.parameters()) {
        let destination = builder
            .build_struct_gep(
                context_type,
                context,
                storage
                    .slot()
                    .checked_add(2)
                    .and_then(|index| u32::try_from(index).ok())
                    .ok_or(CodegenFailure::ResourceExhausted)?,
                "frame.parameter",
            )
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

        match mapping {
            CodegenParameterMapping::Ignore => {}
            CodegenParameterMapping::Direct { .. } => {
                let value = function
                    .get_nth_param(parameter_index)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                builder
                    .build_store(destination, value)
                    .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

                parameter_index += 1;
            }
            CodegenParameterMapping::Indirect { .. } => {
                return Err(CodegenFailure::GeneratedModuleInvariant);
            }
        }
    }

    Ok(())
}

fn translate_frame_adapter<'context>(
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
        ProtectedFrameOperation::MoveBeforeStart,
    )?;

    let descriptor = instance
        .mir()
        .frame_descriptor()
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let context = types.context();
    let builder = context.create_builder();
    let block = context.append_basic_block(function, "frame.adapter");
    builder.position_at_end(block);

    let completion = types.map(descriptor.result_type())?;

    let frame_layout = (
        types.target_data().get_store_size(&context_type),
        types.target_data().get_abi_alignment(&context_type),
    );

    let completion_layout = (
        types.target_data().get_store_size(&completion),
        types.target_data().get_abi_alignment(&completion),
    );

    let identity = context.i8_type().const_array(
        &descriptor
            .frame()
            .digest()
            .iter()
            .map(|byte| context.i8_type().const_int(u64::from(*byte), false))
            .collect::<Vec<_>>(),
    );

    let callbacks = [
        ProtectedFrameOperation::CancellationEntry,
        ProtectedFrameOperation::Resume,
        ProtectedFrameOperation::TaskBroadcast,
        ProtectedFrameOperation::LifecycleResolution,
        ProtectedFrameOperation::CompletionMove,
        ProtectedFrameOperation::Destruction,
    ]
    .map(|operation| {
        frame_operation_function(module, request, instance, operation)
            .map(|function| function.as_global_value().as_pointer_value())
    });

    let [state, resume, broadcast, resolve, move_completion, destroy] = callbacks;

    let usize = crate::native::pointer_integer_type(context, request.target());

    let fields: [BasicValueEnum<'context>; 13] = [
        function
            .get_first_param()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?,
        identity.into(),
        context
            .i32_type()
            .const_int(
                u64::try_from(descriptor.states().len())
                    .map_err(|_| CodegenFailure::ResourceExhausted)?,
                false,
            )
            .into(),
        usize.const_int(frame_layout.0, false).into(),
        usize.const_int(u64::from(frame_layout.1), false).into(),
        usize.const_int(completion_layout.0, false).into(),
        usize
            .const_int(u64::from(completion_layout.1), false)
            .into(),
        state?.into(),
        resume?.into(),
        broadcast?.into(),
        resolve?.into(),
        move_completion?.into(),
        destroy?.into(),
    ];

    let mut frame = crate::native::protected_frame_type(context, request.target()).get_undef();

    for (index, field) in fields.into_iter().enumerate() {
        frame = builder
            .build_insert_value(
                frame,
                field,
                u32::try_from(index).map_err(|_| CodegenFailure::ResourceExhausted)?,
                "frame.adapter.field",
            )
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?
            .into_struct_value();
    }

    builder
        .build_return(Some(&frame))
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    Ok(())
}

fn translate_state_callback(
    context: &Context,
    module: &Module<'_>,
    request: CodegenRequest<'_>,
    instance: &CodegenInstance,
) -> Result<(), CodegenFailure> {
    let function = frame_operation_function(
        module,
        request,
        instance,
        ProtectedFrameOperation::CancellationEntry,
    )?;

    let descriptor = instance
        .mir()
        .frame_descriptor()
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let state = function
        .get_nth_param(1)
        .and_then(|value| match value {
            BasicValueEnum::IntValue(value) => Some(value),
            _ => None,
        })
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let builder = context.create_builder();
    let dispatch = context.append_basic_block(function, "frame.state");
    let invalid = context.append_basic_block(function, "frame.state.invalid");
    let mut cases = Vec::with_capacity(descriptor.states().len());

    for facts in descriptor.states() {
        let block = context.append_basic_block(function, "frame.state.known");

        let lane_requirements = facts.lane_requirements().iter().fold(
            0_u64,
            |requirements, lane| {
                requirements
                    | match lane {
                        bray_runtime_interface::ExecutionLaneRequirement::Blocking => 1,
                        bray_runtime_interface::ExecutionLaneRequirement::Compute => 2,
                        bray_runtime_interface::ExecutionLaneRequirement::MainThread => 4,
                    }
            },
        );

        let affinity =
            if facts.lane_requirements().contains(
                &bray_runtime_interface::ExecutionLaneRequirement::MainThread,
            ) {
                2
            } else {
                0
            };

        builder.position_at_end(block);

        builder
            .build_return(Some(
                &crate::native::frame_state_type(context).const_named_struct(&[
                    context.i32_type().const_int(affinity, false).into(),
                    context
                        .i32_type()
                        .const_int(lane_requirements, false)
                        .into(),
                ]),
            ))
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

        cases.push((
            context
                .i32_type()
                .const_int(u64::from(facts.state().raw()), false),
            block,
        ));
    }

    builder.position_at_end(dispatch);

    builder
        .build_switch(state, invalid, &cases)
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    builder.position_at_end(invalid);

    builder
        .build_return(Some(
            &crate::native::frame_state_type(context).const_zero(),
        ))
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    Ok(())
}

fn translate_action_callbacks<'context>(
    module: &Module<'context>,
    request: CodegenRequest<'_>,
    instance: &CodegenInstance,
    context_type: StructType<'context>,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<(), CodegenFailure> {
    for operation in [
        ProtectedFrameOperation::TaskBroadcast,
        ProtectedFrameOperation::LifecycleResolution,
    ] {
        let function = frame_operation_function(module, request, instance, operation)?;

        let block = module
            .get_context()
            .append_basic_block(function, operation.as_str());

        let builder = module.get_context().create_builder();
        builder.position_at_end(block);

        builder
            .build_return(None)
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;
    }

    translate_completion_move(module, request, instance, context_type, types)?;
    translate_destruction(module, request, instance)?;

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
    )?;

    let descriptor = instance
        .mir()
        .frame_descriptor()
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let builder = types.context().create_builder();

    let block = types
        .context()
        .append_basic_block(function, "frame.completion.move");

    builder.position_at_end(block);

    let context = integer_pointer(&builder, function, 0, types)?;
    let destination = integer_pointer(&builder, function, 1, types)?;

    let source = builder
        .build_struct_gep(context_type, context, 1, "frame.completion")
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    let completion = builder
        .build_load(
            types.map(descriptor.result_type())?,
            source,
            "frame.completion.value",
        )
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    builder
        .build_store(destination, completion)
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    builder
        .build_return(None)
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

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
    )?;

    let context = module.get_context();
    let builder = context.create_builder();
    let block = context.append_basic_block(function, "frame.destroy");

    builder.position_at_end(block);

    let pointer = context.ptr_type(AddressSpace::default());

    let free = module
        .get_function("free")
        .unwrap_or_else(|| {
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
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let storage = builder
        .build_int_to_ptr(storage, pointer, "frame.storage")
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    builder
        .build_call(free, &[storage.into()], "")
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    builder
        .build_return(None)
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    Ok(())
}

fn integer_pointer<'context>(
    builder: &inkwell::builder::Builder<'context>,
    function: FunctionValue<'context>,
    index: u32,
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<PointerValue<'context>, CodegenFailure> {
    let integer = function
        .get_nth_param(index)
        .and_then(|value| match value {
            BasicValueEnum::IntValue(value) => Some(value),
            _ => None,
        })
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    builder
        .build_int_to_ptr(
            integer,
            types.context().ptr_type(AddressSpace::default()),
            "frame.pointer",
        )
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)
}

fn frame_operation_function<'context>(
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

fn pointer_value(value: BasicValueEnum<'_>) -> Option<PointerValue<'_>> {
    match value {
        BasicValueEnum::PointerValue(value) => Some(value),
        _ => None,
    }
}
