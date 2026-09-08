use bray_codegen::{CodegenCleanupIncident, CodegenFailure, CodegenInstanceKey, CodegenMappings};
use inkwell::AddressSpace;
use inkwell::builder::Builder;
use inkwell::module::Module;
use inkwell::types::{PointerType, StructType};
use inkwell::values::{FunctionValue, PointerValue, StructValue};

use super::LlvmTypeMappings;
use super::boundary::{
    declare_generated_callback, invoke_generated_call, load_boundary_outcome,
    mapped_instance_function, merge_boundary_outcomes, panic_report_callbacks,
    panic_report_callbacks_type,
};

#[expect(
    clippy::too_many_arguments,
    reason = "incident emission retains its concrete owner, payload, callback identity, and LLVM context"
)]
pub(crate) fn create_owned_cleanup_incident<'context>(
    module: &Module<'context>,
    mappings: &CodegenMappings,
    owner: &CodegenInstanceKey,
    memory: &CodegenCleanupIncident,
    builder: &Builder<'context>,
    error_pointer: PointerValue<'context>,
    boundary_destination: PointerValue<'context>,
    allocation_failure: inkwell::basic_block::BasicBlock<'context>,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<StructValue<'context>, CodegenFailure> {
    let context = types.context();
    let usize = crate::native::pointer_integer_type(context, types.target());

    let error_layout = mappings
        .instance_ty(owner, memory.ty())
        .and_then(bray_codegen::CodegenTypeMapping::layout)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let (allocation, allocation_signature) =
        mapped_instance_function(module, mappings, memory.allocation())?;

    let (allocation_call, allocation_outcome) = invoke_generated_call(
        builder,
        allocation,
        allocation_signature,
        &[
            usize.const_int(error_layout.size(), false).into(),
            usize
                .const_int(error_layout.alignment().get(), false)
                .into(),
        ],
        "cleanup.incident.payload",
        types,
    )?;

    if let Some(allocation_outcome) = allocation_outcome {
        release_error_on_allocation_failure(
            module,
            mappings,
            memory,
            builder,
            error_pointer,
            allocation_outcome,
            boundary_destination,
            allocation_failure,
            types,
        )?;
    }

    let payload = allocation_call
        .try_as_basic_value()
        .basic()
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?
        .into_pointer_value();

    let alignment = crate::conversion::target_value(
        error_layout.alignment().get(),
        "static_finalization_error_alignment",
    )?;

    builder
        .build_memcpy(
            payload,
            alignment,
            error_pointer,
            alignment,
            usize.const_int(error_layout.size(), false),
        )
        .map_err(CodegenFailure::backend_library)?;

    let name = mappings
        .instance_symbol(memory.cleanup())
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?
        .name()
        .as_str();

    let report = crate::native::declare_runtime_function(
        module,
        context,
        types.target(),
        bray_runtime_interface::RuntimeAbiRole::CleanupIncidentDetailReporting,
    )?;

    let destroy = declare_incident_destroyer(
        module,
        mappings,
        memory,
        name,
        error_layout.size(),
        error_layout.alignment().get(),
        types,
    )?;

    incident_value(
        builder,
        memory,
        payload,
        report,
        destroy,
        panic_report_callbacks(module, types)?,
        types,
    )
}

fn declare_incident_destroyer<'context>(
    module: &Module<'context>,
    mappings: &CodegenMappings,
    memory: &CodegenCleanupIncident,
    name: &str,
    payload_size: u64,
    payload_alignment: u64,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<FunctionValue<'context>, CodegenFailure> {
    let name = format!("{name}.incident.destroy");

    if let Some(callback) = module.get_function(&name) {
        return Ok(callback);
    }

    let context = types.context();
    let usize = crate::native::pointer_integer_type(context, types.target());
    let pointer = context.ptr_type(AddressSpace::default());

    let callback = declare_generated_callback(
        module,
        &name,
        "cleanup.incident.destroy",
        usize.fn_type(&[usize.into()], false),
        types,
    );

    let builder = context.create_builder();

    let entry = callback
        .get_first_basic_block()
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let payload = callback
        .get_first_param()
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?
        .into_int_value();

    builder.position_at_end(entry);

    let payload_pointer = builder
        .build_int_to_ptr(payload, pointer, "cleanup.incident.pointer")
        .map_err(CodegenFailure::backend_library)?;

    let cleanup = memory.cleanup();

    let symbol = mappings
        .instance_symbol(cleanup)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let function = module
        .get_function(symbol.name().as_str())
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let (_, cleanup_outcome) = invoke_generated_call(
        &builder,
        function,
        symbol.signature(),
        &[payload_pointer.into()],
        "",
        types,
    )?;

    let (deallocation, deallocation_signature) =
        mapped_instance_function(module, mappings, memory.deallocation())?;

    let (_, release_outcome) = invoke_generated_call(
        &builder,
        deallocation,
        deallocation_signature,
        &[
            payload_pointer.into(),
            usize.const_int(payload_size, false).into(),
            usize.const_int(payload_alignment, false).into(),
        ],
        "",
        types,
    )?;

    let outcome =
        merge_boundary_outcomes(module, &builder, cleanup_outcome, release_outcome, types)?;

    let outcome = load_boundary_outcome(&builder, outcome, types)?;

    builder
        .build_return(Some(&outcome))
        .map_err(CodegenFailure::backend_library)?;

    Ok(callback)
}

fn release_error_on_allocation_failure<'context>(
    module: &Module<'context>,
    mappings: &CodegenMappings,
    memory: &CodegenCleanupIncident,
    builder: &Builder<'context>,
    error_pointer: PointerValue<'context>,
    allocation_outcome: PointerValue<'context>,
    destination: PointerValue<'context>,
    allocation_failure: inkwell::basic_block::BasicBlock<'context>,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<(), CodegenFailure> {
    let context = types.context();
    let outcome = load_boundary_outcome(builder, Some(allocation_outcome), types)?;

    let failed = builder
        .build_int_compare(
            inkwell::IntPredicate::NE,
            outcome,
            outcome.get_type().const_zero(),
            "cleanup.incident.allocation.failed",
        )
        .map_err(CodegenFailure::backend_library)?;

    let function = builder
        .get_insert_block()
        .and_then(inkwell::basic_block::BasicBlock::get_parent)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let failure = context.append_basic_block(function, "cleanup.incident.allocation.failure");
    let continued = context.append_basic_block(function, "cleanup.incident.allocation.completed");

    builder
        .build_conditional_branch(failed, failure, continued)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(failure);

    let (cleanup, signature) = mapped_instance_function(module, mappings, memory.cleanup())?;

    let (_, cleanup_outcome) = invoke_generated_call(
        builder,
        cleanup,
        signature,
        &[error_pointer.into()],
        "",
        types,
    )?;

    let merged = merge_boundary_outcomes(
        module,
        builder,
        Some(allocation_outcome),
        cleanup_outcome,
        types,
    )?;

    let merged = load_boundary_outcome(builder, merged, types)?;

    builder
        .build_store(destination, merged)
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_unconditional_branch(allocation_failure)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(continued);

    Ok(())
}

fn incident_value<'context>(
    builder: &Builder<'context>,
    incident: &CodegenCleanupIncident,
    payload: PointerValue<'context>,
    report: FunctionValue<'context>,
    destroy: FunctionValue<'context>,
    panics: StructValue<'context>,
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<StructValue<'context>, CodegenFailure> {
    let context = types.context();
    let usize = crate::native::pointer_integer_type(context, types.target());
    let pointer = context.ptr_type(AddressSpace::default());

    let identity = crate::native::type_identity_value(context, incident.type_identity());

    let source = crate::native::source_anchor_from_mir(context, incident.source());
    let incident_type = incident_type(context, usize, pointer);
    let incident = incident_type.const_zero();

    let incident = builder
        .build_insert_value(
            incident,
            builder
                .build_ptr_to_int(payload, usize, "cleanup.incident.address")
                .map_err(CodegenFailure::backend_library)?,
            0,
            "cleanup.incident.payload",
        )
        .map_err(CodegenFailure::backend_library)?
        .into_struct_value();

    let incident = builder
        .build_insert_value(incident, identity, 1, "cleanup.incident.type")
        .map_err(CodegenFailure::backend_library)?
        .into_struct_value();

    let incident = builder
        .build_insert_value(incident, source, 2, "cleanup.incident.source")
        .map_err(CodegenFailure::backend_library)?
        .into_struct_value();

    let incident = builder
        .build_insert_value(
            incident,
            report.as_global_value().as_pointer_value(),
            3,
            "cleanup.incident.report",
        )
        .map_err(CodegenFailure::backend_library)?
        .into_struct_value();

    let incident = builder
        .build_insert_value(
            incident,
            destroy.as_global_value().as_pointer_value(),
            4,
            "cleanup.incident.destroy",
        )
        .map_err(CodegenFailure::backend_library)?
        .into_struct_value();

    builder
        .build_insert_value(incident, panics, 5, "cleanup.incident.panics")
        .map_err(CodegenFailure::backend_library)
        .map(inkwell::values::AggregateValueEnum::into_struct_value)
}

fn incident_type<'context>(
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
            panic_report_callbacks_type(context).into(),
        ],
        false,
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn incident_layout_matches_the_runtime_abi() {
        let fixture = bray_codegen::test_support::codegen_request();
        let request = fixture.request();
        let machine = crate::machine::LlvmTargetMachine::create(request.target()).unwrap();
        let data = machine.target_data();
        let context = inkwell::context::Context::create();
        let word = crate::native::pointer_integer_type(&context, request.target());

        let ty = super::incident_type(
            &context,
            word,
            context.ptr_type(inkwell::AddressSpace::default()),
        );

        let bytes = u64::from(word.get_bit_width() / 8);

        assert_eq!(data.offset_of_element(&ty, 0), Some(0));
        assert_eq!(data.offset_of_element(&ty, 1), Some(bytes));

        assert_eq!(
            data.offset_of_element(&ty, 4).unwrap() + bytes,
            data.offset_of_element(&ty, 5).unwrap()
        );

        assert_eq!(
            data.get_store_size(&ty),
            data.offset_of_element(&ty, 5).unwrap() + 4 * bytes
        );
    }
}
