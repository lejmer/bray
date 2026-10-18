use bray_codegen::{CodegenCleanupRequirement, CodegenFailure, CodegenSymbolMapping};
use inkwell::builder::Builder;
use inkwell::module::Module;
use inkwell::types::{PointerType, StructType};
use inkwell::values::{FunctionValue, PointerValue, StructValue};
use inkwell::{AddressSpace, IntPredicate};

use crate::mapping::LlvmTypeMappings;
use crate::native::declare_callback;

pub(crate) fn claim_storage<'context>(
    module: &Module<'context>,
    builder: &Builder<'context>,
    requirement: &CodegenCleanupRequirement,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<(StructValue<'context>, PointerValue<'context>), CodegenFailure> {
    let context = types.context();
    let pointer = context.ptr_type(AddressSpace::default());

    let descriptor = crate::mapping::cleanup::descriptors(
        module,
        builder,
        std::slice::from_ref(requirement),
        types,
    )?;

    let lease_type = crate::mapping::cleanup::lease_type(context, types.target());

    let lease =
        crate::translation::allocate_temporary(context, builder, lease_type, "incident.lease")?;

    builder
        .build_store(lease, lease_type.const_zero())
        .map_err(CodegenFailure::backend_library)?;

    let claim = crate::native::declare_runtime_function(
        module,
        context,
        types.target(),
        bray_runtime_interface::RuntimeAbiRole::CleanupStorageClaim,
    )?;

    builder
        .build_call(claim, &[descriptor.into(), lease.into()], "")
        .map_err(CodegenFailure::backend_library)?;

    let backing = builder
        .build_load(lease_type, lease, "incident.backing")
        .map_err(CodegenFailure::backend_library)?
        .into_struct_value();

    let address = builder
        .build_extract_value(backing, 4, "incident.address")
        .map_err(CodegenFailure::backend_library)?
        .into_int_value();

    let payload = builder
        .build_int_to_ptr(address, pointer, "incident.payload")
        .map_err(CodegenFailure::backend_library)?;

    Ok((backing, payload))
}

pub(crate) fn declare_reporter<'context>(
    module: &Module<'context>,
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<FunctionValue<'context>, CodegenFailure> {
    let name = "__bray_incident_report_identity";

    if let Some(callback) = module.get_function(name) {
        return Ok(callback);
    }

    let context = types.context();
    let usize = crate::native::pointer_integer_type(context, types.target());

    let callback = declare_callback(
        module,
        name,
        "incident.report",
        context.i32_type().fn_type(
            &[
                usize.into(),
                context.ptr_type(AddressSpace::default()).into(),
            ],
            false,
        ),
        types.target(),
    );

    let builder = context.create_builder();

    let entry = callback
        .get_first_basic_block()
        .expect("incident reporter has an entry block");

    builder.position_at_end(entry);

    let outcome = callback
        .get_nth_param(1)
        .expect("incident reporter receives its outcome")
        .into_pointer_value();

    builder
        .build_store(
            outcome,
            crate::native::run_outcome_type(context, types.target()).const_zero(),
        )
        .map_err(CodegenFailure::backend_library)?;

    // Source reports the descriptor identity. No checked value formatter is selected.
    builder
        .build_return(Some(&context.i32_type().const_zero()))
        .map_err(CodegenFailure::backend_library)?;

    Ok(callback)
}

pub(crate) fn declare_destroyer<'context>(
    module: &Module<'context>,
    cleanup: Option<(&CodegenSymbolMapping, bray_ir::MirGeneratedLifecycleRole)>,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<FunctionValue<'context>, CodegenFailure> {
    let name = cleanup.map_or_else(
        || "__bray_incident_release".to_owned(),
        |(cleanup, _)| format!("{}.incident.destroy", cleanup.name().as_str()),
    );

    if let Some(callback) = module.get_function(&name) {
        return Ok(callback);
    }

    let context = types.context();
    let usize = crate::native::pointer_integer_type(context, types.target());
    let pointer = context.ptr_type(AddressSpace::default());

    let callback = declare_callback(
        module,
        &name,
        "incident.destroy",
        context
            .void_type()
            .fn_type(&[pointer.into(), pointer.into(), pointer.into()], false),
        types.target(),
    );

    let builder = context.create_builder();

    let entry = callback
        .get_first_basic_block()
        .expect("incident destruction requires its checked cleanup mapping");

    let incident = callback
        .get_first_param()
        .expect("incident destructor receives its owned transport")
        .into_pointer_value();

    builder.position_at_end(entry);

    let incident_type = record_type(context, usize, pointer, types.target());

    let payload_address = builder
        .build_struct_gep(incident_type, incident, 0, "incident.payload.slot")
        .map_err(CodegenFailure::backend_library)?;

    let payload = builder
        .build_load(usize, payload_address, "incident.payload")
        .map_err(CodegenFailure::backend_library)?
        .into_int_value();

    let lease = builder
        .build_struct_gep(incident_type, incident, 5, "incident.backing")
        .map_err(CodegenFailure::backend_library)?;

    let payload_pointer = builder
        .build_int_to_ptr(payload, pointer, "incident.pointer")
        .map_err(CodegenFailure::backend_library)?;

    let destruction_outcome = callback
        .get_nth_param(1)
        .expect("incident destructor receives its destruction outcome")
        .into_pointer_value();

    let release_outcome = callback
        .get_nth_param(2)
        .expect("incident destructor receives its backing-release outcome")
        .into_pointer_value();

    // Both destinations are write-only at this boundary. A successful source call
    // may leave its failure context untouched, and backing release returns a status.
    for outcome in [destruction_outcome, release_outcome] {
        builder
            .build_store(
                outcome,
                crate::native::run_outcome_type(context, types.target()).const_zero(),
            )
            .map_err(CodegenFailure::backend_library)?;
    }

    if let Some((cleanup, role)) = cleanup {
        let function = module
            .get_function(cleanup.name().as_str())
            .expect("incident destruction requires its checked cleanup mapping");

        let mut arguments = vec![payload_pointer.into()];

        if role
            == bray_ir::MirGeneratedLifecycleRole::Destroy(
                bray_ir::MirDestructionDisposition::Dynamic,
            )
        {
            arguments.push(context.bool_type().const_int(1, false).into());
        }

        let collection = crate::mapping::cleanup::begin_collection(
            module,
            &builder,
            destruction_outcome,
            types,
        )?;

        arguments.extend(collection.map(inkwell::values::BasicMetadataValueEnum::from));

        crate::mapping::invoke_with_outcome(
            &builder,
            function,
            cleanup.signature(),
            &arguments,
            destruction_outcome,
            "",
            types,
        )?;

        crate::mapping::cleanup::publish_collection(module, &builder, collection[0], types)?;
    }

    let release = crate::native::declare_runtime_function(
        module,
        context,
        types.target(),
        bray_runtime_interface::RuntimeAbiRole::CleanupStorageRelease,
    )?;

    let status = builder
        .build_call(release, &[lease.into()], "incident.backing.release")
        .map_err(CodegenFailure::backend_library)?
        .try_as_basic_value()
        .basic()
        .expect("backing release returns its boundary status")
        .into_int_value();

    let valid = builder
        .build_int_compare(
            IntPredicate::EQ,
            status,
            context.i32_type().const_zero(),
            "incident.backing.valid",
        )
        .map_err(CodegenFailure::backend_library)?;

    let completed = context.append_basic_block(callback, "incident.released");
    let invalid = context.append_basic_block(callback, "incident.invalid.lease");

    builder
        .build_conditional_branch(valid, completed, invalid)
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

    builder.position_at_end(completed);

    builder
        .build_return(None)
        .map_err(CodegenFailure::backend_library)?;

    Ok(callback)
}

pub(crate) fn record_value<'context>(
    builder: &Builder<'context>,
    identity: [u8; 32],
    source: StructValue<'context>,
    storage: (StructValue<'context>, PointerValue<'context>),
    report: FunctionValue<'context>,
    destroy: FunctionValue<'context>,
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<StructValue<'context>, CodegenFailure> {
    let context = types.context();
    let usize = crate::native::pointer_integer_type(context, types.target());
    let pointer = context.ptr_type(AddressSpace::default());

    let (backing, payload) = storage;

    let address = builder
        .build_ptr_to_int(payload, usize, "incident.address")
        .map_err(CodegenFailure::backend_library)?;

    let fields: [inkwell::values::BasicValueEnum<'context>; 6] = [
        address.into(),
        crate::native::identity_value(context, identity).into(),
        source.into(),
        report.as_global_value().as_pointer_value().into(),
        destroy.as_global_value().as_pointer_value().into(),
        backing.into(),
    ];

    let mut incident = record_type(context, usize, pointer, types.target()).const_zero();

    for (index, field) in fields.into_iter().enumerate() {
        incident = builder
            .build_insert_value(
                incident,
                field,
                u32::try_from(index).expect("incident field ordinal fits its native record"),
                "incident.field",
            )
            .map_err(CodegenFailure::backend_library)?
            .into_struct_value();
    }

    Ok(incident)
}

fn record_type<'context>(
    context: &'context inkwell::context::Context,
    usize: inkwell::types::IntType<'context>,
    pointer: PointerType<'context>,
    target: &bray_codegen::CodegenTarget,
) -> StructType<'context> {
    context.struct_type(
        &[
            usize.into(),
            context.i8_type().array_type(32).into(),
            crate::native::source_anchor_type(context).into(),
            pointer.into(),
            pointer.into(),
            crate::mapping::cleanup::lease_type(context, target).into(),
        ],
        false,
    )
}
