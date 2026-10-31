use bray_codegen::{
    CodegenFailure, CodegenMappings, CodegenProductHostMapping, CodegenProductHostStatic,
    CodegenStaticStorageMapping,
};
use inkwell::attributes::AttributeLoc;
use inkwell::types::{ArrayType, IntType, PointerType, StructType};
use inkwell::values::{FunctionValue, GlobalValue, PointerValue};
use inkwell::{
    GlobalVisibility,
    module::{Linkage, Module},
};

use super::super::LlvmTypeMappings;
use super::storage::pointer_type;

#[derive(Clone, Copy)]
pub(super) struct StaticLifecycleCallbacks<'context> {
    pub(super) prepare: FunctionValue<'context>,
    pub(super) finalizer: StaticFinalizerCallbacks<'context>,
    pub(super) destroy: FunctionValue<'context>,
    pub(super) detach: FunctionValue<'context>,
}

#[derive(Clone, Copy)]
pub(super) struct StaticFinalizerCallbacks<'context> {
    pub(super) execution: u64,
    pub(super) admission: FunctionValue<'context>,
    pub(super) start: FunctionValue<'context>,
    pub(super) resolve: FunctionValue<'context>,
}

pub(super) fn declare_thread_static_registration<'context>(
    module: &Module<'context>,
    product_host: &CodegenProductHostMapping,
    mapping: &CodegenStaticStorageMapping,
    callbacks: StaticLifecycleCallbacks<'context>,
    pointer: PointerType<'context>,
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<GlobalValue<'context>, CodegenFailure> {
    let context = types.context();
    let descriptor = declare_product_descriptor(module, product_host, types)?;

    let entry = product_host
        .statics()
        .iter()
        .find(|entry| entry.host_symbol().as_str() == mapping.host_name())
        .expect("static-storage realization requires an established mapping or value");

    let registration_type = context.struct_type(
        &[
            pointer.into(),
            static_identity_type(context).into(),
            pointer.into(),
            static_finalizer_type(context, pointer).into(),
            pointer.into(),
            pointer.into(),
        ],
        false,
    );

    let initializer = registration_type.const_named_struct(&[
        descriptor.as_pointer_value().into(),
        crate::native::identity_value(context, entry.identity().bytes()).into(),
        callbacks
            .prepare
            .as_global_value()
            .as_pointer_value()
            .into(),
        static_finalizer_value(context, callbacks.finalizer, pointer).into(),
        callbacks
            .destroy
            .as_global_value()
            .as_pointer_value()
            .into(),
        callbacks.detach.as_global_value().as_pointer_value().into(),
    ]);

    let name = format!("{}.registration", mapping.prepare_name());

    let registration = module.get_global(&name).unwrap_or_else(|| {
        let registration = module.add_global(registration_type, None, &name);

        registration.set_constant(true);
        registration.set_initializer(&initializer);
        registration.set_linkage(Linkage::WeakODR);
        registration.set_visibility(GlobalVisibility::Hidden);

        crate::comdat::attach(
            module,
            registration,
            &name,
            types.target().machine().object_format(),
        );

        registration
    });

    Ok(registration)
}

pub(super) fn declare_static_host_entry<'context>(
    module: &Module<'context>,
    mapping: &CodegenStaticStorageMapping,
    host_mapping: &CodegenProductHostStatic,
    storage: PointerValue<'context>,
    accessor: FunctionValue<'context>,
    callbacks: StaticLifecycleCallbacks<'context>,
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<GlobalValue<'context>, CodegenFailure> {
    let name = mapping.host_name();

    if let Some(entry) = module.get_global(&name) {
        return Ok(entry);
    }

    let context = types.context();
    let pointer = pointer_type(types)?;
    let usize = usize_type(types)?;
    let entry_type = static_host_entry_type(context, pointer, usize);

    let dependency = declare_static_dependency_lookup(
        module,
        host_mapping,
        usize,
        context,
        types.target().machine().object_format(),
    )?;

    let duration = match mapping.instance().duration() {
        bray_symbols::StaticStorageDuration::Product => 0,
        bray_symbols::StaticStorageDuration::ExactThread => 1,
    };

    let initializer = entry_type.const_named_struct(&[
        context
            .i32_type()
            .const_int(u64::from(bray_runtime_abi::PRODUCT_HOST_ABI_VERSION), false)
            .into(),
        context.i32_type().const_int(duration, false).into(),
        crate::native::identity_value(context, host_mapping.identity().bytes()).into(),
        // The final descriptor supplies order. Reusable records must be product independent.
        context.i64_type().const_zero().into(),
        storage.into(),
        accessor.as_global_value().as_pointer_value().into(),
        callbacks
            .prepare
            .as_global_value()
            .as_pointer_value()
            .into(),
        static_finalizer_value(context, callbacks.finalizer, pointer).into(),
        callbacks
            .destroy
            .as_global_value()
            .as_pointer_value()
            .into(),
        callbacks.detach.as_global_value().as_pointer_value().into(),
        dependency.as_global_value().as_pointer_value().into(),
        usize
            .const_int(
                u64::try_from(host_mapping.dependencies().len())
                    .expect("static dependency count must fit the target index constant"),
                false,
            )
            .into(),
    ]);

    let entry = module.add_global(entry_type, None, &name);

    entry.set_constant(true);
    entry.set_initializer(&initializer);
    entry.set_linkage(Linkage::WeakODR);
    entry.set_visibility(GlobalVisibility::Hidden);

    crate::comdat::attach(
        module,
        entry,
        &name,
        types.target().machine().object_format(),
    );

    Ok(entry)
}

fn declare_static_dependency_lookup<'context>(
    module: &Module<'context>,
    mapping: &CodegenProductHostStatic,
    usize: IntType<'context>,
    context: &'context inkwell::context::Context,
    object_format: bray_target::ObjectFormat,
) -> Result<FunctionValue<'context>, CodegenFailure> {
    let name = format!("{}.dependency", mapping.host_symbol().as_str());

    if let Some(dependency) = module.get_function(&name) {
        return Ok(dependency);
    }

    let dependency = module.add_function(
        &name,
        context.void_type().fn_type(
            &[
                context.ptr_type(inkwell::AddressSpace::default()).into(),
                usize.into(),
            ],
            false,
        ),
        Some(Linkage::WeakODR),
    );

    dependency.add_attribute(
        AttributeLoc::Param(0),
        crate::native::indirect_result_attribute(context, static_identity_type(context).into())?,
    );

    let destination = dependency
        .get_first_param()
        .expect("dependency lookup owns its result slot")
        .into_pointer_value();

    dependency
        .as_global_value()
        .set_visibility(GlobalVisibility::Hidden);

    crate::comdat::attach(module, dependency.as_global_value(), &name, object_format);

    let builder = context.create_builder();
    let entry = context.append_basic_block(dependency, "static.dependency.lookup");
    let invalid = context.append_basic_block(dependency, "static.dependency.invalid");
    let mut cases = Vec::with_capacity(mapping.dependencies().len());

    for (index, identity) in mapping.dependencies().iter().copied().enumerate() {
        let block = context.append_basic_block(dependency, "static.dependency.found");

        builder.position_at_end(block);

        builder
            .build_store(
                destination,
                crate::native::identity_value(context, identity.bytes()),
            )
            .map_err(CodegenFailure::backend_library)?;

        builder
            .build_return(None)
            .map_err(CodegenFailure::backend_library)?;

        cases.push((
            usize.const_int(
                u64::try_from(index)
                    .expect("static dependency ordinal must fit the target index constant"),
                false,
            ),
            block,
        ));
    }

    builder.position_at_end(invalid);

    builder
        .build_store(destination, static_identity_type(context).const_zero())
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_return(None)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(entry);

    let index = dependency
        .get_last_param()
        .expect("static-storage realization requires an established mapping or value")
        .into_int_value();

    builder
        .build_switch(index, invalid, &cases)
        .map_err(CodegenFailure::backend_library)?;

    Ok(dependency)
}

pub(super) fn declare_product_descriptor<'context>(
    module: &Module<'context>,
    product_host: &CodegenProductHostMapping,
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<GlobalValue<'context>, CodegenFailure> {
    let descriptor_type =
        product_host_descriptor_type(types.context(), pointer_type(types)?, usize_type(types)?);

    Ok(module
        .get_global(product_host.descriptor_symbol().as_str())
        .unwrap_or_else(|| {
            module.add_global(
                descriptor_type,
                None,
                product_host.descriptor_symbol().as_str(),
            )
        }))
}

pub(super) fn declare_product_host<'context>(
    module: &Module<'context>,
    product_host: &CodegenProductHostMapping,
    mappings: &CodegenMappings,
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<(), CodegenFailure> {
    let context = types.context();
    let pointer = pointer_type(types)?;
    let usize = usize_type(types)?;
    let descriptor_type = product_host_descriptor_type(context, pointer, usize);
    let descriptor = declare_product_descriptor(module, product_host, types)?;

    if !product_host.is_final_image() || product_host.owner() != mappings.unit() {
        return Ok(());
    }

    let entry_type = static_host_entry_type(context, pointer, usize);

    let static_entry = declare_static_host_lookup(
        module,
        product_host,
        entry_type,
        usize,
        context,
        types.target().machine().object_format(),
    )?;

    let execution = if product_host.execution().is_some() {
        crate::native::declare_runtime_function(
            module,
            context,
            types.target(),
            bray_runtime_interface::RuntimeAbiRole::ProductExecutionProvider,
        )?
        .as_global_value()
        .as_pointer_value()
    } else {
        pointer.const_null()
    };

    let descriptor_initializer = descriptor_type.const_named_struct(&[
        context
            .i32_type()
            .const_int(u64::from(bray_runtime_abi::PRODUCT_HOST_ABI_VERSION), false)
            .into(),
        context
            .i32_type()
            .const_int(
                u64::from(
                    product_host.execution() == Some(bray_symbols::CallableExecution::Asynchronous),
                ),
                false,
            )
            .into(),
        crate::native::identity_value(context, product_host.identity().bytes()).into(),
        static_entry.as_global_value().as_pointer_value().into(),
        usize
            .const_int(
                u64::try_from(product_host.statics().len())
                    .expect("product static count must fit the target index constant"),
                false,
            )
            .into(),
        execution.into(),
        if product_host.is_independent() {
            let activation = module.add_global(usize, None, "__bray_provider_activation");

            activation.set_initializer(&usize.const_zero());
            activation.set_linkage(Linkage::Internal);

            activation.as_pointer_value()
        } else {
            pointer.const_null()
        }
        .into(),
        declare_provider_operations(module, product_host, pointer, types)?.into(),
    ]);

    descriptor.set_constant(true);
    descriptor.set_initializer(&descriptor_initializer);
    descriptor.set_linkage(Linkage::External);

    // ELF interposition must not bind a loaded product to another image's host.
    if types.target().machine().object_format() == bray_target::ObjectFormat::Elf {
        descriptor.set_visibility(GlobalVisibility::Protected);
    }

    if !product_host.requires_control() {
        return Ok(());
    }

    let observation_type = crate::native::product_observation_type(context, types.target());

    let control_type = context
        .void_type()
        .fn_type(&[pointer.into(), context.i32_type().into()], false);

    let control = module.add_function(
        product_host.control_symbol().as_str(),
        control_type,
        Some(Linkage::WeakODR),
    );

    control.add_attribute(
        AttributeLoc::Param(0),
        crate::native::indirect_result_attribute(context, observation_type.into())?,
    );

    crate::comdat::attach(
        module,
        control.as_global_value(),
        product_host.control_symbol().as_str(),
        types.target().machine().object_format(),
    );

    let builder = context.create_builder();
    let entry = context.append_basic_block(control, "product.host.control");

    builder.position_at_end(entry);

    let operation = control
        .get_last_param()
        .expect("static-storage realization requires an established mapping or value");

    let observation = crate::native::invoke_product_control(
        context,
        module,
        &builder,
        types.target(),
        descriptor.as_pointer_value(),
        operation.into_int_value(),
    )?;

    let destination = control
        .get_first_param()
        .expect("product control owns its observation output")
        .into_pointer_value();

    builder
        .build_store(destination, observation)
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_return(None)
        .map_err(CodegenFailure::backend_library)?;

    Ok(())
}

fn declare_static_host_lookup<'context>(
    module: &Module<'context>,
    product_host: &CodegenProductHostMapping,
    entry_type: StructType<'context>,
    usize: IntType<'context>,
    context: &'context inkwell::context::Context,
    object_format: bray_target::ObjectFormat,
) -> Result<FunctionValue<'context>, CodegenFailure> {
    let name = format!("{}.static_entry", product_host.control_symbol().as_str());

    let lookup = module.add_function(
        &name,
        context.void_type().fn_type(
            &[
                context.ptr_type(inkwell::AddressSpace::default()).into(),
                usize.into(),
            ],
            false,
        ),
        Some(Linkage::WeakODR),
    );

    lookup.add_attribute(
        AttributeLoc::Param(0),
        crate::native::indirect_result_attribute(context, entry_type.into())?,
    );

    let destination = lookup
        .get_first_param()
        .expect("static lookup owns its result slot")
        .into_pointer_value();

    lookup
        .as_global_value()
        .set_visibility(GlobalVisibility::Hidden);

    crate::comdat::attach(module, lookup.as_global_value(), &name, object_format);

    let builder = context.create_builder();
    let entry = context.append_basic_block(lookup, "static.host.lookup");
    let invalid = context.append_basic_block(lookup, "static.host.invalid");
    let mut cases = Vec::with_capacity(product_host.statics().len());

    for (index, mapping) in product_host.statics().iter().enumerate() {
        let host_entry = module
            .get_global(mapping.host_symbol().as_str())
            .unwrap_or_else(|| module.add_global(entry_type, None, mapping.host_symbol().as_str()));

        let block = context.append_basic_block(lookup, "static.host.found");

        builder.position_at_end(block);

        let value = builder
            .build_load(
                entry_type,
                host_entry.as_pointer_value(),
                "static.host.entry",
            )
            .map_err(CodegenFailure::backend_library)?;

        let value = builder
            .build_insert_value(
                value.into_struct_value(),
                context.i64_type().const_int(mapping.order(), false),
                3,
                "static.host.order",
            )
            .map_err(CodegenFailure::backend_library)?;

        builder
            .build_store(destination, value)
            .map_err(CodegenFailure::backend_library)?;

        builder
            .build_return(None)
            .map_err(CodegenFailure::backend_library)?;

        cases.push((
            usize.const_int(
                u64::try_from(index)
                    .expect("product static ordinal must fit the target index constant"),
                false,
            ),
            block,
        ));
    }

    builder.position_at_end(invalid);

    builder
        .build_store(destination, entry_type.const_zero())
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_return(None)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(entry);

    let index = lookup
        .get_last_param()
        .expect("static-storage realization requires an established mapping or value")
        .into_int_value();

    builder
        .build_switch(index, invalid, &cases)
        .map_err(CodegenFailure::backend_library)?;

    Ok(lookup)
}

fn static_host_entry_type<'context>(
    context: &'context inkwell::context::Context,
    pointer: PointerType<'context>,
    usize: IntType<'context>,
) -> StructType<'context> {
    context.struct_type(
        &[
            context.i32_type().into(),
            context.i32_type().into(),
            static_identity_type(context).into(),
            context.i64_type().into(),
            pointer.into(),
            pointer.into(),
            pointer.into(),
            static_finalizer_type(context, pointer).into(),
            pointer.into(),
            pointer.into(),
            pointer.into(),
            usize.into(),
        ],
        false,
    )
}

fn static_finalizer_type<'context>(
    context: &'context inkwell::context::Context,
    pointer: PointerType<'context>,
) -> StructType<'context> {
    context.struct_type(
        &[
            context.i32_type().into(),
            pointer.into(),
            pointer.into(),
            pointer.into(),
        ],
        false,
    )
}

fn static_finalizer_value<'context>(
    context: &'context inkwell::context::Context,
    finalizer: StaticFinalizerCallbacks<'context>,
    pointer: PointerType<'context>,
) -> inkwell::values::StructValue<'context> {
    static_finalizer_type(context, pointer).const_named_struct(&[
        context
            .i32_type()
            .const_int(finalizer.execution, false)
            .into(),
        finalizer
            .admission
            .as_global_value()
            .as_pointer_value()
            .into(),
        finalizer.start.as_global_value().as_pointer_value().into(),
        finalizer
            .resolve
            .as_global_value()
            .as_pointer_value()
            .into(),
    ])
}

fn product_host_descriptor_type<'context>(
    context: &'context inkwell::context::Context,
    pointer: PointerType<'context>,
    usize: IntType<'context>,
) -> StructType<'context> {
    context.struct_type(
        &[
            context.i32_type().into(),
            context.i32_type().into(),
            product_identity_type(context).into(),
            pointer.into(),
            usize.into(),
            pointer.into(),
            pointer.into(),
            pointer.into(),
        ],
        false,
    )
}

fn declare_provider_operations<'context>(
    module: &Module<'context>,
    product_host: &CodegenProductHostMapping,
    pointer: PointerType<'context>,
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<PointerValue<'context>, CodegenFailure> {
    if !product_host.is_independent() {
        return Ok(pointer.const_null());
    }

    let context = types.context();

    let callbacks = [
        bray_runtime_interface::RuntimeAbiRole::ProductProviderFormation,
        bray_runtime_interface::RuntimeAbiRole::ProductProviderRoot,
    ]
    .map(|role| crate::native::declare_runtime_function(module, context, types.target(), role));

    let [formation, root] = callbacks;

    let formation = formation?;
    let root = root?;

    if types.target().machine().object_format() == bray_target::ObjectFormat::Elf {
        formation
            .as_global_value()
            .set_visibility(GlobalVisibility::Protected);

        root.as_global_value()
            .set_visibility(GlobalVisibility::Protected);
    }

    let ty = context.struct_type(&[pointer.into(), pointer.into()], false);
    let operations = module.add_global(ty, None, "__bray_provider_operations");

    operations.set_constant(true);
    operations.set_linkage(Linkage::Internal);

    operations.set_initializer(&ty.const_named_struct(&[
        formation.as_global_value().as_pointer_value().into(),
        root.as_global_value().as_pointer_value().into(),
    ]));

    Ok(operations.as_pointer_value())
}

fn static_identity_type(context: &inkwell::context::Context) -> ArrayType<'_> {
    context.i8_type().array_type(32)
}

fn product_identity_type(context: &inkwell::context::Context) -> ArrayType<'_> {
    context.i8_type().array_type(32)
}

fn usize_type<'context>(
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<IntType<'context>, CodegenFailure> {
    let width = std::num::NonZeroU32::new(u32::from(
        types.target().machine().pointer_width_bits().get(),
    ))
    .expect("static-storage realization requires an established mapping or value");

    types
        .context()
        .custom_width_int_type(width)
        .map_err(CodegenFailure::backend_library)
}
