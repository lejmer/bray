use bray_codegen::{CodegenFailure, CodegenInstance, CodegenRequest};
use bray_runtime_interface::ProtectedFrameOperation;
use inkwell::{
    context::Context,
    module::Module,
    types::StructType,
    values::{BasicValueEnum, GlobalValue},
};

use crate::mapping::LlvmTypeMappings;
use crate::native::frame_operation_function;

pub(super) fn declare<'context>(
    module: &Module<'context>,
    request: CodegenRequest<'_>,
    instance: &CodegenInstance,
    context_type: StructType<'context>,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<GlobalValue<'context>, CodegenFailure> {
    let context = types.context();

    let storage =
        crate::mapping::cleanup::storage_descriptor(module, request.mappings(), instance.key());

    let descriptor = instance
        .mir()
        .frame_descriptor()
        .expect("source frame retains its checked descriptor");

    let completion = types.map(descriptor.result_type())?;
    let word = crate::native::pointer_integer_type(context, request.target());

    let offsets = [context_type.count_fields() - 1, 1].map(|field| {
        types
            .target_data()
            .offset_of_element(&context_type, field)
            .expect("frame header and completion are concrete struct fields")
    });

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

    let descriptor_type = crate::native::activation::descriptor_type(context, request.target());

    let value = descriptor_type.const_named_struct(&[
        storage
            .get_initializer()
            .expect("concrete storage layout is initialized before its activation descriptor"),
        word.const_int(offsets[0], false).into(),
        word.const_int(offsets[1], false).into(),
        word.const_int(types.target_data().get_store_size(&completion), false)
            .into(),
        word.const_int(
            u64::from(types.target_data().get_abi_alignment(&completion)),
            false,
        )
        .into(),
        context
            .i32_type()
            .const_int(
                crate::conversion::resource_limit(descriptor.states().len(), "frame state count")?,
                false,
            )
            .into(),
        callbacks[0].into(),
        callbacks[1].into(),
        callbacks[2].into(),
        callbacks[3].into(),
        callbacks[4].into(),
        callbacks[5].into(),
        callbacks[6].into(),
    ]);

    let name = format!(
        "{}.activation",
        storage
            .get_name()
            .to_str()
            .expect("generated storage symbol is UTF-8")
    );

    let global = module.add_global(descriptor_type, None, &name);

    global.set_linkage(inkwell::module::Linkage::Internal);
    global.set_constant(true);
    global.set_initializer(&value);

    Ok(global)
}

pub(super) fn translate_state_callback<'context>(
    context: &'context Context,
    module: &Module<'context>,
    request: CodegenRequest<'_>,
    instance: &CodegenInstance,
) -> Result<(), CodegenFailure> {
    let function = frame_operation_function(
        module,
        request,
        instance,
        ProtectedFrameOperation::StateDescription,
    );

    let descriptor = instance
        .mir()
        .frame_descriptor()
        .expect("protected-frame translation requires an established mapping or value");

    let state = function
        .get_nth_param(1)
        .and_then(|value| match value {
            BasicValueEnum::IntValue(value) => Some(value),
            _ => None,
        })
        .expect("protected-frame translation requires an established mapping or value");

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
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(invalid);

    crate::native::call_void_intrinsic(module, context, &builder, "llvm.trap")?;

    builder
        .build_unreachable()
        .map_err(CodegenFailure::backend_library)?;

    Ok(())
}
