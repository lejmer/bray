use bray_codegen::{CodegenFailure, CodegenProductHostMapping, CodegenTarget};
use bray_runtime_interface::RuntimeAbiRole;
use inkwell::AddressSpace;
use inkwell::builder::Builder;
use inkwell::module::Module;
use inkwell::types::StructType;
use inkwell::values::{BasicValueEnum, PointerValue};

pub(crate) fn provider_owner_type<'context>(
    context: &'context inkwell::context::Context,
    target: &CodegenTarget,
) -> StructType<'context> {
    let pointer = context.ptr_type(AddressSpace::default());

    context.struct_type(
        &[
            super::pointer_integer_type(context, target).into(),
            pointer.into(),
            pointer.into(),
            pointer.into(),
        ],
        false,
    )
}

pub(crate) fn invoke_product_control<'context>(
    context: &'context inkwell::context::Context,
    module: &Module<'context>,
    builder: &Builder<'context>,
    target: &CodegenTarget,
    host: &CodegenProductHostMapping,
    abi: bray_runtime_interface::RuntimeAbiVersion,
    operation: bray_runtime_abi::NativeProductHostOperation,
) -> Result<BasicValueEnum<'context>, CodegenFailure> {
    let role = RuntimeAbiRole::ProductHostControl;
    let function = super::declare_runtime_function(module, context, target, role)?;

    let descriptor = module
        .get_global(host.descriptor_symbol().as_str())
        .expect("a product host mapping must declare its descriptor in every contributing unit");

    let key = bray_codegen::CodegenSymbolKey::Runtime(bray_ir::MirRuntimeReference::new(role, abi));

    Ok(super::invoke_function(
        context,
        builder,
        target,
        &key,
        function,
        &[
            descriptor.as_pointer_value().into(),
            context
                .i32_type()
                .const_int(u64::from(operation.code()), false)
                .into(),
        ],
        "product.control",
    )?
    .expect("product control must return an observation"))
}

pub(crate) fn retain_product_provider<'context>(
    context: &'context inkwell::context::Context,
    module: &Module<'context>,
    builder: &Builder<'context>,
    target: &CodegenTarget,
    host: &CodegenProductHostMapping,
    abi: bray_runtime_interface::RuntimeAbiVersion,
    destination: PointerValue<'context>,
) -> Result<(), CodegenFailure> {
    let role = RuntimeAbiRole::ProductProviderRetention;
    let function = super::declare_runtime_function(module, context, target, role)?;

    let descriptor = module
        .get_global(host.descriptor_symbol().as_str())
        .expect("provider retention requires the consuming product descriptor");

    let key = bray_codegen::CodegenSymbolKey::Runtime(bray_ir::MirRuntimeReference::new(role, abi));

    let status = super::invoke_function(
        context,
        builder,
        target,
        &key,
        function,
        &[descriptor.as_pointer_value().into(), destination.into()],
        "provider.retained",
    )?
    .expect("provider retention must return its native status")
    .into_int_value();

    let admitted = builder
        .build_int_compare(
            inkwell::IntPredicate::EQ,
            status,
            status.get_type().const_zero(),
            "provider.admitted",
        )
        .map_err(CodegenFailure::backend_library)?;

    let parent = builder
        .get_insert_block()
        .and_then(|block| block.get_parent())
        .expect("provider retention must occur inside a generated function");

    let continued = context.append_basic_block(parent, "provider.retained");
    let failed = context.append_basic_block(parent, "provider.binding.invalid");

    builder
        .build_conditional_branch(admitted, continued, failed)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(failed);

    let trap = module.get_function("llvm.trap").unwrap_or_else(|| {
        module.add_function("llvm.trap", context.void_type().fn_type(&[], false), None)
    });

    // Entry validates external formation before the body can accept owners.
    // Losing that admitted binding while publishing a report is an internal contract violation.
    builder
        .build_call(trap, &[], "")
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_unreachable()
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(continued);

    Ok(())
}
