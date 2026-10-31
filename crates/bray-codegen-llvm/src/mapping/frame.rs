use bray_codegen::{CodegenFailure, CodegenInstanceKey, CodegenMappings, CodegenSymbolKey};
use inkwell::{
    AddressSpace, builder::Builder, context::Context, module::Module, types::StructType,
    values::PointerValue,
};

pub(crate) fn factory_type(context: &Context) -> StructType<'_> {
    let pointer = context.ptr_type(AddressSpace::default());

    context.struct_type(&[pointer.into(), pointer.into()], false)
}

pub(crate) fn initialize_factory(
    context: &Context,
    module: &Module<'_>,
    mappings: &CodegenMappings,
    instance: &CodegenInstanceKey,
) {
    let factory = mappings
        .symbol(&CodegenSymbolKey::FrameFactory(instance.clone()))
        .expect("frame constructor has its immutable factory record");

    let constructor = mappings
        .instance_symbol(instance)
        .expect("factory retains its constructor");

    let function = module
        .get_function(constructor.name().as_str())
        .expect("factory constructor is declared");

    let descriptor = super::cleanup::storage_descriptor(module, mappings, instance);

    let global = module
        .get_global(factory.name().as_str())
        .expect("factory record is declared");

    let value = factory_type(context).const_named_struct(&[
        function.as_global_value().as_pointer_value().into(),
        descriptor.as_pointer_value().into(),
    ]);

    global.set_initializer(&value);
}

pub(crate) fn factory_field<'context>(
    context: &'context Context,
    builder: &Builder<'context>,
    factory: PointerValue<'context>,
    field: u32,
) -> Result<PointerValue<'context>, CodegenFailure> {
    let member = builder
        .build_struct_gep(
            factory_type(context),
            factory,
            field,
            "frame.factory.member",
        )
        .map_err(CodegenFailure::backend_library)?;

    Ok(builder
        .build_load(
            context.ptr_type(AddressSpace::default()),
            member,
            "frame.factory.value",
        )
        .map_err(CodegenFailure::backend_library)?
        .into_pointer_value())
}

/// Gives native frame cleanup an explicit outcome without invoking the unwind entry shim.
pub(crate) fn cleanup_callback<'context>(
    module: &Module<'context>,
    mappings: &CodegenMappings,
    helper: &bray_codegen::CodegenHelperMapping,
    types: &mut super::LlvmTypeMappings<'context, '_>,
) -> Result<Option<PointerValue<'context>>, CodegenFailure> {
    let Some(key) = helper.symbol() else {
        return Ok(None);
    };

    let symbol = mappings
        .symbol(key)
        .expect("selected cleanup has its symbol");

    let name = format!("{}.frame.cleanup", symbol.name().as_str());

    if let Some(callback) = module.get_function(&name) {
        return Ok(Some(callback.as_global_value().as_pointer_value()));
    }

    let context = types.context();
    let pointer = context.ptr_type(AddressSpace::default());

    let callback = module.add_function(
        &name,
        context
            .void_type()
            .fn_type(&[pointer.into(), pointer.into()], false),
        Some(inkwell::module::Linkage::Internal),
    );

    let builder = context.create_builder();
    let entry = context.append_basic_block(callback, "frame.cleanup");

    builder.position_at_end(entry);

    let payload = callback
        .get_nth_param(0)
        .expect("cleanup receives its payload");

    let outcome = callback
        .get_nth_param(1)
        .expect("cleanup receives its outcome")
        .into_pointer_value();

    let function = module
        .get_function(symbol.name().as_str())
        .expect("selected cleanup is declared");

    assert!(
        matches!(
            symbol.signature().result(),
            bray_codegen::CodegenResultMapping::Void
        ),
        "frame cleanup consumes its value without returning another owner"
    );

    let collection = super::cleanup::begin_collection(module, &builder, outcome, types)?;

    super::invoke_with_outcome(
        &builder,
        function,
        symbol.signature(),
        &[payload.into(), collection[0].into(), collection[1].into()],
        outcome,
        "",
        types,
    )?;

    super::cleanup::publish_collection(module, &builder, collection[0], types)?;

    builder
        .build_return(None)
        .map_err(CodegenFailure::backend_library)?;

    Ok(Some(callback.as_global_value().as_pointer_value()))
}

/// Constructs typed asynchronous cleanup without running its body. The caller
/// retains both the result and its collection until the returned frame retires.
pub(crate) fn lifecycle_factory_callback<'context>(
    module: &Module<'context>,
    mappings: &CodegenMappings,
    helper: &bray_codegen::CodegenHelperMapping,
    types: &mut super::LlvmTypeMappings<'context, '_>,
) -> Result<Option<(PointerValue<'context>, PointerValue<'context>)>, CodegenFailure> {
    let Some(CodegenSymbolKey::Instance(instance)) = helper.symbol() else {
        assert!(helper.symbol().is_none(), "a lifecycle factory selects its concrete constructor");

        return Ok(None);
    };

    let symbol = mappings.instance_symbol(instance).expect("typed cleanup retains its constructor");
    let descriptor = super::cleanup::storage_descriptor(module, mappings, instance).as_pointer_value();
    let name = format!("{}.frame.factory", symbol.name().as_str());

    if let Some(callback) = module.get_function(&name) {
        return Ok(Some((callback.as_global_value().as_pointer_value(), descriptor)));
    }

    let context = types.context();
    let pointer = context.ptr_type(AddressSpace::default());

    let callback = module.add_function(
        &name,
        context.void_type().fn_type(&[pointer.into(); 4], false),
        Some(inkwell::module::Linkage::Internal),
    );

    let builder = context.create_builder();

    builder.position_at_end(context.append_basic_block(callback, "frame.factory"));

    let payload = callback.get_nth_param(0).expect("the factory borrows its value");
    let collection = callback.get_nth_param(1).expect("the factory borrows its persistent collection").into_pointer_value();
    let output = callback.get_nth_param(2).expect("the factory receives its token destination").into_pointer_value();
    let outcome = callback.get_nth_param(3).expect("the factory receives its construction outcome").into_pointer_value();
    let collection_type = crate::native::run_outcome_type(context, types.target());

    let state = builder.build_struct_gep(collection_type, collection, 0, "cleanup.state")
        .map_err(CodegenFailure::backend_library)?;

    let report = builder.build_struct_gep(collection_type, collection, 2, "cleanup.report")
        .map_err(CodegenFailure::backend_library)?;

    let function = module.get_function(symbol.name().as_str()).expect("typed cleanup is declared");
    let mut arguments = vec![payload.into(), state.into(), report.into()];

    let indirect = match symbol.signature().result() {
        bray_codegen::CodegenResultMapping::Indirect { pointee, .. } => {
            let ty = types.map(*pointee)?;
            let slot = crate::translation::allocate_temporary(context, &builder, ty, "cleanup.future")?;

            arguments.insert(0, slot.into());

            Some((ty, slot))
        }
        bray_codegen::CodegenResultMapping::Direct { coercion, .. } => {
            assert!(coercion.is_none(), "Bray lifecycle constructors return their protected future representation");

            None
        }
        bray_codegen::CodegenResultMapping::Void => panic!("a lifecycle factory returns an inactive future"),
    };

    let call = super::invoke_with_outcome(
        &builder, function, symbol.signature(), &arguments, outcome,
        if indirect.is_some() { "" } else { "cleanup.future" }, types,
    )?;

    let future = if let Some((ty, slot)) = indirect {
        builder.build_load(ty, slot, "cleanup.future.value").map_err(CodegenFailure::backend_library)?
    } else {
        call.try_as_basic_value().basic().expect("the constructor returns its inactive future")
    };

    let future = future.into_struct_value();

    let frame = builder.build_extract_value(future, 0, "cleanup.context")
        .map_err(CodegenFailure::backend_library)?.into_pointer_value();

    let activation = builder.build_extract_value(future, 1, "cleanup.descriptor")
        .map_err(CodegenFailure::backend_library)?.into_pointer_value();

    let token = crate::native::activation::inactive_token(context, types.target(), &builder, frame, activation)?;

    builder.build_store(output, token).map_err(CodegenFailure::backend_library)?;
    builder.build_return(None).map_err(CodegenFailure::backend_library)?;

    Ok(Some((callback.as_global_value().as_pointer_value(), descriptor)))
}
