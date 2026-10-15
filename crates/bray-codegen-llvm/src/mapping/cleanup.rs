use bray_codegen::CodegenTarget;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::StructType;
use inkwell::values::{GlobalValue, StructValue};

pub(crate) fn lease_type<'context>(
    context: &'context Context,
    target: &CodegenTarget,
) -> StructType<'context> {
    let integer = crate::native::pointer_integer_type(context, target);

    context.struct_type(
        &[
            integer.into(),
            integer.into(),
            integer.into(),
            context.i64_type().into(),
            integer.into(),
        ],
        false,
    )
}

pub(crate) fn descriptor_type<'context>(
    context: &'context Context,
    target: &CodegenTarget,
) -> StructType<'context> {
    let integer = crate::native::pointer_integer_type(context, target);

    context.struct_type(
        &[
            context.i8_type().array_type(32).into(),
            integer.into(),
            integer.into(),
        ],
        false,
    )
}

pub(crate) fn descriptor_value<'context>(
    context: &'context Context,
    target: &CodegenTarget,
    identity: [u8; 32],
    bytes: u64,
    alignment: u32,
) -> StructValue<'context> {
    let integer = crate::native::pointer_integer_type(context, target);

    let identity = context
        .i8_type()
        .const_array(&identity.map(|byte| context.i8_type().const_int(u64::from(byte), false)));

    descriptor_type(context, target).const_named_struct(&[
        identity.into(),
        integer.const_int(bytes, false).into(),
        integer.const_int(u64::from(alignment), false).into(),
    ])
}

pub(crate) fn descriptors<'context>(
    module: &Module<'context>,
    builder: &inkwell::builder::Builder<'context>,
    requirements: &[bray_codegen::CodegenCleanupRequirement],
    types: &mut super::LlvmTypeMappings<'context, '_>,
) -> Result<inkwell::values::PointerValue<'context>, bray_codegen::CodegenFailure> {
    use bray_codegen::CodegenFailure;

    let context = types.context();
    let descriptor = descriptor_type(context, types.target());

    let count =
        u32::try_from(requirements.len()).expect("cleanup descriptor count fits native array");

    let ty = descriptor.array_type(count);
    let mut value = ty.get_undef();

    for (index, requirement) in requirements.iter().enumerate() {
        let element = if requirement.execution() == bray_symbols::CallableExecution::Asynchronous {
            let global = storage_descriptor(module, types.mappings(), requirement.callable());

            builder
                .build_load(descriptor, global.as_pointer_value(), "cleanup.descriptor")
                .map_err(CodegenFailure::backend_library)?
                .into_struct_value()
        } else {
            let completion = types.map(requirement.completion())?;

            descriptor_value(
                context,
                types.target(),
                requirement.identity(),
                types.target_data().get_store_size(&completion),
                types.target_data().get_abi_alignment(&completion),
            )
        };

        value = builder
            .build_insert_value(
                value,
                element,
                u32::try_from(index).expect("descriptor index fits its array"),
                "cleanup.descriptors",
            )
            .map_err(CodegenFailure::backend_library)?
            .into_array_value();
    }

    let pointer = crate::translation::allocate_temporary(
        context,
        builder,
        ty,
        "cleanup.descriptors.address",
    )?;

    builder
        .build_store(pointer, value)
        .map_err(CodegenFailure::backend_library)?;

    Ok(pointer)
}

pub(crate) fn bundle<'context>(
    builder: &inkwell::builder::Builder<'context>,
    descriptors: inkwell::values::PointerValue<'context>,
    count: inkwell::values::IntValue<'context>,
    types: &super::LlvmTypeMappings<'context, '_>,
) -> Result<inkwell::values::PointerValue<'context>, bray_codegen::CodegenFailure> {
    use bray_codegen::CodegenFailure;

    let context = types.context();

    let span_type = context.struct_type(
        &[descriptors.get_type().into(), count.get_type().into()],
        false,
    );

    let span = builder
        .build_insert_value(
            span_type.get_undef(),
            descriptors,
            0,
            "cleanup.span.pointer",
        )
        .map_err(CodegenFailure::backend_library)?;

    let span = builder
        .build_insert_value(span, count, 1, "cleanup.span.count")
        .map_err(CodegenFailure::backend_library)?;

    let bundle_type = context.struct_type(&[count.get_type().into(), span_type.into()], false);

    let value = builder
        .build_insert_value(bundle_type.get_undef(), count, 0, "cleanup.bundle.records")
        .map_err(CodegenFailure::backend_library)?;

    let value = builder
        .build_insert_value(value, span, 1, "cleanup.bundle.descriptors")
        .map_err(CodegenFailure::backend_library)?;

    let pointer =
        crate::translation::allocate_temporary(context, builder, bundle_type, "cleanup.bundle")?;

    builder
        .build_store(pointer, value)
        .map_err(CodegenFailure::backend_library)?;

    Ok(pointer)
}

/// Returns the immutable concrete backing descriptor emitted with its callable.
pub(crate) fn storage_descriptor<'context>(
    module: &Module<'context>,
    mappings: &bray_codegen::CodegenMappings,
    instance: &bray_codegen::CodegenInstanceKey,
) -> GlobalValue<'context> {
    let mapping = mappings
        .symbol(&bray_codegen::CodegenSymbolKey::CleanupStorage(
            instance.clone(),
        ))
        .expect("concrete frame backing must have a mapped descriptor");

    module
        .get_global(mapping.name().as_str())
        .expect("concrete frame backing descriptor must have been declared")
}
