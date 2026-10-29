use super::super::LlvmTypeMappings;
use bray_codegen::{CodegenCleanupRequirement, CodegenFailure, CodegenStaticStorageMapping};
use bray_runtime_interface::RuntimeAbiRole;
use inkwell::module::Module;
use inkwell::values::FunctionValue;

pub(super) fn declare<'context>(
    module: &Module<'context>,
    mapping: &CodegenStaticStorageMapping,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<FunctionValue<'context>, CodegenFailure> {
    let context = types.context();
    let pointer = context.ptr_type(inkwell::AddressSpace::default());

    let callback = crate::native::declare_callback(
        module,
        &format!("{}.admission", mapping.symbol().as_str()),
        "static.admission",
        context
            .void_type()
            .fn_type(&[context.i32_type().into(), pointer.into()], false),
        types.target(),
    );

    let builder = context.create_builder();

    builder.position_at_end(
        callback
            .get_first_basic_block()
            .expect("static admission callback has an entry"),
    );

    let requirements = mapping.outgoing_requirements();
    let retained = retained_resolution_requirement(mapping);

    if requirements.is_empty() && retained.is_none() {
        builder
            .build_return(None)
            .map_err(CodegenFailure::backend_library)?;

        return Ok(callback);
    }

    let admitted = requirements
        .iter()
        .chain(retained)
        .cloned()
        .collect::<Vec<_>>();

    let descriptors = super::super::cleanup::descriptors(module, &builder, &admitted, types)?;

    let count = crate::native::pointer_integer_type(context, types.target()).const_int(
        u64::try_from(admitted.len()).expect("static descriptor count fits native ABI"),
        false,
    );

    let bundle = super::super::cleanup::bundle(&builder, descriptors, count, types)?;

    let retained_bundle = retained
        .map(|requirement| {
            let descriptors = super::super::cleanup::descriptors(
                module,
                &builder,
                std::slice::from_ref(requirement),
                types,
            )?;

            let count =
                crate::native::pointer_integer_type(context, types.target()).const_int(1, false);

            super::super::cleanup::bundle(&builder, descriptors, count, types).map(Some)
        })
        .transpose()?
        .flatten();

    let admit = context.append_basic_block(callback, "static.admit");
    let rollback = context.append_basic_block(callback, "static.rollback");
    let resolution = context.append_basic_block(callback, "static.resolution");
    let invalid = context.append_basic_block(callback, "static.invalid");

    builder
        .build_switch(
            callback
                .get_nth_param(0)
                .expect("static transition parameter")
                .into_int_value(),
            invalid,
            &[
                (context.i32_type().const_zero(), admit),
                (context.i32_type().const_int(1, false), rollback),
                (context.i32_type().const_int(2, false), resolution),
            ],
        )
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(admit);

    let admission = crate::native::declare_runtime_function(
        module,
        context,
        types.target(),
        RuntimeAbiRole::OutgoingAdmission,
    )?;

    builder
        .build_call(
            admission,
            &[
                bundle.into(),
                callback
                    .get_nth_param(1)
                    .expect("static outcome parameter")
                    .into(),
            ],
            "",
        )
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_return(None)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(rollback);

    let retirement = crate::native::declare_runtime_function(
        module,
        context,
        types.target(),
        RuntimeAbiRole::OutgoingDischarge,
    )?;

    builder
        .build_call(retirement, &[bundle.into()], "")
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_return(None)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(resolution);

    if let Some(retained_bundle) = retained_bundle {
        builder
            .build_call(retirement, &[retained_bundle.into()], "")
            .map_err(CodegenFailure::backend_library)?;
    }

    builder
        .build_return(None)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(invalid);

    let trap = inkwell::intrinsics::Intrinsic::find("llvm.trap")
        .expect("LLVM supplies trap")
        .get_declaration(module, &[])
        .expect("trap is not overloaded");

    builder
        .build_call(trap, &[], "")
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_unreachable()
        .map_err(CodegenFailure::backend_library)?;

    Ok(callback)
}

fn retained_resolution_requirement(
    mapping: &CodegenStaticStorageMapping,
) -> Option<&CodegenCleanupRequirement> {
    let finalization = mapping.finalization()?;

    if !matches!(
        finalization.result(),
        bray_runtime_interface::ExecutableEntryResult::Fallible { .. }
    ) {
        return None;
    }

    let selected = finalization.selected()?;

    mapping
        .outgoing_requirements()
        .iter()
        .find(|requirement| requirement.callable() == selected)
}
