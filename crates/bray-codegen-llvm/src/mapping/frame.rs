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
