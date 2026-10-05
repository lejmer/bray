use bray_codegen::{
    CodegenFailure, CodegenMappings, CodegenResultMapping, CodegenStaticStorageMapping,
    CodegenTypeKind,
};
use bray_runtime_interface::ExecutableEntryResult;
use inkwell::module::Module;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{FunctionValue, PointerValue};
use inkwell::{AddressSpace, IntPredicate};

use super::super::LlvmTypeMappings;
use super::boundary::{
    branch_on_static_failure, clean_failed_static_allocation, invoke_static_boundary,
    mapped_instance_function, static_outcome,
};
use super::host::StaticFinalizerCallbacks;
use super::incident::{
    declare_static_incident_destroyer, declare_static_incident_reporter, static_incident_value,
    store_static_incident,
};
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
                        .expect(
                            "static-storage realization requires an established mapping or value",
                        );

                    (layout.size(), layout.alignment().get())
                }
                ExecutableEntryResult::I32 => {
                    panic!("static-storage realization violated an established compiler contract");
                }
            };

            (u64::from(execution.code()), size, alignment)
        }
    };

    let resolve = declare_static_finalizer_resolver(module, mappings, mapping, types)?;
    let start = declare_static_finalizer_start(module, mappings, mapping, storage, resolve, types)?;

    Ok(StaticFinalizerCallbacks {
        execution,
        outgoing_capacity: mapping.outgoing_capacity(),
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
        .expect("static-storage realization requires an established mapping or value");

    builder.position_at_end(entry);

    if let Some(finalization) = mapping.finalization() {
        let symbol = mappings
            .instance_symbol(finalization.instance())
            .expect("static-storage realization requires an established mapping or value");

        let function = module
            .get_function(symbol.name().as_str())
            .expect("static-storage realization requires an established mapping or value");

        let supplied_destination = callback
            .get_first_param()
            .expect("static-storage realization requires an established mapping or value")
            .into_int_value();

        let destination = match (finalization.execution(), finalization.result()) {
            (
                bray_symbols::CallableExecution::Synchronous,
                ExecutableEntryResult::Fallible { ty, .. },
            ) => builder
                .build_alloca(types.map(ty)?, "static.finalize.result")
                .map_err(CodegenFailure::backend_library)?,
            (bray_symbols::CallableExecution::Synchronous, ExecutableEntryResult::Unit) => {
                context.ptr_type(AddressSpace::default()).const_null()
            }
            (bray_symbols::CallableExecution::Synchronous, ExecutableEntryResult::I32) => {
                panic!("static-storage realization violated an established compiler contract");
            }
            (bray_symbols::CallableExecution::Asynchronous, _) => builder
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

        let call = invoke_static_boundary(
            &builder,
            function,
            symbol.signature(),
            &arguments,
            static_outcome(callback),
            name,
            types,
        )?;

        let continued = branch_on_static_failure(&builder, callback, types)?;

        builder
            .build_return(Some(&context.i32_type().const_zero()))
            .map_err(CodegenFailure::backend_library)?;

        builder.position_at_end(continued);

        if matches!(
            symbol.signature().result(),
            CodegenResultMapping::Direct { .. }
        ) {
            let result = call
                .try_as_basic_value()
                .basic()
                .expect("static-storage realization requires an established mapping or value");

            builder
                .build_store(destination, result)
                .map_err(CodegenFailure::backend_library)?;
        }

        if finalization.execution() == bray_symbols::CallableExecution::Synchronous
            && matches!(
                finalization.result(),
                ExecutableEntryResult::Fallible { .. }
            )
        {
            let completion = builder
                .build_ptr_to_int(destination, usize, "static.finalize.completion")
                .map_err(CodegenFailure::backend_library)?;

            let status = builder
                .build_call(
                    resolve,
                    &[
                        completion.into(),
                        supplied_destination.into(),
                        static_outcome(callback).into(),
                    ],
                    "static.finalize.status",
                )
                .map_err(CodegenFailure::backend_library)?
                .try_as_basic_value()
                .basic()
                .expect("static-storage realization requires an established mapping or value");

            builder
                .build_return(Some(&status))
                .map_err(CodegenFailure::backend_library)?;

            return Ok(callback);
        }
    }

    builder
        .build_return(Some(&context.i32_type().const_zero()))
        .map_err(CodegenFailure::backend_library)?;

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
        context.i32_type().fn_type(
            &[
                usize.into(),
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
        .expect("static-storage realization requires an established mapping or value");

    builder.position_at_end(entry);

    let Some(finalization) = mapping.finalization() else {
        builder
            .build_return(Some(&context.i32_type().const_zero()))
            .map_err(CodegenFailure::backend_library)?;

        return Ok(callback);
    };

    let ExecutableEntryResult::Fallible {
        ty,
        error,
        success_variant,
    } = finalization.result()
    else {
        builder
            .build_return(Some(&context.i32_type().const_zero()))
            .map_err(CodegenFailure::backend_library)?;

        return Ok(callback);
    };

    let result = callback
        .get_first_param()
        .expect("static-storage realization requires an established mapping or value")
        .into_int_value();

    let incident_destination = callback
        .get_nth_param(1)
        .expect("static-storage realization requires an established mapping or value")
        .into_int_value();

    let result = builder
        .build_int_to_ptr(
            result,
            context.ptr_type(AddressSpace::default()),
            "static.finalize.completion",
        )
        .map_err(CodegenFailure::backend_library)?;

    let result_mapping = mappings
        .instance_ty(mapping.owner(), ty)
        .expect("static-storage realization requires an established mapping or value");

    let CodegenTypeKind::Union { tag, .. } = result_mapping.kind() else {
        panic!("static-storage realization violated an established compiler contract");
    };

    let Some(tag) = *tag else {
        panic!("static-storage realization violated an established compiler contract");
    };

    let BasicTypeEnum::IntType(tag_type) = types.map(tag)? else {
        panic!("static-storage realization violated an established compiler contract");
    };

    let tag = builder
        .build_load(tag_type, result, "static.finalize.tag")
        .map_err(CodegenFailure::backend_library)?
        .into_int_value();

    let success = result_mapping
        .kind()
        .union_variant(success_variant)
        .and_then(bray_codegen::CodegenUnionVariantLayout::tag)
        .expect("static-storage realization requires an established mapping or value");

    let success = crate::translation::integer_constant(tag_type, success);

    let succeeded = builder
        .build_int_compare(IntPredicate::EQ, tag, success, "static.finalize.succeeded")
        .map_err(CodegenFailure::backend_library)?;

    let success_block = context.append_basic_block(callback, "static.finalize.success");
    let failure_block = context.append_basic_block(callback, "static.finalize.failure");

    builder
        .build_conditional_branch(succeeded, success_block, failure_block)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(success_block);

    builder
        .build_return(Some(&context.i32_type().const_zero()))
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(failure_block);

    let (_, error_field) = result_mapping
        .kind()
        .fallible_error_field(success_variant, error);

    let error_address = builder
        .build_ptr_to_int(result, usize, "static.finalize.error.base")
        .and_then(|address| {
            builder.build_int_add(
                address,
                usize.const_int(error_field.offset_bytes(), false),
                "static.finalize.error",
            )
        })
        .map_err(CodegenFailure::backend_library)?;

    let error_layout = mappings
        .instance_ty(mapping.owner(), error)
        .and_then(bray_codegen::CodegenTypeMapping::layout)
        .expect("static-storage realization requires an established mapping or value");

    let pointer = context.ptr_type(AddressSpace::default());

    let memory = finalization
        .incident_memory()
        .expect("static-storage realization requires an established mapping or value");

    let (allocation, allocation_signature) =
        mapped_instance_function(module, mappings, memory.allocation());

    let allocation_call = invoke_static_boundary(
        &builder,
        allocation,
        allocation_signature,
        &[
            usize.const_int(error_layout.size(), false).into(),
            usize
                .const_int(error_layout.alignment().get(), false)
                .into(),
        ],
        static_outcome(callback),
        "static.finalize.incident.payload",
        types,
    )?;

    let continued = branch_on_static_failure(&builder, callback, types)?;

    clean_failed_static_allocation(
        module,
        mappings,
        mapping,
        &builder,
        error_address,
        static_outcome(callback),
        types,
    )?;

    builder.position_at_end(continued);

    let payload = allocation_call
        .try_as_basic_value()
        .basic()
        .expect("static-storage realization requires an established mapping or value")
        .into_pointer_value();

    let error_pointer = builder
        .build_int_to_ptr(error_address, pointer, "static.finalize.error.pointer")
        .map_err(CodegenFailure::backend_library)?;

    let alignment = crate::conversion::target_value(
        error_layout.alignment().get(),
        "static_finalization_error_alignment",
    )?;

    builder
        .build_memcpy(
            payload,
            alignment,
            error_pointer,
            alignment,
            usize.const_int(error_layout.size(), false),
        )
        .map_err(CodegenFailure::backend_library)?;

    let report = declare_static_incident_reporter(module, mapping, error_layout.size(), types)?;

    let destroy = declare_static_incident_destroyer(
        module,
        mappings,
        mapping,
        error_layout.size(),
        error_layout.alignment().get(),
        types,
    )?;

    let incident = static_incident_value(
        &builder,
        mapping,
        finalization,
        payload,
        report,
        destroy,
        types,
    )?;

    store_static_incident(
        module,
        mappings,
        mapping,
        types,
        &builder,
        incident,
        incident_destination,
    )?;

    builder
        .build_return(Some(&context.i32_type().const_int(1, false)))
        .map_err(CodegenFailure::backend_library)?;

    Ok(callback)
}
