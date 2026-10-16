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
        .expect("protected-frame translation requires an established mapping or value");

    let context_type = frame_context_type(context, instance, types)?;

    let storage = request
        .mappings()
        .symbol(&CodegenSymbolKey::CleanupStorage(instance.key().clone()))
        .expect("protected frame must publish its cleanup storage descriptor");

    let identity = instance
        .protected_frame_identity()
        .expect("protected frame has a concrete instance identity");

    let value = crate::mapping::cleanup::descriptor_value(
        context,
        request.target(),
        identity.digest(),
        types.target_data().get_store_size(&context_type),
        types.target_data().get_abi_alignment(&context_type),
    );

    module
        .get_global(storage.name().as_str())
        .expect("cleanup descriptor must have been declared")
        .set_initializer(&value);

    crate::mapping::frame::initialize_factory(context, module, request.mappings(), instance.key());

    let activation = super::description::declare(module, request, instance, context_type, types)?;

    translate_constructor(
        module,
        request,
        instance,
        context_type,
        activation.as_pointer_value(),
        types,
    )?;

    translate_frame_adapter(module, request, instance, context_type, types)?;
    super::description::translate_state_callback(context, module, request, instance, context_type)?;
    translate_cancellation_entry(module, request, instance, context_type, types)?;
    translate_action_callbacks(module, request, instance, context_type, types)?;

    let resume =
        frame_operation_function(module, request, instance, ProtectedFrameOperation::Resume);

    let source = instance
        .mir()
        .blocks()
        .first()
        .map(bray_ir::MirBlock::source)
        .expect("protected-frame translation requires an established mapping or value");

    let debug_scope = debug.and_then(|debug| {
        let linkage_name = resume.get_name().to_str().ok()?;

        debug.attach_function(resume, "frame.resume", linkage_name, source)
    });

    UnitTranslator::for_frame_operation(
        context,
        module,
        request,
        instance,
        resume,
        context_type,
        ProtectedFrameOperation::Resume,
        types,
        debug,
        debug_scope,
    )?
    .translate()?;

    let broadcast = frame_operation_function(
        module,
        request,
        instance,
        ProtectedFrameOperation::TaskBroadcast,
    );

    UnitTranslator::for_frame_operation(
        context,
        module,
        request,
        instance,
        broadcast,
        context_type,
        ProtectedFrameOperation::TaskBroadcast,
        types,
        None,
        None,
    )?
    .translate()?;

    if descriptor.states().is_empty() {
        panic!("protected-frame translation violated an established compiler contract");
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
        .expect("protected-frame translation requires an established mapping or value");

    let mut fields = Vec::with_capacity(instance.mir().storages().len() + 4);

    fields.push(context.i32_type().into());
    fields.push(types.map(descriptor.result_type())?);
    fields.push(context.i8_type().into());

    for (_, storage) in instance.mir().storages_with_ids() {
        fields.push(
            if matches!(
                storage.kind(),
                MirStorageKind::BroadcastLocal | MirStorageKind::FrameCompletion
            ) {
                // Preserve storage indexing without reserving callback scratch bytes in each frame.
                context.i8_type().array_type(0).into()
            } else {
                types.map(storage.ty())?
            },
        );
    }

    fields.push(crate::native::activation::header_type(context, types.target()).into());

    Ok(context.struct_type(&fields, false))
}

fn translate_constructor<'context>(
    module: &Module<'context>,
    request: CodegenRequest<'_>,
    instance: &CodegenInstance,
    context_type: StructType<'context>,
    activation: PointerValue<'context>,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<(), CodegenFailure> {
    let symbol = request
        .mappings()
        .instance_symbol(instance.key())
        .expect("protected-frame translation requires an established mapping or value");

    let function = module
        .get_function(symbol.name().as_str())
        .expect("protected-frame translation requires an established mapping or value");

    let context = types.context();
    let builder = context.create_builder();
    let block = context.append_basic_block(function, "frame.create");

    builder.position_at_end(block);

    let storage = super::storage::claim(
        module,
        request,
        instance,
        context_type,
        activation,
        &builder,
        types,
    )?;

    initialize_parameters(
        &builder,
        function,
        symbol.signature(),
        instance,
        context_type,
        storage,
        types,
    )?;

    super::storage::publish(
        module,
        request,
        context_type,
        storage,
        activation,
        &builder,
        types,
    )?;

    let adapter = frame_operation_function(
        module,
        request,
        instance,
        ProtectedFrameOperation::MoveBeforeStart,
    )
    .as_global_value()
    .as_pointer_value();

    let result_type = match symbol.signature().result() {
        CodegenResultMapping::Direct { ty, .. }
        | CodegenResultMapping::Indirect { pointee: ty, .. } => types.map(*ty)?,
        CodegenResultMapping::Void => {
            panic!("protected-frame translation violated an established compiler contract");
        }
    };

    let result_type = match result_type {
        inkwell::types::BasicTypeEnum::StructType(ty) => Some(ty),
        _ => None,
    }
    .expect("protected-frame translation requires an established mapping or value");

    let inactive = result_type.const_zero();

    let inactive = builder
        .build_insert_value(inactive, storage, 0, "frame.inactive.context")
        .map_err(CodegenFailure::backend_library)?
        .into_struct_value();

    let inactive = builder
        .build_insert_value(inactive, adapter, 1, "frame.inactive.adapter")
        .map_err(CodegenFailure::backend_library)?
        .into_struct_value();

    match symbol.signature().result() {
        CodegenResultMapping::Direct { .. } => {
            builder
                .build_return(Some(&inactive))
                .map_err(CodegenFailure::backend_library)?;
        }
        CodegenResultMapping::Indirect { .. } => {
            let destination = function
                .get_first_param()
                .and_then(pointer_value)
                .expect("protected-frame translation requires an established mapping or value");

            builder
                .build_store(destination, inactive)
                .map_err(CodegenFailure::backend_library)?;

            builder
                .build_return(None)
                .map_err(CodegenFailure::backend_library)?;
        }
        CodegenResultMapping::Void => {
            panic!("protected-frame translation violated an established compiler contract");
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
                    .expect("protected-frame translation requires an established mapping or value");

                store_frame_parameter(builder, context_type, context, storage, value)?;

                parameter_index += 1;
            }
            CodegenParameterMapping::Indirect { pointee, .. } => {
                let source = function
                    .get_nth_param(parameter_index)
                    .and_then(pointer_value)
                    .expect("protected-frame translation requires an established mapping or value");

                let value = builder
                    .build_load(types.map(*pointee)?, source, "frame.parameter.indirect")
                    .map_err(CodegenFailure::backend_library)?;

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
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_store(destination, value)
        .map_err(CodegenFailure::backend_library)?;

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
    );

    let descriptor = instance
        .mir()
        .frame_descriptor()
        .expect("protected-frame translation requires an established mapping or value");

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

    let identity = crate::native::identity_value(
        context,
        instance
            .protected_frame_identity()
            .expect("protected frame has a concrete instance identity")
            .digest(),
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
            .as_global_value()
            .as_pointer_value()
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
            .expect("protected-frame translation requires an established mapping or value"),
        identity.into(),
        context
            .i32_type()
            .const_int(
                crate::conversion::resource_limit(descriptor.states().len(), "frame_state_count")?,
                false,
            )
            .into(),
        usize.const_int(frame_layout.0, false).into(),
        usize.const_int(u64::from(frame_layout.1), false).into(),
        usize.const_int(completion_layout.0, false).into(),
        usize
            .const_int(u64::from(completion_layout.1), false)
            .into(),
        state.into(),
        resume.into(),
        cancel.into(),
        broadcast.into(),
        resolve.into(),
        move_completion.into(),
        destroy.into(),
    ];

    let mut frame = crate::native::protected_frame_type(context, request.target()).get_undef();

    for (index, field) in fields.into_iter().enumerate() {
        frame = builder
            .build_insert_value(
                frame,
                field,
                crate::conversion::resource_limit(index, "frame_adapter_field_index")?,
                "frame.adapter.field",
            )
            .map_err(CodegenFailure::backend_library)?
            .into_struct_value();
    }

    crate::native::return_frame_result(
        types.context(),
        &builder,
        function,
        request.target(),
        ProtectedFrameOperation::MoveBeforeStart,
        frame.into(),
    )?;

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
    );

    let resume =
        frame_operation_function(module, request, instance, ProtectedFrameOperation::Resume);

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
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_store(requested, types.context().i8_type().const_int(1, false))
        .map_err(CodegenFailure::backend_library)?;

    let context = function
        .get_nth_param(parameter)
        .expect("protected-frame translation requires an established mapping or value");

    let frame = instance
        .protected_frame_identity()
        .expect("protected-frame translation requires an established mapping or value");

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
    .expect("protected-frame translation requires an established mapping or value");

    crate::native::return_frame_result(
        types.context(),
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
        .expect("protected-frame translation requires an established mapping or value");

    builder
        .build_int_to_ptr(
            integer,
            types.context().ptr_type(AddressSpace::default()),
            "frame.pointer",
        )
        .map_err(CodegenFailure::backend_library)
}
