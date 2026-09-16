use bray_codegen::{
    CodegenFailure, CodegenMappings, CodegenProductHostMapping, CodegenStaticStorageMapping,
    CodegenTarget,
};
use inkwell::module::{Linkage, Module};
use inkwell::types::{BasicTypeEnum, FunctionType, PointerType};
use inkwell::values::{BasicValueEnum, FunctionValue, GlobalValue, PointerValue};
use inkwell::{DLLStorageClass, GlobalVisibility, IntPredicate};

use super::super::LlvmTypeMappings;
use super::boundary::{invoke_static_boundary, mapped_instance_function, static_outcome};
use super::constant::static_initializer;
use super::finalization::declare_static_finalizer;
use super::host::{
    StaticLifecycleCallbacks, declare_product_host, declare_static_host_entry,
    declare_thread_static_registration, retain_globals,
};

pub(in crate::mapping) fn declare_static_storages<'context, 'mappings>(
    module: &Module<'context>,
    mappings: &'mappings CodegenMappings,
    target: &CodegenTarget,
    types: &mut LlvmTypeMappings<'context, 'mappings>,
) -> Result<(), CodegenFailure> {
    let product_host = mappings.product_host();
    let mut retained_globals = Vec::new();

    for mapping in mappings.static_storages() {
        types.select_instance(mapping.owner());

        let initializer = static_initializer(module, mapping, mappings, types)?;

        let alignment = mappings
            .instance_ty(mapping.owner(), mapping.ty())
            .and_then(bray_codegen::CodegenTypeMapping::layout)
            .and_then(|layout| u32::try_from(layout.alignment().get()).ok())
            .expect("static-storage realization requires an established mapping or value");

        let global = declare_static_global(module, mapping, target, initializer, alignment);

        if mapping.native_binding().is_some() && mapping.defines_storage() {
            retained_globals.push(global);
        }

        let attachment = declare_static_attachment(module, mapping, types)?;

        let callbacks = declare_static_lifecycle_callbacks(
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
            callbacks,
            product_host,
            types,
        )?;

        let host_mapping = product_host
            .and_then(|host| {
                host.statics()
                    .iter()
                    .find(|entry| entry.host_symbol().as_str() == mapping.host_name())
            })
            .expect("static-storage realization requires an established mapping or value");

        retained_globals.push(declare_static_host_entry(
            module,
            mapping,
            host_mapping,
            global.as_pointer_value(),
            accessor,
            callbacks,
            types,
        )?);
    }

    retain_globals(module, &retained_globals, "llvm.compiler.used", types)?;

    if let Some(product_host) = product_host {
        declare_product_host(module, product_host, mappings, types)?;
    }

    Ok(())
}

fn declare_static_global<'context>(
    module: &Module<'context>,
    mapping: &CodegenStaticStorageMapping,
    target: &CodegenTarget,
    initializer: BasicValueEnum<'context>,
    alignment: u32,
) -> GlobalValue<'context> {
    let name = mapping.symbol().as_str();

    let global = module
        .get_global(name)
        .unwrap_or_else(|| module.add_global(initializer.get_type(), None, name));

    if let Some(binding) = mapping.native_binding() {
        if mapping.defines_storage() {
            global.set_initializer(&initializer);
            global.set_alignment(alignment);

            global.set_linkage(native_static_definition_linkage(binding));
        } else {
            global.set_linkage(Linkage::External);
        }

        global.set_visibility(GlobalVisibility::Default);

        if mapping.defines_storage()
            && target.machine().object_format() == bray_target::ObjectFormat::Coff
        {
            global.set_dll_storage_class(DLLStorageClass::Export);
        }
    } else {
        global.set_initializer(&initializer);
        global.set_alignment(alignment);
        global.set_linkage(Linkage::WeakODR);
        global.set_visibility(GlobalVisibility::Hidden);
    }

    if mapping.instance().duration() == bray_symbols::StaticStorageDuration::ExactThread {
        global.set_thread_local(true);
    }

    if mapping.native_binding().is_none() {
        crate::comdat::attach(module, global, name, target.machine().object_format());
    }

    global
}

const fn native_static_definition_linkage(binding: bray_symbols::NativeSymbolBinding) -> Linkage {
    match binding {
        bray_symbols::NativeSymbolBinding::Strong => Linkage::External,
        bray_symbols::NativeSymbolBinding::Weak => Linkage::WeakAny,
    }
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

        crate::comdat::attach(
            module,
            attachment,
            &name,
            types.target().machine().object_format(),
        );

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
    callbacks: StaticLifecycleCallbacks<'context>,
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

    crate::comdat::attach(
        module,
        accessor.as_global_value(),
        &accessor_name,
        types.target().machine().object_format(),
    );

    let context = types.context();
    let builder = context.create_builder();
    let entry = context.append_basic_block(accessor, "static.access");

    builder.position_at_end(entry);

    if let Some(attachment) = attachment {
        let product_host = product_host
            .expect("static-storage realization requires an established mapping or value");

        let registration = declare_thread_static_registration(
            module,
            product_host,
            mapping,
            callbacks,
            pointer,
            types,
        )?;

        let descriptor = module
            .get_global(product_host.descriptor_symbol().as_str())
            .expect("static-storage realization requires an established mapping or value");

        let identity = crate::native::declare_runtime_function(
            module,
            context,
            types.target(),
            bray_runtime_interface::RuntimeAbiRole::ThreadAttachmentIdentity,
        )?;

        let current = builder
            .build_call(
                identity,
                &[descriptor.as_pointer_value().into()],
                "static.attachment.current",
            )
            .map_err(CodegenFailure::backend_library)?
            .try_as_basic_value()
            .basic()
            .expect("static-storage realization requires an established mapping or value")
            .into_int_value();

        let previous = builder
            .build_load(context.i64_type(), attachment, "static.attachment.previous")
            .map_err(CodegenFailure::backend_library)?
            .into_int_value();

        let changed = builder
            .build_int_compare(
                IntPredicate::NE,
                current,
                previous,
                "static.attachment.changed",
            )
            .map_err(CodegenFailure::backend_library)?;

        let cleaning = builder
            .build_int_compare(
                IntPredicate::EQ,
                previous,
                context.i64_type().const_all_ones(),
                "static.attachment.cleaning",
            )
            .map_err(CodegenFailure::backend_library)?;

        let available = builder
            .build_int_compare(
                IntPredicate::NE,
                current,
                context.i64_type().const_zero(),
                "static.attachment.attached",
            )
            .map_err(CodegenFailure::backend_library)?;

        let not_cleaning = builder
            .build_not(cleaning, "static.attachment.not_cleaning")
            .map_err(CodegenFailure::backend_library)?;

        let available = builder
            .build_and(available, not_cleaning, "static.attachment.available")
            .map_err(CodegenFailure::backend_library)?;

        let decision = context.append_basic_block(accessor, "static.attachment.decision");
        let initialize_block = context.append_basic_block(accessor, "static.attachment.initialize");
        let ready = context.append_basic_block(accessor, "static.attachment.ready");
        let unavailable = context.append_basic_block(accessor, "static.attachment.unavailable");

        builder
            .build_conditional_branch(available, decision, unavailable)
            .map_err(CodegenFailure::backend_library)?;

        builder.position_at_end(decision);

        builder
            .build_conditional_branch(changed, initialize_block, ready)
            .map_err(CodegenFailure::backend_library)?;

        builder.position_at_end(initialize_block);

        builder
            .build_store(storage, initializer)
            .map_err(CodegenFailure::backend_library)?;

        let register = crate::native::declare_runtime_function(
            module,
            context,
            types.target(),
            bray_runtime_interface::RuntimeAbiRole::ThreadStaticCleanupRegistration,
        )?;

        let status = builder
            .build_call(
                register,
                &[registration.as_pointer_value().into()],
                "static.register",
            )
            .map_err(CodegenFailure::backend_library)?
            .try_as_basic_value()
            .basic()
            .expect("static-storage realization requires an established mapping or value")
            .into_int_value();

        let registered = builder
            .build_int_compare(
                IntPredicate::EQ,
                status,
                context.i32_type().const_zero(),
                "static.registered",
            )
            .map_err(CodegenFailure::backend_library)?;

        let registered_block = context.append_basic_block(accessor, "static.attachment.registered");

        builder
            .build_conditional_branch(registered, registered_block, unavailable)
            .map_err(CodegenFailure::backend_library)?;

        builder.position_at_end(registered_block);

        builder
            .build_store(attachment, current)
            .map_err(CodegenFailure::backend_library)?;

        builder
            .build_unconditional_branch(ready)
            .map_err(CodegenFailure::backend_library)?;

        builder.position_at_end(ready);

        builder
            .build_return(Some(&storage))
            .map_err(CodegenFailure::backend_library)?;

        builder.position_at_end(unavailable);

        builder
            .build_return(Some(&pointer.const_null()))
            .map_err(CodegenFailure::backend_library)?;

        return Ok(accessor);
    }

    builder
        .build_return(Some(&storage))
        .map_err(CodegenFailure::backend_library)?;

    Ok(accessor)
}

fn declare_static_lifecycle_callbacks<'context>(
    module: &Module<'context>,
    mappings: &CodegenMappings,
    mapping: &CodegenStaticStorageMapping,
    storage: PointerValue<'context>,
    attachment: Option<PointerValue<'context>>,
    initializer: BasicValueEnum<'context>,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<StaticLifecycleCallbacks<'context>, CodegenFailure> {
    let prepare = declare_static_prepare(module, mapping, attachment, types)?;

    let finalizer = declare_static_finalizer(module, mappings, mapping, storage, types)?;

    let destroy = declare_static_lifecycle_phase(
        module,
        mappings,
        mapping.destroy(),
        &mapping.destroy_name(),
        "static.destroy",
        storage,
        types,
    )?;

    let detach = declare_static_detach(module, mapping, storage, attachment, initializer, types)?;

    Ok(StaticLifecycleCallbacks {
        prepare,
        finalizer,
        destroy,
        detach,
    })
}

fn declare_static_prepare<'context>(
    module: &Module<'context>,
    mapping: &CodegenStaticStorageMapping,
    attachment: Option<PointerValue<'context>>,
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<FunctionValue<'context>, CodegenFailure> {
    let name = mapping.prepare_name();

    if let Some(prepare) = module.get_function(&name) {
        return Ok(prepare);
    }

    let context = types.context();
    let prepare = declare_static_callback(module, &name, "static.prepare", types);
    let builder = context.create_builder();

    let entry = prepare
        .get_first_basic_block()
        .expect("static-storage realization requires an established mapping or value");

    builder.position_at_end(entry);

    if let Some(attachment) = attachment {
        builder
            .build_store(attachment, context.i64_type().const_all_ones())
            .map_err(CodegenFailure::backend_library)?;
    }

    builder
        .build_return(None)
        .map_err(CodegenFailure::backend_library)?;

    Ok(prepare)
}

#[expect(
    clippy::too_many_arguments,
    reason = "one lifecycle phase keeps its selected native function and storage explicit"
)]
fn declare_static_lifecycle_phase<'context>(
    module: &Module<'context>,
    mappings: &CodegenMappings,
    instance: Option<&bray_codegen::CodegenInstanceKey>,
    name: &str,
    block_name: &str,
    storage: PointerValue<'context>,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<FunctionValue<'context>, CodegenFailure> {
    if let Some(callback) = module.get_function(name) {
        return Ok(callback);
    }

    let callback = declare_static_callback_with_type(
        module,
        name,
        block_name,
        types
            .context()
            .void_type()
            .fn_type(&[pointer_type(types)?.into()], false),
        types,
    );

    let builder = types.context().create_builder();

    let entry = callback
        .get_first_basic_block()
        .expect("static-storage realization requires an established mapping or value");

    builder.position_at_end(entry);

    if let Some(instance) = instance {
        let (function, signature) = mapped_instance_function(module, mappings, instance);

        invoke_static_boundary(
            &builder,
            function,
            signature,
            &[storage.into()],
            static_outcome(callback),
            "",
            types,
        )?;
    }

    builder
        .build_return(None)
        .map_err(CodegenFailure::backend_library)?;

    Ok(callback)
}

fn declare_static_detach<'context>(
    module: &Module<'context>,
    mapping: &CodegenStaticStorageMapping,
    storage: PointerValue<'context>,
    attachment: Option<PointerValue<'context>>,
    initializer: BasicValueEnum<'context>,
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<FunctionValue<'context>, CodegenFailure> {
    let name = mapping.detach_name();

    if let Some(detach) = module.get_function(&name) {
        return Ok(detach);
    }

    let context = types.context();
    let detach = declare_static_callback(module, &name, "static.detach", types);
    let builder = context.create_builder();

    let entry = detach
        .get_first_basic_block()
        .expect("static-storage realization requires an established mapping or value");

    builder.position_at_end(entry);

    if let Some(attachment) = attachment {
        builder
            .build_store(storage, initializer)
            .map_err(CodegenFailure::backend_library)?;

        builder
            .build_store(attachment, context.i64_type().const_zero())
            .map_err(CodegenFailure::backend_library)?;
    }

    builder
        .build_return(None)
        .map_err(CodegenFailure::backend_library)?;

    Ok(detach)
}

fn declare_static_callback<'context>(
    module: &Module<'context>,
    name: &str,
    block_name: &str,
    types: &LlvmTypeMappings<'context, '_>,
) -> FunctionValue<'context> {
    declare_static_callback_with_type(
        module,
        name,
        block_name,
        types.context().void_type().fn_type(&[], false),
        types,
    )
}

pub(super) fn declare_static_callback_with_type<'context>(
    module: &Module<'context>,
    name: &str,
    block_name: &str,
    ty: FunctionType<'context>,
    types: &LlvmTypeMappings<'context, '_>,
) -> FunctionValue<'context> {
    let callback = module.add_function(name, ty, None);

    callback.set_linkage(Linkage::WeakODR);

    callback
        .as_global_value()
        .set_visibility(GlobalVisibility::Hidden);

    crate::comdat::attach_any(
        module,
        callback.as_global_value(),
        name,
        types.target().machine().object_format(),
    );

    module
        .get_context()
        .append_basic_block(callback, block_name);

    callback
}

pub(super) fn pointer_type<'context>(
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<PointerType<'context>, CodegenFailure> {
    let BasicTypeEnum::PointerType(pointer) = types.default_pointer_type()? else {
        panic!("static-storage realization violated an established compiler contract");
    };

    Ok(pointer)
}

#[cfg(test)]
mod tests {
    use bray_symbols::NativeSymbolBinding;
    use inkwell::module::Linkage;

    use super::native_static_definition_linkage;

    #[test]
    fn native_static_definitions_preserve_the_selected_binding_strength() {
        assert_eq!(
            native_static_definition_linkage(NativeSymbolBinding::Strong),
            Linkage::External
        );

        assert_eq!(
            native_static_definition_linkage(NativeSymbolBinding::Weak),
            Linkage::WeakAny
        );
    }
}
