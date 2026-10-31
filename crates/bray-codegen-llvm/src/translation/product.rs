use bray_codegen::{CodegenFailure, CodegenRequest};
use bray_runtime_abi::NativeProductHostOperation;
use inkwell::{builder::Builder, context::Context, module::Module, values::BasicValueEnum};

/// Invoke the already formed product descriptor's admission or closure operation.
pub(crate) fn control<'context>(
    context: &'context Context,
    module: &Module<'context>,
    request: CodegenRequest<'_>,
    builder: &Builder<'context>,
    operation: NativeProductHostOperation,
) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
    let host = request
        .mappings()
        .product_host()
        .expect("product control requires the selected product host mapping");

    if !host.requires_control() {
        return Ok(None);
    }

    let descriptor = module
        .get_global(host.descriptor_symbol().as_str())
        .expect("product control requires the emitted descriptor");

    Ok(Some(
        crate::native::invoke_product_control(
            context,
            module,
            builder,
            request.target(),
            descriptor.as_pointer_value(),
            context
                .i32_type()
                .const_int(u64::from(operation.code()), false),
        )?
        .into(),
    ))
}
