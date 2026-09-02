use bray_codegen::CodegenFailure;
use inkwell::IntPredicate;
use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::types::IntType;
use inkwell::values::{IntValue, PointerValue};

pub(crate) fn branch_on_pending_panic<'context>(
    context: &'context Context,
    builder: &Builder<'context>,
    ty: IntType<'context>,
    storage: PointerValue<'context>,
) -> Result<(IntValue<'context>, BasicBlock<'context>), CodegenFailure> {
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

    builder
        .build_conditional_branch(pending, propagate, continued)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(propagate);

    Ok((report, continued))
}
