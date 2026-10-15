use bray_codegen::{
    CodegenFailure, CodegenRequest, CodegenResultMapping, CodegenSymbolKey, CodegenSymbolMapping,
};
use bray_ir::MirRuntimeReference;
use bray_runtime_abi::NativeRunState;
use bray_runtime_interface::RuntimeAbiRole;
use inkwell::AddressSpace;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::{Linkage, Module};
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicMetadataValueEnum, FunctionValue, PointerValue};

use super::super::support::{
    int_value, llvm, native_run_outcome, native_run_outcome_value, native_run_state_is,
    pointer_value,
};
use crate::mapping::{LlvmTypeMappings, apply_signature_call_attributes, declare_native_entry};

use super::boundary::{
    resolve_callback_outcome, resolve_terminal_status, translate_runtime_boundary,
};

pub(in crate::translation::unit) fn prepare<'context, 'request>(
    module: &Module<'context>,
    request: CodegenRequest<'request>,
    symbol: &'request CodegenSymbolMapping,
    function: FunctionValue<'context>,
    types: &mut LlvmTypeMappings<'context, 'request>,
) -> Result<(FunctionValue<'context>, Option<FunctionValue<'context>>), CodegenFailure> {
    if symbol.native_entry().is_none() {
        return Ok((function, None));
    }

    let trampoline = declare_native_entry(module, symbol, request.target(), true, types)?
        .expect("checked MIR translation requires an established mapping or value");

    Ok((function, Some(trampoline)))
}

pub(in crate::translation::unit) fn translate<'context, 'request>(
    context: &'context Context,
    module: &Module<'context>,
    request: CodegenRequest<'request>,
    symbol: &'request CodegenSymbolMapping,
    body: FunctionValue<'context>,
    trampoline: FunctionValue<'context>,
    types: &mut LlvmTypeMappings<'context, 'request>,
) -> Result<(), CodegenFailure> {
    if symbol.native_entry().is_some_and(|entry| {
        entry.kind() != bray_runtime_interface::NativeEntryKind::ForeignCallback
    }) {
        return translate_runtime_boundary(
            context, module, request, symbol, body, trampoline, types,
        );
    }

    let builder = context.create_builder();
    let entry = context.append_basic_block(trampoline, "callback.entry");

    builder.position_at_end(entry);

    let (state_type, state, result_field) =
        prepare_state(context, &builder, symbol, trampoline, types)?;

    let result_type = trampoline.get_type().get_return_type();

    let return_block = context.append_basic_block(trampoline, "callback.return");
    let invoke_block = context.append_basic_block(trampoline, "callback.admitted");

    let observation = crate::translation::product::control(
        context,
        module,
        request,
        &builder,
        bray_runtime_abi::NativeProductHostOperation::ACQUIRE_ENTRY,
    )?;

    let status = super::super::support::extract_value(&builder, observation, 0)?.into_int_value();

    let admitted = llvm(builder.build_int_compare(
        inkwell::IntPredicate::EQ,
        status,
        status.get_type().const_zero(),
        "callback.admission",
    ))?;

    llvm(builder.build_conditional_branch(admitted, invoke_block, return_block))?;
    builder.position_at_end(invoke_block);

    let callback = declare_callback(context, module, request, symbol, state_type, body, types)?;

    let usize = crate::native::pointer_integer_type(context, request.target());
    let state_handle = llvm(builder.build_ptr_to_int(state, usize, "callback.state.handle"))?;

    let runtime = MirRuntimeReference::new(
        RuntimeAbiRole::ForeignCallbackExecution,
        request.unit().target().runtime_abi(),
    );

    let key = CodegenSymbolKey::Runtime(runtime);

    let runtime_function = request
        .mappings()
        .symbol(&key)
        .and_then(|mapping| module.get_function(mapping.name().as_str()))
        .expect("checked MIR translation requires an established mapping or value");

    let outcome_type = crate::native::run_outcome_type(context, request.target());
    let destination = llvm(builder.build_alloca(outcome_type, "callback.outcome"))?;

    let initial = native_run_outcome_value(
        context,
        &builder,
        request.target(),
        NativeRunState::RUNTIME_FAILURE,
        usize.const_int(4, false).into(),
    )?;

    llvm(builder.build_store(destination, initial))?;

    let arguments: [BasicMetadataValueEnum<'context>; 3] = [
        callback.as_global_value().as_pointer_value().into(),
        state_handle.into(),
        destination.into(),
    ];

    let terminal_status = crate::native::invoke_function(
        context,
        &builder,
        request.target(),
        &key,
        runtime_function,
        &arguments,
        "callback.boundary",
    )?
    .expect("checked MIR translation requires an established mapping or value");

    let outcome = llvm(builder.build_load(outcome_type, destination, "callback.body.outcome"))?;

    resolve_callback_outcome(context, module, request, &builder, trampoline, outcome)?;

    resolve_terminal_status(
        context,
        module,
        request,
        &builder,
        trampoline,
        terminal_status,
    )?;

    let observation = crate::translation::product::control(
        context,
        module,
        request,
        &builder,
        bray_runtime_abi::NativeProductHostOperation::RELEASE_ENTRY,
    )?;

    let status = super::super::support::extract_value(&builder, observation, 0)?.into_int_value();

    let failure = llvm(builder.build_int_compare(
        inkwell::IntPredicate::EQ,
        status,
        status.get_type().const_int(
            u64::from(bray_runtime_abi::NativeProductHostStatus::RUNTIME_FAILURE.code()),
            false,
        ),
        "callback.product.failure",
    ))?;

    let failure =
        llvm(builder.build_int_z_extend(failure, context.i32_type(), "callback.product.status"))?;

    resolve_terminal_status(
        context,
        module,
        request,
        &builder,
        trampoline,
        failure.into(),
    )?;

    llvm(builder.build_unconditional_branch(return_block))?;
    builder.position_at_end(return_block);

    match result_field {
        Some(field) => {
            let source =
                field_pointer(&builder, state_type, state, field, "callback.result.source")?;

            let result_type = result_type
                .expect("checked MIR translation requires an established mapping or value");

            let result = llvm(builder.build_load(result_type, source, "callback.result"))?;

            llvm(builder.build_return(Some(&result)))?;
        }
        None => {
            llvm(builder.build_return(None))?;
        }
    }

    Ok(())
}

fn prepare_state<'context>(
    context: &'context Context,
    builder: &Builder<'context>,
    symbol: &CodegenSymbolMapping,
    trampoline: FunctionValue<'context>,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<
    (
        inkwell::types::StructType<'context>,
        PointerValue<'context>,
        Option<usize>,
    ),
    CodegenFailure,
> {
    let parameters = trampoline.get_params();
    let result_type = trampoline.get_type().get_return_type();

    let mut fields: Vec<BasicTypeEnum<'context>> =
        parameters.iter().map(|value| value.get_type()).collect();

    let result_field = result_type.map(|result| {
        let field = fields.len();

        fields.push(result);

        field
    });

    let state_type = context.struct_type(&fields, false);
    let state = llvm(builder.build_alloca(state_type, "callback.state"))?;

    for (field, parameter) in parameters.iter().copied().enumerate() {
        let destination = field_pointer(builder, state_type, state, field, "callback.argument")?;

        llvm(builder.build_store(destination, parameter))?;
    }

    if let Some(field) = result_field {
        let destination = field_pointer(
            builder,
            state_type,
            state,
            field,
            "callback.result.destination",
        )?;

        let result_type =
            result_type.expect("checked MIR translation requires an established mapping or value");

        llvm(builder.build_store(destination, result_type.const_zero()))?;
    }

    initialize_indirect_result(builder, symbol, trampoline, types)?;

    Ok((state_type, state, result_field))
}

fn initialize_indirect_result(
    builder: &Builder<'_>,
    symbol: &CodegenSymbolMapping,
    trampoline: FunctionValue<'_>,
    types: &mut LlvmTypeMappings<'_, '_>,
) -> Result<(), CodegenFailure> {
    let CodegenResultMapping::Indirect { pointee, .. } = symbol.signature().result() else {
        return Ok(());
    };

    let destination = trampoline
        .get_first_param()
        .and_then(pointer_value)
        .expect("checked MIR translation requires an established mapping or value");

    let result_type = types.map(*pointee)?;

    llvm(builder.build_store(destination, result_type.const_zero()))?;

    Ok(())
}

fn declare_callback<'context>(
    context: &'context Context,
    module: &Module<'context>,
    request: CodegenRequest<'_>,
    symbol: &CodegenSymbolMapping,
    state_type: inkwell::types::StructType<'context>,
    body: FunctionValue<'context>,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<FunctionValue<'context>, CodegenFailure> {
    let target = request.target();
    let usize = crate::native::pointer_integer_type(context, target);

    let callback_type = context.void_type().fn_type(
        &[
            usize.into(),
            context.ptr_type(AddressSpace::default()).into(),
            context.ptr_type(AddressSpace::default()).into(),
        ],
        false,
    );

    let name = format!("{}.bray_callback_invoke", symbol.name().as_str());
    let callback = module.add_function(&name, callback_type, Some(Linkage::Private));
    let entry = context.append_basic_block(callback, "callback.invoke");
    let builder = context.create_builder();

    builder.position_at_end(entry);

    let state_handle = callback
        .get_first_param()
        .and_then(int_value)
        .expect("checked MIR translation requires an established mapping or value");

    let state = llvm(builder.build_int_to_ptr(
        state_handle,
        context.ptr_type(AddressSpace::default()),
        "callback.state",
    ))?;

    assert!(symbol.signature().has_panic_report_context());

    let parameter_count = body.count_params() as usize - 1;

    let outcome = callback
        .get_nth_param(1)
        .and_then(pointer_value)
        .expect("native callback receives its admitted outcome destination");

    let outcome_type = crate::native::run_outcome_type(context, target);

    llvm(builder.build_store(outcome, outcome_type.const_zero()))?;

    let mut arguments = Vec::with_capacity(parameter_count + 1);

    for (field, parameter) in body
        .get_params()
        .iter()
        .copied()
        .take(parameter_count)
        .enumerate()
    {
        let source = field_pointer(
            &builder,
            state_type,
            state,
            field,
            "callback.argument.source",
        )?;

        let argument = llvm(builder.build_load(parameter.get_type(), source, "callback.argument"))?;

        arguments.push(argument.into());
    }

    arguments.push(outcome.into());

    let call = llvm(builder.build_call(body, &arguments, "callback.body"))?;

    call.set_call_convention(body.get_call_conventions());
    apply_signature_call_attributes(call, symbol.signature(), types)?;

    let returned = llvm(builder.build_load(outcome_type, outcome, "callback.outcome"))?;

    let (run_state, _) = native_run_outcome(&builder, returned)?;

    let completed = native_run_state_is(
        &builder,
        run_state,
        NativeRunState::COMPLETED,
        "callback.completed",
    )?;

    let publish = context.append_basic_block(callback, "callback.publish");
    let finish = context.append_basic_block(callback, "callback.finish");

    llvm(builder.build_conditional_branch(completed, publish, finish))?;
    builder.position_at_end(publish);

    if let Some(result) = call.try_as_basic_value().basic() {
        let field = parameter_count;

        let destination = field_pointer(
            &builder,
            state_type,
            state,
            field,
            "callback.result.destination",
        )?;

        llvm(builder.build_store(destination, result))?;
    }

    llvm(builder.build_unconditional_branch(finish))?;
    builder.position_at_end(finish);

    // A native C callback is a terminal reporting boundary. Resolve the body's
    // report while its entry attachment and optional execution bridge remain live.
    resolve_callback_outcome(context, module, request, &builder, callback, returned)?;

    let completed = native_run_outcome_value(
        context,
        &builder,
        target,
        NativeRunState::COMPLETED,
        state_handle.into(),
    )?;

    llvm(builder.build_store(outcome, completed))?;
    llvm(builder.build_return(None))?;

    Ok(callback)
}

fn field_pointer<'context>(
    builder: &Builder<'context>,
    state_type: inkwell::types::StructType<'context>,
    state: PointerValue<'context>,
    field: usize,
    name: &str,
) -> Result<PointerValue<'context>, CodegenFailure> {
    let field = crate::conversion::resource_limit(field, "callback_field_index")?;

    llvm(builder.build_struct_gep(state_type, state, field, name))
}
