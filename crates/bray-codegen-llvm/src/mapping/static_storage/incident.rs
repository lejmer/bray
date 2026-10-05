use bray_codegen::{CodegenFailure, CodegenMappings, CodegenStaticStorageMapping};
use inkwell::AddressSpace;
use inkwell::builder::Builder;
use inkwell::module::Module;
use inkwell::types::{PointerType, StructType};
use inkwell::values::{FunctionValue, PointerValue, StructValue};

use super::super::LlvmTypeMappings;
use super::boundary::{invoke_static_boundary, mapped_instance_function, static_outcome};

use super::storage::declare_static_callback_with_type;

pub(super) fn store_static_incident<'context>(
    module: &Module<'context>,
    mappings: &CodegenMappings,
    mapping: &CodegenStaticStorageMapping,
    types: &LlvmTypeMappings<'context, '_>,
    builder: &Builder<'context>,
    incident: StructValue<'context>,
    incident_destination: inkwell::values::IntValue<'context>,
) -> Result<(), CodegenFailure> {
    let context = types.context();

    let pointer = context.ptr_type(AddressSpace::default());

    let incident_destination = builder
        .build_int_to_ptr(
            incident_destination,
            pointer,
            "static.finalize.incident.destination",
        )
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_store(incident_destination, incident)
        .map_err(CodegenFailure::backend_library)?;

    if let Some(host) = mappings.product_host() {
        let owner = builder
            .build_struct_gep(
                incident.get_type(),
                incident_destination,
                5,
                "static.incident.provider",
            )
            .map_err(CodegenFailure::backend_library)?;

        crate::native::retain_product_provider(
            context,
            module,
            builder,
            types.target(),
            host,
            mapping.owner().target().runtime_abi(),
            owner,
        )?;
    }

    Ok(())
}

pub(super) fn declare_static_incident_reporter<'context>(
    module: &Module<'context>,
    mapping: &CodegenStaticStorageMapping,
    payload_size: u64,
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
        context.i32_type().fn_type(&[usize.into()], false),
        types,
    );

    let reporter = crate::native::declare_runtime_function(
        module,
        context,
        types.target(),
        bray_runtime_interface::RuntimeAbiRole::EntryFailureReporting,
    )?;

    let builder = context.create_builder();

    let entry = callback
        .get_first_basic_block()
        .expect("static-storage realization requires an established mapping or value");

    let payload = callback
        .get_first_param()
        .expect("static-storage realization requires an established mapping or value");

    builder.position_at_end(entry);

    let status = builder
        .build_call(
            reporter,
            &[payload.into(), usize.const_int(payload_size, false).into()],
            "static.finalize.incident.report.status",
        )
        .map_err(CodegenFailure::backend_library)?
        .try_as_basic_value()
        .basic()
        .expect("static-storage realization requires an established mapping or value");

    builder
        .build_return(Some(&status))
        .map_err(CodegenFailure::backend_library)?;

    Ok(callback)
}

pub(super) fn declare_static_incident_destroyer<'context>(
    module: &Module<'context>,
    mappings: &CodegenMappings,
    mapping: &CodegenStaticStorageMapping,
    payload_size: u64,
    payload_alignment: u64,
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
            .fn_type(&[usize.into(), pointer.into(), pointer.into()], false),
        types,
    );

    let builder = context.create_builder();

    let entry = callback
        .get_first_basic_block()
        .expect("static-storage realization requires an established mapping or value");

    let payload = callback
        .get_first_param()
        .expect("static-storage realization requires an established mapping or value")
        .into_int_value();

    builder.position_at_end(entry);

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

    invoke_static_boundary(
        &builder,
        function,
        symbol.signature(),
        &[payload_pointer.into()],
        destruction_outcome,
        "",
        types,
    )?;

    let memory = mapping
        .finalization()
        .and_then(bray_codegen::CodegenStaticFinalization::incident_memory)
        .expect("static-storage realization requires an established mapping or value");

    let (deallocation, deallocation_signature) =
        mapped_instance_function(module, mappings, memory.deallocation());

    invoke_static_boundary(
        &builder,
        deallocation,
        deallocation_signature,
        &[
            payload_pointer.into(),
            usize.const_int(payload_size, false).into(),
            usize.const_int(payload_alignment, false).into(),
        ],
        static_outcome(callback),
        "",
        types,
    )?;

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

    let identity = context.i8_type().const_array(
        &identity
            .into_iter()
            .map(|byte| context.i8_type().const_int(u64::from(byte), false))
            .collect::<Vec<_>>(),
    );

    let namespace = types
        .mappings()
        .unit()
        .compatibility(mapping.owner())
        .expect("static storage owner must have a partition compatibility")
        .source_namespace();

    let source = native_source_anchor_value(context, namespace, finalization.source());
    let incident_type = static_incident_type(context, usize, pointer);
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
) -> StructType<'context> {
    context.struct_type(
        &[
            usize.into(),
            context.i8_type().array_type(32).into(),
            crate::native::source_anchor_type(context).into(),
            pointer.into(),
            pointer.into(),
            context
                .struct_type(
                    &[usize.into(), pointer.into(), pointer.into(), pointer.into()],
                    false,
                )
                .into(),
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
