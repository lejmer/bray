use bray_codegen::CodegenFailure;
use inkwell::IntPredicate;
use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::types::IntType;
use inkwell::values::{IntValue, PointerValue};

pub(crate) const CANCELLATION_OUTCOME_SENTINEL: u64 = 1;

pub(crate) fn branch_on_pending_outcome<'context>(
    context: &'context Context,
    builder: &Builder<'context>,
    ty: IntType<'context>,
    storage: PointerValue<'context>,
) -> Result<
    (
        IntValue<'context>,
        BasicBlock<'context>,
        BasicBlock<'context>,
    ),
    CodegenFailure,
> {
    let report = builder
        .build_load(ty, storage, "call.panic.report")
        .map_err(CodegenFailure::backend_library)?
        .into_int_value();

    let pending = builder
        .build_int_compare(
            IntPredicate::NE,
            report,
            ty.const_zero(),
            "call.panic.pending",
        )
        .map_err(CodegenFailure::backend_library)?;

    let function = builder
        .get_insert_block()
        .and_then(BasicBlock::get_parent)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let propagate = context.append_basic_block(function, "call.panic.propagate");
    let continued = context.append_basic_block(function, "call.panic.continue");
    let inspect = context.append_basic_block(function, "call.outcome.inspect");
    let cancelled = context.append_basic_block(function, "call.cancelled.propagate");

    builder
        .build_conditional_branch(pending, inspect, continued)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(inspect);

    let cancellation = builder
        .build_int_compare(
            IntPredicate::EQ,
            report,
            ty.const_int(CANCELLATION_OUTCOME_SENTINEL, false),
            "call.cancelled",
        )
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_conditional_branch(cancellation, cancelled, propagate)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(propagate);

    Ok((report, continued, cancelled))
}
