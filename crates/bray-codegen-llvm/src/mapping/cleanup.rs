use bray_codegen::{CodegenLinkage, CodegenSymbolMapping, CodegenTarget};
use inkwell::context::Context;
use inkwell::module::{Linkage, Module};
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

pub(crate) fn declare<'context>(
    module: &Module<'context>,
    context: &'context Context,
    target: &CodegenTarget,
    mapping: &CodegenSymbolMapping,
    defined: bool,
) -> GlobalValue<'context> {
    let global = module.add_global(
        descriptor_type(context, target),
        None,
        mapping.name().as_str(),
    );

    global.set_constant(true);
    global.set_visibility(inkwell::GlobalVisibility::Hidden);

    if defined {
        assert_eq!(
            mapping.linkage(),
            CodegenLinkage::LinkOnce,
            "cleanup descriptors must share their concrete callable identity"
        );

        global.set_linkage(Linkage::WeakODR);

        crate::comdat::attach_any(
            module,
            global,
            mapping.name().as_str(),
            target.machine().object_format(),
        );
    }

    global
}

pub(crate) fn descriptors<'context>(
    module: &Module<'context>,
    builder: &inkwell::builder::Builder<'context>,
    requirements: &[bray_codegen::CodegenCleanupRequirement],
    types: &mut super::LlvmTypeMappings<'context, '_>,
) -> Result<inkwell::values::PointerValue<'context>, bray_codegen::CodegenFailure> {
    use bray_codegen::{CodegenFailure, CodegenSymbolKey};

    let context = types.context();
    let descriptor = descriptor_type(context, types.target());

    let count =
        u32::try_from(requirements.len()).expect("cleanup descriptor count fits native array");

    let ty = descriptor.array_type(count);
    let mut value = ty.get_undef();

    for (index, requirement) in requirements.iter().enumerate() {
        let element = if requirement.execution() == bray_symbols::CallableExecution::Asynchronous {
            let mapping = types
                .mappings()
                .symbol(&CodegenSymbolKey::CleanupStorage(
                    requirement.callable().clone(),
                ))
                .expect("asynchronous cleanup must retain its emitted storage layout");

            let global = module
                .get_global(mapping.name().as_str())
                .expect("cleanup descriptor must have been declared");

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
