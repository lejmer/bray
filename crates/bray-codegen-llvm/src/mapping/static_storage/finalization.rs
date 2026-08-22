use bray_codegen::{
    CodegenFailure, CodegenMappings, CodegenResultMapping, CodegenStaticStorageMapping,
    CodegenTypeKind,
};
use bray_runtime_interface::ExecutableEntryResult;
use inkwell::builder::Builder;
use inkwell::module::Module;
use inkwell::types::{BasicTypeEnum, PointerType, StructType};
use inkwell::values::{FunctionValue, PointerValue, StructValue};
use inkwell::{AddressSpace, IntPredicate};

use super::super::LlvmTypeMappings;
use super::super::symbol::apply_signature_call_attributes;
use super::host::StaticFinalizerCallbacks;
use super::storage::declare_static_callback_with_type;

pub(super) fn declare_static_finalizer<'context>(
    module: &Module<'context>,
    mappings: &CodegenMappings,
    mapping: &CodegenStaticStorageMapping,
    storage: PointerValue<'context>,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<StaticFinalizerCallbacks<'context>, CodegenFailure> {
    let finalization = mapping.finalization();

    let (execution, result_size, result_alignment) = match finalization {
        None => (
            u64::from(bray_runtime_abi::NativeStaticFinalizerExecution::NONE.code()),
            0,
            1,
        ),
        Some(finalization) => {
            let execution = match finalization.execution() {
                bray_symbols::CallableExecution::Synchronous => {
                    bray_runtime_abi::NativeStaticFinalizerExecution::SYNCHRONOUS
                }
                bray_symbols::CallableExecution::Asynchronous => {
                    bray_runtime_abi::NativeStaticFinalizerExecution::ASYNCHRONOUS
                }
            };

            let (size, alignment) = match finalization.result() {
                ExecutableEntryResult::Unit => (0, 1),
                ExecutableEntryResult::Fallible { ty, .. } => {
                    let layout = mappings
                        .instance_ty(mapping.owner(), ty)
                        .and_then(bray_codegen::CodegenTypeMapping::layout)
                        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                    (layout.size(), layout.alignment().get())
                }
                ExecutableEntryResult::I32 => {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                }
            };

            (u64::from(execution.code()), size, alignment)
        }
    };

    let resolve = declare_static_finalizer_resolver(module, mappings, mapping, types)?;
    let start = declare_static_finalizer_start(module, mappings, mapping, storage, resolve, types)?;

    Ok(StaticFinalizerCallbacks {
        execution,
        result_size,
        result_alignment,
        start,
        resolve,
    })
}

fn declare_static_finalizer_start<'context>(
    module: &Module<'context>,
    mappings: &CodegenMappings,
    mapping: &CodegenStaticStorageMapping,
    storage: PointerValue<'context>,
    resolve: FunctionValue<'context>,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<FunctionValue<'context>, CodegenFailure> {
    let name = mapping.finalize_name();

    if let Some(callback) = module.get_function(&name) {
        return Ok(callback);
    }

    let context = types.context();
    let usize = crate::native::pointer_integer_type(context, types.target());

    let callback = declare_static_callback_with_type(
        module,
        &name,
        "static.finalize",
        context.i32_type().fn_type(&[usize.into()], false),
    );

    let builder = context.create_builder();

    let entry = callback
        .get_first_basic_block()
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    builder.position_at_end(entry);

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

        let destination = match (finalization.execution(), finalization.result()) {
            (
                bray_symbols::CallableExecution::Synchronous,
                ExecutableEntryResult::Fallible { ty, .. },
            ) => builder
                .build_alloca(types.map(ty)?, "static.finalize.result")
                .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?,
            (bray_symbols::CallableExecution::Synchronous, ExecutableEntryResult::Unit) => {
                context.ptr_type(AddressSpace::default()).const_null()
            }
            (bray_symbols::CallableExecution::Synchronous, ExecutableEntryResult::I32) => {
                return Err(CodegenFailure::GeneratedModuleInvariant);
            }
            (bray_symbols::CallableExecution::Asynchronous, _) => builder
                .build_int_to_ptr(
                    supplied_destination,
                    context.ptr_type(AddressSpace::default()),
                    "static.finalize.frame",
                )
                .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?,
        };

        let call = match symbol.signature().result() {
            CodegenResultMapping::Void => builder.build_call(function, &[storage.into()], ""),
            CodegenResultMapping::Direct { .. } => {
                builder.build_call(function, &[storage.into()], "static.finalize.value")
            }
            CodegenResultMapping::Indirect { .. } => {
                builder.build_call(function, &[destination.into(), storage.into()], "")
            }
        }
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

        call.set_call_convention(function.get_call_conventions());
        apply_signature_call_attributes(call, symbol.signature(), types)?;

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
                .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;
        }

        if finalization.execution() == bray_symbols::CallableExecution::Synchronous
            && matches!(
                finalization.result(),
                ExecutableEntryResult::Fallible { .. }
            )
        {
            let completion = builder
                .build_ptr_to_int(destination, usize, "static.finalize.completion")
                .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

            let status = builder
                .build_call(
                    resolve,
                    &[completion.into(), supplied_destination.into()],
                    "static.finalize.status",
                )
                .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?
                .try_as_basic_value()
                .basic()
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            builder
                .build_return(Some(&status))
                .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

            return Ok(callback);
        }
    }

    builder
        .build_return(Some(&context.i32_type().const_zero()))
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    Ok(callback)
}

fn declare_static_finalizer_resolver<'context>(
    module: &Module<'context>,
    mappings: &CodegenMappings,
    mapping: &CodegenStaticStorageMapping,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<FunctionValue<'context>, CodegenFailure> {
    let name = format!("{}.resolve", mapping.finalize_name());

    if let Some(callback) = module.get_function(&name) {
        return Ok(callback);
    }

    let context = types.context();
    let usize = crate::native::pointer_integer_type(context, types.target());

    let callback = declare_static_callback_with_type(
        module,
        &name,
        "static.finalize.resolve",
        context
            .i32_type()
            .fn_type(&[usize.into(), usize.into()], false),
    );

    let builder = context.create_builder();

    let entry = callback
        .get_first_basic_block()
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    builder.position_at_end(entry);

    let Some(finalization) = mapping.finalization() else {
        builder
            .build_return(Some(&context.i32_type().const_zero()))
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

        return Ok(callback);
    };

    let ExecutableEntryResult::Fallible {
        ty,
        error,
        success_variant,
    } = finalization.result()
    else {
        if finalization.result() == ExecutableEntryResult::I32 {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        builder
            .build_return(Some(&context.i32_type().const_zero()))
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

        return Ok(callback);
    };

    let result = callback
        .get_first_param()
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?
        .into_int_value();

    let incident_destination = callback
        .get_nth_param(1)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?
        .into_int_value();

    let result = builder
        .build_int_to_ptr(
            result,
            context.ptr_type(AddressSpace::default()),
            "static.finalize.completion",
        )
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    let result_mapping = mappings
        .instance_ty(mapping.owner(), ty)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let CodegenTypeKind::Union { tag, variants } = result_mapping.kind() else {
        return Err(CodegenFailure::GeneratedModuleInvariant);
    };

    let Some(tag) = *tag else {
        return Err(CodegenFailure::GeneratedModuleInvariant);
    };

    let BasicTypeEnum::IntType(tag_type) = types.map(tag)? else {
        return Err(CodegenFailure::GeneratedModuleInvariant);
    };

    let tag = builder
        .build_load(tag_type, result, "static.finalize.tag")
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?
        .into_int_value();

    let success = variants
        .iter()
        .find(|variant| variant.variant() == success_variant)
        .and_then(bray_codegen::CodegenUnionVariantLayout::tag)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let success = crate::translation::integer_constant(tag_type, success);

    let succeeded = builder
        .build_int_compare(IntPredicate::EQ, tag, success, "static.finalize.succeeded")
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    let success_block = context.append_basic_block(callback, "static.finalize.success");
    let failure_block = context.append_basic_block(callback, "static.finalize.failure");

    builder
        .build_conditional_branch(succeeded, success_block, failure_block)
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    builder.position_at_end(success_block);

    builder
        .build_return(Some(&context.i32_type().const_zero()))
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    builder.position_at_end(failure_block);

    let error_variant = variants
        .iter()
        .find(|variant| variant.variant() != success_variant)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let [error_field] = error_variant.fields() else {
        return Err(CodegenFailure::GeneratedModuleInvariant);
    };

    if error_field.ty() != error {
        return Err(CodegenFailure::GeneratedModuleInvariant);
    }

    let error_address = builder
        .build_ptr_to_int(result, usize, "static.finalize.error.base")
        .and_then(|address| {
            builder.build_int_add(
                address,
                usize.const_int(error_field.offset_bytes(), false),
                "static.finalize.error",
            )
        })
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    let error_layout = mappings
        .instance_ty(mapping.owner(), error)
        .and_then(bray_codegen::CodegenTypeMapping::layout)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let pointer = context.ptr_type(AddressSpace::default());

    let allocation = module
        .get_function(bray_runtime_abi::MEMORY_ALLOCATION_SYMBOL)
        .unwrap_or_else(|| {
            module.add_function(
                bray_runtime_abi::MEMORY_ALLOCATION_SYMBOL,
                pointer.fn_type(&[usize.into(), usize.into()], false),
                None,
            )
        });

    let payload = builder
        .build_call(
            allocation,
            &[
                usize.const_int(error_layout.size(), false).into(),
                usize
                    .const_int(error_layout.alignment().get(), false)
                    .into(),
            ],
            "static.finalize.incident.payload",
        )
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?
        .try_as_basic_value()
        .basic()
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?
        .into_pointer_value();

    let error_pointer = builder
        .build_int_to_ptr(error_address, pointer, "static.finalize.error.pointer")
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    let alignment = u32::try_from(error_layout.alignment().get())
        .map_err(|_| CodegenFailure::UnsupportedTarget)?;

    builder
        .build_memcpy(
            payload,
            alignment,
            error_pointer,
            alignment,
            usize.const_int(error_layout.size(), false),
        )
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    let report = declare_static_incident_reporter(module, mapping, error_layout.size(), types)?;

    let destroy = declare_static_incident_destroyer(
        module,
        mappings,
        mapping,
        error_layout.size(),
        error_layout.alignment().get(),
        types,
    )?;

    let incident = static_incident_value(&builder, finalization, payload, report, destroy, types)?;

    let incident_destination = builder
        .build_int_to_ptr(
            incident_destination,
            pointer,
            "static.finalize.incident.destination",
        )
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    builder
        .build_store(incident_destination, incident)
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    builder
        .build_return(Some(&context.i32_type().const_int(1, false)))
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    Ok(callback)
}

fn declare_static_incident_reporter<'context>(
    module: &Module<'context>,
    mapping: &CodegenStaticStorageMapping,
    payload_size: u64,
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<FunctionValue<'context>, CodegenFailure> {
    let name = format!("{}.incident.report", mapping.finalize_name());

    if let Some(callback) = module.get_function(&name) {
        return Ok(callback);
    }

    let context = types.context();
    let usize = crate::native::pointer_integer_type(context, types.target());

    let callback = declare_static_callback_with_type(
        module,
        &name,
        "static.finalize.incident.report",
        context.i32_type().fn_type(&[usize.into()], false),
    );

    let reporter = module
        .get_function(bray_runtime_abi::ENTRY_FAILURE_REPORTING_SYMBOL)
        .unwrap_or_else(|| {
            module.add_function(
                bray_runtime_abi::ENTRY_FAILURE_REPORTING_SYMBOL,
                context
                    .i32_type()
                    .fn_type(&[usize.into(), usize.into()], false),
                None,
            )
        });

    let builder = context.create_builder();

    let entry = callback
        .get_first_basic_block()
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let payload = callback
        .get_first_param()
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    builder.position_at_end(entry);

    let status = builder
        .build_call(
            reporter,
            &[payload.into(), usize.const_int(payload_size, false).into()],
            "static.finalize.incident.report.status",
        )
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?
        .try_as_basic_value()
        .basic()
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    builder
        .build_return(Some(&status))
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    Ok(callback)
}

fn declare_static_incident_destroyer<'context>(
    module: &Module<'context>,
    mappings: &CodegenMappings,
    mapping: &CodegenStaticStorageMapping,
    payload_size: u64,
    payload_alignment: u64,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<FunctionValue<'context>, CodegenFailure> {
    let name = format!("{}.incident.destroy", mapping.finalize_name());

    if let Some(callback) = module.get_function(&name) {
        return Ok(callback);
    }

    let context = types.context();
    let usize = crate::native::pointer_integer_type(context, types.target());
    let pointer = context.ptr_type(AddressSpace::default());

    let callback = declare_static_callback_with_type(
        module,
        &name,
        "static.finalize.incident.destroy",
        context.void_type().fn_type(&[usize.into()], false),
    );

    let builder = context.create_builder();

    let entry = callback
        .get_first_basic_block()
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let payload = callback
        .get_first_param()
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?
        .into_int_value();

    builder.position_at_end(entry);

    let payload_pointer = builder
        .build_int_to_ptr(payload, pointer, "static.finalize.incident.pointer")
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    let cleanup = mapping
        .finalization()
        .and_then(bray_codegen::CodegenStaticFinalization::incident_cleanup)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let symbol = mappings
        .instance_symbol(cleanup)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let function = module
        .get_function(symbol.name().as_str())
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let call = builder
        .build_call(function, &[payload_pointer.into()], "")
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    call.set_call_convention(function.get_call_conventions());
    apply_signature_call_attributes(call, symbol.signature(), types)?;

    let deallocation = module
        .get_function(bray_runtime_abi::MEMORY_DEALLOCATION_SYMBOL)
        .unwrap_or_else(|| {
            module.add_function(
                bray_runtime_abi::MEMORY_DEALLOCATION_SYMBOL,
                context
                    .void_type()
                    .fn_type(&[pointer.into(), usize.into(), usize.into()], false),
                None,
            )
        });

    builder
        .build_call(
            deallocation,
            &[
                payload_pointer.into(),
                usize.const_int(payload_size, false).into(),
                usize.const_int(payload_alignment, false).into(),
            ],
            "",
        )
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    builder
        .build_return(None)
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    Ok(callback)
}

fn static_incident_value<'context>(
    builder: &Builder<'context>,
    finalization: &bray_codegen::CodegenStaticFinalization,
    payload: PointerValue<'context>,
    report: FunctionValue<'context>,
    destroy: FunctionValue<'context>,
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<StructValue<'context>, CodegenFailure> {
    let context = types.context();
    let usize = crate::native::pointer_integer_type(context, types.target());
    let pointer = context.ptr_type(AddressSpace::default());

    let identity = finalization
        .error_type_identity()
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let identity = context.i8_type().const_array(
        &identity
            .into_iter()
            .map(|byte| context.i8_type().const_int(u64::from(byte), false))
            .collect::<Vec<_>>(),
    );

    let source = native_source_anchor_value(context, finalization.source());
    let incident_type = static_incident_type(context, usize, pointer);
    let incident = incident_type.const_zero();

    let incident = builder
        .build_insert_value(
            incident,
            builder
                .build_ptr_to_int(payload, usize, "static.finalize.incident.address")
                .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?,
            0,
            "static.finalize.incident.payload",
        )
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?
        .into_struct_value();

    let incident = builder
        .build_insert_value(incident, identity, 1, "static.finalize.incident.type")
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?
        .into_struct_value();

    let incident = builder
        .build_insert_value(incident, source, 2, "static.finalize.incident.source")
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?
        .into_struct_value();

    let incident = builder
        .build_insert_value(
            incident,
            report.as_global_value().as_pointer_value(),
            3,
            "static.finalize.incident.report",
        )
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?
        .into_struct_value();

    builder
        .build_insert_value(
            incident,
            destroy.as_global_value().as_pointer_value(),
            4,
            "static.finalize.incident.destroy",
        )
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)
        .map(inkwell::values::AggregateValueEnum::into_struct_value)
}

fn static_incident_type<'context>(
    context: &'context inkwell::context::Context,
    usize: inkwell::types::IntType<'context>,
    pointer: PointerType<'context>,
) -> StructType<'context> {
    context.struct_type(
        &[
            usize.into(),
            context.i8_type().array_type(32).into(),
            crate::native::source_anchor_type(context).into(),
            pointer.into(),
            pointer.into(),
        ],
        false,
    )
}

fn native_source_anchor_value<'context>(
    context: &'context inkwell::context::Context,
    source: Option<&bray_ir::MirSourceAnchor>,
) -> StructValue<'context> {
    let source = match source {
        Some(bray_ir::MirSourceAnchor::Source(origin)) => {
            let anchor = origin.source_anchor();
            let syntax = anchor.syntax();
            let range = syntax.full_range();

            bray_runtime_abi::NativeSourceAnchor::new(
                syntax.source_id().raw(),
                range.start().bytes(),
                range.end().bytes(),
                anchor.source_version().raw(),
            )
        }
        Some(
            bray_ir::MirSourceAnchor::ExecutableHost(_)
            | bray_ir::MirSourceAnchor::GeneratedLifecycle(_)
            | bray_ir::MirSourceAnchor::ImportedExecutable(_),
        )
        | None => bray_runtime_abi::NativeSourceAnchor::unavailable(),
    };

    crate::native::source_anchor_type(context).const_named_struct(&[
        context
            .i32_type()
            .const_int(u64::from(source.is_available()), false)
            .into(),
        context
            .i32_type()
            .const_int(u64::from(source.source()), false)
            .into(),
        context
            .i32_type()
            .const_int(u64::from(source.start()), false)
            .into(),
        context
            .i32_type()
            .const_int(u64::from(source.end()), false)
            .into(),
        context.i64_type().const_int(source.version(), false).into(),
    ])
}
