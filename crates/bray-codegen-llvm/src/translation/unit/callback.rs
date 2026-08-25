use bray_codegen::{
    CodegenFailure, CodegenRequest, CodegenResultMapping, CodegenSymbolKey,
    CodegenSymbolMapping,
};
use bray_ir::MirRuntimeReference;
use bray_runtime_abi::NativeRunState;
use bray_runtime_interface::RuntimeAbiRole;
use inkwell::AddressSpace;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::{Linkage, Module};
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue, PointerValue};

use super::support::{int_value, llvm, native_run_outcome, native_run_state_is, pointer_value};
use crate::mapping::{LlvmTypeMappings, apply_signature_call_attributes, declare_symbol};

pub(super) fn prepare<'context, 'request>(
    module: &Module<'context>,
    request: CodegenRequest<'request>,
    symbol: &'request CodegenSymbolMapping,
    function: FunctionValue<'context>,
    types: &mut LlvmTypeMappings<'context, 'request>,
) -> Result<(FunctionValue<'context>, Option<FunctionValue<'context>>), CodegenFailure> {
    let Some(native_entry) = symbol.native_entry() else {
        return Ok((function, None));
    };

    let entry_symbol = CodegenSymbolMapping::new(
        symbol.key().clone(),
        native_entry.name().clone(),
        native_entry.linkage(),
        symbol.signature().clone(),
    );

    let trampoline = declare_symbol(module, &entry_symbol, request.target(), true, types)?;

    Ok((function, Some(trampoline)))
}

pub(super) fn translate<'context, 'request>(
    context: &'context Context,
    module: &Module<'context>,
    request: CodegenRequest<'request>,
    symbol: &'request CodegenSymbolMapping,
    body: FunctionValue<'context>,
    trampoline: FunctionValue<'context>,
    types: &mut LlvmTypeMappings<'context, 'request>,
) -> Result<(), CodegenFailure> {
    let builder = context.create_builder();
    let entry = context.append_basic_block(trampoline, "callback.entry");

    builder.position_at_end(entry);

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
        let destination = field_pointer(&builder, state_type, state, field, "callback.argument")?;

        llvm(builder.build_store(destination, parameter))?;
    }

    if let Some(field) = result_field {
        let destination = field_pointer(
            &builder,
            state_type,
            state,
            field,
            "callback.result.destination",
        )?;

        let result_type = result_type.ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        llvm(builder.build_store(destination, result_type.const_zero()))?;
    }

    initialize_indirect_result(&builder, symbol, trampoline, types)?;

    let callback = declare_callback(
        context,
        module,
        request.target(),
        symbol,
        state_type,
        body,
        types,
    )?;

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
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let arguments: [BasicMetadataValueEnum<'context>; 2] = [
        callback.as_global_value().as_pointer_value().into(),
        state_handle.into(),
    ];

    let outcome = crate::native::invoke_function(
        context,
        &builder,
        request.target(),
        &key,
        runtime_function,
        &arguments,
        "callback.boundary",
    )?
    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    resolve_callback_outcome(context, module, request, &builder, trampoline, outcome)?;

    match result_field {
        Some(field) => {
            let source =
                field_pointer(&builder, state_type, state, field, "callback.result.source")?;

            let result_type = result_type.ok_or(CodegenFailure::GeneratedModuleInvariant)?;
            let result = llvm(builder.build_load(result_type, source, "callback.result"))?;

            llvm(builder.build_return(Some(&result)))?;
        }
        None => {
            llvm(builder.build_return(None))?;
        }
    }

    Ok(())
}

fn resolve_callback_outcome<'context>(
    context: &'context Context,
    module: &Module<'context>,
    request: CodegenRequest<'_>,
    builder: &Builder<'context>,
    trampoline: FunctionValue<'context>,
    outcome: BasicValueEnum<'context>,
) -> Result<(), CodegenFailure> {
    let (state, payload) = native_run_outcome(builder, outcome)?;

    let panicked = native_run_state_is(
        builder,
        state,
        NativeRunState::PANICKED,
        "callback.panicked",
    )?;

    let report = context.append_basic_block(trampoline, "callback.report_panic");
    let complete = context.append_basic_block(trampoline, "callback.complete");

    llvm(builder.build_conditional_branch(panicked, report, complete))?;
    builder.position_at_end(report);

    let panic = MirRuntimeReference::new(
        RuntimeAbiRole::PanicReporting,
        request.unit().target().runtime_abi(),
    );

    let key = CodegenSymbolKey::Runtime(panic);

    let function = request
        .mappings()
        .symbol(&key)
        .and_then(|mapping| module.get_function(mapping.name().as_str()))
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    crate::native::invoke_function(
        context,
        builder,
        request.target(),
        &key,
        function,
        &[payload.into()],
        "callback.report_panic",
    )?
    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    llvm(builder.build_unconditional_branch(complete))?;
    builder.position_at_end(complete);

    Ok(())
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
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let result_type = types.map(*pointee)?;

    llvm(builder.build_store(destination, result_type.const_zero()))?;

    Ok(())
}

fn declare_callback<'context>(
    context: &'context Context,
    module: &Module<'context>,
    target: &bray_codegen::CodegenTarget,
    symbol: &CodegenSymbolMapping,
    state_type: inkwell::types::StructType<'context>,
    body: FunctionValue<'context>,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<FunctionValue<'context>, CodegenFailure> {
    let usize = crate::native::pointer_integer_type(context, target);
    let callback_type = context.void_type().fn_type(&[usize.into()], false);
    let name = format!("{}.bray_callback_invoke", symbol.name().as_str());
    let callback = module.add_function(&name, callback_type, Some(Linkage::Private));
    let entry = context.append_basic_block(callback, "callback.invoke");
    let builder = context.create_builder();

    builder.position_at_end(entry);

    let state = callback
        .get_first_param()
        .and_then(int_value)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let state = llvm(builder.build_int_to_ptr(
        state,
        context.ptr_type(AddressSpace::default()),
        "callback.state",
    ))?;

    let mut arguments = Vec::with_capacity(body.count_params() as usize);

    for (field, parameter) in body.get_params().iter().copied().enumerate() {
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

    let call = llvm(builder.build_call(body, &arguments, "callback.body"))?;

    call.set_call_convention(body.get_call_conventions());
    apply_signature_call_attributes(call, symbol.signature(), types)?;

    if let Some(result) = call.try_as_basic_value().basic() {
        let field =
            usize::try_from(body.count_params()).map_err(|_| CodegenFailure::ResourceExhausted)?;

        let destination = field_pointer(
            &builder,
            state_type,
            state,
            field,
            "callback.result.destination",
        )?;

        llvm(builder.build_store(destination, result))?;
    }

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
    let field = u32::try_from(field).map_err(|_| CodegenFailure::ResourceExhausted)?;

    llvm(builder.build_struct_gep(state_type, state, field, name))
}
