use bray_codegen::{
    CodegenFailure, CodegenMappings, CodegenProductHostMapping, CodegenProductHostStatic,
    CodegenStaticStorageMapping,
};
use inkwell::attributes::AttributeLoc;
use inkwell::types::{ArrayType, BasicType, IntType, PointerType, StructType};
use inkwell::values::{ArrayValue, FunctionValue, GlobalValue, PointerValue};
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
    pub(super) result_size: u64,
    pub(super) result_alignment: u64,
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
    let usize = usize_type(types)?;
    let descriptor_type = product_host_descriptor_type(context, pointer, usize);

    let descriptor = module
        .get_global(product_host.descriptor_symbol().as_str())
        .unwrap_or_else(|| {
            module.add_global(
                descriptor_type,
                None,
                product_host.descriptor_symbol().as_str(),
            )
        });

    let entry = product_host
        .statics()
        .iter()
        .find(|entry| entry.host_symbol().as_str() == mapping.host_name())
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let registration_type = context.struct_type(
        &[
            pointer.into(),
            static_identity_type(context).into(),
            pointer.into(),
            static_finalizer_type(context, pointer, usize).into(),
            pointer.into(),
            pointer.into(),
        ],
        false,
    );

    let initializer = registration_type.const_named_struct(&[
        descriptor.as_pointer_value().into(),
        static_identity_value(context, entry.identity()).into(),
        callbacks
            .prepare
            .as_global_value()
            .as_pointer_value()
            .into(),
        static_finalizer_value(context, callbacks.finalizer, pointer, usize).into(),
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
        static_identity_value(context, host_mapping.identity()).into(),
        context
            .i64_type()
            .const_int(host_mapping.order(), false)
            .into(),
        storage.into(),
        accessor.as_global_value().as_pointer_value().into(),
        callbacks
            .prepare
            .as_global_value()
            .as_pointer_value()
            .into(),
        static_finalizer_value(context, callbacks.finalizer, pointer, usize).into(),
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
                    .map_err(CodegenFailure::backend_library)?,
                false,
            )
            .into(),
    ]);

    let entry = module.add_global(entry_type, None, &name);

    entry.set_constant(true);
    entry.set_initializer(&initializer);
    entry.set_linkage(Linkage::WeakODR);
    entry.set_visibility(GlobalVisibility::Hidden);

    entry.set_section(Some(bray_codegen::static_host_section_name(
        types.target().machine().object_format(),
    )));

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
        static_identity_type(context).fn_type(&[usize.into()], false),
        Some(Linkage::WeakODR),
    );

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
            .build_return(Some(&static_identity_value(context, identity)))
            .map_err(CodegenFailure::backend_library)?;

        cases.push((
            usize.const_int(
                u64::try_from(index).map_err(CodegenFailure::backend_library)?,
                false,
            ),
            block,
        ));
    }

    builder.position_at_end(invalid);

    builder
        .build_return(Some(&static_identity_type(context).const_zero()))
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(entry);

    let index = dependency
        .get_first_param()
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?
        .into_int_value();

    builder
        .build_switch(index, invalid, &cases)
        .map_err(CodegenFailure::backend_library)?;

    Ok(dependency)
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

    let descriptor = module
        .get_global(product_host.descriptor_symbol().as_str())
        .unwrap_or_else(|| {
            module.add_global(
                descriptor_type,
                None,
                product_host.descriptor_symbol().as_str(),
            )
        });

    if product_host.owner() != mappings.unit() {
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

    let descriptor_initializer = descriptor_type.const_named_struct(&[
        context
            .i32_type()
            .const_int(u64::from(bray_runtime_abi::PRODUCT_HOST_ABI_VERSION), false)
            .into(),
        context.i32_type().const_zero().into(),
        product_identity_value(context, product_host.identity()).into(),
        static_entry.as_global_value().as_pointer_value().into(),
        usize
            .const_int(
                u64::try_from(product_host.statics().len())
                    .map_err(CodegenFailure::backend_library)?,
                false,
            )
            .into(),
    ]);

    descriptor.set_constant(true);
    descriptor.set_initializer(&descriptor_initializer);
    descriptor.set_linkage(Linkage::WeakODR);

    crate::comdat::attach(
        module,
        descriptor,
        product_host.descriptor_symbol().as_str(),
        types.target().machine().object_format(),
    );

    let role = bray_runtime_interface::RuntimeAbiRole::ProductHostControl;
    let runtime = crate::native::declare_runtime_function(module, context, types.target(), role)?;
    let runtime_type = runtime.get_type();
    let result = crate::native::runtime_indirect_result_type(context, types.target(), role);

    let mut parameters = runtime_type.get_param_types();

    // The product wrapper captures the descriptor while preserving the native result parameter.
    parameters.remove(usize::from(result.is_some()));

    let control_type = match runtime_type.get_return_type() {
        Some(result) => result.fn_type(&parameters, false),
        None => context.void_type().fn_type(&parameters, false),
    };

    let control = module.add_function(
        product_host.control_symbol().as_str(),
        control_type,
        Some(Linkage::WeakODR),
    );

    if let Some(result) = result {
        let attribute = crate::native::indirect_result_attribute(context, result)?;

        control.add_attribute(AttributeLoc::Param(0), attribute);
    }

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
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let mut arguments = Vec::new();

    if result.is_some() {
        arguments.push(
            control
                .get_first_param()
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?
                .into(),
        );
    }

    arguments.push(descriptor.as_pointer_value().into());
    arguments.push(operation.into());

    let call = builder
        .build_call(runtime, &arguments, "product.host.observation")
        .map_err(CodegenFailure::backend_library)?;

    if let Some(result) = result {
        call.add_attribute(
            AttributeLoc::Param(0),
            crate::native::indirect_result_attribute(context, result)?,
        );
    }

    let observation = call.try_as_basic_value().basic();

    builder
        .build_return(
            observation
                .as_ref()
                .map(|value| value as &dyn inkwell::values::BasicValue),
        )
        .map_err(CodegenFailure::backend_library)?;

    retain_globals(
        module,
        &[
            descriptor,
            static_entry.as_global_value(),
            control.as_global_value(),
        ],
        "llvm.used",
        types,
    )
}

fn declare_static_host_lookup<'context>(
    module: &Module<'context>,
    product_host: &CodegenProductHostMapping,
    entry_type: StructType<'context>,
    usize: IntType<'context>,
    context: &'context inkwell::context::Context,
    object_format: bray_target::ObjectFormat,
) -> Result<FunctionValue<'context>, CodegenFailure> {
    let name = format!("{}.static_entry", product_host.descriptor_symbol().as_str());

    let lookup = module.add_function(
        &name,
        entry_type.fn_type(&[usize.into()], false),
        Some(Linkage::WeakODR),
    );

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

        builder
            .build_return(Some(&value))
            .map_err(CodegenFailure::backend_library)?;

        cases.push((
            usize.const_int(
                u64::try_from(index).map_err(CodegenFailure::backend_library)?,
                false,
            ),
            block,
        ));
    }

    builder.position_at_end(invalid);

    builder
        .build_return(Some(&entry_type.const_zero()))
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(entry);

    let index = lookup
        .get_first_param()
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?
        .into_int_value();

    builder
        .build_switch(index, invalid, &cases)
        .map_err(CodegenFailure::backend_library)?;

    Ok(lookup)
}

pub(super) fn retain_globals<'context>(
    module: &Module<'context>,
    globals: &[GlobalValue<'context>],
    name: &str,
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<(), CodegenFailure> {
    if globals.is_empty() {
        return Ok(());
    }

    let pointer = pointer_type(types)?;

    let values = globals
        .iter()
        .map(|global| global.as_pointer_value())
        .collect::<Vec<_>>();

    let initializer = pointer.const_array(&values);
    let used = module.add_global(initializer.get_type(), None, name);

    used.set_initializer(&initializer);
    used.set_linkage(Linkage::Appending);
    used.set_section(Some("llvm.metadata"));

    Ok(())
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
            static_finalizer_type(context, pointer, usize).into(),
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
    usize: IntType<'context>,
) -> StructType<'context> {
    context.struct_type(
        &[
            context.i32_type().into(),
            context.i32_type().into(),
            usize.into(),
            usize.into(),
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
    usize: IntType<'context>,
) -> inkwell::values::StructValue<'context> {
    static_finalizer_type(context, pointer, usize).const_named_struct(&[
        context
            .i32_type()
            .const_int(finalizer.execution, false)
            .into(),
        context.i32_type().const_zero().into(),
        usize.const_int(finalizer.result_size, false).into(),
        usize.const_int(finalizer.result_alignment, false).into(),
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
        ],
        false,
    )
}

fn static_identity_type(context: &inkwell::context::Context) -> ArrayType<'_> {
    context.i8_type().array_type(32)
}

fn product_identity_type(context: &inkwell::context::Context) -> ArrayType<'_> {
    context.i8_type().array_type(32)
}

fn static_identity_value<'context>(
    context: &'context inkwell::context::Context,
    identity: bray_runtime_abi::NativeStaticIdentity,
) -> ArrayValue<'context> {
    byte_array(context, identity.bytes())
}

fn product_identity_value<'context>(
    context: &'context inkwell::context::Context,
    identity: bray_runtime_abi::NativeProductIdentity,
) -> ArrayValue<'context> {
    byte_array(context, identity.bytes())
}

fn byte_array<'context>(
    context: &'context inkwell::context::Context,
    bytes: [u8; 32],
) -> ArrayValue<'context> {
    let values = bytes
        .into_iter()
        .map(|byte| context.i8_type().const_int(u64::from(byte), false))
        .collect::<Vec<_>>();

    context.i8_type().const_array(&values)
}

fn usize_type<'context>(
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<IntType<'context>, CodegenFailure> {
    let width = std::num::NonZeroU32::new(u32::from(
        types.target().machine().pointer_width_bits().get(),
    ))
    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    types
        .context()
        .custom_width_int_type(width)
        .map_err(CodegenFailure::backend_library)
}
