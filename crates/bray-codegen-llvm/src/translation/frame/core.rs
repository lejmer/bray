use crate::mapping::{LlvmDebugInfo, LlvmTypeMappings};
use crate::native::frame_operation_function;
use crate::translation::unit::{UnitTranslator, pointer_value};
use bray_codegen::{
    CodegenFailure, CodegenInstance, CodegenParameterMapping, CodegenRequest, CodegenResultMapping,
    CodegenSymbolKey,
};
use bray_ir::{MirStorageId, MirStorageKind};
use bray_runtime_interface::ProtectedFrameOperation;
use inkwell::AddressSpace;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::StructType;
use inkwell::values::{BasicValueEnum, FunctionValue, PointerValue};

use super::{cleanup::translate_action_callbacks, support::frame_storage_field_index};

pub(crate) fn translate_protected_instance<'context, 'request>(
    context: &'context Context,
    module: &Module<'context>,
    request: CodegenRequest<'request>,
    instance: &'request CodegenInstance,
    types: &mut LlvmTypeMappings<'context, 'request>,
    debug: Option<&LlvmDebugInfo<'context>>,
) -> Result<(), CodegenFailure> {
    let descriptor = instance
        .mir()
        .frame_descriptor()
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let context_type = frame_context_type(context, instance, types)?;

    translate_constructor(module, request, instance, context_type, types)?;
    translate_frame_adapter(module, request, instance, context_type, types)?;
    translate_state_callback(context, module, request, instance)?;
    translate_cancellation_entry(module, request, instance, context_type, types)?;
    translate_action_callbacks(module, request, instance, context_type, types)?;

    let resume =
        frame_operation_function(module, request, instance, ProtectedFrameOperation::Resume)?;

    let source = instance
        .mir()
        .blocks()
        .first()
        .map(bray_ir::MirBlock::source)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let debug_scope = debug.and_then(|debug| {
        let linkage_name = resume.get_name().to_str().ok()?;

        debug.attach_function(resume, "frame.resume", linkage_name, source)
    });

    UnitTranslator::for_frame_resume(
        context,
        module,
        request,
        instance,
        resume,
        context_type,
        types,
        debug,
        debug_scope,
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

    let mut fields = Vec::with_capacity(instance.mir().storages().len() + 3);

    fields.push(context.i32_type().into());
    fields.push(types.map(descriptor.result_type())?);
    fields.push(context.i8_type().into());

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
        types,
    )?;

    let adapter = frame_operation_function(
        module,
        request,
        instance,
        ProtectedFrameOperation::MoveBeforeStart,
    )?
    .as_global_value()
    .as_pointer_value();

    let result_type = match symbol.signature().result() {
        CodegenResultMapping::Direct { ty, .. }
        | CodegenResultMapping::Indirect { pointee: ty, .. } => types.map(*ty)?,
        CodegenResultMapping::Void => {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }
    };

    let result_type = match result_type {
        inkwell::types::BasicTypeEnum::StructType(ty) => Some(ty),
        _ => None,
    }
    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let inactive = result_type.const_zero();

    let inactive = builder
        .build_insert_value(inactive, storage, 0, "frame.inactive.context")
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?
        .into_struct_value();

    let inactive = builder
        .build_insert_value(inactive, adapter, 1, "frame.inactive.adapter")
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?
        .into_struct_value();

    match symbol.signature().result() {
        CodegenResultMapping::Direct { .. } => {
            builder
                .build_return(Some(&inactive))
                .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;
        }
        CodegenResultMapping::Indirect { .. } => {
            let destination = function
                .get_first_param()
                .and_then(pointer_value)
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            builder
                .build_store(destination, inactive)
                .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

            builder
                .build_return(None)
                .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;
        }
        CodegenResultMapping::Void => {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }
    }

    Ok(())
}

fn initialize_parameters(
    builder: &inkwell::builder::Builder<'_>,
    function: FunctionValue<'_>,
    signature: &bray_codegen::CodegenCallableSignature,
    instance: &CodegenInstance,
    context_type: StructType<'_>,
    context: PointerValue<'_>,
    types: &mut LlvmTypeMappings<'_, '_>,
) -> Result<(), CodegenFailure> {
    let storages = instance
        .mir()
        .storages_with_ids()
        .filter_map(|(id, storage)| match storage.kind() {
            MirStorageKind::Parameter(position) => Some((position, id)),
            _ => None,
        })
        .collect::<std::collections::BTreeMap<_, _>>();

    let mut parameter_index = u32::from(matches!(
        signature.result(),
        CodegenResultMapping::Indirect { .. }
    ));

    for (position, mapping) in signature.parameters().iter().enumerate() {
        let storage = u32::try_from(position)
            .ok()
            .and_then(|position| storages.get(&position).copied());

        match mapping {
            CodegenParameterMapping::Ignore => {}
            CodegenParameterMapping::Direct { .. } => {
                let value = function
                    .get_nth_param(parameter_index)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                store_frame_parameter(builder, context_type, context, storage, value)?;

                parameter_index += 1;
            }
            CodegenParameterMapping::Indirect { pointee, .. } => {
                let source = function
                    .get_nth_param(parameter_index)
                    .and_then(pointer_value)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let value = builder
                    .build_load(types.map(*pointee)?, source, "frame.parameter.indirect")
                    .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

                store_frame_parameter(builder, context_type, context, storage, value)?;

                parameter_index += 1;
            }
        }
    }

    Ok(())
}

fn store_frame_parameter<'context>(
    builder: &inkwell::builder::Builder<'context>,
    context_type: StructType<'context>,
    context: PointerValue<'context>,
    storage: Option<MirStorageId>,
    value: BasicValueEnum<'context>,
) -> Result<(), CodegenFailure> {
    let Some(storage) = storage else {
        return Ok(());
    };

    let destination = builder
        .build_struct_gep(
            context_type,
            context,
            frame_storage_field_index(storage)?,
            "frame.parameter",
        )
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    builder
        .build_store(destination, value)
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

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
        ProtectedFrameOperation::StateDescription,
        ProtectedFrameOperation::Resume,
        ProtectedFrameOperation::CancellationEntry,
        ProtectedFrameOperation::TaskBroadcast,
        ProtectedFrameOperation::LifecycleResolution,
        ProtectedFrameOperation::CompletionMove,
        ProtectedFrameOperation::Destruction,
    ]
    .map(|operation| {
        frame_operation_function(module, request, instance, operation)
            .map(|function| function.as_global_value().as_pointer_value())
    });

    let [
        state,
        resume,
        cancel,
        broadcast,
        resolve,
        move_completion,
        destroy,
    ] = callbacks;

    let usize = crate::native::pointer_integer_type(context, request.target());

    let fields: [BasicValueEnum<'context>; 14] = [
        function
            .get_nth_param(crate::native::frame_parameter_index(
                request.target(),
                ProtectedFrameOperation::MoveBeforeStart,
                0,
            ))
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
        cancel?.into(),
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

    crate::native::return_frame_result(
        &builder,
        function,
        request.target(),
        ProtectedFrameOperation::MoveBeforeStart,
        frame.into(),
    )?;

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
        ProtectedFrameOperation::StateDescription,
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

    for state in descriptor.states() {
        let block = context.append_basic_block(function, "frame.state.known");

        let lane_requirements =
            state
                .lane_requirements()
                .iter()
                .fold(0_u64, |requirements, lane| {
                    requirements
                        | match lane {
                            bray_runtime_interface::ExecutionLaneRequirement::Blocking => 1,
                            bray_runtime_interface::ExecutionLaneRequirement::Compute => 2,
                            bray_runtime_interface::ExecutionLaneRequirement::MainThread => 4,
                        }
                });

        let affinity = if state
            .lane_requirements()
            .contains(&bray_runtime_interface::ExecutionLaneRequirement::MainThread)
        {
            bray_runtime_interface::ProtectedFrameAffinity::MainThread.code()
        } else {
            state.affinity().code()
        };

        builder.position_at_end(block);

        crate::native::return_frame_state(
            context,
            &builder,
            request.target(),
            u64::from(affinity),
            lane_requirements,
        )?;

        cases.push((
            context
                .i32_type()
                .const_int(u64::from(state.state().raw()), false),
            block,
        ));
    }

    builder.position_at_end(dispatch);

    builder
        .build_switch(state, invalid, &cases)
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    builder.position_at_end(invalid);

    crate::native::return_frame_state(context, &builder, request.target(), 0, 0)?;

    Ok(())
}

fn translate_cancellation_entry<'context>(
    module: &Module<'context>,
    request: CodegenRequest<'_>,
    instance: &CodegenInstance,
    context_type: StructType<'context>,
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<(), CodegenFailure> {
    let function = frame_operation_function(
        module,
        request,
        instance,
        ProtectedFrameOperation::CancellationEntry,
    )?;

    let resume =
        frame_operation_function(module, request, instance, ProtectedFrameOperation::Resume)?;

    let builder = types.context().create_builder();

    let block = types.context().append_basic_block(function, "frame.cancel");

    builder.position_at_end(block);

    let parameter = crate::native::frame_parameter_index(
        request.target(),
        ProtectedFrameOperation::CancellationEntry,
        0,
    );

    let context = integer_pointer(&builder, function, parameter, types)?;

    let requested = builder
        .build_struct_gep(context_type, context, 2, "frame.cancellation.pointer")
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    builder
        .build_store(requested, types.context().i8_type().const_int(1, false))
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    let context = function
        .get_nth_param(parameter)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let frame = instance
        .protected_frame_identity()
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let progress = crate::native::invoke_function(
        types.context(),
        &builder,
        request.target(),
        &CodegenSymbolKey::ProtectedFrame {
            frame,
            operation: ProtectedFrameOperation::Resume,
        },
        resume,
        &[context.into()],
        "frame.cancel.progress",
    )?
    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    crate::native::return_frame_result(
        &builder,
        function,
        request.target(),
        ProtectedFrameOperation::CancellationEntry,
        progress,
    )?;

    Ok(())
}

pub(super) fn integer_pointer<'context>(
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
