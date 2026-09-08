use bray_codegen::{CodegenCallableSignature, CodegenFailure, CodegenInstanceKey, CodegenMappings};
use inkwell::GlobalVisibility;
use inkwell::builder::Builder;
use inkwell::module::{Linkage, Module};
use inkwell::types::FunctionType;
use inkwell::values::{BasicMetadataValueEnum, CallSiteValue, FunctionValue, PointerValue};

use super::LlvmTypeMappings;
use super::symbol::apply_signature_call_attributes;

pub(super) fn panic_report_callbacks_type(
    context: &inkwell::context::Context,
) -> inkwell::types::StructType<'_> {
    let pointer = context.ptr_type(inkwell::AddressSpace::default());

    context.struct_type(&[pointer.into(); 4], false)
}

pub(super) fn panic_report_callbacks<'context>(
    module: &Module<'context>,
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<inkwell::values::StructValue<'context>, CodegenFailure> {
    let declare = |role| {
        crate::native::declare_runtime_function(module, types.context(), types.target(), role)
            .map(|function| function.as_global_value().as_pointer_value().into())
    };

    let callbacks = bray_codegen::CLEANUP_RUNTIME_ROLES
        .into_iter()
        .map(declare)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(panic_report_callbacks_type(types.context()).const_named_struct(&callbacks))
}

pub(super) fn load_boundary_outcome<'context>(
    builder: &Builder<'context>,
    storage: Option<PointerValue<'context>>,
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<inkwell::values::IntValue<'context>, CodegenFailure> {
    let ty = crate::native::pointer_integer_type(types.context(), types.target());

    storage.map_or(Ok(ty.const_zero()), |storage| {
        builder
            .build_load(ty, storage, "cleanup.outcome")
            .map(|value| value.into_int_value())
            .map_err(CodegenFailure::backend_library)
    })
}

/// Publishes an abnormal callback outcome before any uninitialized result can be read.
pub(super) fn return_abnormal_callback_outcome<'context>(
    builder: &Builder<'context>,
    storage: Option<PointerValue<'context>>,
    destination: PointerValue<'context>,
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<(), CodegenFailure> {
    let Some(storage) = storage else {
        return Ok(());
    };

    let context = types.context();
    let outcome = load_boundary_outcome(builder, Some(storage), types)?;

    let abnormal = builder
        .build_int_compare(
            inkwell::IntPredicate::NE,
            outcome,
            outcome.get_type().const_zero(),
            "cleanup.abnormal",
        )
        .map_err(CodegenFailure::backend_library)?;

    let function = builder
        .get_insert_block()
        .and_then(inkwell::basic_block::BasicBlock::get_parent)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let failed = context.append_basic_block(function, "cleanup.abnormal.return");
    let continued = context.append_basic_block(function, "cleanup.completed");

    builder
        .build_conditional_branch(abnormal, failed, continued)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(failed);

    builder
        .build_store(destination, outcome)
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_return(Some(&context.i32_type().const_zero()))
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(continued);

    Ok(())
}

pub(super) fn mapped_instance_function<'context, 'mappings>(
    module: &Module<'context>,
    mappings: &'mappings CodegenMappings,
    instance: &CodegenInstanceKey,
) -> Result<(FunctionValue<'context>, &'mappings CodegenCallableSignature), CodegenFailure> {
    let symbol = mappings
        .instance_symbol(instance)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let function = module
        .get_function(symbol.name().as_str())
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    Ok((function, symbol.signature()))
}

pub(super) fn invoke_generated_call<'context>(
    builder: &Builder<'context>,
    function: FunctionValue<'context>,
    signature: &CodegenCallableSignature,
    semantic_arguments: &[BasicMetadataValueEnum<'context>],
    name: &str,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<(CallSiteValue<'context>, Option<PointerValue<'context>>), CodegenFailure> {
    let context = types.context();
    let usize = crate::native::pointer_integer_type(context, types.target());
    let mut arguments = semantic_arguments.to_vec();

    let panic_report_context = if signature.has_panic_report_context() {
        let storage = crate::translation::allocate_temporary(
            context,
            builder,
            usize,
            "generated.call.panic.report.context",
        )?;

        builder
            .build_store(storage, usize.const_zero())
            .map_err(CodegenFailure::backend_library)?;

        arguments.push(storage.into());

        Some(storage)
    } else {
        None
    };

    let call = builder
        .build_call(function, &arguments, name)
        .map_err(CodegenFailure::backend_library)?;

    call.set_call_convention(function.get_call_conventions());

    apply_signature_call_attributes(call, signature, types)?;

    Ok((call, panic_report_context))
}

/// Retains the first panic and suppresses a later panic after both cleanup calls have returned.
pub(super) fn merge_boundary_outcomes<'context>(
    module: &Module<'context>,
    builder: &Builder<'context>,
    first: Option<PointerValue<'context>>,
    second: Option<PointerValue<'context>>,
    types: &LlvmTypeMappings<'context, '_>,
) -> Result<Option<PointerValue<'context>>, CodegenFailure> {
    let (Some(first), Some(second)) = (first, second) else {
        return Ok(first.or(second));
    };

    let context = types.context();
    let ty = crate::native::pointer_integer_type(context, types.target());

    let load = |storage| {
        builder
            .build_load(ty, storage, "cleanup.outcome")
            .map_err(CodegenFailure::backend_library)
            .map(|value| value.into_int_value())
    };

    let primary = load(first)?;
    let secondary = load(second)?;

    let is_panic = |value| {
        builder
            .build_int_compare(
                inkwell::IntPredicate::UGT,
                value,
                ty.const_int(crate::translation::CANCELLATION_OUTCOME_SENTINEL, false),
                "cleanup.panicked",
            )
            .map_err(CodegenFailure::backend_library)
    };

    let primary_panicked = is_panic(primary)?;
    let secondary_panicked = is_panic(secondary)?;

    let both_panicked = builder
        .build_and(
            primary_panicked,
            secondary_panicked,
            "cleanup.both.panicked",
        )
        .map_err(CodegenFailure::backend_library)?;

    let function = builder
        .get_insert_block()
        .and_then(inkwell::basic_block::BasicBlock::get_parent)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let suppress = context.append_basic_block(function, "cleanup.suppress");
    let select = context.append_basic_block(function, "cleanup.select");
    let merged = context.append_basic_block(function, "cleanup.merged");

    builder
        .build_conditional_branch(both_panicked, suppress, select)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(suppress);

    let runtime = crate::native::declare_runtime_function(
        module,
        context,
        types.target(),
        bray_runtime_interface::RuntimeAbiRole::PanicReportSuppression,
    )?;

    let call = builder
        .build_call(
            runtime,
            &[primary.into(), secondary.into()],
            "cleanup.suppressed",
        )
        .map_err(CodegenFailure::backend_library)?;

    call.set_call_convention(runtime.get_call_conventions());

    let report = call
        .try_as_basic_value()
        .basic()
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    builder
        .build_store(first, report)
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_unconditional_branch(merged)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(select);

    // Panic outranks cancellation. With no panic, the only outcomes are zero and the cancellation marker.
    let no_panic = builder
        .build_or(primary, secondary, "cleanup.nonpanic.outcome")
        .map_err(CodegenFailure::backend_library)?;

    let secondary_or_cancelled = builder
        .build_select(
            secondary_panicked,
            secondary,
            no_panic,
            "cleanup.secondary.or.cancelled",
        )
        .map_err(CodegenFailure::backend_library)?;

    let selected = builder
        .build_select(
            primary_panicked,
            primary,
            secondary_or_cancelled.into_int_value(),
            "cleanup.selected",
        )
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_store(first, selected)
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_unconditional_branch(merged)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(merged);

    Ok(Some(first))
}

pub(super) fn declare_generated_callback<'context>(
    module: &Module<'context>,
    name: &str,
    block_name: &str,
    ty: FunctionType<'context>,
    types: &LlvmTypeMappings<'context, '_>,
) -> FunctionValue<'context> {
    let callback = module.add_function(name, ty, None);

    callback.set_linkage(Linkage::WeakODR);

    callback
        .as_global_value()
        .set_visibility(GlobalVisibility::Hidden);

    crate::comdat::attach_any(
        module,
        callback.as_global_value(),
        name,
        types.target().machine().object_format(),
    );

    module
        .get_context()
        .append_basic_block(callback, block_name);

    callback
}

#[cfg(test)]
mod tests {
    use bray_codegen::test_support::codegen_request;
    use inkwell::context::Context;
    use inkwell::values::AnyValue;

    use super::merge_boundary_outcomes;
    use crate::machine::LlvmTargetMachine;
    use crate::mapping::LlvmTypeMappings;

    #[test]
    fn native_callback_outcomes_return_before_reading_a_failed_result() {
        let fixture = codegen_request();
        let request = fixture.request();
        let machine = LlvmTargetMachine::create(request.target()).unwrap();
        let target_data = machine.target_data();
        let context = Context::create();

        let types =
            LlvmTypeMappings::new(&context, request.mappings(), request.target(), &target_data);

        let word = crate::native::pointer_integer_type(&context, request.target());

        for raw in [0, 1, 16] {
            let module = context.create_module("callback_outcome");

            machine.configure_module(&module);

            let function = module.add_function(
                "callback",
                context.i32_type().fn_type(
                    &[context.ptr_type(inkwell::AddressSpace::default()).into()],
                    false,
                ),
                None,
            );

            let entry = context.append_basic_block(function, "entry");
            let builder = context.create_builder();

            builder.position_at_end(entry);

            let outcome = builder.build_alloca(word, "outcome").unwrap();

            builder
                .build_store(outcome, word.const_int(raw, false))
                .unwrap();

            super::return_abnormal_callback_outcome(
                &builder,
                Some(outcome),
                function.get_first_param().unwrap().into_pointer_value(),
                &types,
            )
            .unwrap();

            builder
                .build_return(Some(&context.i32_type().const_int(7, false)))
                .unwrap();

            module.verify().unwrap();

            machine
                .run_passes(&module, "function(sroa,instcombine,simplifycfg)")
                .unwrap();

            let ir = function.print_to_string().to_string();

            assert!(!ir.contains("call "), "{ir}");

            assert!(
                ir.contains(if raw == 0 { "ret i32 7" } else { "ret i32 0" }),
                "{ir}"
            );

            assert_eq!(ir.contains("store "), raw != 0, "{ir}");
        }
    }

    #[test]
    fn cleanup_outcomes_preserve_panic_precedence_and_suppress_secondary_panics() {
        let fixture = codegen_request();
        let request = fixture.request();
        let machine = LlvmTargetMachine::create(request.target()).unwrap();
        let target_data = machine.target_data();
        let context = Context::create();

        let types =
            LlvmTypeMappings::new(&context, request.mappings(), request.target(), &target_data);

        let ty = crate::native::pointer_integer_type(&context, request.target());

        for (first, second, expected) in [
            (0, 0, Some(0)),
            (0, 1, Some(1)),
            (1, 0, Some(1)),
            (1, 1, Some(1)),
            (16, 0, Some(16)),
            (0, 32, Some(32)),
            (16, 1, Some(16)),
            (1, 32, Some(32)),
            (16, 32, None),
        ] {
            let module = context.create_module("cleanup_outcomes");

            machine.configure_module(&module);

            let function = module.add_function("root", ty.fn_type(&[], false), None);
            let entry = context.append_basic_block(function, "entry");
            let builder = context.create_builder();

            builder.position_at_end(entry);

            let primary = builder.build_alloca(ty, "primary").unwrap();
            let secondary = builder.build_alloca(ty, "secondary").unwrap();

            builder
                .build_store(primary, ty.const_int(first, false))
                .unwrap();

            builder
                .build_store(secondary, ty.const_int(second, false))
                .unwrap();

            let outcome =
                merge_boundary_outcomes(&module, &builder, Some(primary), Some(secondary), &types)
                    .unwrap()
                    .unwrap();

            let result = builder.build_load(ty, outcome, "result").unwrap();

            builder.build_return(Some(&result)).unwrap();
            module.verify().unwrap();

            machine
                .run_passes(&module, "function(sroa,instcombine,simplifycfg)")
                .unwrap();

            module.verify().unwrap();

            let ir = function.print_to_string().to_string();

            match expected {
                Some(expected) => {
                    assert!(
                        ir.contains(&format!("ret i{} {expected}", ty.get_bit_width())),
                        "{first}, {second}: {ir}"
                    );

                    assert!(!ir.contains("call "), "{ir}");
                }
                None => {
                    assert_eq!(ir.matches("call ").count(), 1, "{ir}");

                    assert!(
                        ir.contains("@bray_runtime_panic_report_suppression"),
                        "{ir}"
                    );

                    assert!(
                        ir.contains(&format!(
                            "i{} 16, i{} 32",
                            ty.get_bit_width(),
                            ty.get_bit_width()
                        )),
                        "{ir}"
                    );
                }
            }
        }
    }
}
