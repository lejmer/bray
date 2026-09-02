use bray_codegen::{
    CodegenCallableSignature, CodegenFailure, CodegenInstanceKey, CodegenMappings, CodegenSymbolKey,
};
use bray_ir::MirRuntimeReference;
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

#[expect(
    clippy::too_many_arguments,
    reason = "the synthesized boundary keeps its mapping, function, signature, and LLVM inputs explicit"
)]
pub(super) fn invoke_static_boundary<'context>(
    module: &Module<'context>,
    mappings: &CodegenMappings,
    owner: &CodegenInstanceKey,
    builder: &Builder<'context>,
    function: FunctionValue<'context>,
    signature: &CodegenCallableSignature,
    semantic_arguments: &[BasicMetadataValueEnum<'context>],
    name: &str,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<CallSiteValue<'context>, CodegenFailure> {
    let context = types.context();
    let usize = crate::native::pointer_integer_type(context, types.target());
    let mut arguments = semantic_arguments.to_vec();

    let panic_report_context = if signature.has_panic_report_context() {
        let storage = crate::translation::allocate_temporary(
            context,
            builder,
            usize,
            "static.call.panic.report.context",
        )?;

        builder
            .build_store(storage, usize.const_zero())
            .map_err(CodegenFailure::backend_library)?;

        arguments.push(storage.into());

        Some(storage)
    } else {
        None
    };

    let call = builder
        .build_call(function, &arguments, name)
        .map_err(CodegenFailure::backend_library)?;

    call.set_call_convention(function.get_call_conventions());
    apply_signature_call_attributes(call, signature, types)?;

    if let Some(storage) = panic_report_context {
        propagate_static_boundary_panic(module, mappings, owner, builder, storage, types)?;
    }

    Ok(call)
}

fn propagate_static_boundary_panic<'context>(
    module: &Module<'context>,
    mappings: &CodegenMappings,
    owner: &CodegenInstanceKey,
    builder: &Builder<'context>,
    storage: PointerValue<'context>,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<(), CodegenFailure> {
    let context = types.context();
    let usize = crate::native::pointer_integer_type(context, types.target());

    let (report, continued) =
        crate::translation::branch_on_pending_panic(context, builder, usize, storage)?;

    let reference = MirRuntimeReference::new(
        bray_runtime_interface::RuntimeAbiRole::PanicPropagation,
        owner.target().runtime_abi(),
    );

    let symbol = mappings
        .symbol(&CodegenSymbolKey::Runtime(reference))
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let runtime = module
        .get_function(symbol.name().as_str())
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let call = builder
        .build_call(runtime, &[report.into()], reference.role().as_str())
        .map_err(CodegenFailure::backend_library)?;

    call.set_call_convention(runtime.get_call_conventions());
    apply_signature_call_attributes(call, symbol.signature(), types)?;

    builder
        .build_unreachable()
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(continued);

    Ok(())
}
