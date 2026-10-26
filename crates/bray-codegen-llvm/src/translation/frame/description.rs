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
    context_type: StructType<'context>,
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

    let operation = function
        .get_nth_param(2)
        .expect("state query has its callback operation")
        .into_int_value();

    let builder = context.create_builder();
    let dispatch = context.append_basic_block(function, "frame.state");
    let invalid = context.append_basic_block(function, "frame.state.invalid");
    let mut cases = Vec::with_capacity(descriptor.states().len());

    builder.position_at_end(dispatch);

    let storage = builder
        .build_int_to_ptr(
            function
                .get_first_param()
                .expect("state query has its frame context")
                .into_int_value(),
            context.ptr_type(inkwell::AddressSpace::default()),
            "frame.context",
        )
        .map_err(CodegenFailure::backend_library)?;

    let operation = callback_entry(
        context,
        &builder,
        function,
        context_type,
        storage,
        operation,
    )?;

    // Capture resolution selects its own entry after ordinary cancellation routing.
    let (state, operation) =
        capture_entry(context, &builder, function, descriptor, state, operation)?;

    for state in descriptor.states() {
        let block = context.append_basic_block(function, "frame.state.known");

        let mut operations = Vec::with_capacity(6);

        for (index, lanes) in state
            .retention()
            .operation_requirements()
            .iter()
            .map(|lanes| lanes.as_ref())
            .chain(std::iter::once(&[][..]))
            .enumerate()
        {
            let selected = context.append_basic_block(function, "frame.operation.known");

            let lane_requirements = u64::from(crate::native::activation::lane_bits(lanes));

            let affinity =
                if lanes.contains(&bray_runtime_interface::ExecutionLaneRequirement::MainThread) {
                    bray_runtime_interface::ProtectedFrameAffinity::MainThread.code()
                } else {
                    state.retention().affinity().code()
                };

            builder.position_at_end(selected);

            crate::native::return_frame_state(
                context,
                &builder,
                u64::from(affinity),
                lane_requirements,
            )?;

            // Abandonment selects cancellation before this switch. Other resolution,
            // result movement and empty backing release execute no source cleanup.
            let ordinals: &[u64] = match index {
                0 => &[0],
                1 => &[1],
                2 => &[2],
                3 => &[3, 4, 5],
                _ => unreachable!("source metadata has three checked entry kinds"),
            };

            operations.extend(
                ordinals
                    .iter()
                    .map(|ordinal| (context.i32_type().const_int(*ordinal, false), selected)),
            );
        }

        builder.position_at_end(block);

        builder
            .build_switch(operation, invalid, &operations)
            .map_err(CodegenFailure::backend_library)?;

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

fn callback_entry<'context>(
    context: &'context Context,
    builder: &inkwell::builder::Builder<'context>,
    function: inkwell::values::FunctionValue<'context>,
    context_type: StructType<'context>,
    storage: inkwell::values::PointerValue<'context>,
    operation: inkwell::values::IntValue<'context>,
) -> Result<inkwell::values::IntValue<'context>, CodegenFailure> {
    let requested =
        super::support::cancellation_requested(context, builder, context_type, storage)?;

    let resume = builder
        .build_int_compare(
            inkwell::IntPredicate::EQ,
            operation,
            context.i32_type().const_zero(),
            "frame.resuming",
        )
        .map_err(CodegenFailure::backend_library)?;

    let cancelled_resume = builder
        .build_and(resume, requested, "frame.resuming.cleanup")
        .map_err(CodegenFailure::backend_library)?;

    let argument = function
        .get_nth_param(3)
        .expect("state query has its operation argument")
        .into_int_value();

    let resolve = builder
        .build_int_compare(
            inkwell::IntPredicate::EQ,
            operation,
            context.i32_type().const_int(3, false),
            "frame.resolving",
        )
        .map_err(CodegenFailure::backend_library)?;

    let failure = builder
        .build_int_compare(
            inkwell::IntPredicate::EQ,
            argument,
            argument.get_type().const_int(3, false),
            "frame.abandoning",
        )
        .map_err(CodegenFailure::backend_library)?;

    let inactive = builder
        .build_int_compare(
            inkwell::IntPredicate::EQ,
            argument,
            argument.get_type().const_int(6, false),
            "frame.inactive.disposal",
        )
        .map_err(CodegenFailure::backend_library)?;

    let failure = builder
        .build_or(failure, inactive, "frame.abandoned")
        .map_err(CodegenFailure::backend_library)?;

    let abandon = builder
        .build_and(resolve, failure, "frame.resolve.cleanup")
        .map_err(CodegenFailure::backend_library)?;

    let cancellation = builder
        .build_or(cancelled_resume, abandon, "frame.enter.cleanup")
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_select(
            cancellation,
            context.i32_type().const_int(1, false),
            operation,
            "frame.operation",
        )
        .map_err(CodegenFailure::backend_library)
        .map(|value| value.into_int_value())
}

fn capture_entry<'context>(
    context: &'context Context,
    builder: &inkwell::builder::Builder<'context>,
    function: inkwell::values::FunctionValue<'context>,
    descriptor: &bray_ir::MirFrameDescriptor,
    state: inkwell::values::IntValue<'context>,
    operation: inkwell::values::IntValue<'context>,
) -> Result<
    (
        inkwell::values::IntValue<'context>,
        inkwell::values::IntValue<'context>,
    ),
    CodegenFailure,
> {
    let Some((quiescence, destruction)) = descriptor.capture_cleanup() else {
        return Ok((state, operation));
    };

    let argument = function
        .get_nth_param(3)
        .expect("state query retains its operation argument")
        .into_int_value();

    let resolving = builder
        .build_int_compare(
            inkwell::IntPredicate::EQ,
            operation,
            context.i32_type().const_int(3, false),
            "frame.capture.resolve",
        )
        .map_err(CodegenFailure::backend_library)?;

    let mut selected = state;
    let mut selected_operation = operation;

    for (exit, entry) in [(4, quiescence), (5, destruction)] {
        let requested = builder
            .build_int_compare(
                inkwell::IntPredicate::EQ,
                argument,
                argument.get_type().const_int(exit, false),
                "frame.capture.exit",
            )
            .map_err(CodegenFailure::backend_library)?;

        let requested = builder
            .build_and(resolving, requested, "frame.capture.requested")
            .map_err(CodegenFailure::backend_library)?;

        selected = builder
            .build_select(
                requested,
                state.get_type().const_int(u64::from(entry.raw()), false),
                selected,
                "frame.capture.entry",
            )
            .map_err(CodegenFailure::backend_library)?
            .into_int_value();

        selected_operation = builder
            .build_select(
                requested,
                operation.get_type().const_zero(),
                selected_operation,
                "frame.capture.operation",
            )
            .map_err(CodegenFailure::backend_library)?
            .into_int_value();
    }

    Ok((selected, selected_operation))
}
