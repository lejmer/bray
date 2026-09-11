use bray_codegen::{CodegenFailure, CodegenResultMapping, CodegenSymbolMapping};
use bray_runtime_abi::NativeCleanupExecution;
use inkwell::AddressSpace;
use inkwell::module::Module;
use inkwell::values::StructValue;

use super::LlvmTypeMappings;
use super::boundary::{
    declare_generated_callback, invoke_generated_call, panic_report_callbacks,
    panic_report_callbacks_type, return_abnormal_callback_outcome,
};

pub(crate) fn task_terminal_cleanup_descriptor<'context>(
    module: &Module<'context>,
    value: inkwell::values::BasicValueEnum<'context>,
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<StructValue<'context>, CodegenFailure> {
    let context = types.context();

    let descriptor = context.struct_type(
        &[
            context.ptr_type(AddressSpace::default()).into(),
            panic_report_callbacks_type(context).into(),
        ],
        false,
    );

    Ok(descriptor.const_named_struct(&[value, panic_report_callbacks(module, types)?.into()]))
}

pub(crate) fn value_cleanup_descriptor<'context>(
    module: &Module<'context>,
    symbol: &CodegenSymbolMapping,
    frame: Option<bray_runtime_interface::ProtectedAsyncFrameId>,
    mappings: &bray_codegen::CodegenMappings,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<StructValue<'context>, CodegenFailure> {
    let context = types.context();
    let pointer = context.ptr_type(AddressSpace::default());
    let word = crate::native::pointer_integer_type(context, types.target());
    let asynchronous = frame.is_some();

    if asynchronous == matches!(symbol.signature().result(), CodegenResultMapping::Void) {
        return Err(CodegenFailure::generated_module_invariant(symbol.key()));
    }

    let metadata = frame
        .map(|frame| {
            let key = bray_codegen::CodegenSymbolKey::ProtectedFrame {
                frame,
                operation: bray_runtime_interface::ProtectedFrameOperation::MetadataDescription,
            };

            let provider = mappings
                .symbol(&key)
                .and_then(|symbol| module.get_function(symbol.name().as_str()))
                .ok_or_else(|| CodegenFailure::generated_module_invariant(symbol.key()))?;

            Ok::<_, CodegenFailure>(provider.as_global_value().as_pointer_value())
        })
        .transpose()?
        .unwrap_or_else(|| pointer.const_null());

    let result_type = match symbol.signature().result() {
        CodegenResultMapping::Void => None,
        CodegenResultMapping::Direct { ty, .. } => Some(*ty),
        CodegenResultMapping::Indirect { pointee, .. } => Some(*pointee),
    };

    if let Some(result_type) = result_type {
        let represented = types.map(result_type)?;
        let native = crate::native::inactive_frame_type(context);
        let data = types.target_data();

        if data.get_store_size(&represented) != data.get_store_size(&native)
            || data.get_abi_alignment(&represented) != data.get_abi_alignment(&native)
        {
            return Err(CodegenFailure::generated_module_invariant((
                symbol.key(),
                result_type,
            )));
        }
    }

    let execution = if asynchronous {
        NativeCleanupExecution::ASYNCHRONOUS
    } else {
        NativeCleanupExecution::SYNCHRONOUS
    };

    let name = format!("{}.value_cleanup", symbol.name().as_str());

    let callback = if let Some(callback) = module.get_function(&name) {
        callback
    } else {
        let callback = declare_generated_callback(
            module,
            &name,
            "value.cleanup",
            context
                .i32_type()
                .fn_type(&[word.into(), pointer.into(), pointer.into()], false),
            types,
        );

        let builder = context.create_builder();

        let entry = callback
            .get_first_basic_block()
            .ok_or_else(|| CodegenFailure::generated_module_invariant(symbol.key()))?;

        builder.position_at_end(entry);

        let parameters = callback.get_params();

        let [receiver, destination, boundary] = parameters.as_slice() else {
            return Err(CodegenFailure::generated_module_invariant(symbol.key()));
        };

        let receiver = builder
            .build_int_to_ptr(receiver.into_int_value(), pointer, "cleanup.receiver")
            .map_err(CodegenFailure::backend_library)?;

        let destination = destination.into_pointer_value();
        let boundary = boundary.into_pointer_value();

        builder
            .build_store(boundary, word.const_zero())
            .map_err(CodegenFailure::backend_library)?;

        let function = module
            .get_function(symbol.name().as_str())
            .ok_or_else(|| CodegenFailure::generated_module_invariant(symbol.key()))?;

        let arguments = match symbol.signature().result() {
            CodegenResultMapping::Void | CodegenResultMapping::Direct { .. } => {
                vec![receiver.into()]
            }
            CodegenResultMapping::Indirect { .. } => vec![destination.into(), receiver.into()],
        };

        let name = if matches!(
            symbol.signature().result(),
            CodegenResultMapping::Direct { .. }
        ) {
            "cleanup.inactive"
        } else {
            ""
        };

        let (call, outcome) = invoke_generated_call(
            &builder,
            function,
            symbol.signature(),
            &arguments,
            name,
            types,
        )?;

        return_abnormal_callback_outcome(&builder, outcome, boundary, types)?;

        if matches!(
            symbol.signature().result(),
            CodegenResultMapping::Direct { .. }
        ) {
            let result = call
                .try_as_basic_value()
                .basic()
                .ok_or_else(|| CodegenFailure::generated_module_invariant(symbol.key()))?;

            let result = crate::translation::reinterpret_value(
                context,
                &builder,
                result,
                crate::native::inactive_frame_type(context).into(),
                result.get_type(),
                "cleanup.native.frame",
            )?;

            builder
                .build_store(destination, result)
                .map_err(CodegenFailure::backend_library)?;
        }

        builder
            .build_return(Some(&context.i32_type().const_zero()))
            .map_err(CodegenFailure::backend_library)?;

        callback
    };

    let descriptor = context.struct_type(
        &[
            context.i32_type().into(),
            pointer.into(),
            pointer.into(),
            panic_report_callbacks_type(context).into(),
        ],
        false,
    );

    Ok(descriptor.const_named_struct(&[
        context
            .i32_type()
            .const_int(u64::from(execution.code()), false)
            .into(),
        metadata.into(),
        callback.as_global_value().as_pointer_value().into(),
        panic_report_callbacks(module, types)?.into(),
    ]))
}
