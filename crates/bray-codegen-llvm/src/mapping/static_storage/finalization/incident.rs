use bray_codegen::{CodegenFailure, CodegenMappings, CodegenStaticStorageMapping};
use inkwell::builder::Builder;
use inkwell::module::Module;
use inkwell::types::{PointerType, StructType};
use inkwell::values::{FunctionValue, PointerValue, StructValue};
use inkwell::{AddressSpace, IntPredicate};

use super::super::storage::declare_static_callback_with_type;
use crate::mapping::LlvmTypeMappings;

pub(super) fn claim_storage<'context>(
    module: &Module<'context>,
    builder: &Builder<'context>,
    mapping: &CodegenStaticStorageMapping,
    finalization: &bray_codegen::CodegenStaticFinalization,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<(StructValue<'context>, PointerValue<'context>), CodegenFailure> {
    let context = types.context();
    let pointer = context.ptr_type(AddressSpace::default());

    let selected = finalization
        .selected()
        .expect("fallible finalization must retain its selected source callable");

    let requirement = mapping
        .outgoing_requirements()
        .iter()
        .find(|requirement| requirement.callable() == selected)
        .expect("fallible finalization must retain its admitted requirement");

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
        .build_int_to_ptr(address, pointer, "static.finalize.incident.payload")
        .map_err(CodegenFailure::backend_library)?;

    Ok((backing, payload))
}

pub(super) fn declare_static_incident_reporter<'context>(
    module: &Module<'context>,
    mapping: &CodegenStaticStorageMapping,
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<FunctionValue<'context>, CodegenFailure> {
    let name = format!("{}.incident.report", mapping.finalize_name());

    if let Some(callback) = module.get_function(&name) {
        return Ok(callback);
    }

    let context = types.context();
    let usize = crate::native::pointer_integer_type(context, types.target());

    let callback = declare_static_callback_with_type(
        module,
        &name,
        "static.finalize.incident.report",
        context.i32_type().fn_type(
            &[
                usize.into(),
                context.ptr_type(AddressSpace::default()).into(),
            ],
            false,
        ),
        types,
    );

    let builder = context.create_builder();

    let entry = callback
        .get_first_basic_block()
        .expect("static incident reporter has an entry block");

    builder.position_at_end(entry);

    let outcome = callback
        .get_nth_param(1)
        .expect("static incident reporter receives its outcome")
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

pub(super) fn declare_static_incident_destroyer<'context>(
    module: &Module<'context>,
    mappings: &CodegenMappings,
    mapping: &CodegenStaticStorageMapping,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<FunctionValue<'context>, CodegenFailure> {
    let name = format!("{}.incident.destroy", mapping.finalize_name());

    if let Some(callback) = module.get_function(&name) {
        return Ok(callback);
    }

    let context = types.context();
    let usize = crate::native::pointer_integer_type(context, types.target());
    let pointer = context.ptr_type(AddressSpace::default());

    let callback = declare_static_callback_with_type(
        module,
        &name,
        "static.finalize.incident.destroy",
        context
            .void_type()
            .fn_type(&[pointer.into(), pointer.into(), pointer.into()], false),
        types,
    );

    let builder = context.create_builder();

    let entry = callback
        .get_first_basic_block()
        .expect("static-storage realization requires an established mapping or value");

    let incident = callback
        .get_first_param()
        .expect("incident destructor receives its owned transport")
        .into_pointer_value();

    builder.position_at_end(entry);

    let incident_type = static_incident_type(context, usize, pointer, types.target());

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
        .build_int_to_ptr(payload, pointer, "static.finalize.incident.pointer")
        .map_err(CodegenFailure::backend_library)?;

    let destruction_outcome = callback
        .get_nth_param(1)
        .expect("static-storage realization requires an established mapping or value")
        .into_pointer_value();

    let cleanup = mapping
        .finalization()
        .and_then(bray_codegen::CodegenStaticFinalization::incident_cleanup)
        .expect("static-storage realization requires an established mapping or value");

    let symbol = mappings
        .instance_symbol(cleanup)
        .expect("static-storage realization requires an established mapping or value");

    let function = module
        .get_function(symbol.name().as_str())
        .expect("static-storage realization requires an established mapping or value");

    crate::mapping::invoke_with_outcome(
        &builder,
        function,
        symbol.signature(),
        &[payload_pointer.into()],
        destruction_outcome,
        "",
        types,
    )?;

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

pub(super) fn static_incident_value<'context>(
    builder: &Builder<'context>,
    mapping: &CodegenStaticStorageMapping,
    finalization: &bray_codegen::CodegenStaticFinalization,
    payload: PointerValue<'context>,
    report: FunctionValue<'context>,
    destroy: FunctionValue<'context>,
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<StructValue<'context>, CodegenFailure> {
    let context = types.context();
    let usize = crate::native::pointer_integer_type(context, types.target());
    let pointer = context.ptr_type(AddressSpace::default());

    let identity = finalization
        .error_type_identity()
        .expect("static-storage realization requires an established mapping or value");

    let identity = crate::native::identity_value(context, identity);

    let namespace = types
        .mappings()
        .unit()
        .compatibility(mapping.owner())
        .expect("static storage owner must have a partition compatibility")
        .source_namespace();

    let source = native_source_anchor_value(context, namespace, finalization.source());
    let incident_type = static_incident_type(context, usize, pointer, types.target());
    let incident = incident_type.const_zero();

    let incident = builder
        .build_insert_value(
            incident,
            builder
                .build_ptr_to_int(payload, usize, "static.finalize.incident.address")
                .map_err(CodegenFailure::backend_library)?,
            0,
            "static.finalize.incident.payload",
        )
        .map_err(CodegenFailure::backend_library)?
        .into_struct_value();

    let incident = builder
        .build_insert_value(incident, identity, 1, "static.finalize.incident.type")
        .map_err(CodegenFailure::backend_library)?
        .into_struct_value();

    let incident = builder
        .build_insert_value(incident, source, 2, "static.finalize.incident.source")
        .map_err(CodegenFailure::backend_library)?
        .into_struct_value();

    let incident = builder
        .build_insert_value(
            incident,
            report.as_global_value().as_pointer_value(),
            3,
            "static.finalize.incident.report",
        )
        .map_err(CodegenFailure::backend_library)?
        .into_struct_value();

    builder
        .build_insert_value(
            incident,
            destroy.as_global_value().as_pointer_value(),
            4,
            "static.finalize.incident.destroy",
        )
        .map_err(CodegenFailure::backend_library)
        .map(inkwell::values::AggregateValueEnum::into_struct_value)
}

fn static_incident_type<'context>(
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

fn native_source_anchor_value<'context>(
    context: &'context inkwell::context::Context,
    namespace: [u8; 32],
    source: Option<&bray_ir::MirSourceAnchor>,
) -> StructValue<'context> {
    let source = match source {
        Some(bray_ir::MirSourceAnchor::Source(origin)) => {
            let anchor = origin.source_anchor();
            let syntax = anchor.syntax();
            let range = syntax.full_range();

            bray_runtime_abi::NativeSourceAnchor::new(
                namespace,
                syntax.source_id().raw(),
                range.start().bytes(),
                range.end().bytes(),
                anchor.source_version().raw(),
            )
        }
        Some(bray_ir::MirSourceAnchor::ImportedSource {
            namespace,
            span,
            version,
            ..
        }) => bray_runtime_abi::NativeSourceAnchor::new(
            *namespace,
            span.source_id().raw(),
            span.start().bytes(),
            span.end().bytes(),
            version.raw(),
        ),
        Some(
            bray_ir::MirSourceAnchor::ExecutableHost(_)
            | bray_ir::MirSourceAnchor::GeneratedLifecycle(_)
            | bray_ir::MirSourceAnchor::CompilerProvidedCallable(_)
            | bray_ir::MirSourceAnchor::ImportedExecutable(_),
        )
        | None => bray_runtime_abi::NativeSourceAnchor::unavailable(),
    };

    crate::native::source_anchor_value(context, source)
}
