use bray_codegen::{CodegenFailure, CodegenTarget};
use bray_runtime_interface::RuntimeAbiRole;
use inkwell::AddressSpace;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::{Linkage, Module};
use inkwell::values::{PointerValue, StructValue};

/// Reserves aligned context bytes and cleanup machinery before captures transfer.
pub(super) fn allocate<'context>(
    module: &Module<'context>,
    context: &'context Context,
    builder: &Builder<'context>,
    target: &CodegenTarget,
    metadata: StructValue<'context>,
) -> Result<PointerValue<'context>, CodegenFailure> {
    let retained = module.add_global(metadata.get_type(), None, "frame.metadata");
    retained.set_initializer(&metadata);
    retained.set_constant(true);
    retained.set_linkage(Linkage::Private);

    let admission = crate::native::declare_runtime_function(
        module,
        context,
        target,
        RuntimeAbiRole::FrameStorageAdmission,
    )?;

    let address = builder
        .build_call(
            admission,
            &[retained.as_pointer_value().into()],
            "frame.admission",
        )
        .map_err(CodegenFailure::backend_library)?
        .try_as_basic_value()
        .basic()
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?
        .into_int_value();

    builder
        .build_int_to_ptr(
            address,
            context.ptr_type(AddressSpace::default()),
            "frame.storage",
        )
        .map_err(CodegenFailure::backend_library)
}

/// Releases the runtime allocation and any unused cleanup reservations together.
pub(super) fn release<'context>(
    module: &Module<'context>,
    context: &'context Context,
    builder: &Builder<'context>,
    target: &CodegenTarget,
    storage: PointerValue<'context>,
) -> Result<(), CodegenFailure> {
    let release = crate::native::declare_runtime_function(
        module,
        context,
        target,
        RuntimeAbiRole::FrameStorageRelease,
    )?;

    let address = builder
        .build_ptr_to_int(
            storage,
            crate::native::pointer_integer_type(context, target),
            "frame.storage.address",
        )
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_call(release, &[address.into()], "")
        .map_err(CodegenFailure::backend_library)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use bray_codegen::CodegenTarget;
    use bray_target::NativeTarget;
    use inkwell::AddressSpace;
    use inkwell::context::Context;

    #[test]
    fn every_target_uses_matched_runtime_admission_and_release_with_constant_metadata() {
        for native in NativeTarget::ALL {
            let context = Context::create();
            let module = context.create_module("frame.storage");
            let target = CodegenTarget::for_native(native);
            let builder = context.create_builder();
            let pointer = context.ptr_type(AddressSpace::default());
            let create = module.add_function("create", pointer.fn_type(&[], false), None);
            builder.position_at_end(context.append_basic_block(create, "entry"));
            let metadata = crate::native::frame_metadata_type(&context, &target).const_zero();
            let storage = super::allocate(&module, &context, &builder, &target, metadata).unwrap();
            builder.build_return(Some(&storage)).unwrap();

            let destroy = module.add_function(
                "destroy",
                context.void_type().fn_type(&[pointer.into()], false),
                None,
            );

            builder.position_at_end(context.append_basic_block(destroy, "entry"));
            let storage = destroy.get_first_param().unwrap().into_pointer_value();
            super::release(&module, &context, &builder, &target, storage).unwrap();
            builder.build_return(None).unwrap();
            module.verify().unwrap();

            assert!(
                module
                    .get_function(bray_runtime_abi::symbols::FRAME_STORAGE_ADMISSION_SYMBOL)
                    .is_some()
            );

            assert!(
                module
                    .get_function(bray_runtime_abi::symbols::FRAME_STORAGE_RELEASE_SYMBOL)
                    .is_some()
            );

            assert!(module.get_function("calloc").is_none());
            assert!(module.get_function("free").is_none());
            assert!(module.get_global("frame.metadata").unwrap().is_constant());
        }
    }
}
