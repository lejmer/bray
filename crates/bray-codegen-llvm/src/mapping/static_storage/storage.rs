use bray_codegen::{
    CodegenFailure, CodegenMappings, CodegenProductHostMapping, CodegenStaticStorageMapping,
};
use inkwell::module::{Linkage, Module};
use inkwell::types::{BasicTypeEnum, PointerType};
use inkwell::values::{BasicValueEnum, FunctionValue, GlobalValue, PointerValue};
use inkwell::{GlobalVisibility, IntPredicate};

use super::super::LlvmTypeMappings;
use super::super::symbol::apply_signature_call_attributes;
use super::constant::static_initializer;
use super::host::{
    declare_product_host, declare_static_host_entry, declare_thread_static_registration,
    retain_globals,
};

pub(in crate::mapping) fn declare_static_storages<'context, 'mappings>(
    module: &Module<'context>,
    mappings: &'mappings CodegenMappings,
    types: &mut LlvmTypeMappings<'context, 'mappings>,
) -> Result<(), CodegenFailure> {
    let product_host = mappings.product_host();
    let mut host_entries = Vec::new();

    for mapping in mappings.static_storages() {
        types.select_instance(mapping.owner());

        let initializer = static_initializer(module, mapping, mappings, types)?;

        let alignment = mappings
            .instance_ty(mapping.owner(), mapping.ty())
            .and_then(bray_codegen::CodegenTypeMapping::layout)
            .and_then(|layout| u32::try_from(layout.alignment().get()).ok())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let global = declare_static_global(module, mapping, initializer, alignment);
        let attachment = declare_static_attachment(module, mapping, types)?;

        let cleanup = declare_static_cleanup(
            module,
            mappings,
            mapping,
            global.as_pointer_value(),
            attachment.map(GlobalValue::as_pointer_value),
            initializer,
            types,
        )?;

        let accessor = declare_static_accessor(
            module,
            mapping,
            global.as_pointer_value(),
            attachment.map(GlobalValue::as_pointer_value),
            initializer,
            cleanup,
            product_host,
            types,
        )?;

        let host_mapping = product_host
            .and_then(|host| {
                host.statics()
                    .iter()
                    .find(|entry| entry.host_symbol().as_str() == mapping.host_name())
            })
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        host_entries.push(declare_static_host_entry(
            module,
            mapping,
            host_mapping,
            global.as_pointer_value(),
            accessor,
            cleanup,
            types,
        )?);
    }

    retain_globals(module, &host_entries, "llvm.compiler.used", types)?;

    if let Some(product_host) = product_host {
        declare_product_host(module, product_host, mappings, types)?;
    }

    Ok(())
}

fn declare_static_global<'context>(
    module: &Module<'context>,
    mapping: &CodegenStaticStorageMapping,
    initializer: BasicValueEnum<'context>,
    alignment: u32,
) -> GlobalValue<'context> {
    let name = mapping.symbol().as_str();

    module.get_global(name).unwrap_or_else(|| {
        let global = module.add_global(initializer.get_type(), None, name);

        global.set_initializer(&initializer);
        global.set_alignment(alignment);
        global.set_linkage(Linkage::WeakODR);
        global.set_visibility(GlobalVisibility::Hidden);

        if mapping.instance().duration() == bray_symbols::StaticStorageDuration::ExactThread {
            global.set_thread_local(true);
        }

        global.set_comdat(module.get_or_insert_comdat(name));

        global
    })
}

fn declare_static_attachment<'context>(
    module: &Module<'context>,
    mapping: &CodegenStaticStorageMapping,
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<Option<GlobalValue<'context>>, CodegenFailure> {
    if mapping.instance().duration() != bray_symbols::StaticStorageDuration::ExactThread {
        return Ok(None);
    }

    let name = mapping.attachment_name();
    let context = types.context();

    let attachment = module.get_global(&name).unwrap_or_else(|| {
        let attachment = module.add_global(context.i64_type(), None, &name);

        attachment.set_initializer(&context.i64_type().const_zero());
        attachment.set_linkage(Linkage::WeakODR);
        attachment.set_visibility(GlobalVisibility::Hidden);
        attachment.set_thread_local(true);
        attachment.set_comdat(module.get_or_insert_comdat(&name));

        attachment
    });

    Ok(Some(attachment))
}

#[expect(
    clippy::too_many_arguments,
    reason = "the accessor receives every paired native storage and callback explicitly"
)]
fn declare_static_accessor<'context>(
    module: &Module<'context>,
    mapping: &CodegenStaticStorageMapping,
    storage: PointerValue<'context>,
    attachment: Option<PointerValue<'context>>,
    initializer: BasicValueEnum<'context>,
    cleanup: FunctionValue<'context>,
    product_host: Option<&CodegenProductHostMapping>,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<FunctionValue<'context>, CodegenFailure> {
    let accessor_name = mapping.accessor_name();

    if let Some(accessor) = module.get_function(&accessor_name) {
        return Ok(accessor);
    }

    let pointer = pointer_type(types)?;
    let accessor = module.add_function(&accessor_name, pointer.fn_type(&[], false), None);

    accessor.set_linkage(Linkage::WeakODR);

    accessor
        .as_global_value()
        .set_visibility(GlobalVisibility::Hidden);

    accessor
        .as_global_value()
        .set_comdat(module.get_or_insert_comdat(&accessor_name));

    let context = types.context();
    let builder = context.create_builder();
    let entry = context.append_basic_block(accessor, "static.access");

    builder.position_at_end(entry);

    if let Some(attachment) = attachment {
        let product_host = product_host.ok_or(CodegenFailure::GeneratedModuleInvariant)?;
        let identity_name = bray_runtime_abi::THREAD_ATTACHMENT_IDENTITY_SYMBOL;

        let identity = module.get_function(identity_name).unwrap_or_else(|| {
            module.add_function(identity_name, context.i64_type().fn_type(&[], false), None)
        });

        let current = builder
            .build_call(identity, &[], "static.attachment.current")
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?
            .try_as_basic_value()
            .basic()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?
            .into_int_value();

        let previous = builder
            .build_load(context.i64_type(), attachment, "static.attachment.previous")
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?
            .into_int_value();

        let changed = builder
            .build_int_compare(
                IntPredicate::NE,
                current,
                previous,
                "static.attachment.changed",
            )
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

        let cleaning = builder
            .build_int_compare(
                IntPredicate::EQ,
                previous,
                context.i64_type().const_all_ones(),
                "static.attachment.cleaning",
            )
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

        let available = builder
            .build_not(cleaning, "static.attachment.available")
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

        let initialize = builder
            .build_and(changed, available, "static.attachment.initialize")
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

        let initialize_block = context.append_basic_block(accessor, "static.attachment.initialize");
        let ready = context.append_basic_block(accessor, "static.attachment.ready");

        builder
            .build_conditional_branch(initialize, initialize_block, ready)
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

        builder.position_at_end(initialize_block);

        builder
            .build_store(storage, initializer)
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

        let registration = declare_thread_static_registration(
            module,
            product_host,
            mapping,
            cleanup,
            pointer,
            types,
        )?;

        let register_name = bray_runtime_abi::THREAD_STATIC_CLEANUP_REGISTRATION_SYMBOL;

        let register = module.get_function(register_name).unwrap_or_else(|| {
            module.add_function(
                register_name,
                context.i32_type().fn_type(&[pointer.into()], false),
                None,
            )
        });

        let status = builder
            .build_call(
                register,
                &[registration.as_pointer_value().into()],
                "static.register",
            )
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?
            .try_as_basic_value()
            .basic()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?
            .into_int_value();

        let registered = builder
            .build_int_compare(
                IntPredicate::EQ,
                status,
                context.i32_type().const_zero(),
                "static.registered",
            )
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

        let registered_block = context.append_basic_block(accessor, "static.attachment.registered");

        builder
            .build_conditional_branch(registered, registered_block, ready)
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

        builder.position_at_end(registered_block);

        builder
            .build_store(attachment, current)
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

        builder
            .build_unconditional_branch(ready)
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

        builder.position_at_end(ready);
    }

    builder
        .build_return(Some(&storage))
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    Ok(accessor)
}

fn declare_static_cleanup<'context>(
    module: &Module<'context>,
    mappings: &CodegenMappings,
    mapping: &CodegenStaticStorageMapping,
    storage: PointerValue<'context>,
    attachment: Option<PointerValue<'context>>,
    initializer: BasicValueEnum<'context>,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<FunctionValue<'context>, CodegenFailure> {
    let name = mapping.cleanup_name();

    if let Some(cleanup) = module.get_function(&name) {
        return Ok(cleanup);
    }

    let context = types.context();
    let cleanup = module.add_function(&name, context.void_type().fn_type(&[], false), None);

    cleanup.set_linkage(Linkage::WeakODR);

    cleanup
        .as_global_value()
        .set_visibility(GlobalVisibility::Hidden);

    cleanup
        .as_global_value()
        .set_comdat(module.get_or_insert_comdat(&name));

    let builder = context.create_builder();
    let entry = context.append_basic_block(cleanup, "static.cleanup");

    builder.position_at_end(entry);

    let cleanup_storage = if mapping.cleanup().is_some() {
        let value = builder
            .build_load(types.map(mapping.ty())?, storage, "static.cleanup.value")
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

        let cleanup_storage = builder
            .build_alloca(types.map(mapping.ty())?, "static.cleanup.storage")
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

        builder
            .build_store(cleanup_storage, value)
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

        Some(cleanup_storage)
    } else {
        None
    };

    builder
        .build_store(storage, initializer)
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    if let Some(attachment) = attachment {
        builder
            .build_store(attachment, context.i64_type().const_all_ones())
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;
    }

    if let Some(instance) = mapping.cleanup() {
        let symbol = mappings
            .instance_symbol(instance)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let function = module
            .get_function(symbol.name().as_str())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let call = builder
            .build_call(
                function,
                &[cleanup_storage
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?
                    .into()],
                "",
            )
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

        call.set_call_convention(function.get_call_conventions());
        apply_signature_call_attributes(call, symbol.signature(), types)?;
    }

    if let Some(attachment) = attachment {
        builder
            .build_store(attachment, context.i64_type().const_zero())
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;
    }

    builder
        .build_return(None)
        .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

    Ok(cleanup)
}

pub(super) fn pointer_type<'context>(
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<PointerType<'context>, CodegenFailure> {
    let BasicTypeEnum::PointerType(pointer) = types.default_pointer_type()? else {
        return Err(CodegenFailure::GeneratedModuleInvariant);
    };

    Ok(pointer)
}
