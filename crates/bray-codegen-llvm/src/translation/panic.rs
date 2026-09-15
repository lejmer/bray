use bray_codegen::CodegenFailure;
use bray_runtime_abi::NativeRunState;
use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context;
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
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

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
