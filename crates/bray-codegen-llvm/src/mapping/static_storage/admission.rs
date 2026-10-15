use super::super::LlvmTypeMappings;
use bray_codegen::{CodegenFailure, CodegenStaticStorageMapping};
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

    let callback = super::storage::declare_static_callback_with_type(
        module,
        &format!("{}.admission", mapping.symbol().as_str()),
        "static.admission",
        context
            .void_type()
            .fn_type(&[context.i32_type().into(), pointer.into()], false),
        types,
    );

    let builder = context.create_builder();

    builder.position_at_end(
        callback
            .get_first_basic_block()
            .expect("static admission callback has an entry"),
    );

    let requirements = mapping.outgoing_requirements();

    if requirements.is_empty() {
        builder
            .build_return(None)
            .map_err(CodegenFailure::backend_library)?;

        return Ok(callback);
    }

    let descriptors = super::super::cleanup::descriptors(module, &builder, requirements, types)?;

    let count = crate::native::pointer_integer_type(context, types.target()).const_int(
        u64::try_from(requirements.len()).expect("static descriptor count fits native ABI"),
        false,
    );

    let bundle = super::super::cleanup::bundle(&builder, descriptors, count, types)?;

    let release = builder
        .build_int_compare(
            inkwell::IntPredicate::NE,
            callback
                .get_nth_param(0)
                .expect("static transition parameter")
                .into_int_value(),
            context.i32_type().const_zero(),
            "static.discharge",
        )
        .map_err(CodegenFailure::backend_library)?;

    let admit = context.append_basic_block(callback, "static.admit");
    let discharge = context.append_basic_block(callback, "static.discharge");

    builder
        .build_conditional_branch(release, discharge, admit)
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

    builder.position_at_end(discharge);

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

    Ok(callback)
}
