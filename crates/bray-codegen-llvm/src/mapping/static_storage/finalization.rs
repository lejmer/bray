use bray_codegen::{
    CodegenFailure, CodegenMappings, CodegenResultMapping, CodegenStaticStorageMapping,
    CodegenSymbolKey,
};
use inkwell::AddressSpace;
use inkwell::module::Module;
use inkwell::values::{FunctionValue, PointerValue};

use super::super::LlvmTypeMappings;
use super::super::boundary::{
    declare_generated_callback, invoke_generated_call, panic_report_callbacks,
    return_abnormal_callback_outcome,
};
use super::host::StaticFinalizerCallbacks;

pub(super) fn declare_static_finalizer<'context>(
    module: &Module<'context>,
    mappings: &CodegenMappings,
    mapping: &CodegenStaticStorageMapping,
    storage: PointerValue<'context>,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<StaticFinalizerCallbacks<'context>, CodegenFailure> {
    let finalization = mapping.finalization();

    let execution = match finalization.map(|finalization| finalization.execution()) {
        None => bray_runtime_abi::NativeCleanupExecution::NONE,
        Some(bray_symbols::CallableExecution::Synchronous) => {
            bray_runtime_abi::NativeCleanupExecution::SYNCHRONOUS
        }
        Some(bray_symbols::CallableExecution::Asynchronous) => {
            bray_runtime_abi::NativeCleanupExecution::ASYNCHRONOUS
        }
    };

    let metadata = finalization
        .and_then(|finalization| finalization.frame())
        .map(|frame| {
            let key = CodegenSymbolKey::ProtectedFrame {
                frame,
                operation: bray_runtime_interface::ProtectedFrameOperation::MetadataDescription,
            };

            let symbol = mappings
                .symbol(&key)
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            module
                .get_function(symbol.name().as_str())
                .ok_or(CodegenFailure::GeneratedModuleInvariant)
        })
        .transpose()?;

    let start = declare_static_finalizer_start(module, mappings, mapping, storage, types)?;

    Ok(StaticFinalizerCallbacks {
        execution: u64::from(execution.code()),
        metadata,
        start,
        panics: panic_report_callbacks(module, types)?,
    })
}

fn declare_static_finalizer_start<'context>(
    module: &Module<'context>,
    mappings: &CodegenMappings,
    mapping: &CodegenStaticStorageMapping,
    storage: PointerValue<'context>,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<FunctionValue<'context>, CodegenFailure> {
    let name = mapping.finalize_name();

    if let Some(callback) = module.get_function(&name) {
        return Ok(callback);
    }

    let context = types.context();
    let usize = crate::native::pointer_integer_type(context, types.target());

    let callback = declare_generated_callback(
        module,
        &name,
        "static.finalize",
        context.i32_type().fn_type(
            &[
                usize.into(),
                context.ptr_type(AddressSpace::default()).into(),
            ],
            false,
        ),
        types,
    );

    let builder = context.create_builder();

    let entry = callback
        .get_first_basic_block()
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    builder.position_at_end(entry);

    let boundary_destination = callback
        .get_nth_param(1)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?
        .into_pointer_value();

    builder
        .build_store(boundary_destination, usize.const_zero())
        .map_err(CodegenFailure::backend_library)?;

    if let Some(finalization) = mapping.finalization() {
        let symbol = mappings
            .instance_symbol(finalization.instance())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let function = module
            .get_function(symbol.name().as_str())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let supplied_destination = callback
            .get_first_param()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?
            .into_int_value();

        let destination = match finalization.execution() {
            bray_symbols::CallableExecution::Synchronous => {
                context.ptr_type(AddressSpace::default()).const_null()
            }
            bray_symbols::CallableExecution::Asynchronous => builder
                .build_int_to_ptr(
                    supplied_destination,
                    context.ptr_type(AddressSpace::default()),
                    "static.finalize.frame",
                )
                .map_err(CodegenFailure::backend_library)?,
        };

        let (arguments, name) = match symbol.signature().result() {
            CodegenResultMapping::Void => (vec![storage.into()], ""),
            CodegenResultMapping::Direct { .. } => (vec![storage.into()], "static.finalize.value"),
            CodegenResultMapping::Indirect { .. } => (vec![destination.into(), storage.into()], ""),
        };

        let (call, outcome) = invoke_generated_call(
            &builder,
            function,
            symbol.signature(),
            &arguments,
            name,
            types,
        )?;

        return_abnormal_callback_outcome(&builder, outcome, boundary_destination, types)?;

        if matches!(
            symbol.signature().result(),
            CodegenResultMapping::Direct { .. }
        ) {
            let result = call
                .try_as_basic_value()
                .basic()
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            builder
                .build_store(destination, result)
                .map_err(CodegenFailure::backend_library)?;
        }
    }

    builder
        .build_return(Some(&context.i32_type().const_zero()))
        .map_err(CodegenFailure::backend_library)?;

    Ok(callback)
}
