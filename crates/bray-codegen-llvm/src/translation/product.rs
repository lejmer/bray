use bray_codegen::{CodegenFailure, CodegenRequest, CodegenSymbolKey};
use bray_ir::MirRuntimeReference;
use bray_runtime_abi::NativeProductHostOperation;
use bray_runtime_interface::RuntimeAbiRole;
use inkwell::{builder::Builder, context::Context, module::Module, values::BasicValueEnum};

/// Invoke the already formed product descriptor's admission or closure operation.
pub(crate) fn control<'context>(
    context: &'context Context,
    module: &Module<'context>,
    request: CodegenRequest<'_>,
    builder: &Builder<'context>,
    operation: NativeProductHostOperation,
) -> Result<BasicValueEnum<'context>, CodegenFailure> {
    let host = request
        .mappings()
        .product_host()
        .expect("product control requires the selected product host mapping");

    let descriptor = module
        .get_global(host.descriptor_symbol().as_str())
        .expect("product control requires the emitted descriptor");

    let role = RuntimeAbiRole::ProductHostControl;

    let function =
        crate::native::declare_runtime_function(module, context, request.target(), role)?;

    let key = CodegenSymbolKey::Runtime(MirRuntimeReference::new(
        role,
        request.unit().target().runtime_abi(),
    ));

    Ok(crate::native::invoke_function(
        context,
        builder,
        request.target(),
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
    .expect("product control returns its native observation"))
}
