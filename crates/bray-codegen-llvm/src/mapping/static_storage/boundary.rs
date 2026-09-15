use bray_codegen::{CodegenCallableSignature, CodegenFailure, CodegenInstanceKey, CodegenMappings};
use inkwell::builder::Builder;
use inkwell::module::Module;
use inkwell::values::{BasicMetadataValueEnum, CallSiteValue, FunctionValue, PointerValue};

use super::super::LlvmTypeMappings;
use super::super::symbol::apply_signature_call_attributes;

pub(super) fn mapped_instance_function<'context, 'mappings>(
    module: &Module<'context>,
    mappings: &'mappings CodegenMappings,
    instance: &CodegenInstanceKey,
) -> Result<(FunctionValue<'context>, &'mappings CodegenCallableSignature), CodegenFailure> {
    let symbol = mappings
        .instance_symbol(instance)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let function = module
        .get_function(symbol.name().as_str())
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    Ok((function, symbol.signature()))
}

pub(super) fn invoke_static_boundary<'context>(
    builder: &Builder<'context>,
    function: FunctionValue<'context>,
    signature: &CodegenCallableSignature,
    semantic_arguments: &[BasicMetadataValueEnum<'context>],
    outcome: PointerValue<'context>,
    name: &str,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<CallSiteValue<'context>, CodegenFailure> {
    let mut arguments = semantic_arguments.to_vec();

    if signature.has_panic_report_context() {
        arguments.push(outcome.into());
    }

    let call = builder
        .build_call(function, &arguments, name)
        .map_err(CodegenFailure::backend_library)?;

    call.set_call_convention(function.get_call_conventions());
    apply_signature_call_attributes(call, signature, types)?;

    Ok(call)
}

pub(super) fn static_outcome(
    callback: FunctionValue<'_>,
) -> Result<PointerValue<'_>, CodegenFailure> {
    callback
        .get_last_param()
        .and_then(|value| match value {
            inkwell::values::BasicValueEnum::PointerValue(pointer) => Some(pointer),
            _ => None,
        })
        .ok_or(CodegenFailure::GeneratedModuleInvariant)
}

pub(super) fn branch_on_static_failure<'context>(
    builder: &Builder<'context>,
    callback: FunctionValue<'context>,
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<inkwell::basic_block::BasicBlock<'context>, CodegenFailure> {
    let context = types.context();

    let outcome = builder
        .build_load(
            crate::native::run_outcome_type(context, types.target()),
            static_outcome(callback)?,
            "static.outcome",
        )
        .map_err(CodegenFailure::backend_library)?
        .into_struct_value();

    let state = builder
        .build_extract_value(outcome, 0, "static.state")
        .map_err(CodegenFailure::backend_library)?
        .into_int_value();

    let completed = builder
        .build_int_compare(
            inkwell::IntPredicate::EQ,
            state,
            state.get_type().const_zero(),
            "static.completed",
        )
        .map_err(CodegenFailure::backend_library)?;

    let continued = context.append_basic_block(callback, "static.continue");
    let failed = context.append_basic_block(callback, "static.failed");

    builder
        .build_conditional_branch(completed, continued, failed)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(failed);

    Ok(continued)
}

pub(super) fn clean_failed_static_allocation<'context>(
    module: &Module<'context>,
    mappings: &CodegenMappings,
    mapping: &bray_codegen::CodegenStaticStorageMapping,
    builder: &Builder<'context>,
    error_address: inkwell::values::IntValue<'context>,
    destination: PointerValue<'context>,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<(), CodegenFailure> {
    let context = types.context();
    let pointer = context.ptr_type(inkwell::AddressSpace::default());

    let error_pointer = builder
        .build_int_to_ptr(error_address, pointer, "static.finalize.failed.error")
        .map_err(CodegenFailure::backend_library)?;

    let cleanup = mapping
        .finalization()
        .and_then(bray_codegen::CodegenStaticFinalization::incident_cleanup)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let (cleanup, cleanup_signature) = mapped_instance_function(module, mappings, cleanup)?;

    let outcome_type = crate::native::run_outcome_type(context, types.target());

    let cleanup_outcome = crate::translation::allocate_temporary(
        context,
        builder,
        outcome_type,
        "static.failed.allocation.cleanup",
    )?;

    builder
        .build_store(cleanup_outcome, outcome_type.const_zero())
        .map_err(CodegenFailure::backend_library)?;

    invoke_static_boundary(
        builder,
        cleanup,
        cleanup_signature,
        &[error_pointer.into()],
        cleanup_outcome,
        "",
        types,
    )?;

    merge_static_outcomes(
        module,
        builder,
        mapping.owner(),
        destination,
        cleanup_outcome,
        types,
    )?;

    builder
        .build_return(Some(&context.i32_type().const_zero()))
        .map_err(CodegenFailure::backend_library)?;

    Ok(())
}

fn merge_static_outcomes<'context>(
    module: &Module<'context>,
    builder: &Builder<'context>,
    owner: &CodegenInstanceKey,
    destination: PointerValue<'context>,
    secondary: PointerValue<'context>,
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<(), CodegenFailure> {
    let context = types.context();
    let outcome_type = crate::native::run_outcome_type(context, types.target());

    let first = builder
        .build_load(outcome_type, destination, "static.first")
        .map_err(CodegenFailure::backend_library)?
        .into_struct_value();

    let second = builder
        .build_load(outcome_type, secondary, "static.second")
        .map_err(CodegenFailure::backend_library)?
        .into_struct_value();

    let first_state = builder
        .build_extract_value(first, 0, "static.first.state")
        .map_err(CodegenFailure::backend_library)?
        .into_int_value();

    let second_state = builder
        .build_extract_value(second, 0, "static.second.state")
        .map_err(CodegenFailure::backend_library)?
        .into_int_value();

    let panic = context.i32_type().const_int(
        u64::from(bray_runtime_abi::NativeRunState::PANICKED.code()),
        false,
    );

    let first_panicked = builder
        .build_int_compare(
            inkwell::IntPredicate::EQ,
            first_state,
            panic,
            "static.first.panicked",
        )
        .map_err(CodegenFailure::backend_library)?;

    let second_panicked = builder
        .build_int_compare(
            inkwell::IntPredicate::EQ,
            second_state,
            panic,
            "static.second.panicked",
        )
        .map_err(CodegenFailure::backend_library)?;

    let both_panicked = builder
        .build_and(first_panicked, second_panicked, "static.both.panicked")
        .map_err(CodegenFailure::backend_library)?;

    let function = builder
        .get_insert_block()
        .and_then(|block| block.get_parent())
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let merge = context.append_basic_block(function, "static.merge");
    let inspect = context.append_basic_block(function, "static.inspect.second");
    let replace = context.append_basic_block(function, "static.replace.outcome");
    let finished = context.append_basic_block(function, "static.outcomes.finished");

    builder
        .build_conditional_branch(both_panicked, merge, inspect)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(merge);

    let first_report = builder
        .build_struct_gep(outcome_type, destination, 2, "static.first.report")
        .map_err(CodegenFailure::backend_library)?;

    let second_report = builder
        .build_struct_gep(outcome_type, secondary, 2, "static.second.report")
        .map_err(CodegenFailure::backend_library)?;

    let role = bray_runtime_interface::RuntimeAbiRole::PanicReportSuppression;
    let runtime = crate::native::declare_runtime_function(module, context, types.target(), role)?;

    let report = crate::native::invoke_function(
        context,
        builder,
        types.target(),
        &bray_codegen::CodegenSymbolKey::Runtime(bray_ir::MirRuntimeReference::new(
            role,
            owner.target().runtime_abi(),
        )),
        runtime,
        &[first_report.into(), second_report.into()],
        "static.suppressed.report",
    )?
    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    builder
        .build_store(first_report, report)
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_unconditional_branch(finished)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(inspect);

    let second_completed = builder
        .build_int_compare(
            inkwell::IntPredicate::EQ,
            second_state,
            second_state.get_type().const_zero(),
            "static.second.completed",
        )
        .map_err(CodegenFailure::backend_library)?;

    let keep = builder
        .build_or(first_panicked, second_completed, "static.keep.first")
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_conditional_branch(keep, finished, replace)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(replace);

    builder
        .build_store(destination, second)
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_unconditional_branch(finished)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(finished);

    Ok(())
}
