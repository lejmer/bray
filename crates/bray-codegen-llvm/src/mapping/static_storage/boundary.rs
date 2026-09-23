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
) -> (FunctionValue<'context>, &'mappings CodegenCallableSignature) {
    let symbol = mappings
        .instance_symbol(instance)
        .expect("static-storage realization requires an established mapping or value");

    let function = module
        .get_function(symbol.name().as_str())
        .expect("static-storage realization requires an established mapping or value");

    (function, symbol.signature())
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

pub(super) fn static_outcome(callback: FunctionValue<'_>) -> PointerValue<'_> {
    callback
        .get_last_param()
        .and_then(|value| match value {
            inkwell::values::BasicValueEnum::PointerValue(pointer) => Some(pointer),
            _ => None,
        })
        .expect("static callback must carry its outcome pointer")
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
            static_outcome(callback),
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
        .expect("static-storage realization requires an established mapping or value");

    let (cleanup, cleanup_signature) = mapped_instance_function(module, mappings, cleanup);

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

    crate::translation::merge_pending_outcomes(
        module,
        context,
        builder,
        types.target(),
        mapping.owner().target().runtime_abi(),
        destination,
        cleanup_outcome,
    )?;

    builder
        .build_return(Some(&context.i32_type().const_zero()))
        .map_err(CodegenFailure::backend_library)?;

    Ok(())
}
