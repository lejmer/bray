use bray_codegen::{CodegenFailure, CodegenSymbolKey, CodegenTarget};
use bray_runtime_abi::NativeRunState;
use bray_runtime_interface::{RuntimeAbiRole, RuntimeAbiVersion};
use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::StructType;
use inkwell::values::{BasicValueEnum, PointerValue};

use super::unit::{native_run_outcome, native_run_state_is};

pub(crate) fn branch_on_pending_outcome<'context>(
    context: &'context Context,
    builder: &Builder<'context>,
    ty: StructType<'context>,
    storage: PointerValue<'context>,
) -> Result<
    (
        BasicValueEnum<'context>,
        BasicBlock<'context>,
        BasicBlock<'context>,
    ),
    CodegenFailure,
> {
    let outcome = builder
        .build_load(ty, storage, "call.panic.report")
        .map_err(CodegenFailure::backend_library)?;

    let (state, _) = native_run_outcome(builder, outcome)?;

    let report = builder
        .build_extract_value(outcome.into_struct_value(), 2, "call.report")
        .map_err(CodegenFailure::backend_library)?;

    let completed =
        native_run_state_is(builder, state, NativeRunState::COMPLETED, "call.completed")?;

    let function = builder
        .get_insert_block()
        .and_then(BasicBlock::get_parent)
        .expect("LLVM translation requires an established mapping or value");

    let propagate = context.append_basic_block(function, "call.panic.propagate");
    let continued = context.append_basic_block(function, "call.panic.continue");
    let inspect = context.append_basic_block(function, "call.outcome.inspect");
    let cancelled = context.append_basic_block(function, "call.cancelled.propagate");

    builder
        .build_conditional_branch(completed, continued, inspect)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(inspect);

    let cancellation =
        native_run_state_is(builder, state, NativeRunState::CANCELLED, "call.cancelled")?;

    builder
        .build_conditional_branch(cancellation, cancelled, propagate)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(propagate);

    Ok((report, continued, cancelled))
}

pub(crate) fn merge_pending_outcomes<'context>(
    module: &Module<'context>,
    context: &'context Context,
    builder: &Builder<'context>,
    target: &CodegenTarget,
    runtime_abi: RuntimeAbiVersion,
    destination: PointerValue<'context>,
    secondary: PointerValue<'context>,
) -> Result<(), CodegenFailure> {
    let ty = crate::native::run_outcome_type(context, target);

    let load_field = |pointer, index, name| {
        let field = builder
            .build_struct_gep(ty, pointer, index, name)
            .map_err(CodegenFailure::backend_library)?;

        let value = builder
            .build_load(
                ty.get_field_type_at_index(index)
                    .expect("native outcome field exists"),
                field,
                name,
            )
            .map_err(CodegenFailure::backend_library)?;

        Ok::<_, CodegenFailure>((field, value))
    };

    let (state, first_state) = load_field(destination, 0, "outcome.first.state")?;

    let (_, second_state) = load_field(secondary, 0, "outcome.second.state")?;

    let first_panicked = native_run_state_is(
        builder,
        first_state.into_int_value(),
        NativeRunState::PANICKED,
        "outcome.first.panicked",
    )?;

    let second_completed = native_run_state_is(
        builder,
        second_state.into_int_value(),
        NativeRunState::COMPLETED,
        "outcome.second.completed",
    )?;

    let keep_first = builder
        .build_or(first_panicked, second_completed, "outcome.keep.first")
        .map_err(CodegenFailure::backend_library)?;

    let selected = builder
        .build_select(keep_first, first_state, second_state, "outcome.state")
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_store(state, selected)
        .map_err(CodegenFailure::backend_library)?;

    let (payload, first_payload) = load_field(destination, 1, "outcome.first.payload")?;

    let (_, second_payload) = load_field(secondary, 1, "outcome.second.payload")?;

    let selected = builder
        .build_select(keep_first, first_payload, second_payload, "outcome.payload")
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_store(payload, selected)
        .map_err(CodegenFailure::backend_library)?;

    let first_report = builder
        .build_struct_gep(ty, destination, 2, "outcome.first.report")
        .map_err(CodegenFailure::backend_library)?;

    let second_report = builder
        .build_struct_gep(ty, secondary, 2, "outcome.second.report")
        .map_err(CodegenFailure::backend_library)?;

    merge_report_slots(
        module,
        context,
        builder,
        target,
        runtime_abi,
        first_report,
        second_report,
    )?;

    builder
        .build_store(secondary, ty.const_zero())
        .map_err(CodegenFailure::backend_library)?;

    Ok(())
}

/// Collects an ordinary native call's outcome into borrowed structural cleanup storage.
pub(crate) fn collect_cleanup_outcome<'context>(
    module: &Module<'context>,
    context: &'context Context,
    builder: &Builder<'context>,
    target: &CodegenTarget,
    runtime_abi: RuntimeAbiVersion,
    collection: [PointerValue<'context>; 2],
    incoming: PointerValue<'context>,
) -> Result<(), CodegenFailure> {
    let ty = crate::native::run_outcome_type(context, target);

    let function = builder
        .get_insert_block()
        .and_then(BasicBlock::get_parent)
        .expect("cleanup collection requires an active function");

    let state = builder
        .build_struct_gep(ty, incoming, 0, "cleanup.incoming.state")
        .map_err(CodegenFailure::backend_library)?;

    let state = builder
        .build_load(context.i32_type(), state, "cleanup.incoming.state")
        .map_err(CodegenFailure::backend_library)?
        .into_int_value();

    let report = builder
        .build_struct_gep(ty, incoming, 2, "cleanup.incoming.report")
        .map_err(CodegenFailure::backend_library)?;

    let normal = native_run_state_is(
        builder,
        state,
        NativeRunState::COMPLETED,
        "cleanup.incoming.normal",
    )?;

    let failed = builder
        .build_not(normal, "cleanup.incoming.failed")
        .map_err(CodegenFailure::backend_library)?;

    let owned = report_slot_has_ownership(context, builder, report)?;

    let present = builder
        .build_or(failed, owned, "cleanup.incoming.present")
        .map_err(CodegenFailure::backend_library)?;

    let collect = context.append_basic_block(function, "cleanup.collect");
    let finished = context.append_basic_block(function, "cleanup.collected");

    builder
        .build_conditional_branch(present, collect, finished)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(collect);

    let previous = builder
        .build_load(context.i32_type(), collection[0], "cleanup.previous")
        .map_err(CodegenFailure::backend_library)?
        .into_int_value();

    let empty = builder
        .build_int_compare(
            inkwell::IntPredicate::EQ,
            previous,
            context.i32_type().const_zero(),
            "cleanup.empty",
        )
        .map_err(CodegenFailure::backend_library)?;

    let initialize = context.append_basic_block(function, "cleanup.initialize");
    let merge = context.append_basic_block(function, "cleanup.merge");
    let retained = context.append_basic_block(function, "cleanup.retained");

    builder
        .build_conditional_branch(empty, initialize, merge)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(initialize);

    let value = builder
        .build_load(
            crate::native::panic_report_type(context),
            report,
            "cleanup.first.report",
        )
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_store(collection[1], value)
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_unconditional_branch(retained)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(merge);

    merge_report_slots(
        module,
        context,
        builder,
        target,
        runtime_abi,
        collection[1],
        report,
    )?;

    builder
        .build_unconditional_branch(retained)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(retained);

    let mut incoming_state = context
        .i32_type()
        .const_int(u64::from(bray_ir::MirCleanupState::Incidents.code()), false);

    for (native, collected) in [
        (
            NativeRunState::CANCELLED,
            bray_ir::MirCleanupState::Cancelled,
        ),
        (NativeRunState::PANICKED, bray_ir::MirCleanupState::Panicked),
    ] {
        let matches = native_run_state_is(builder, state, native, "cleanup.incoming.kind")?;

        incoming_state = builder
            .build_select(
                matches,
                context
                    .i32_type()
                    .const_int(u64::from(collected.code()), false),
                incoming_state,
                "cleanup.incoming.collection",
            )
            .map_err(CodegenFailure::backend_library)?
            .into_int_value();
    }

    // Cleanup discriminants encode precedence: normal < incidents < cancellation < panic.
    let preserve = builder
        .build_int_compare(
            inkwell::IntPredicate::UGT,
            previous,
            incoming_state,
            "cleanup.preserve",
        )
        .map_err(CodegenFailure::backend_library)?;

    let selected = builder
        .build_select(preserve, previous, incoming_state, "cleanup.state")
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_store(collection[0], selected)
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_store(incoming, ty.const_zero())
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_unconditional_branch(finished)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(finished);

    Ok(())
}

fn merge_report_slots<'context>(
    module: &Module<'context>,
    context: &'context Context,
    builder: &Builder<'context>,
    target: &CodegenTarget,
    runtime_abi: RuntimeAbiVersion,
    first: PointerValue<'context>,
    second: PointerValue<'context>,
) -> Result<(), CodegenFailure> {
    let function = builder
        .get_insert_block()
        .and_then(BasicBlock::get_parent)
        .expect("LLVM report merge requires an active function");

    let inspect = context.append_basic_block(function, "report.inspect.first");
    let replace = context.append_basic_block(function, "report.replace");
    let merge = context.append_basic_block(function, "report.merge");
    let finished = context.append_basic_block(function, "report.finished");
    let incoming = report_slot_has_ownership(context, builder, second)?;

    builder
        .build_conditional_branch(incoming, inspect, finished)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(inspect);

    let retained = report_slot_has_ownership(context, builder, first)?;

    builder
        .build_conditional_branch(retained, merge, replace)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(replace);

    let report = builder
        .build_load(
            crate::native::panic_report_type(context),
            second,
            "report.incoming",
        )
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_store(first, report)
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_unconditional_branch(finished)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(merge);

    let role = RuntimeAbiRole::PanicReportSuppression;
    let runtime = crate::native::declare_runtime_function(module, context, target, role)?;

    let report = crate::native::invoke_function(
        context,
        builder,
        target,
        &CodegenSymbolKey::Runtime(bray_ir::MirRuntimeReference::new(role, runtime_abi)),
        runtime,
        &[first.into(), second.into()],
        "outcome.suppressed.report",
    )?
    .expect("report suppression returns the combined owner");

    builder
        .build_store(first, report)
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_unconditional_branch(finished)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(finished);

    Ok(())
}

fn report_slot_has_ownership<'context>(
    context: &'context Context,
    builder: &Builder<'context>,
    report: PointerValue<'context>,
) -> Result<inkwell::values::IntValue<'context>, CodegenFailure> {
    let ty = crate::native::panic_report_type(context);

    report_has_ownership(context, builder, |index| {
        let pointer = builder
            .build_struct_gep(ty, report, index, "report.ownership.field")
            .map_err(CodegenFailure::backend_library)?;

        builder
            .build_load(
                ty.get_field_type_at_index(index)
                    .expect("report ownership field exists"),
                pointer,
                "report.ownership",
            )
            .map(inkwell::values::BasicValueEnum::into_int_value)
            .map_err(CodegenFailure::backend_library)
    })
}

pub(crate) fn report_has_ownership<'context>(
    context: &'context Context,
    builder: &Builder<'context>,
    mut field: impl FnMut(u32) -> Result<inkwell::values::IntValue<'context>, CodegenFailure>,
) -> Result<inkwell::values::IntValue<'context>, CodegenFailure> {
    let mut present = context.bool_type().const_zero();

    // The native header owns its primary, incident chain and unused reservation independently.
    for index in [10, 17, 18] {
        let value = field(index)?;

        let owned = builder
            .build_int_compare(
                inkwell::IntPredicate::NE,
                value,
                value.get_type().const_zero(),
                "report.owned",
            )
            .map_err(CodegenFailure::backend_library)?;

        present = builder
            .build_or(present, owned, "report.has_ownership")
            .map_err(CodegenFailure::backend_library)?;
    }

    Ok(present)
}

pub(crate) fn require_runtime_success<'context>(
    module: &Module<'context>,
    context: &'context Context,
    builder: &Builder<'context>,
    status: BasicValueEnum<'context>,
    name: &str,
) -> Result<(), CodegenFailure> {
    let status = status.into_int_value();

    let function = builder
        .get_insert_block()
        .and_then(BasicBlock::get_parent)
        .expect("runtime status check belongs to a generated function");

    let failed = context.append_basic_block(function, &format!("{name}.failed"));
    let succeeded = context.append_basic_block(function, &format!("{name}.succeeded"));

    let success = builder
        .build_int_compare(
            inkwell::IntPredicate::EQ,
            status,
            status.get_type().const_zero(),
            &format!("{name}.success"),
        )
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_conditional_branch(success, succeeded, failed)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(failed);
    crate::native::call_void_intrinsic(module, context, builder, "llvm.trap")?;

    builder
        .build_unreachable()
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(succeeded);

    Ok(())
}
