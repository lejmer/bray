use bray_codegen::CodegenFailure;
use bray_runtime_interface::RuntimeAbiRole;
use inkwell::values::{IntValue, StructValue};
use inkwell::{AddressSpace, builder::Builder, module::Module};

use super::LlvmTypeMappings;

/// Archives resolve this reference to the final consumer's descriptor at link time.
/// Known resident final images need neither a root nor a descriptor lookup.
pub(crate) fn key<'context>(
    module: &Module<'context>,
    builder: &Builder<'context>,
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<IntValue<'context>, CodegenFailure> {
    let usize = crate::native::pointer_integer_type(types.context(), types.target());

    let Some(host) = types.mappings().product_host() else {
        return Ok(usize.const_zero());
    };

    if !host.requires_provider_reference() {
        return Ok(usize.const_zero());
    }

    let descriptor = module
        .get_global(host.descriptor_symbol().as_str())
        .expect("provider-dependent code requires its final-consumer descriptor");

    builder
        .build_ptr_to_int(descriptor.as_pointer_value(), usize, "provider.descriptor")
        .map_err(CodegenFailure::backend_library)
}

pub(crate) fn acquire<'context>(
    module: &Module<'context>,
    builder: &Builder<'context>,
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<StructValue<'context>, CodegenFailure> {
    let context = types.context();
    let usize = crate::native::pointer_integer_type(context, types.target());
    let pointer = context.ptr_type(AddressSpace::default());
    let ty = context.struct_type(&[usize.into(), pointer.into()], false);
    let key = key(module, builder, types)?;

    if key.is_const() && key.get_zero_extended_constant() == Some(0) {
        return Ok(ty.const_zero());
    }

    let slot = crate::translation::allocate_temporary(context, builder, ty, "provider.root")?;

    builder
        .build_store(slot, ty.const_zero())
        .map_err(CodegenFailure::backend_library)?;

    let acquire = crate::native::declare_runtime_function(
        module,
        context,
        types.target(),
        RuntimeAbiRole::ProviderReferenceAcquire,
    )?;

    builder
        .build_call(acquire, &[key.into(), slot.into()], "provider.acquired")
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_load(ty, slot, "provider.owner")
        .map_err(CodegenFailure::backend_library)
        .map(inkwell::values::BasicValueEnum::into_struct_value)
}
