use bray_codegen::{CodegenFailure, CodegenRequest, CodegenSymbolKey, CodegenSymbolMapping};
use bray_ir::MirRuntimeReference;
use bray_runtime_abi::NativeRunState;
use bray_runtime_interface::RuntimeAbiRole;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue, PointerValue};

use super::super::support::{
    llvm, native_run_outcome, native_run_outcome_value, native_run_state_is, pointer_value,
};
use crate::mapping::{LlvmTypeMappings, apply_signature_call_attributes};

pub(super) fn translate_runtime_boundary<'context>(
    context: &'context Context,
    module: &Module<'context>,
    request: CodegenRequest<'_>,
    symbol: &CodegenSymbolMapping,
    body: FunctionValue<'context>,
    trampoline: FunctionValue<'context>,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<(), CodegenFailure> {
    assert!(symbol.signature().has_panic_report_context());

    let builder = context.create_builder();

    builder.position_at_end(context.append_basic_block(trampoline, "runtime.entry"));

    let outcome_type = crate::native::run_outcome_type(context, request.target());
    let outcome = llvm(builder.build_alloca(outcome_type, "runtime.outcome"))?;

    llvm(builder.build_store(outcome, outcome_type.const_zero()))?;

    let mut arguments: Vec<BasicMetadataValueEnum<'context>> = trampoline
        .get_params()
        .into_iter()
        .map(Into::into)
        .collect();

    arguments.push(outcome.into());

    let call = llvm(builder.build_call(body, &arguments, ""))?;

    call.set_call_convention(body.get_call_conventions());
    apply_signature_call_attributes(call, symbol.signature(), types)?;

    let result = llvm(builder.build_load(outcome_type, outcome, "runtime.outcome.result"))?;

    let kind = symbol
        .native_entry()
        .expect("native boundary has an entry mapping")
        .kind();

    if kind == bray_runtime_interface::NativeEntryKind::EntryStatus {
        merge_entry_cleanup(
            context, module, request, &builder, trampoline, outcome, result,
        )?;
    } else {
        resolve_callback_outcome(context, module, request, &builder, trampoline, result)?;
    }

    if matches!(
        kind,
        bray_runtime_interface::NativeEntryKind::RuntimeStatus
            | bray_runtime_interface::NativeEntryKind::EntryStatus
    ) {
        let (state, _) = native_run_outcome(&builder, result)?;

        let completed = native_run_state_is(
            &builder,
            state,
            NativeRunState::COMPLETED,
            "runtime.completed",
        )?;

        let status = call
            .try_as_basic_value()
            .basic()
            .expect("runtime status body returns its native status");

        let failure = if kind == bray_runtime_interface::NativeEntryKind::EntryStatus {
            // The adapter has already transferred or consumed this terminal report.
            let panicked = native_run_state_is(
                &builder,
                state,
                NativeRunState::PANICKED,
                "entry.cleanup.panicked",
            )?;

            llvm(builder.build_select(
                panicked,
                context.i32_type().const_int(
                    u64::from(bray_runtime_abi::NativeRuntimeStatus::PANICKED.code()),
                    false,
                ),
                context.i32_type().const_int(
                    u64::from(bray_runtime_abi::NativeRuntimeStatus::RUNTIME_FAILURE.code()),
                    false,
                ),
                "entry.cleanup.status",
            ))?
            .into_int_value()
        } else {
            context.i32_type().const_int(
                u64::from(bray_runtime_abi::NativeRuntimeStatus::RUNTIME_FAILURE.code()),
                false,
            )
        };

        let returned = llvm(builder.build_select(
            completed,
            status,
            BasicValueEnum::IntValue(failure),
            "runtime.status",
        ))?;

        llvm(builder.build_return(Some(&returned)))?;
    } else {
        llvm(builder.build_return(None))?;
    }

    Ok(())
}

fn merge_entry_cleanup<'context>(
    context: &'context Context,
    module: &Module<'context>,
    request: CodegenRequest<'_>,
    builder: &Builder<'context>,
    trampoline: FunctionValue<'context>,
    cleanup: PointerValue<'context>,
    cleanup_value: BasicValueEnum<'context>,
) -> Result<(), CodegenFailure> {
    let destination = trampoline
        .get_last_param()
        .and_then(pointer_value)
        .expect("entry ABI ends with the caller-owned body outcome");

    let outcome_type = crate::native::run_outcome_type(context, request.target());
    let body = llvm(builder.build_load(outcome_type, destination, "entry.body.outcome"))?;

    let (state, _) = native_run_outcome(builder, body)?;

    let completed = native_run_state_is(
        builder,
        state,
        NativeRunState::COMPLETED,
        "entry.body.completed",
    )?;

    let report = context.append_basic_block(trampoline, "entry.report.cleanup");
    let merge = context.append_basic_block(trampoline, "entry.merge.cleanup");
    let finished = context.append_basic_block(trampoline, "entry.cleanup.finished");

    llvm(builder.build_conditional_branch(completed, report, merge))?;
    builder.position_at_end(report);

    // A completed result may own backing. Terminal cleanup cannot erase it.
    resolve_callback_outcome(context, module, request, builder, trampoline, cleanup_value)?;
    llvm(builder.build_unconditional_branch(finished))?;
    builder.position_at_end(merge);

    crate::translation::panic::merge_pending_outcomes(
        module,
        context,
        builder,
        request.target(),
        request.unit().target().runtime_abi(),
        destination,
        cleanup,
    )?;

    llvm(builder.build_unconditional_branch(finished))?;
    builder.position_at_end(finished);

    Ok(())
}

pub(super) fn resolve_callback_outcome<'context>(
    context: &'context Context,
    module: &Module<'context>,
    request: CodegenRequest<'_>,
    builder: &Builder<'context>,
    trampoline: FunctionValue<'context>,
    outcome: BasicValueEnum<'context>,
) -> Result<(), CodegenFailure> {
    let (state, _) = native_run_outcome(builder, outcome)?;

    let payload = super::super::support::extract_value(builder, outcome, 2)?;

    let panicked = native_run_state_is(
        builder,
        state,
        NativeRunState::PANICKED,
        "callback.panicked",
    )?;

    let report = context.append_basic_block(trampoline, "callback.report_panic");
    let complete = context.append_basic_block(trampoline, "callback.complete");

    llvm(builder.build_conditional_branch(panicked, report, complete))?;
    builder.position_at_end(report);

    let panic = MirRuntimeReference::new(
        RuntimeAbiRole::PanicReporting,
        request.unit().target().runtime_abi(),
    );

    let key = CodegenSymbolKey::Runtime(panic);

    let function = request
        .mappings()
        .symbol(&key)
        .and_then(|mapping| module.get_function(mapping.name().as_str()))
        .expect("checked MIR translation requires an established mapping or value");

    crate::native::invoke_function(
        context,
        builder,
        request.target(),
        &key,
        function,
        &[payload.into()],
        "callback.report_panic",
    )?
    .expect("checked MIR translation requires an established mapping or value");

    llvm(builder.build_unconditional_branch(complete))?;
    builder.position_at_end(complete);

    Ok(())
}

pub(super) fn resolve_terminal_status<'context>(
    context: &'context Context,
    module: &Module<'context>,
    request: CodegenRequest<'_>,
    builder: &Builder<'context>,
    trampoline: FunctionValue<'context>,
    status: BasicValueEnum<'context>,
) -> Result<(), CodegenFailure> {
    let status = status.into_int_value();

    let failed = llvm(builder.build_int_compare(
        inkwell::IntPredicate::NE,
        status,
        status.get_type().const_zero(),
        "entry.failed",
    ))?;

    let unreported = llvm(builder.build_int_compare(
        inkwell::IntPredicate::NE,
        status,
        status.get_type().const_int(
            u64::from(bray_runtime_abi::NativeRuntimeStatus::PANICKED.code()),
            false,
        ),
        "entry.unreported",
    ))?;

    let failed = llvm(builder.build_and(failed, unreported, "entry.unreported_failure"))?;
    let report = context.append_basic_block(trampoline, "entry.report.failure");
    let finished = context.append_basic_block(trampoline, "entry.status.finished");

    llvm(builder.build_conditional_branch(failed, report, finished))?;
    builder.position_at_end(report);

    let report = crate::native::panic_report_type(context).const_zero();

    let report = super::super::support::insert_value(
        builder,
        report.into(),
        context
            .i32_type()
            .const_int(
                u64::from(bray_runtime_abi::NativePanicCause::ENTRY_FAILURE.code()),
                false,
            )
            .into(),
        9,
    )?;

    let consumer = crate::native::declare_runtime_function(
        module,
        context,
        request.target(),
        RuntimeAbiRole::ReportConsumer,
    )?;

    let report = super::super::support::insert_value(
        builder,
        report,
        consumer.as_global_value().as_pointer_value().into(),
        18,
    )?;

    let outcome = native_run_outcome_value(
        context,
        builder,
        request.target(),
        NativeRunState::PANICKED,
        report,
    )?;

    resolve_callback_outcome(context, module, request, builder, trampoline, outcome)?;
    llvm(builder.build_unconditional_branch(finished))?;
    builder.position_at_end(finished);

    Ok(())
}
