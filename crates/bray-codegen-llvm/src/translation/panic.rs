use bray_codegen::{CodegenFailure, CodegenSymbolKey, CodegenTarget};
use bray_runtime_interface::{RuntimeAbiRole, RuntimeAbiVersion};
use bray_runtime_abi::NativeRunState;
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

    let first = builder
        .build_load(ty, destination, "outcome.first")
        .map_err(CodegenFailure::backend_library)?
        .into_struct_value();

    let second = builder
        .build_load(ty, secondary, "outcome.second")
        .map_err(CodegenFailure::backend_library)?
        .into_struct_value();

    let first_state = builder
        .build_extract_value(first, 0, "outcome.first.state")
        .map_err(CodegenFailure::backend_library)?
        .into_int_value();

    let second_state = builder
        .build_extract_value(second, 0, "outcome.second.state")
        .map_err(CodegenFailure::backend_library)?
        .into_int_value();

    let first_panicked =
        native_run_state_is(builder, first_state, NativeRunState::PANICKED, "outcome.first.panicked")?;

    let second_panicked = native_run_state_is(
        builder,
        second_state,
        NativeRunState::PANICKED,
        "outcome.second.panicked",
    )?;

    let both_panicked = builder
        .build_and(first_panicked, second_panicked, "outcome.both.panicked")
        .map_err(CodegenFailure::backend_library)?;

    let function = builder
        .get_insert_block()
        .and_then(BasicBlock::get_parent)
        .expect("LLVM outcome merge requires an active function");

    let merge = context.append_basic_block(function, "outcome.merge");
    let inspect = context.append_basic_block(function, "outcome.inspect.second");
    let replace = context.append_basic_block(function, "outcome.replace");
    let finished = context.append_basic_block(function, "outcome.finished");

    builder
        .build_conditional_branch(both_panicked, merge, inspect)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(merge);

    let first_report = builder
        .build_struct_gep(ty, destination, 2, "outcome.first.report")
        .map_err(CodegenFailure::backend_library)?;

    let second_report = builder
        .build_struct_gep(ty, secondary, 2, "outcome.second.report")
        .map_err(CodegenFailure::backend_library)?;

    let role = RuntimeAbiRole::PanicReportSuppression;
    let runtime = crate::native::declare_runtime_function(module, context, target, role)?;

    let report = crate::native::invoke_function(
        context,
        builder,
        target,
        &CodegenSymbolKey::Runtime(bray_ir::MirRuntimeReference::new(role, runtime_abi)),
        runtime,
        &[first_report.into(), second_report.into()],
        "outcome.suppressed.report",
    )?
    .expect("panic report suppression must return a report");

    builder
        .build_store(first_report, report)
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_unconditional_branch(finished)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(inspect);

    let second_completed =
        native_run_state_is(builder, second_state, NativeRunState::COMPLETED, "outcome.second.completed")?;

    let keep_first = builder
        .build_or(first_panicked, second_completed, "outcome.keep.first")
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_conditional_branch(keep_first, finished, replace)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(replace);

    builder
        .build_store(destination, second)
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_unconditional_branch(finished)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(finished);

    Ok(())
}
