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
